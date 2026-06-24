use std::{
    collections::{HashMap, VecDeque},
    env,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use rand::random;
use serde::Serialize;
use tokio::sync::{RwLock, broadcast};

const DEFAULT_TERMINAL_ROWS: u16 = 28;
const DEFAULT_TERMINAL_COLS: u16 = 100;
const TERMINAL_HISTORY_BYTES: usize = 512 * 1024;
const ROOT_TERMINAL_SCOPE: &str = "root";

#[derive(Default)]
pub struct TerminalSessionManager {
    sessions: RwLock<HashMap<String, Arc<TerminalSession>>>,
}

#[derive(Clone, Debug, Serialize)]
pub struct TerminalSessionSummary {
    pub id: String,
    pub scope: String,
    pub project: Option<String>,
    pub title: String,
    pub cwd: String,
    pub created_at_ms: u128,
    pub connected_clients: usize,
    pub alive: bool,
}

pub struct TerminalSession {
    id: String,
    scope: String,
    project: Option<String>,
    title: String,
    cwd: PathBuf,
    created_at_ms: u128,
    master: Mutex<Box<dyn MasterPty + Send>>,
    writer: Mutex<Box<dyn Write + Send>>,
    child: Mutex<Box<dyn Child + Send + Sync>>,
    output_tx: broadcast::Sender<Vec<u8>>,
    history: Arc<Mutex<TerminalHistory>>,
    connected_clients: AtomicUsize,
    alive: Arc<AtomicBool>,
}

#[derive(Default)]
struct TerminalHistory {
    chunks: VecDeque<Vec<u8>>,
    byte_count: usize,
}

impl TerminalSessionManager {
    pub async fn list_root(&self) -> Vec<TerminalSessionSummary> {
        self.list_scope(ROOT_TERMINAL_SCOPE).await
    }

    pub async fn list_project(&self, project: &str) -> Vec<TerminalSessionSummary> {
        self.list_scope(&project_terminal_scope(project)).await
    }

    async fn list_scope(&self, scope: &str) -> Vec<TerminalSessionSummary> {
        let sessions = self.sessions.read().await;
        let mut summaries = sessions
            .values()
            .filter(|session| session.scope == scope)
            .map(|session| session.summary())
            .collect::<Vec<_>>();
        summaries.sort_by_key(|summary| summary.created_at_ms);
        summaries
    }

    pub async fn get_root_session(&self, id: &str) -> Option<Arc<TerminalSession>> {
        self.get_scope_session(ROOT_TERMINAL_SCOPE, id).await
    }

    pub async fn get_project_session(
        &self,
        project: &str,
        id: &str,
    ) -> Option<Arc<TerminalSession>> {
        self.get_scope_session(&project_terminal_scope(project), id)
            .await
    }

    async fn get_scope_session(&self, scope: &str, id: &str) -> Option<Arc<TerminalSession>> {
        let sessions = self.sessions.read().await;
        sessions
            .get(id)
            .filter(|session| session.scope == scope)
            .cloned()
    }

    pub async fn create_root_session(&self) -> Result<Arc<TerminalSession>, String> {
        self.create_scoped_session(
            ROOT_TERMINAL_SCOPE.to_string(),
            None,
            "Root Terminal",
            root_terminal_cwd(),
        )
        .await
    }

    pub async fn create_session(
        &self,
        project: &str,
        project_dir: &Path,
    ) -> Result<Arc<TerminalSession>, String> {
        self.create_scoped_session(
            project_terminal_scope(project),
            Some(project.to_string()),
            "Terminal",
            terminal_cwd(project_dir),
        )
        .await
    }

    async fn create_scoped_session(
        &self,
        scope: String,
        project: Option<String>,
        title_prefix: &str,
        cwd: PathBuf,
    ) -> Result<Arc<TerminalSession>, String> {
        let title = {
            let sessions = self.sessions.read().await;
            let count = sessions
                .values()
                .filter(|session| session.scope == scope)
                .count();
            format!("{title_prefix} {}", count + 1)
        };

        let session = TerminalSession::spawn(scope, project, title, cwd)?;
        self.sessions
            .write()
            .await
            .insert(session.id.clone(), session.clone());
        Ok(session)
    }

    pub async fn close_root_session(&self, id: &str) -> bool {
        self.close_scope_session(ROOT_TERMINAL_SCOPE, id).await
    }

    pub async fn close_project_session(&self, project: &str, id: &str) -> bool {
        self.close_scope_session(&project_terminal_scope(project), id)
            .await
    }

    async fn close_scope_session(&self, scope: &str, id: &str) -> bool {
        let removed = {
            let mut sessions = self.sessions.write().await;
            if sessions
                .get(id)
                .is_some_and(|session| session.scope == scope)
            {
                sessions.remove(id)
            } else {
                None
            }
        };

        if let Some(session) = removed {
            session.kill();
            true
        } else {
            false
        }
    }
}

impl TerminalSession {
    fn spawn(
        scope: String,
        project: Option<String>,
        title: String,
        cwd: PathBuf,
    ) -> Result<Arc<Self>, String> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: DEFAULT_TERMINAL_ROWS,
                cols: DEFAULT_TERMINAL_COLS,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| format!("Latitude could not open a PTY: {error}"))?;

        let command = terminal_pty_command(&cwd);
        let child = pair
            .slave
            .spawn_command(command)
            .map_err(|error| format!("Latitude could not start a shell: {error}"))?;
        drop(pair.slave);

        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|error| format!("Latitude could not attach terminal output: {error}"))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|error| format!("Latitude could not attach terminal input: {error}"))?;

        let (output_tx, _) = broadcast::channel::<Vec<u8>>(256);
        let history = Arc::new(Mutex::new(TerminalHistory::default()));
        let alive = Arc::new(AtomicBool::new(true));
        let session = Arc::new(Self {
            id: terminal_session_id(),
            scope,
            project,
            title,
            cwd,
            created_at_ms: current_time_ms(),
            master: Mutex::new(pair.master),
            writer: Mutex::new(writer),
            child: Mutex::new(child),
            output_tx,
            history,
            connected_clients: AtomicUsize::new(0),
            alive,
        });

        session.start_reader(reader);
        let intro = format!(
            "\x1b[32mLatitude terminal connected: {}\x1b[0m\r\n",
            display_path(&session.cwd)
        );
        session.push_output(intro.into_bytes());

        Ok(session)
    }

    pub fn summary(&self) -> TerminalSessionSummary {
        TerminalSessionSummary {
            id: self.id.clone(),
            scope: self.scope.clone(),
            project: self.project.clone(),
            title: self.title.clone(),
            cwd: display_path(&self.cwd),
            created_at_ms: self.created_at_ms,
            connected_clients: self.connected_clients.load(Ordering::SeqCst),
            alive: self.alive.load(Ordering::SeqCst),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Vec<u8>> {
        self.output_tx.subscribe()
    }

    pub fn history(&self) -> Vec<Vec<u8>> {
        self.history
            .lock()
            .map(|history| history.chunks.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn attach_client(&self) {
        self.connected_clients.fetch_add(1, Ordering::SeqCst);
    }

    pub fn detach_client(&self) {
        self.connected_clients.fetch_sub(1, Ordering::SeqCst);
    }

    pub fn write_input(&self, data: &str) {
        if let Ok(mut writer) = self.writer.lock() {
            let _ = writer.write_all(data.as_bytes());
            let _ = writer.flush();
        }
    }

    pub fn resize(&self, cols: u16, rows: u16) {
        let cols = cols.clamp(20, 500);
        let rows = rows.clamp(5, 200);
        if let Ok(master) = self.master.lock() {
            let _ = master.resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            });
        }
    }

    pub fn kill(&self) {
        self.alive.store(false, Ordering::SeqCst);
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
        }
    }

    fn start_reader(self: &Arc<Self>, mut reader: Box<dyn Read + Send>) {
        let session = self.clone();
        std::thread::spawn(move || {
            let mut buffer = [0_u8; 8192];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(count) => session.push_output(buffer[..count].to_vec()),
                    Err(_) => break,
                }
            }

            session.alive.store(false, Ordering::SeqCst);
            session.push_output(b"\r\n[Latitude terminal process exited]\r\n".to_vec());
        });
    }

    fn push_output(&self, output: Vec<u8>) {
        if let Ok(mut history) = self.history.lock() {
            history.byte_count += output.len();
            history.chunks.push_back(output.clone());
            while history.byte_count > TERMINAL_HISTORY_BYTES {
                if let Some(removed) = history.chunks.pop_front() {
                    history.byte_count = history.byte_count.saturating_sub(removed.len());
                } else {
                    break;
                }
            }
        }
        let _ = self.output_tx.send(output);
    }
}

