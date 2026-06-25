use std::{
    collections::HashSet,
    net::SocketAddr,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::fs;
use url::Url;

pub const MAX_PAGE_CONTENT_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_PAGE_BINARY_CONTENT_BYTES: usize = 25 * 1024 * 1024;
pub const MAX_PAGE_TITLE_CHARS: usize = 160;
pub const DEFAULT_PUBLIC_PASSWORD: &str = "test";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LatitudeConfig {
    #[serde(default = "default_public_bind")]
    pub public_bind: String,
    #[serde(default = "default_command_bind")]
    pub command_bind: String,
    #[serde(default = "default_public_password")]
    pub public_password: String,
    #[serde(default)]
    pub desktop: DesktopConfig,
    #[serde(default)]
    pub share_links: Vec<DeploymentShareConfig>,
    #[serde(default)]
    pub projects: Vec<ProjectConfig>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_desktop_label")]
    pub label: String,
    #[serde(default)]
    pub mode: DesktopMode,
    #[serde(default)]
    pub managed_provider: ManagedDesktopProvider,
    #[serde(default = "default_desktop_managed_executable")]
    pub managed_executable: PathBuf,
    #[serde(default = "default_desktop_vnc_host")]
    pub vnc_host: String,
    #[serde(default = "default_desktop_vnc_port")]
    pub vnc_port: u16,
    #[serde(default = "default_true")]
    pub view_only: bool,
    #[serde(default)]
    pub allow_non_loopback: bool,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DesktopMode {
    #[default]
    External,
    Managed,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ManagedDesktopProvider {
    #[default]
    UltraVnc,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectConfig {
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub project_dir: PathBuf,
    #[serde(default)]
    pub deployments: Vec<ApplicationConfig>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApplicationConfig {
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(flatten)]
    pub target: ApplicationTarget,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeploymentShareConfig {
    pub token: String,
    pub project: String,
    pub deployment: String,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub expires_at: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ApplicationTarget {
    ReverseProxy {
        upstream: String,
        #[serde(default = "default_true")]
        strip_prefix: bool,
    },
    Static {
        root: PathBuf,
        #[serde(default = "default_index_file")]
        index_file: String,
        #[serde(default)]
        spa_fallback: bool,
    },
    Page {
        content: String,
        #[serde(default)]
        format: PageFormat,
        #[serde(default)]
        media_type: Option<String>,
        #[serde(default)]
        title: Option<String>,
    },
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PageFormat {
    #[default]
    Html,
    Markdown,
    Binary,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read or write config: {0}")]
    Io(#[from] std::io::Error),
    #[error("config JSON is invalid: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid config: {0}")]
    Invalid(String),
}

impl Default for LatitudeConfig {
    fn default() -> Self {
        Self {
            public_bind: default_public_bind(),
            command_bind: default_command_bind(),
            public_password: default_public_password(),
            desktop: DesktopConfig::default(),
            share_links: Vec::new(),
            projects: Vec::new(),
        }
    }
}

impl Default for DesktopConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            label: default_desktop_label(),
            mode: DesktopMode::default(),
            managed_provider: ManagedDesktopProvider::default(),
            managed_executable: default_desktop_managed_executable(),
            vnc_host: default_desktop_vnc_host(),
            vnc_port: default_desktop_vnc_port(),
            view_only: true,
            allow_non_loopback: false,
        }
    }
}

impl LatitudeConfig {
    pub async fn load_or_default(path: &Path) -> Result<Self, ConfigError> {
        match fs::read(path).await {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error.into()),
        }
    }

    pub async fn save_to(&self, path: &Path) -> Result<(), ConfigError> {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent).await?;
        }

        let mut bytes = serde_json::to_vec_pretty(self)?;
        bytes.push(b'\n');
        fs::write(path, bytes).await?;
        Ok(())
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        self.public_bind
            .parse::<SocketAddr>()
            .map_err(|error| ConfigError::Invalid(format!("public_bind is not valid: {error}")))?;

        let command_bind = self
            .command_bind
            .parse::<SocketAddr>()
            .map_err(|error| ConfigError::Invalid(format!("command_bind is not valid: {error}")))?;

        if !command_bind.ip().is_loopback() {
            return Err(ConfigError::Invalid(
                "command_bind must use a loopback address because the command API is unauthenticated"
                    .to_string(),
            ));
        }

        if self.public_password.is_empty() {
            return Err(ConfigError::Invalid(
                "public_password must not be empty".to_string(),
            ));
        }

        self.desktop.validate()?;

        let mut seen_share_tokens = HashSet::new();
        for share in &self.share_links {
            share.validate()?;
            if !seen_share_tokens.insert(share.token.clone()) {
                return Err(ConfigError::Invalid(format!(
                    "duplicate share link token '{}'",
                    share.token
                )));
            }
        }

        let mut seen_names = HashSet::new();
        for project in &self.projects {
            project.validate()?;
            if !seen_names.insert(project.name.clone()) {
                return Err(ConfigError::Invalid(format!(
                    "duplicate project name '{}'",
                    project.name
                )));
            }
        }

        Ok(())
    }
}

impl DesktopConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if !self.enabled {
            return Ok(());
        }

        if self.label.trim().is_empty() {
            return Err(ConfigError::Invalid(
                "desktop label must not be empty when desktop is enabled".to_string(),
            ));
        }

        if self.mode == DesktopMode::Managed {
            if self.managed_executable.as_os_str().is_empty() {
                return Err(ConfigError::Invalid(
                    "desktop managed_executable must not be empty when managed desktop is enabled"
                        .to_string(),
                ));
            }
            return Ok(());
        }

        if self.vnc_host.trim().is_empty() {
            return Err(ConfigError::Invalid(
                "desktop vnc_host must not be empty when desktop is enabled".to_string(),
            ));
        }

        if self.vnc_port == 0 {
            return Err(ConfigError::Invalid(
                "desktop vnc_port must be between 1 and 65535".to_string(),
            ));
        }

        if !self.allow_non_loopback && !is_loopback_host(&self.vnc_host) {
            return Err(ConfigError::Invalid(
                "desktop vnc_host must be loopback unless allow_non_loopback is true".to_string(),
            ));
        }

        Ok(())
    }
}

