use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use hmac::{Hmac, Mac};
use rand::random;
use reqwest::Client;
use sha2::Sha256;
use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard, RwLock};

use crate::{
    config::{BootConfig, ConfigError},
    desktop::{ManagedDesktopManager, NativeSessionBridge},
    device::current_hostname,
    project_files::ProjectFileService,
    server::GitStatusSummary,
    storage::CatalogStore,
    terminal::TerminalSessionManager,
    workspace::WorkspaceBridge,
};

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone)]
pub struct AppState {
    inner: Arc<AppStateInner>,
}

pub enum GitRefreshAccess {
    Leader(GitRefreshPermit),
    Reused,
}

pub struct GitRefreshPermit {
    state: AppState,
    fetch_remote: bool,
    _guard: OwnedMutexGuard<()>,
}

struct AppStateInner {
    config_path: PathBuf,
    config: RwLock<BootConfig>,
    catalog: CatalogStore,
    client: Client,
    device_hostname: String,
    public_auth_secret: [u8; 32],
    desktop_manager: Arc<ManagedDesktopManager>,
    native_session_bridge: Option<NativeSessionBridge>,
    workspace_bridge: Option<WorkspaceBridge>,
    terminal_sessions: Arc<TerminalSessionManager>,
    project_files: ProjectFileService,
    project_git_statuses: RwLock<HashMap<String, GitStatusSummary>>,
    git_refresh_lock: Arc<AsyncMutex<()>>,
    git_refresh_generation: AtomicU64,
    git_remote_fetch_generation: AtomicU64,
    git_refresh_completed_at: Mutex<Option<Instant>>,
    git_remote_fetch_completed_at: Mutex<Option<Instant>>,
}

impl AppState {
    #[cfg(test)]
    pub fn new(config_path: PathBuf, config: BootConfig, catalog: CatalogStore) -> Self {
        Self::new_with_bridges(config_path, config, catalog, None, None)
    }

    pub fn new_with_bridges(
        config_path: PathBuf,
        config: BootConfig,
        catalog: CatalogStore,
        native_session_bridge: Option<NativeSessionBridge>,
        workspace_bridge: Option<WorkspaceBridge>,
    ) -> Self {
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("reqwest client builder should be valid");

        Self {
            inner: Arc::new(AppStateInner {
                config_path,
                config: RwLock::new(config),
                catalog,
                client,
                device_hostname: current_hostname(),
                public_auth_secret: random(),
                desktop_manager: Arc::new(ManagedDesktopManager::default()),
                native_session_bridge,
                workspace_bridge,
                terminal_sessions: Arc::new(TerminalSessionManager::default()),
                project_files: ProjectFileService::default(),
                project_git_statuses: RwLock::new(HashMap::new()),
                git_refresh_lock: Arc::new(AsyncMutex::new(())),
                git_refresh_generation: AtomicU64::new(0),
                git_remote_fetch_generation: AtomicU64::new(0),
                git_refresh_completed_at: Mutex::new(None),
                git_remote_fetch_completed_at: Mutex::new(None),
            }),
        }
    }

    pub fn client(&self) -> &Client {
        &self.inner.client
    }

    pub fn device_hostname(&self) -> &str {
        &self.inner.device_hostname
    }

    pub fn terminal_sessions(&self) -> Arc<TerminalSessionManager> {
        self.inner.terminal_sessions.clone()
    }

    pub fn desktop_manager(&self) -> Arc<ManagedDesktopManager> {
        self.inner.desktop_manager.clone()
    }

    pub(crate) fn native_session_bridge(&self) -> Option<NativeSessionBridge> {
        self.inner.native_session_bridge.clone()
    }

    pub(crate) fn workspace_bridge(&self) -> Option<WorkspaceBridge> {
        self.inner.workspace_bridge.clone()
    }

    pub fn catalog(&self) -> &CatalogStore {
        &self.inner.catalog
    }

    pub(crate) fn project_files(&self) -> &ProjectFileService {
        &self.inner.project_files
    }

    pub fn public_auth_cookie_value(&self, password: &str) -> String {
        encode_hex(public_auth_tag(&self.inner.public_auth_secret, password))
    }