pub fn root_terminal_cwd() -> PathBuf {
    terminal_cwd(&user_home_dir())
}

pub fn terminal_cwd(project_dir: &Path) -> PathBuf {
    let cwd = std::fs::canonicalize(project_dir).unwrap_or_else(|_| project_dir.to_path_buf());
    strip_windows_extended_path_prefix(cwd)
}

fn project_terminal_scope(project: &str) -> String {
    format!("project:{project}")
}

fn user_home_dir() -> PathBuf {
    if cfg!(windows) {
        if let Some(profile) = non_empty_var("USERPROFILE") {
            return PathBuf::from(profile);
        }

        if let (Some(drive), Some(path)) =
            (non_empty_string("HOMEDRIVE"), non_empty_string("HOMEPATH"))
        {
            return PathBuf::from(format!("{drive}{path}"));
        }
    }

    if let Some(home) = non_empty_var("HOME") {
        return PathBuf::from(home);
    }

    env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn non_empty_var(name: &str) -> Option<std::ffi::OsString> {
    let value = env::var_os(name)?;
    if value.is_empty() { None } else { Some(value) }
}

fn non_empty_string(name: &str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.is_empty())
}

fn terminal_pty_command(cwd: &Path) -> CommandBuilder {
    let mut command = if cfg!(windows) {
        let mut command = CommandBuilder::new("powershell.exe");
        command.arg("-NoLogo");
        command
    } else {
        CommandBuilder::new(std::env::var("SHELL").unwrap_or_else(|_| "sh".to_string()))
    };
    command.cwd(cwd);
    command.env("TERM", "xterm-256color");
    command.env("COLORTERM", "truecolor");
    command
}