impl DeploymentShareConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if !is_valid_url_segment(&self.token) {
            return Err(ConfigError::Invalid(format!(
                "share link token '{}' must contain only ASCII letters, digits, '-' or '_'",
                self.token
            )));
        }

        if !is_valid_url_segment(&self.project) {
            return Err(ConfigError::Invalid(format!(
                "share link project '{}' must contain only ASCII letters, digits, '-' or '_'",
                self.project
            )));
        }

        if !is_valid_url_segment(&self.deployment) {
            return Err(ConfigError::Invalid(format!(
                "share link deployment '{}' must contain only ASCII letters, digits, '-' or '_'",
                self.deployment
            )));
        }

        if self
            .password
            .as_deref()
            .is_some_and(|password| password.is_empty())
        {
            return Err(ConfigError::Invalid(format!(
                "share link '{}' password must not be empty",
                self.token
            )));
        }

        if self.expires_at == Some(0) {
            return Err(ConfigError::Invalid(format!(
                "share link '{}' expires_at must be a Unix timestamp greater than 0",
                self.token
            )));
        }

        Ok(())
    }

    pub fn is_expired(&self, now: u64) -> bool {
        self.expires_at.is_some_and(|expires_at| expires_at <= now)
    }
}

impl ProjectConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if !is_valid_url_segment(&self.name) {
            return Err(ConfigError::Invalid(format!(
                "project name '{}' must contain only ASCII letters, digits, '-' or '_'",
                self.name
            )));
        }

        if self.project_dir.as_os_str().is_empty() {
            return Err(ConfigError::Invalid(format!(
                "project '{}' project_dir must not be empty",
                self.name
            )));
        }

        let mut seen_names = HashSet::new();
        for app in &self.deployments {
            app.validate()?;
            if !seen_names.insert(app.name.clone()) {
                return Err(ConfigError::Invalid(format!(
                    "project '{}' has duplicate deployment name '{}'",
                    self.name, app.name
                )));
            }
        }

        Ok(())
    }
}

