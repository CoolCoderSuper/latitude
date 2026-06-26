use std::{path::PathBuf, sync::Arc};

use hmac::{Hmac, Mac};
use rand::random;
use reqwest::Client;
use sha2::Sha256;
use tokio::sync::RwLock;

use crate::{
    config::{BootConfig, ConfigError},
    desktop::ManagedDesktopManager,
    device::current_hostname,
    storage::CatalogStore,
    terminal::TerminalSessionManager,
};

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone)]
pub struct AppState {
    inner: Arc<AppStateInner>,
}

struct AppStateInner {
    config_path: PathBuf,
    config: RwLock<BootConfig>,
    catalog: CatalogStore,
    client: Client,
    device_hostname: String,
    public_auth_secret: [u8; 32],
    desktop_manager: Arc<ManagedDesktopManager>,
    terminal_sessions: Arc<TerminalSessionManager>,
}

impl AppState {
    pub fn new(config_path: PathBuf, config: BootConfig, catalog: CatalogStore) -> Self {
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
                terminal_sessions: Arc::new(TerminalSessionManager::default()),
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

    pub fn catalog(&self) -> &CatalogStore {
        &self.inner.catalog
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

    pub async fn replace_config(&self, config: BootConfig) -> Result<(), ConfigError> {
        config.validate()?;
        config.save_to(&self.inner.config_path).await?;
        *self.inner.config.write().await = config;
        Ok(())
    }
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
    if bytes.len() % 2 != 0 {
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
