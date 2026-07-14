use std::{
    collections::HashSet,
    net::{IpAddr, SocketAddr},
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
pub struct BootConfig {
    #[serde(default = "default_public_bind")]
    pub public_bind: String,
    #[serde(default = "default_command_bind")]
    pub command_bind: String,
    #[serde(default = "default_public_password")]
    pub public_password: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_dir: Option<PathBuf>,
    #[serde(default)]
    pub desktop: DesktopConfig,
    #[serde(default)]
    pub t3code: T3CodeConfig,
}

#[derive(Clone, Debug, Default)]
pub struct CatalogSeed {
    pub share_links: Vec<DeploymentShareConfig>,
    pub projects: Vec<SeedProjectConfig>,
}

#[derive(Clone, Debug)]
pub struct LoadedConfig {
    pub boot: BootConfig,
    pub catalog_seed: CatalogSeed,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigFile {
    #[serde(default = "default_public_bind")]
    public_bind: String,
    #[serde(default = "default_command_bind")]
    command_bind: String,
    #[serde(default = "default_public_password")]
    public_password: String,
    #[serde(default)]
    data_dir: Option<PathBuf>,
    #[serde(default)]
    desktop: DesktopConfig,
    #[serde(default)]
    t3code: T3CodeConfig,
    #[serde(default)]
    share_links: Vec<DeploymentShareConfig>,
    #[serde(default)]
    projects: Vec<SeedProjectConfig>,
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

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct T3CodeConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_t3code_base_url")]
    pub base_url: String,
    #[serde(default = "default_t3code_base_url")]
    pub server_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gateway_bind: Option<String>,
    #[serde(default = "default_t3code_command")]
    pub command: PathBuf,
    #[serde(default)]
    pub command_args: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_dir: Option<PathBuf>,
    #[serde(default)]
    pub start_if_needed: bool,
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
        #[serde(default)]
        format: PageFormat,
        #[serde(default)]
        media_type: Option<String>,
        #[serde(default)]
        title: Option<String>,
    },
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeedProjectConfig {
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub project_dir: PathBuf,
    #[serde(default)]
    pub deployments: Vec<SeedApplicationConfig>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SeedApplicationConfig {
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(flatten)]
    pub target: SeedApplicationTarget,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SeedApplicationTarget {
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
        #[serde(default)]
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
    #[error("config file is not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid config: {0}")]
    Invalid(String),
}

impl Default for BootConfig {
    fn default() -> Self {
        Self {
            public_bind: default_public_bind(),
            command_bind: default_command_bind(),
            public_password: default_public_password(),
            data_dir: None,
            desktop: DesktopConfig::default(),
            t3code: T3CodeConfig::default(),
        }
    }
}

impl Default for ConfigFile {
    fn default() -> Self {
        BootConfig::default().into()
    }
}

impl From<BootConfig> for ConfigFile {
    fn from(config: BootConfig) -> Self {
        Self {
            public_bind: config.public_bind,
            command_bind: config.command_bind,
            public_password: config.public_password,
            data_dir: config.data_dir,
            desktop: config.desktop,
            t3code: config.t3code,
            share_links: Vec::new(),
            projects: Vec::new(),
        }
    }
}

impl ConfigFile {
    fn into_loaded(self) -> LoadedConfig {
        LoadedConfig {
            boot: BootConfig {
                public_bind: self.public_bind,
                command_bind: self.command_bind,
                public_password: self.public_password,
                data_dir: self.data_dir,
                desktop: self.desktop,
                t3code: self.t3code,
            },
            catalog_seed: CatalogSeed {
                share_links: self.share_links,
                projects: self.projects,
            },
        }
    }
}

impl From<&SeedProjectConfig> for ProjectConfig {
    fn from(project: &SeedProjectConfig) -> Self {
        Self {
            name: project.name.clone(),
            enabled: project.enabled,
            project_dir: project.project_dir.clone(),
            deployments: project
                .deployments
                .iter()
                .map(ApplicationConfig::from)
                .collect(),
        }
    }
}

impl From<&SeedApplicationConfig> for ApplicationConfig {
    fn from(app: &SeedApplicationConfig) -> Self {
        let target = match &app.target {
            SeedApplicationTarget::ReverseProxy {
                upstream,
                strip_prefix,
            } => ApplicationTarget::ReverseProxy {
                upstream: upstream.clone(),
                strip_prefix: *strip_prefix,
            },
            SeedApplicationTarget::Static {
                root,
                index_file,
                spa_fallback,
            } => ApplicationTarget::Static {
                root: root.clone(),
                index_file: index_file.clone(),
                spa_fallback: *spa_fallback,
            },
            SeedApplicationTarget::Page {
                format,
                media_type,
                title,
                ..
            } => ApplicationTarget::Page {
                format: *format,
                media_type: media_type.clone(),
                title: title.clone(),
            },
        };

        Self {
            name: app.name.clone(),
            enabled: app.enabled,
            target,
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

impl Default for T3CodeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: default_t3code_base_url(),
            server_url: default_t3code_base_url(),
            gateway_bind: None,
            command: default_t3code_command(),
            command_args: Vec::new(),
            base_dir: None,
            start_if_needed: false,
        }
    }
}

impl LoadedConfig {
    pub async fn load_or_default(path: &Path) -> Result<Self, ConfigError> {
        match fs::read(path).await {
            Ok(bytes) => Ok(serde_json::from_slice::<ConfigFile>(&bytes)?.into_loaded()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(ConfigFile::default().into_loaded())
            }
            Err(error) => Err(error.into()),
        }
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        self.boot.validate()?;
        self.catalog_seed.validate()
    }
}

impl CatalogSeed {
    pub fn is_empty(&self) -> bool {
        self.projects.is_empty() && self.share_links.is_empty()
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        validate_catalog(&self.projects, &self.share_links)
    }
}

impl BootConfig {
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

    pub fn resolved_data_dir(&self, config_path: &Path) -> Result<PathBuf, ConfigError> {
        let config_path = absolute_path(config_path)?;
        let base = config_path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));

        Ok(match &self.data_dir {
            Some(path) if path.is_absolute() => path.clone(),
            Some(path) => base.join(path),
            None => base.join("latitude-data"),
        })
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
        self.t3code.validate()?;

        if self
            .data_dir
            .as_deref()
            .is_some_and(|path| path.as_os_str().is_empty())
        {
            return Err(ConfigError::Invalid(
                "data_dir must not be empty when configured".to_string(),
            ));
        }

        Ok(())
    }
}

fn validate_catalog(
    projects: &[SeedProjectConfig],
    share_links: &[DeploymentShareConfig],
) -> Result<(), ConfigError> {
    let mut seen_share_tokens = HashSet::new();
    for share in share_links {
        share.validate()?;
        if !seen_share_tokens.insert(share.token.clone()) {
            return Err(ConfigError::Invalid(format!(
                "duplicate share link token '{}'",
                share.token
            )));
        }
    }

    let mut seen_names = HashSet::new();
    for project in projects {
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

impl T3CodeConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if !self.enabled {
            return Ok(());
        }

        if self.base_url == "auto" {
            if self.gateway_bind.is_none() {
                return Err(ConfigError::Invalid(
                    "t3code base_url may be 'auto' only when gateway_bind is configured"
                        .to_string(),
                ));
            }
        } else {
            let base_url = Url::parse(&self.base_url).map_err(|error| {
                ConfigError::Invalid(format!("t3code base_url is not a valid URL: {error}"))
            })?;
            if !matches!(base_url.scheme(), "http" | "https") || base_url.host_str().is_none() {
                return Err(ConfigError::Invalid(
                    "t3code base_url must be 'auto' or an absolute http or https URL".to_string(),
                ));
            }
        }
        if let Some(gateway_bind) = &self.gateway_bind
            && gateway_bind.parse::<std::net::SocketAddr>().is_err()
        {
            return Err(ConfigError::Invalid(
                "t3code gateway_bind must be a socket address such as 0.0.0.0:5598".to_string(),
            ));
        }
        let server_url = Url::parse(&self.server_url).map_err(|error| {
            ConfigError::Invalid(format!("t3code server_url is not a valid URL: {error}"))
        })?;
        if server_url.scheme() != "http" || server_url.host_str().is_none() {
            return Err(ConfigError::Invalid(
                "t3code server_url must be an absolute http URL".to_string(),
            ));
        }
        if self.start_if_needed && server_url.port_or_known_default().is_none() {
            return Err(ConfigError::Invalid(
                "t3code server_url must include a usable port when start_if_needed is enabled"
                    .to_string(),
            ));
        }
        if self.start_if_needed
            && !server_url.host_str().is_some_and(|host| {
                host.eq_ignore_ascii_case("localhost")
                    || host
                        .parse::<IpAddr>()
                        .is_ok_and(|address| address.is_loopback())
            })
        {
            return Err(ConfigError::Invalid(
                "t3code server_url must use a loopback host when start_if_needed is enabled"
                    .to_string(),
            ));
        }
        if self.command.as_os_str().is_empty() {
            return Err(ConfigError::Invalid(
                "t3code command must not be empty when the integration is enabled".to_string(),
            ));
        }
        if self
            .base_dir
            .as_deref()
            .is_some_and(|path| path.as_os_str().is_empty())
        {
            return Err(ConfigError::Invalid(
                "t3code base_dir must not be empty when configured".to_string(),
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
                validate_upstream(&self.name, upstream)?;
            }
            ApplicationTarget::Static { index_file, .. } => {
                validate_static_index_file(&self.name, index_file)?;
            }
            ApplicationTarget::Page {
                format,
                media_type,
                title,
            } => {
                validate_page_metadata(
                    &self.name,
                    *format,
                    media_type.as_deref(),
                    title.as_deref(),
                )?;
            }
        }

        Ok(())
    }
}

fn validate_upstream(name: &str, upstream: &str) -> Result<(), ConfigError> {
    let url = Url::parse(upstream).map_err(|error| {
        ConfigError::Invalid(format!("application '{name}' upstream is invalid: {error}"))
    })?;

    if url.scheme() != "http" && url.scheme() != "https" {
        return Err(ConfigError::Invalid(format!(
            "application '{name}' upstream must use http or https"
        )));
    }

    Ok(())
}

fn validate_static_index_file(name: &str, index_file: &str) -> Result<(), ConfigError> {
    if index_file.contains('/') || index_file.contains('\\') || index_file.is_empty() {
        return Err(ConfigError::Invalid(format!(
            "application '{name}' index_file must be a single file name"
        )));
    }

    Ok(())
}

fn validate_page_metadata(
    name: &str,
    format: PageFormat,
    media_type: Option<&str>,
    title: Option<&str>,
) -> Result<(), ConfigError> {
    if format == PageFormat::Binary {
        let Some(media_type) = media_type else {
            return Err(ConfigError::Invalid(format!(
                "application '{name}' binary page content must include media_type"
            )));
        };
        if !is_binary_document_media_type(media_type) {
            return Err(ConfigError::Invalid(format!(
                "application '{name}' binary page media_type must be an image/* or video/* type"
            )));
        }
    } else if media_type.is_some() {
        return Err(ConfigError::Invalid(format!(
            "application '{name}' page media_type is only supported for binary content"
        )));
    }

    if let Some(title) = title
        && title.chars().count() > MAX_PAGE_TITLE_CHARS
    {
        return Err(ConfigError::Invalid(format!(
            "application '{name}' page title must be at most {MAX_PAGE_TITLE_CHARS} characters"
        )));
    }

    Ok(())
}

fn validate_page_content(name: &str, content: &str, format: PageFormat) -> Result<(), ConfigError> {
    if format == PageFormat::Binary {
        let bytes = decode_page_binary_content(content).map_err(|error| {
            ConfigError::Invalid(format!(
                "application '{name}' binary page content must be base64: {error}"
            ))
        })?;
        if bytes.len() > MAX_PAGE_BINARY_CONTENT_BYTES {
            return Err(ConfigError::Invalid(format!(
                "application '{name}' binary page content must be at most {MAX_PAGE_BINARY_CONTENT_BYTES} bytes"
            )));
        }
    } else if content.len() > MAX_PAGE_CONTENT_BYTES {
        return Err(ConfigError::Invalid(format!(
            "application '{name}' page content must be at most {MAX_PAGE_CONTENT_BYTES} bytes"
        )));
    }

    Ok(())
}

impl SeedProjectConfig {
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

impl SeedApplicationConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if !is_valid_url_segment(&self.name) {
            return Err(ConfigError::Invalid(format!(
                "application name '{}' must contain only ASCII letters, digits, '-' or '_'",
                self.name
            )));
        }

        match &self.target {
            SeedApplicationTarget::ReverseProxy { upstream, .. } => {
                validate_upstream(&self.name, upstream)?;
            }
            SeedApplicationTarget::Static { index_file, .. } => {
                validate_static_index_file(&self.name, index_file)?;
            }
            SeedApplicationTarget::Page {
                content,
                format,
                media_type,
                title,
            } => {
                validate_page_metadata(
                    &self.name,
                    *format,
                    media_type.as_deref(),
                    title.as_deref(),
                )?;
                validate_page_content(&self.name, content, *format)?;
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

fn absolute_path(path: &Path) -> Result<PathBuf, ConfigError> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
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

fn default_t3code_base_url() -> String {
    "http://127.0.0.1:3773".to_string()
}

fn default_t3code_command() -> PathBuf {
    PathBuf::from("t3")
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
        let config = BootConfig {
            command_bind: "0.0.0.0:7600".to_string(),
            ..BootConfig::default()
        };

        assert!(config.validate().is_err());
    }

    #[test]
    fn defaults_public_password_to_test() {
        assert_eq!(BootConfig::default().public_password, "test");
    }

    #[test]
    fn resolves_default_data_dir_next_to_config() {
        let config_path = std::env::temp_dir()
            .join("latitude-config-a")
            .join("latitude.json");
        let data_dir = BootConfig::default()
            .resolved_data_dir(&config_path)
            .unwrap();

        assert_eq!(
            data_dir,
            config_path.parent().unwrap().join("latitude-data")
        );
    }

    #[test]
    fn resolves_relative_data_dir_next_to_config() {
        let config_path = std::env::temp_dir()
            .join("latitude-config-b")
            .join("latitude.json");
        let config = BootConfig {
            data_dir: Some(PathBuf::from("catalog")),
            ..BootConfig::default()
        };

        assert_eq!(
            config.resolved_data_dir(&config_path).unwrap(),
            config_path.parent().unwrap().join("catalog")
        );
    }

    #[test]
    fn preserves_absolute_data_dir() {
        let data_dir = std::env::temp_dir().join("latitude-absolute-data");
        let config = BootConfig {
            data_dir: Some(data_dir.clone()),
            ..BootConfig::default()
        };

        assert_eq!(
            config
                .resolved_data_dir(&PathBuf::from("latitude.json"))
                .unwrap(),
            data_dir
        );
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
        let config = BootConfig {
            desktop: DesktopConfig {
                enabled: true,
                vnc_host: "192.168.1.25".to_string(),
                ..DesktopConfig::default()
            },
            ..BootConfig::default()
        };

        assert!(config.validate().is_err());
    }

    #[test]
    fn accepts_non_loopback_desktop_host_when_explicitly_allowed() {
        let config = BootConfig {
            desktop: DesktopConfig {
                enabled: true,
                vnc_host: "192.168.1.25".to_string(),
                allow_non_loopback: true,
                ..DesktopConfig::default()
            },
            ..BootConfig::default()
        };

        assert!(config.validate().is_ok());
    }

    #[test]
    fn accepts_automatic_t3code_gateway_url() {
        let config = BootConfig {
            t3code: T3CodeConfig {
                enabled: true,
                base_url: "auto".to_string(),
                gateway_bind: Some("0.0.0.0:5598".to_string()),
                ..T3CodeConfig::default()
            },
            ..BootConfig::default()
        };

        assert!(config.validate().is_ok());
    }

    #[test]
    fn rejects_automatic_t3code_url_without_gateway() {
        let config = BootConfig {
            t3code: T3CodeConfig {
                enabled: true,
                base_url: "auto".to_string(),
                ..T3CodeConfig::default()
            },
            ..BootConfig::default()
        };

        assert!(config.validate().is_err());
    }

    #[test]
    fn managed_desktop_skips_external_vnc_target_validation() {
        let config = BootConfig {
            desktop: DesktopConfig {
                enabled: true,
                mode: DesktopMode::Managed,
                vnc_host: String::new(),
                vnc_port: 0,
                ..DesktopConfig::default()
            },
            ..BootConfig::default()
        };

        assert!(config.validate().is_ok());
    }

    #[test]
    fn rejects_empty_public_password() {
        let config = BootConfig {
            public_password: String::new(),
            ..BootConfig::default()
        };

        assert!(config.validate().is_err());
    }

    #[test]
    fn rejects_unknown_top_level_applications_config() {
        let error = serde_json::from_str::<ConfigFile>(r#"{"applications":[]}"#).unwrap_err();

        assert!(error.to_string().contains("unknown field"));
    }

    #[test]
    fn requires_project_dir_for_projects() {
        let error = serde_json::from_str::<ConfigFile>(
            r#"{"projects":[{"name":"demo","deployments":[]}]}"#,
        )
        .unwrap_err();

        assert!(error.to_string().contains("project_dir"));
    }

    #[test]
    fn rejects_unknown_project_applications_field() {
        let error = serde_json::from_str::<ConfigFile>(
            r#"{"projects":[{"name":"demo","project_dir":".","applications":[]}]}"#,
        )
        .unwrap_err();

        assert!(error.to_string().contains("unknown field"));
    }

    #[test]
    fn rejects_duplicate_project_names() {
        let seed = CatalogSeed {
            projects: vec![
                SeedProjectConfig {
                    name: "demo".to_string(),
                    enabled: true,
                    project_dir: PathBuf::from("."),
                    deployments: Vec::new(),
                },
                SeedProjectConfig {
                    name: "demo".to_string(),
                    enabled: true,
                    project_dir: PathBuf::from("./other"),
                    deployments: Vec::new(),
                },
            ],
            ..CatalogSeed::default()
        };

        assert!(seed.validate().is_err());
    }

    #[test]
    fn accepts_deployment_share_links() {
        let seed = CatalogSeed {
            share_links: vec![DeploymentShareConfig {
                token: "abc123".to_string(),
                project: "demo".to_string(),
                deployment: "site".to_string(),
                password: Some("secret".to_string()),
                expires_at: Some(4_102_444_800),
            }],
            ..CatalogSeed::default()
        };

        assert!(seed.validate().is_ok());
    }

    #[test]
    fn rejects_duplicate_share_link_tokens() {
        let seed = CatalogSeed {
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
            ..CatalogSeed::default()
        };

        assert!(seed.validate().is_err());
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
        let seed = CatalogSeed {
            projects: vec![SeedProjectConfig {
                name: "demo".to_string(),
                enabled: true,
                project_dir: PathBuf::from("."),
                deployments: vec![
                    SeedApplicationConfig {
                        name: "site".to_string(),
                        enabled: true,
                        target: SeedApplicationTarget::ReverseProxy {
                            upstream: "http://127.0.0.1:3000".to_string(),
                            strip_prefix: true,
                        },
                    },
                    SeedApplicationConfig {
                        name: "site".to_string(),
                        enabled: true,
                        target: SeedApplicationTarget::Static {
                            root: PathBuf::from("public"),
                            index_file: "index.html".to_string(),
                            spa_fallback: false,
                        },
                    },
                ],
            }],
            ..CatalogSeed::default()
        };

        assert!(seed.validate().is_err());
    }

    #[test]
    fn accepts_page_application_inside_project() {
        let seed = CatalogSeed {
            projects: vec![SeedProjectConfig {
                name: "demo".to_string(),
                enabled: true,
                project_dir: PathBuf::from("."),
                deployments: vec![SeedApplicationConfig {
                    name: "agent-note".to_string(),
                    enabled: true,
                    target: SeedApplicationTarget::Page {
                        content: "# Agent Note".to_string(),
                        format: PageFormat::Markdown,
                        media_type: None,
                        title: Some("Agent Note".to_string()),
                    },
                }],
            }],
            ..CatalogSeed::default()
        };

        assert!(seed.validate().is_ok());
    }

    #[test]
    fn rejects_oversized_page_content() {
        let seed = CatalogSeed {
            projects: vec![SeedProjectConfig {
                name: "demo".to_string(),
                enabled: true,
                project_dir: PathBuf::from("."),
                deployments: vec![SeedApplicationConfig {
                    name: "agent-note".to_string(),
                    enabled: true,
                    target: SeedApplicationTarget::Page {
                        content: "x".repeat(MAX_PAGE_CONTENT_BYTES + 1),
                        format: PageFormat::Html,
                        media_type: None,
                        title: None,
                    },
                }],
            }],
            ..CatalogSeed::default()
        };

        assert!(seed.validate().is_err());
    }

    #[test]
    fn accepts_binary_image_page_application() {
        let seed = CatalogSeed {
            projects: vec![SeedProjectConfig {
                name: "demo".to_string(),
                enabled: true,
                project_dir: PathBuf::from("."),
                deployments: vec![SeedApplicationConfig {
                    name: "snapshot".to_string(),
                    enabled: true,
                    target: SeedApplicationTarget::Page {
                        content: encode_page_binary_content(b"png bytes"),
                        format: PageFormat::Binary,
                        media_type: Some("image/png".to_string()),
                        title: Some("Snapshot".to_string()),
                    },
                }],
            }],
            ..CatalogSeed::default()
        };

        assert!(seed.validate().is_ok());
    }

    #[test]
    fn rejects_non_media_binary_page_application() {
        let seed = CatalogSeed {
            projects: vec![SeedProjectConfig {
                name: "demo".to_string(),
                enabled: true,
                project_dir: PathBuf::from("."),
                deployments: vec![SeedApplicationConfig {
                    name: "asset".to_string(),
                    enabled: true,
                    target: SeedApplicationTarget::Page {
                        content: encode_page_binary_content(b"pdf bytes"),
                        format: PageFormat::Binary,
                        media_type: Some("application/pdf".to_string()),
                        title: None,
                    },
                }],
            }],
            ..CatalogSeed::default()
        };

        assert!(seed.validate().is_err());
    }
}
