use std::{
    net::TcpListener,
    path::{Path, PathBuf},
    process::Stdio,
    time::{Duration, Instant},
};

use tokio::{
    net::TcpStream,
    process::{Child, Command},
    sync::Mutex,
    time::{sleep, timeout},
};
use tracing::info;

use super::{DesktopError, DesktopTarget};
use crate::config::{DesktopConfig, DesktopMode, ManagedDesktopProvider};

const START_TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Debug, Default)]
pub struct ManagedDesktopManager {
    process: Mutex<Option<ManagedDesktopProcess>>,
}

impl ManagedDesktopManager {
    pub async fn target_for(&self, config: &DesktopConfig) -> Result<DesktopTarget, DesktopError> {
        match config.mode {
            DesktopMode::External => Ok(DesktopTarget::external(config)),
            DesktopMode::Managed => self.ensure_ultravnc(config).await,
            DesktopMode::Native => {
                if !cfg!(windows) {
                    return Err(DesktopError::UnsupportedNativePlatform);
                }
                Ok(DesktopTarget::native(config))
            }
        }
    }

    async fn ensure_ultravnc(&self, config: &DesktopConfig) -> Result<DesktopTarget, DesktopError> {
        if !cfg!(windows) {
            return Err(DesktopError::UnsupportedManagedPlatform);
        }

        let executable = resolve_managed_executable(&config.managed_executable)?;
        let mut process = self.process.lock().await;
        if let Some(existing) = process.as_mut()
            && existing.matches(config.managed_provider, &executable, config.view_only)
            && existing.is_running().await?
        {
            return Ok(existing.target(config));
        }

        if let Some(mut existing) = process.take() {
            existing.stop();
        }

        let port = available_loopback_port()?;
        let parent = executable
            .parent()
            .ok_or_else(|| DesktopError::MissingManagedExecutableParent(executable.clone()))?;
        write_ultravnc_ini(parent, port, config.view_only).await?;

        let mut child = Command::new(&executable)
            .arg("-multi")
            .arg("-run")
            .current_dir(parent)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()?;

        wait_for_listener(&mut child, port).await?;

        let managed = ManagedDesktopProcess {
            child,
            executable,
            provider: config.managed_provider,
            port,
            view_only: config.view_only,
        };
        let target = managed.target(config);
        *process = Some(managed);
        info!(port = target.port, "managed UltraVNC desktop started");
        Ok(target)
    }
}

#[derive(Debug)]
struct ManagedDesktopProcess {
    child: Child,
    executable: PathBuf,
    provider: ManagedDesktopProvider,
    port: u16,
    view_only: bool,
}

impl ManagedDesktopProcess {
    fn matches(
        &self,
        provider: ManagedDesktopProvider,
        executable: &Path,
        view_only: bool,
    ) -> bool {
        self.provider == provider && self.executable == executable && self.view_only == view_only
    }

    fn target(&self, config: &DesktopConfig) -> DesktopTarget {
        DesktopTarget::managed(config, self.port)
    }

    async fn is_running(&mut self) -> Result<bool, DesktopError> {
        if self.child.try_wait()?.is_none() {
            return Ok(true);
        }

        Ok(listener_is_open(self.port).await)
    }

    fn stop(&mut self) {
        let _ = self.child.start_kill();
    }
}

impl Drop for ManagedDesktopProcess {
    fn drop(&mut self) {
        self.stop();
    }
}

fn resolve_managed_executable(path: &Path) -> Result<PathBuf, DesktopError> {
    if path.as_os_str().is_empty() {
        return Err(DesktopError::EmptyManagedExecutable);
    }

    let executable = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };

    if !executable.is_file() {
        return Err(DesktopError::MissingManagedExecutable(executable));
    }

    Ok(executable)
}

fn available_loopback_port() -> Result<u16, std::io::Error> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    Ok(listener.local_addr()?.port())
}

async fn write_ultravnc_ini(
    parent: &Path,
    port: u16,
    view_only: bool,
) -> Result<(), std::io::Error> {
    let inputs_enabled = if view_only { 0 } else { 1 };
    let ini = format!(
        "\
[admin]\n\
UseRegistry=0\n\
SocketConnect=1\n\
primary=1\n\
secondary=1\n\
PortNumber={port}\n\
AutoPortSelect=0\n\
HTTPConnect=0\n\
HTTPPortNumber=0\n\
InputsEnabled={inputs_enabled}\n\
AllowLoopback=1\n\
LoopbackOnly=1\n\
AuthRequired=0\n\
AuthHosts=+127.0.0.1:+::1:\n\
QuerySetting=0\n\
QueryAccept=1\n\
QueryIfNoLogon=0\n\
ConnectPriority=1\n\
MaxViewerSetting=0\n\
MaxViewers=128\n\
IdleTimeout=0\n\
IdleInputTimeout=0\n\
KeepAliveInterval=5\n\
sendbuffer=8192\n\
LockSetting=0\n\
AllowShutdown=0\n\
AllowProperties=0\n\
DisableTrayIcon=1\n\
RemoveWallpaper=0\n\
\n\
[poll]\n\
PollFullScreen=1\n\
PollForeground=1\n\
PollUnderCursor=1\n\
OnlyPollConsole=0\n\
OnlyPollOnEvent=0\n\
MaxCpu2=100\n\
MaxFPS=60\n\
EnableHook=1\n\
EnableDriver=0\n\
EnableVirtual=0\n\
TurboMode=1\n"
    );

    tokio::fs::write(parent.join("ultravnc.portable"), b"").await?;
    tokio::fs::write(parent.join("ultravnc.ini"), ini).await
}

async fn wait_for_listener(child: &mut Child, port: u16) -> Result<(), DesktopError> {
    let started_at = Instant::now();
    let timeout_seconds = START_TIMEOUT.as_secs();
    let mut last_error = "listener was not checked".to_string();

    loop {
        if let Some(status) = child.try_wait()? {
            return Err(DesktopError::ManagedProcessExited(status.to_string()));
        }

        if started_at.elapsed() >= START_TIMEOUT {
            return Err(DesktopError::ManagedStartupTimedOut {
                port,
                timeout_seconds,
                last_error,
            });
        }

        match timeout(
            Duration::from_millis(250),
            TcpStream::connect(("127.0.0.1", port)),
        )
        .await
        {
            Ok(Ok(_)) => return Ok(()),
            Ok(Err(error)) => {
                last_error = error.to_string();
            }
            Err(_) => {
                last_error = "connection attempt timed out".to_string();
            }
        }

        sleep(Duration::from_millis(100)).await;
    }
}

async fn listener_is_open(port: u16) -> bool {
    matches!(
        timeout(
            Duration::from_millis(250),
            TcpStream::connect(("127.0.0.1", port)),
        )
        .await,
        Ok(Ok(_))
    )
}