impl ApplicationConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if !is_valid_url_segment(&self.name) {
            return Err(ConfigError::Invalid(format!(
                "application name '{}' must contain only ASCII letters, digits, '-' or '_'",
                self.name
            )));
        }

        match &self.target {
            ApplicationTarget::ReverseProxy { upstream, .. } => {
                let url = Url::parse(upstream).map_err(|error| {
                    ConfigError::Invalid(format!(
                        "application '{}' upstream is invalid: {error}",
                        self.name
                    ))
                })?;

                if url.scheme() != "http" && url.scheme() != "https" {
                    return Err(ConfigError::Invalid(format!(
                        "application '{}' upstream must use http or https",
                        self.name
                    )));
                }
            }
            ApplicationTarget::Static { index_file, .. } => {
                if index_file.contains('/') || index_file.contains('\\') || index_file.is_empty() {
                    return Err(ConfigError::Invalid(format!(
                        "application '{}' index_file must be a single file name",
                        self.name
                    )));
                }
            }
            ApplicationTarget::Page {
                content,
                format,
                media_type,
                title,
            } => {
                if *format == PageFormat::Binary {
                    let Some(media_type) = media_type.as_deref() else {
                        return Err(ConfigError::Invalid(format!(
                            "application '{}' binary page content must include media_type",
                            self.name
                        )));
                    };
                    if !is_binary_document_media_type(media_type) {
                        return Err(ConfigError::Invalid(format!(
                            "application '{}' binary page media_type must be an image/* or video/* type",
                            self.name
                        )));
                    }

                    let bytes = decode_page_binary_content(content).map_err(|error| {
                        ConfigError::Invalid(format!(
                            "application '{}' binary page content must be base64: {error}",
                            self.name
                        ))
                    })?;
                    if bytes.len() > MAX_PAGE_BINARY_CONTENT_BYTES {
                        return Err(ConfigError::Invalid(format!(
                            "application '{}' binary page content must be at most {} bytes",
                            self.name, MAX_PAGE_BINARY_CONTENT_BYTES
                        )));
                    }
                } else {
                    if media_type.is_some() {
                        return Err(ConfigError::Invalid(format!(
                            "application '{}' page media_type is only supported for binary content",
                            self.name
                        )));
                    }
                    if content.len() > MAX_PAGE_CONTENT_BYTES {
                        return Err(ConfigError::Invalid(format!(
                            "application '{}' page content must be at most {} bytes",
                            self.name, MAX_PAGE_CONTENT_BYTES
                        )));
                    }
                }

                if let Some(title) = title
                    && title.chars().count() > MAX_PAGE_TITLE_CHARS
                {
                    return Err(ConfigError::Invalid(format!(
                        "application '{}' page title must be at most {} characters",
                        self.name, MAX_PAGE_TITLE_CHARS
                    )));
                }
            }
        }

        Ok(())
    }
}

fn is_valid_url_segment(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
}