fn terminal_session_id() -> String {
    format!("{:032x}", random::<u128>())
}

fn current_time_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

fn display_path(path: &Path) -> String {
    display_path_text(path)
}

fn display_path_text(path: &Path) -> String {
    let path = path.display().to_string();
    strip_windows_extended_path_text(&path).into_owned()
}

fn strip_windows_extended_path_prefix(path: PathBuf) -> PathBuf {
    let path_text = path.display().to_string();
    let stripped = strip_windows_extended_path_text(&path_text);
    if stripped == path_text.as_str() {
        path
    } else {
        PathBuf::from(stripped.into_owned())
    }
}

fn strip_windows_extended_path_text(path: &str) -> std::borrow::Cow<'_, str> {
    const EXTENDED_UNC_PREFIX: &str = "\\\\?\\UNC\\";
    const EXTENDED_PATH_PREFIX: &str = "\\\\?\\";

    if let Some(stripped) = path.strip_prefix(EXTENDED_UNC_PREFIX) {
        return std::borrow::Cow::Owned(format!("\\\\{stripped}"));
    }

    path.strip_prefix(EXTENDED_PATH_PREFIX)
        .map(std::borrow::Cow::Borrowed)
        .unwrap_or_else(|| std::borrow::Cow::Borrowed(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_windows_extended_drive_prefix_for_terminal_cwd() {
        assert_eq!(
            strip_windows_extended_path_prefix(PathBuf::from(r"\\?\C:\work\demo")),
            PathBuf::from(r"C:\work\demo")
        );
    }

    #[test]
    fn strips_windows_extended_unc_prefix_for_terminal_cwd() {
        assert_eq!(
            strip_windows_extended_path_prefix(PathBuf::from(r"\\?\UNC\server\share\demo")),
            PathBuf::from(r"\\server\share\demo")
        );
    }

    #[test]
    fn keeps_normal_terminal_cwd_paths() {
        assert_eq!(
            strip_windows_extended_path_prefix(PathBuf::from(r"C:\work\demo")),
            PathBuf::from(r"C:\work\demo")
        );
    }
}