    pub fn verify_public_auth_cookie(&self, password: &str, cookie_value: &str) -> bool {
        let Some(tag) = decode_hex(cookie_value) else {
            return false;
        };
        let mac = public_auth_mac(&self.inner.public_auth_secret, password);
        mac.verify_slice(&tag).is_ok()
    }

    pub async fn config_snapshot(&self) -> BootConfig {
        self.inner.config.read().await.clone()
    }

    pub async fn project_git_statuses(&self) -> HashMap<String, GitStatusSummary> {
        self.inner.project_git_statuses.read().await.clone()
    }

    pub async fn set_project_git_status(&self, project: String, status: GitStatusSummary) {
        self.inner
            .project_git_statuses
            .write()
            .await
            .insert(project, status);
    }

    pub async fn acquire_git_refresh(
        &self,
        fetch_remote: bool,
        max_snapshot_age: Duration,
    ) -> GitRefreshAccess {
        let observed_generation = self.inner.git_refresh_generation.load(Ordering::Acquire);
        let guard = self.inner.git_refresh_lock.clone().lock_owned().await;
        let completed_generation = self.inner.git_refresh_generation.load(Ordering::Acquire);
        let remote_fetch_generation = self
            .inner
            .git_remote_fetch_generation
            .load(Ordering::Acquire);
        let local_snapshot_is_recent =
            completed_recently(&self.inner.git_refresh_completed_at, max_snapshot_age);
        let remote_fetch_is_recent =
            completed_recently(&self.inner.git_remote_fetch_completed_at, max_snapshot_age);
        if (completed_generation > observed_generation || local_snapshot_is_recent)
            && (!fetch_remote
                || remote_fetch_generation > observed_generation
                || remote_fetch_is_recent)
        {
            return GitRefreshAccess::Reused;
        }
        GitRefreshAccess::Leader(GitRefreshPermit {
            state: self.clone(),
            fetch_remote,
            _guard: guard,
        })
    }

    pub async fn replace_config(&self, config: BootConfig) -> Result<(), ConfigError> {
        config.validate()?;
        config.save_to(&self.inner.config_path).await?;
        *self.inner.config.write().await = config;
        Ok(())
    }
}

impl GitRefreshPermit {
    pub fn complete(self) {
        if let Ok(mut completed_at) = self.state.inner.git_refresh_completed_at.lock() {
            *completed_at = Some(Instant::now());
        }
        let generation = self
            .state
            .inner
            .git_refresh_generation
            .fetch_add(1, Ordering::AcqRel)
            + 1;
        if self.fetch_remote {
            if let Ok(mut completed_at) = self.state.inner.git_remote_fetch_completed_at.lock() {
                *completed_at = Some(Instant::now());
            }
            self.state
                .inner
                .git_remote_fetch_generation
                .store(generation, Ordering::Release);
        }
    }
}

fn completed_recently(completed_at: &Mutex<Option<Instant>>, max_age: Duration) -> bool {
    completed_at.lock().is_ok_and(|completed_at| {
        completed_at.is_some_and(|completed| completed.elapsed() <= max_age)
    })
}

fn public_auth_tag(secret: &[u8], password: &str) -> impl AsRef<[u8]> {
    let mac = public_auth_mac(secret, password);
    mac.finalize().into_bytes()
}

fn public_auth_mac(secret: &[u8], password: &str) -> HmacSha256 {
    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC should accept any secret length");
    mac.update(password.as_bytes());
    mac
}

fn encode_hex(bytes: impl AsRef<[u8]>) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let bytes = bytes.as_ref();
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn decode_hex(value: &str) -> Option<Vec<u8>> {
    let bytes = value.as_bytes();
    if !bytes.len().is_multiple_of(2) {
        return None;
    }

    let mut output = Vec::with_capacity(bytes.len() / 2);
    for pair in bytes.chunks_exact(2) {
        let high = hex_value(pair[0])?;
        let low = hex_value(pair[1])?;
        output.push((high << 4) | low);
    }
    Some(output)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