fn is_loopback_host(host: &str) -> bool {
    let host = host.trim();
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

pub fn is_binary_document_media_type(media_type: &str) -> bool {
    let media_type = media_type
        .split(';')
        .next()
        .unwrap_or(media_type)
        .trim()
        .to_ascii_lowercase();

    media_type.starts_with("image/") || media_type.starts_with("video/")
}

pub fn encode_page_binary_content(bytes: &[u8]) -> String {
    BASE64_STANDARD.encode(bytes)
}

pub fn decode_page_binary_content(content: &str) -> Result<Vec<u8>, base64::DecodeError> {
    BASE64_STANDARD.decode(content)
}

pub fn current_unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn default_public_bind() -> String {
    "0.0.0.0:8080".to_string()
}

fn default_command_bind() -> String {
    "127.0.0.1:7600".to_string()
}

fn default_public_password() -> String {
    DEFAULT_PUBLIC_PASSWORD.to_string()
}

fn default_desktop_label() -> String {
    "Desktop".to_string()
}

fn default_desktop_managed_executable() -> PathBuf {
    PathBuf::from("tools/ultravnc/winvnc.exe")
}

fn default_desktop_vnc_host() -> String {
    "127.0.0.1".to_string()
}

fn default_desktop_vnc_port() -> u16 {
    5900
}

fn default_index_file() -> String {
    "index.html".to_string()
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_loopback_command_api_bind() {
        let config = LatitudeConfig {
            command_bind: "0.0.0.0:7600".to_string(),
            ..LatitudeConfig::default()
        };

        assert!(config.validate().is_err());
    }

    #[test]
    fn defaults_public_password_to_test() {
        assert_eq!(LatitudeConfig::default().public_password, "test");
    }

    #[test]
    fn desktop_defaults_to_disabled_loopback_view_only() {
        let desktop = DesktopConfig::default();

        assert!(!desktop.enabled);
        assert_eq!(desktop.mode, DesktopMode::External);
        assert_eq!(desktop.managed_provider, ManagedDesktopProvider::UltraVnc);
        assert_eq!(
            desktop.managed_executable,
            PathBuf::from("tools/ultravnc/winvnc.exe")
        );
        assert_eq!(desktop.vnc_host, "127.0.0.1");
        assert_eq!(desktop.vnc_port, 5900);
        assert!(desktop.view_only);
        assert!(!desktop.allow_non_loopback);
    }

    #[test]
    fn rejects_non_loopback_desktop_host_by_default() {
        let config = LatitudeConfig {
            desktop: DesktopConfig {
                enabled: true,
                vnc_host: "192.168.1.25".to_string(),
                ..DesktopConfig::default()
            },
            ..LatitudeConfig::default()
        };

        assert!(config.validate().is_err());
    }

    #[test]
    fn accepts_non_loopback_desktop_host_when_explicitly_allowed() {
        let config = LatitudeConfig {
            desktop: DesktopConfig {
                enabled: true,
                vnc_host: "192.168.1.25".to_string(),
                allow_non_loopback: true,
                ..DesktopConfig::default()
            },
            ..LatitudeConfig::default()
        };

        assert!(config.validate().is_ok());
    }

    #[test]
    fn managed_desktop_skips_external_vnc_target_validation() {
        let config = LatitudeConfig {
            desktop: DesktopConfig {
                enabled: true,
                mode: DesktopMode::Managed,
                vnc_host: String::new(),
                vnc_port: 0,
                ..DesktopConfig::default()
            },
            ..LatitudeConfig::default()
        };

        assert!(config.validate().is_ok());
    }

    #[test]
    fn rejects_empty_public_password() {
        let config = LatitudeConfig {
            public_password: String::new(),
            ..LatitudeConfig::default()
        };

        assert!(config.validate().is_err());
    }

    #[test]
    fn rejects_legacy_top_level_applications_config() {
        let error = serde_json::from_str::<LatitudeConfig>(r#"{"applications":[]}"#).unwrap_err();

        assert!(error.to_string().contains("unknown field"));
    }

    #[test]
    fn requires_project_dir_for_projects() {
        let error = serde_json::from_str::<LatitudeConfig>(
            r#"{"projects":[{"name":"demo","deployments":[]}]}"#,
        )
        .unwrap_err();

        assert!(error.to_string().contains("project_dir"));
    }

    #[test]
    fn rejects_legacy_project_applications_field() {
        let error = serde_json::from_str::<LatitudeConfig>(
            r#"{"projects":[{"name":"demo","project_dir":".","applications":[]}]}"#,
        )
        .unwrap_err();

        assert!(error.to_string().contains("unknown field"));
    }

    #[test]
    fn rejects_duplicate_project_names() {
        let config = LatitudeConfig {
            projects: vec![
                ProjectConfig {
                    name: "demo".to_string(),
                    enabled: true,
                    project_dir: PathBuf::from("."),
                    deployments: Vec::new(),
                },
                ProjectConfig {
                    name: "demo".to_string(),
                    enabled: true,
                    project_dir: PathBuf::from("./other"),
                    deployments: Vec::new(),
                },
            ],
            ..LatitudeConfig::default()
        };

        assert!(config.validate().is_err());
    }

    #[test]
    fn accepts_deployment_share_links() {
        let config = LatitudeConfig {
            share_links: vec![DeploymentShareConfig {
                token: "abc123".to_string(),
                project: "demo".to_string(),
                deployment: "site".to_string(),
                password: Some("secret".to_string()),
                expires_at: Some(4_102_444_800),
            }],
            ..LatitudeConfig::default()
        };

        assert!(config.validate().is_ok());
    }

    #[test]
    fn rejects_duplicate_share_link_tokens() {
        let config = LatitudeConfig {
            share_links: vec![
                DeploymentShareConfig {
                    token: "abc123".to_string(),
                    project: "demo".to_string(),
                    deployment: "site".to_string(),
                    password: None,
                    expires_at: None,
                },
                DeploymentShareConfig {
                    token: "abc123".to_string(),
                    project: "demo".to_string(),
                    deployment: "other".to_string(),
                    password: None,
                    expires_at: None,
                },
            ],
            ..LatitudeConfig::default()
        };

        assert!(config.validate().is_err());
    }

    #[test]
    fn share_link_expiry_uses_unix_seconds() {
        let share = DeploymentShareConfig {
            token: "abc123".to_string(),
            project: "demo".to_string(),
            deployment: "site".to_string(),
            password: None,
            expires_at: Some(10),
        };

        assert!(!share.is_expired(9));
        assert!(share.is_expired(10));
    }

    #[test]
    fn rejects_duplicate_app_names_within_project() {
        let config = LatitudeConfig {
            projects: vec![ProjectConfig {
                name: "demo".to_string(),
                enabled: true,
                project_dir: PathBuf::from("."),
                deployments: vec![
                    ApplicationConfig {
                        name: "site".to_string(),
                        enabled: true,
                        target: ApplicationTarget::ReverseProxy {
                            upstream: "http://127.0.0.1:3000".to_string(),
                            strip_prefix: true,
                        },
                    },
                    ApplicationConfig {
                        name: "site".to_string(),
                        enabled: true,
                        target: ApplicationTarget::Static {
                            root: PathBuf::from("public"),
                            index_file: "index.html".to_string(),
                            spa_fallback: false,
                        },
                    },
                ],
            }],
            ..LatitudeConfig::default()
        };

        assert!(config.validate().is_err());
    }

    #[test]
    fn accepts_page_application_inside_project() {
        let config = LatitudeConfig {
            projects: vec![ProjectConfig {
                name: "demo".to_string(),
                enabled: true,
                project_dir: PathBuf::from("."),
                deployments: vec![ApplicationConfig {
                    name: "agent-note".to_string(),
                    enabled: true,
                    target: ApplicationTarget::Page {
                        content: "# Agent Note".to_string(),
                        format: PageFormat::Markdown,
                        media_type: None,
                        title: Some("Agent Note".to_string()),
                    },
                }],
            }],
            ..LatitudeConfig::default()
        };

        assert!(config.validate().is_ok());
    }

    #[test]
    fn rejects_oversized_page_content() {
        let config = LatitudeConfig {
            projects: vec![ProjectConfig {
                name: "demo".to_string(),
                enabled: true,
                project_dir: PathBuf::from("."),
                deployments: vec![ApplicationConfig {
                    name: "agent-note".to_string(),
                    enabled: true,
                    target: ApplicationTarget::Page {
                        content: "x".repeat(MAX_PAGE_CONTENT_BYTES + 1),
                        format: PageFormat::Html,
                        media_type: None,
                        title: None,
                    },
                }],
            }],
            ..LatitudeConfig::default()
        };

        assert!(config.validate().is_err());
    }

    #[test]
    fn accepts_binary_image_page_application() {
        let config = LatitudeConfig {
            projects: vec![ProjectConfig {
                name: "demo".to_string(),
                enabled: true,
                project_dir: PathBuf::from("."),
                deployments: vec![ApplicationConfig {
                    name: "snapshot".to_string(),
                    enabled: true,
                    target: ApplicationTarget::Page {
                        content: encode_page_binary_content(b"png bytes"),
                        format: PageFormat::Binary,
                        media_type: Some("image/png".to_string()),
                        title: Some("Snapshot".to_string()),
                    },
                }],
            }],
            ..LatitudeConfig::default()
        };

        assert!(config.validate().is_ok());
    }

    #[test]
    fn rejects_non_media_binary_page_application() {
        let config = LatitudeConfig {
            projects: vec![ProjectConfig {
                name: "demo".to_string(),
                enabled: true,
                project_dir: PathBuf::from("."),
                deployments: vec![ApplicationConfig {
                    name: "asset".to_string(),
                    enabled: true,
                    target: ApplicationTarget::Page {
                        content: encode_page_binary_content(b"pdf bytes"),
                        format: PageFormat::Binary,
                        media_type: Some("application/pdf".to_string()),
                        title: None,
                    },
                }],
            }],
            ..LatitudeConfig::default()
        };

        assert!(config.validate().is_err());
    }
}
