use std::{
    collections::HashMap,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use hmac::{Hmac, Mac};
use rand::random;
use reqwest::Client;
use sha2::Sha256;
use tokio::sync::{Mutex, OwnedMutexGuard, RwLock};

use crate::{
    config::{BootConfig, ConfigError},
    desktop::NativeSessionBridge,
    device::current_hostname,
    server::GitStatusSummary,
    storage::CatalogStore,
    util::{decode_hex, encode_hex},
    workspace::{WorkspaceBridge, WorkspaceServices},
};

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone)]
pub(crate) struct AppState {
    inner: Arc<AppStateInner>,
}

pub(crate) enum GitRefreshAccess {
    Leader(GitRefreshPermit),
    Reused,
}

pub(crate) struct GitRefreshPermit {
    fetch_remote: bool,
    state: OwnedMutexGuard<GitRefreshState>,
}

#[derive(Default)]
struct GitRefreshState {
    completed_at: Option<Instant>,
    remote_fetch_completed_at: Option<Instant>,
}

struct AppStateInner {
    config_path: PathBuf,
    config: RwLock<BootConfig>,
    catalog: CatalogStore,
    client: Client,
    device_hostname: String,
    public_auth_secret: [u8; 32],
    native_session_bridge: Option<NativeSessionBridge>,
    workspace: WorkspaceServices,
    project_git_statuses: RwLock<HashMap<String, GitStatusSummary>>,
    git_refresh: Arc<Mutex<GitRefreshState>>,
}

impl AppState {
    #[cfg(test)]
    pub(crate) fn new(config_path: PathBuf, config: BootConfig, catalog: CatalogStore) -> Self {
        Self::new_with_bridges(config_path, config, catalog, None, None)
    }

    pub(crate) fn new_with_bridges(
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
                native_session_bridge,
                workspace: WorkspaceServices::new(workspace_bridge),
                project_git_statuses: RwLock::new(HashMap::new()),
                git_refresh: Arc::new(Mutex::new(GitRefreshState::default())),
            }),
        }
    }

    pub(crate) fn client(&self) -> &Client {
        &self.inner.client
    }

    pub(crate) fn device_hostname(&self) -> &str {
        &self.inner.device_hostname
    }

    pub(crate) fn native_session_bridge(&self) -> Option<NativeSessionBridge> {
        self.inner.native_session_bridge.clone()
    }

    pub(crate) fn workspace(&self) -> &WorkspaceServices {
        &self.inner.workspace
    }

    pub(crate) fn catalog(&self) -> &CatalogStore {
        &self.inner.catalog
    }

    pub(crate) fn public_auth_cookie_value(&self, password: &str) -> String {
        encode_hex(public_auth_tag(&self.inner.public_auth_secret, password))
    }

    pub(crate) fn verify_public_auth_cookie(&self, password: &str, cookie_value: &str) -> bool {
        let Some(tag) = decode_hex(cookie_value) else {
            return false;
        };
        let mac = public_auth_mac(&self.inner.public_auth_secret, password);
        mac.verify_slice(&tag).is_ok()
    }

    pub(crate) async fn config_snapshot(&self) -> BootConfig {
        self.inner.config.read().await.clone()
    }

    pub(crate) async fn project_git_statuses(&self) -> HashMap<String, GitStatusSummary> {
        self.inner.project_git_statuses.read().await.clone()
    }

    pub(crate) async fn set_project_git_status(&self, project: String, status: GitStatusSummary) {
        self.inner
            .project_git_statuses
            .write()
            .await
            .insert(project, status);
    }

    pub(crate) async fn acquire_git_refresh(
        &self,
        fetch_remote: bool,
        max_snapshot_age: Duration,
    ) -> GitRefreshAccess {
        let requested_at = Instant::now();
        let state = self.inner.git_refresh.clone().lock_owned().await;
        if completed_since_or_recent(state.completed_at, requested_at, max_snapshot_age)
            && (!fetch_remote
                || completed_since_or_recent(
                    state.remote_fetch_completed_at,
                    requested_at,
                    max_snapshot_age,
                ))
        {
            return GitRefreshAccess::Reused;
        }
        GitRefreshAccess::Leader(GitRefreshPermit {
            fetch_remote,
            state,
        })
    }

    pub(crate) async fn replace_config(&self, config: BootConfig) -> Result<(), ConfigError> {
        config.validate()?;
        config.save_to(&self.inner.config_path).await?;
        *self.inner.config.write().await = config;
        Ok(())
    }
}

impl GitRefreshPermit {
    pub(crate) fn complete(mut self) {
        let completed_at = Instant::now();
        self.state.completed_at = Some(completed_at);
        if self.fetch_remote {
            self.state.remote_fetch_completed_at = Some(completed_at);
        }
    }
}

fn completed_since_or_recent(
    completed_at: Option<Instant>,
    requested_at: Instant,
    max_age: Duration,
) -> bool {
    completed_at
        .is_some_and(|completed| completed >= requested_at || completed.elapsed() <= max_age)
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
