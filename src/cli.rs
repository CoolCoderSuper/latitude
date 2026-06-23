use std::{
    io::{self, Read},
    net::{IpAddr, SocketAddr},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow};
use clap::{Args, Parser, Subcommand, ValueEnum};
use reqwest::{Client as HttpClient, Method, Response, StatusCode, Url, header};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;

use crate::config::{
    ApplicationConfig, ApplicationTarget, PageFormat, ProjectConfig, encode_page_binary_content,
    is_binary_document_media_type,
};

const DEFAULT_COMMAND_URL: &str = "http://127.0.0.1:7600";

#[derive(Debug, Parser)]
#[command(
    author,
    version,
    about = "Latitude path-based proxy and static-site gateway"
)]
pub struct Cli {
    #[arg(long, env = "LATITUDE_CONFIG", default_value = "latitude.json")]
    pub config: PathBuf,

    #[arg(long, env = "LATITUDE_PUBLIC_BIND")]
    pub public_bind: Option<String>,

    #[arg(long, env = "LATITUDE_COMMAND_BIND")]
    pub command_bind: Option<String>,

    #[arg(long, env = "LATITUDE_COMMAND_URL", global = true)]
    pub command_url: Option<String>,

    #[command(subcommand)]
    pub command: Option<CliCommand>,
}

#[derive(Debug, Subcommand)]
pub enum CliCommand {
    /// Read command API health.
    Health,
    /// Read or replace Latitude config through the command API.
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    /// Manage projects through the command API.
    Project {
        #[command(subcommand)]
        command: ProjectCommand,
    },
    /// Publish agent output through the command API.
    Publish {
        #[command(subcommand)]
        command: PublishCommand,
    },
    /// Register static or reverse-proxy deployments through the command API.
    Deploy {
        #[command(subcommand)]
        command: DeployCommand,
    },
    /// Inspect or delete deployments through the command API.
    Deployment {
        #[command(subcommand)]
        command: DeploymentCommand,
    },
}

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    /// Print the active Latitude config as JSON.
    Get,
    /// Replace the active Latitude config from a JSON file.
    Put { file: PathBuf },
}

#[derive(Debug, Subcommand)]
pub enum ProjectCommand {
    /// List configured projects.
    List,
    /// Print one configured project as JSON.
    Get { name: String },
    /// Create a project if it does not already exist.
    Ensure(ProjectEnsureArgs),
}

#[derive(Debug, Args)]
pub struct ProjectEnsureArgs {
    pub name: String,
    #[arg(long, value_name = "DIR")]
    pub project_dir: PathBuf,
    #[arg(long)]
    pub disabled: bool,
}

#[derive(Debug, Subcommand)]
pub enum PublishCommand {
    /// Publish a Markdown, HTML, image, or video document.
    Page(PublishPageArgs),
}

#[derive(Debug, Args)]
pub struct PublishPageArgs {
    pub project: String,
    pub name: String,
    #[arg(short, long, value_name = "FILE")]
    pub file: Option<String>,
    #[arg(long)]
    pub title: Option<String>,
    #[arg(long, value_enum, default_value_t = PageInputFormat::Auto)]
    pub format: PageInputFormat,
    #[arg(long, value_name = "DIR")]
    pub project_dir: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum PageInputFormat {
    Auto,
    Html,
    Markdown,
}

#[derive(Debug, Subcommand)]
pub enum DeployCommand {
    /// Register a static file deployment.
    Static(DeployStaticArgs),
    /// Register a reverse proxy deployment.
    Proxy(DeployProxyArgs),
}

#[derive(Debug, Args)]
pub struct DeployStaticArgs {
    pub project: String,
    pub name: String,
    #[arg(long, value_name = "DIR")]
    pub root: PathBuf,
    #[arg(long, default_value = "index.html")]
    pub index_file: String,
    #[arg(long)]
    pub spa_fallback: bool,
    #[arg(long)]
    pub disabled: bool,
    #[arg(long, value_name = "DIR")]
    pub project_dir: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct DeployProxyArgs {
    pub project: String,
    pub name: String,
    #[arg(long)]
    pub upstream: String,
    #[arg(long)]
    pub no_strip_prefix: bool,
    #[arg(long)]
    pub disabled: bool,
    #[arg(long, value_name = "DIR")]
    pub project_dir: Option<PathBuf>,
}

#[derive(Debug, Subcommand)]
pub enum DeploymentCommand {
    /// List deployments in one project.
    List { project: String },
    /// Print one deployment as JSON.
    Get { project: String, name: String },
    /// Delete one deployment.
    Delete { project: String, name: String },
}

#[derive(Debug, Deserialize, Serialize)]
struct HealthResponse {
    status: String,
    public_bind: String,
    command_bind: String,
    project_count: usize,
    deployment_count: usize,
}

#[derive(Debug, Deserialize)]
struct ApiErrorBody {
    error: String,
}

#[derive(Debug, Serialize)]
struct PageJsonPayload<'a> {
    content: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    format: Option<PageFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    media_type: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<&'a str>,
}

#[derive(Debug, Serialize)]
struct DeploymentResult<T> {
    public_url: Option<String>,
    deployment: T,
}

#[derive(Debug, Serialize)]
struct DeleteResult {
    deleted: bool,
}

struct CommandClient {
    http: HttpClient,
    base: Url,
}

impl Cli {
    pub fn command_api_url(&self) -> Result<Url> {
        let url = if let Some(command_url) = &self.command_url {
            command_url.clone()
        } else if let Some(command_bind) = &self.command_bind {
            command_bind_to_url(command_bind)
        } else {
            DEFAULT_COMMAND_URL.to_string()
        };

        Url::parse(&url).with_context(|| format!("command API URL is invalid: {url}"))
    }
}

pub async fn run_command(cli: &Cli, command: &CliCommand) -> Result<()> {
    let client = CommandClient::new(cli.command_api_url()?);

    match command {
        CliCommand::Health => print_json(&client.get_json::<HealthResponse>("/health").await?),
        CliCommand::Config { command } => run_config_command(&client, command).await,
        CliCommand::Project { command } => run_project_command(&client, command).await,
        CliCommand::Publish { command } => run_publish_command(&client, command).await,
        CliCommand::Deploy { command } => run_deploy_command(&client, command).await,
        CliCommand::Deployment { command } => run_deployment_command(&client, command).await,
    }
}

async fn run_config_command(client: &CommandClient, command: &ConfigCommand) -> Result<()> {
    match command {
        ConfigCommand::Get => print_json(&client.get_json::<Value>("/api/config").await?),
        ConfigCommand::Put { file } => {
            let bytes = tokio::fs::read(file)
                .await
                .with_context(|| format!("failed to read config file {}", file.display()))?;
            let value = client
                .send_raw_json::<Value>(Method::PUT, "/api/config", bytes)
                .await?;
            print_json(&value)
        }
    }
}

async fn run_project_command(client: &CommandClient, command: &ProjectCommand) -> Result<()> {
    match command {
        ProjectCommand::List => print_json(&client.get_json::<Value>("/api/projects").await?),
        ProjectCommand::Get { name } => print_json(
            &client
                .get_json::<Value>(&format!("/api/projects/{name}"))
                .await?,
        ),
        ProjectCommand::Ensure(args) => {
            let project =
                ensure_project_exists(client, &args.name, &args.project_dir, !args.disabled)
                    .await?;
            print_json(&project)
        }
    }
}

async fn run_publish_command(client: &CommandClient, command: &PublishCommand) -> Result<()> {
    match command {
        PublishCommand::Page(args) => publish_page(client, args).await,
    }
}

async fn run_deploy_command(client: &CommandClient, command: &DeployCommand) -> Result<()> {
    match command {
        DeployCommand::Static(args) => deploy_static(client, args).await,
        DeployCommand::Proxy(args) => deploy_proxy(client, args).await,
    }
}

async fn run_deployment_command(client: &CommandClient, command: &DeploymentCommand) -> Result<()> {
    match command {
        DeploymentCommand::List { project } => print_json(
            &client
                .get_json::<Value>(&format!("/api/projects/{project}/deployments"))
                .await?,
        ),
        DeploymentCommand::Get { project, name } => print_json(
            &client
                .get_json::<Value>(&format!("/api/projects/{project}/deployments/{name}"))
                .await?,
        ),
        DeploymentCommand::Delete { project, name } => {
            client
                .delete(&format!("/api/projects/{project}/deployments/{name}"))
                .await?;
            print_json(&DeleteResult { deleted: true })
        }
    }
}

async fn publish_page(client: &CommandClient, args: &PublishPageArgs) -> Result<()> {
    maybe_ensure_project(client, &args.project, args.project_dir.as_deref()).await?;

    let input = read_input_bytes(args.file.as_deref())?;
    let content_type = args
        .format
        .content_type()
        .map(str::to_string)
        .unwrap_or_else(|| infer_page_content_type(args.file.as_deref().map(Path::new), &input));
    let media_type = content_type_media_type(&content_type);
    let is_binary_document = media_type
        .as_deref()
        .is_some_and(is_binary_document_media_type);
    let path = format!("/api/projects/{}/pages/{}", args.project, args.name);
    let deployment: ApplicationConfig = if is_binary_document {
        if args.title.is_some() {
            let encoded_content = encode_page_binary_content(&input);
            let payload = PageJsonPayload {
                content: &encoded_content,
                format: Some(PageFormat::Binary),
                media_type: media_type.as_deref(),
                title: args.title.as_deref(),
            };
            client.send_json(Method::PUT, &path, &payload).await?
        } else {
            client
                .send_raw(Method::PUT, &path, &content_type, input)
                .await?
        }
    } else {
        let content = String::from_utf8(input).with_context(|| {
            let source = args.file.as_deref().unwrap_or("stdin");
            format!(
                "page content from {source} must be UTF-8 text unless the file extension maps to an image/* or video/* media type"
            )
        })?;
        if args.title.is_some() {
            let payload = PageJsonPayload {
                content: &content,
                format: args.format.page_format(),
                media_type: None,
                title: args.title.as_deref(),
            };
            client.send_json(Method::PUT, &path, &payload).await?
        } else {
            client
                .send_raw(Method::PUT, &path, &content_type, content)
                .await?
        }
    };
    let public_url = client.public_url(&args.project, &args.name).await;

    print_json(&DeploymentResult {
        public_url,
        deployment,
    })
}

async fn deploy_static(client: &CommandClient, args: &DeployStaticArgs) -> Result<()> {
    maybe_ensure_project(client, &args.project, args.project_dir.as_deref()).await?;

    let app = ApplicationConfig {
        name: args.name.clone(),
        enabled: !args.disabled,
        target: ApplicationTarget::Static {
            root: args.root.clone(),
            index_file: args.index_file.clone(),
            spa_fallback: args.spa_fallback,
        },
    };
    let path = format!("/api/projects/{}/deployments/{}", args.project, args.name);
    let deployment: ApplicationConfig = client.send_json(Method::PUT, &path, &app).await?;
    let public_url = client.public_url(&args.project, &args.name).await;

    print_json(&DeploymentResult {
        public_url,
        deployment,
    })
}

async fn deploy_proxy(client: &CommandClient, args: &DeployProxyArgs) -> Result<()> {
    maybe_ensure_project(client, &args.project, args.project_dir.as_deref()).await?;

    let app = ApplicationConfig {
        name: args.name.clone(),
        enabled: !args.disabled,
        target: ApplicationTarget::ReverseProxy {
            upstream: args.upstream.clone(),
            strip_prefix: !args.no_strip_prefix,
        },
    };
    let path = format!("/api/projects/{}/deployments/{}", args.project, args.name);
    let deployment: ApplicationConfig = client.send_json(Method::PUT, &path, &app).await?;
    let public_url = client.public_url(&args.project, &args.name).await;

    print_json(&DeploymentResult {
        public_url,
        deployment,
    })
}

async fn maybe_ensure_project(
    client: &CommandClient,
    name: &str,
    project_dir: Option<&Path>,
) -> Result<()> {
    if let Some(project_dir) = project_dir {
        ensure_project_exists(client, name, project_dir, true).await?;
    }
    Ok(())
}

async fn ensure_project_exists(
    client: &CommandClient,
    name: &str,
    project_dir: &Path,
    enabled: bool,
) -> Result<ProjectConfig> {
    let path = format!("/api/projects/{name}");
    let response = client.get(&path).await?;

    match response.status() {
        StatusCode::OK => decode_response(response).await,
        StatusCode::NOT_FOUND => {
            let project = ProjectConfig {
                name: name.to_string(),
                enabled,
                project_dir: project_dir.to_path_buf(),
                deployments: Vec::new(),
            };
            client.send_json(Method::PUT, &path, &project).await
        }
        _ => Err(api_error(response).await),
    }
}

impl CommandClient {
    fn new(base: Url) -> Self {
        Self {
            http: HttpClient::new(),
            base,
        }
    }

    async fn get(&self, path: &str) -> Result<Response> {
        self.http
            .get(self.url(path)?)
            .send()
            .await
            .context("failed to call Latitude command API")
    }

    async fn get_json<T>(&self, path: &str) -> Result<T>
    where
        T: DeserializeOwned,
    {
        decode_response(self.get(path).await?).await
    }

    async fn send_json<B, T>(&self, method: Method, path: &str, body: &B) -> Result<T>
    where
        B: Serialize + ?Sized,
        T: DeserializeOwned,
    {
        let body = serde_json::to_vec(body).context("failed to serialize request body as JSON")?;
        let response = self
            .http
            .request(method, self.url(path)?)
            .header(header::CONTENT_TYPE, "application/json")
            .body(body)
            .send()
            .await
            .context("failed to call Latitude command API")?;

        decode_response(response).await
    }

    async fn send_raw_json<T>(&self, method: Method, path: &str, body: Vec<u8>) -> Result<T>
    where
        T: DeserializeOwned,
    {
        self.send_raw(method, path, "application/json", body).await
    }

    async fn send_raw<B, T>(
        &self,
        method: Method,
        path: &str,
        content_type: &str,
        body: B,
    ) -> Result<T>
    where
        B: Into<reqwest::Body>,
        T: DeserializeOwned,
    {
        let response = self
            .http
            .request(method, self.url(path)?)
            .header(header::CONTENT_TYPE, content_type)
            .body(body)
            .send()
            .await
            .context("failed to call Latitude command API")?;

        decode_response(response).await
    }

    async fn delete(&self, path: &str) -> Result<()> {
        let response = self
            .http
            .delete(self.url(path)?)
            .send()
            .await
            .context("failed to call Latitude command API")?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(api_error(response).await)
        }
    }

    async fn public_url(&self, project: &str, deployment: &str) -> Option<String> {
        let health = self.get_json::<HealthResponse>("/health").await.ok()?;
        local_public_url(&health.public_bind, project, deployment)
    }

    fn url(&self, path: &str) -> Result<Url> {
        let mut url = self.base.clone();
        url.set_path(path.trim_start_matches('/'));
        url.set_query(None);
        url.set_fragment(None);
        Ok(url)
    }
}

async fn decode_response<T>(response: Response) -> Result<T>
where
    T: DeserializeOwned,
{
    if !response.status().is_success() {
        return Err(api_error(response).await);
    }

    let status = response.status();
    let text = response
        .text()
        .await
        .context("failed to read Latitude command API response")?;

    serde_json::from_str(&text).with_context(|| {
        format!("Latitude command API returned {status}, but the JSON response could not be parsed")
    })
}

async fn api_error(response: Response) -> anyhow::Error {
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    if let Ok(error) = serde_json::from_str::<ApiErrorBody>(&text) {
        anyhow!("Latitude command API returned {status}: {}", error.error)
    } else if text.trim().is_empty() {
        anyhow!("Latitude command API returned {status}")
    } else {
        anyhow!("Latitude command API returned {status}: {}", text.trim())
    }
}

fn print_json<T>(value: &T) -> Result<()>
where
    T: Serialize,
{
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn read_input_bytes(file: Option<&str>) -> Result<Vec<u8>> {
    match file {
        Some("-") | None => {
            let mut content = Vec::new();
            io::stdin()
                .read_to_end(&mut content)
                .context("failed to read page content from stdin")?;
            Ok(content)
        }
        Some(file) => {
            std::fs::read(file).with_context(|| format!("failed to read page content from {file}"))
        }
    }
}

impl PageInputFormat {
    fn page_format(self) -> Option<PageFormat> {
        match self {
            Self::Auto => None,
            Self::Html => Some(PageFormat::Html),
            Self::Markdown => Some(PageFormat::Markdown),
        }
    }

    fn content_type(self) -> Option<&'static str> {
        match self {
            Self::Auto => None,
            Self::Html => Some("text/html"),
            Self::Markdown => Some("text/markdown"),
        }
    }
}

fn infer_page_content_type(file: Option<&Path>, content: &[u8]) -> String {
    if let Some(extension) = file
        .and_then(Path::extension)
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
    {
        if let Some(media_type) = file
            .and_then(|file| mime_guess::from_path(file).first())
            .map(|mime| mime.essence_str().to_string())
            .filter(|media_type| is_binary_document_media_type(media_type))
        {
            return media_type;
        }
        if matches!(extension.as_str(), "html" | "htm") {
            return "text/html".to_string();
        }
        if matches!(extension.as_str(), "md" | "markdown" | "mdown") {
            return "text/markdown".to_string();
        }
    }

    let Ok(content) = std::str::from_utf8(content) else {
        return "application/octet-stream".to_string();
    };
    let trimmed = content.trim_start().to_ascii_lowercase();
    if trimmed.starts_with("<!doctype html")
        || trimmed.starts_with("<html")
        || trimmed.starts_with("<section")
        || trimmed.starts_with("<article")
    {
        "text/html".to_string()
    } else {
        "text/markdown".to_string()
    }
}

fn content_type_media_type(content_type: &str) -> Option<String> {
    content_type
        .split(';')
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_lowercase)
}

fn command_bind_to_url(command_bind: &str) -> String {
    if command_bind.starts_with("http://") || command_bind.starts_with("https://") {
        command_bind.to_string()
    } else {
        format!("http://{command_bind}")
    }
}

fn local_public_url(public_bind: &str, project: &str, deployment: &str) -> Option<String> {
    let addr = public_bind.parse::<SocketAddr>().ok()?;
    let host = match addr.ip() {
        IpAddr::V4(ip) if ip.is_unspecified() => "127.0.0.1".to_string(),
        IpAddr::V4(ip) => ip.to_string(),
        IpAddr::V6(ip) if ip.is_unspecified() => "[::1]".to_string(),
        IpAddr::V6(ip) => format!("[{ip}]"),
    };

    Some(format!(
        "http://{host}:{}/{project}/{deployment}",
        addr.port()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_bind_can_be_used_as_command_url() {
        assert_eq!(
            command_bind_to_url("127.0.0.1:7601"),
            "http://127.0.0.1:7601"
        );
        assert_eq!(
            command_bind_to_url("http://127.0.0.1:7602"),
            "http://127.0.0.1:7602"
        );
    }

    #[test]
    fn public_url_uses_loopback_for_unspecified_bind() {
        assert_eq!(
            local_public_url("0.0.0.0:8080", "demo", "report").as_deref(),
            Some("http://127.0.0.1:8080/demo/report")
        );
    }

    #[test]
    fn infers_image_page_content_type_from_file_extension() {
        assert_eq!(
            infer_page_content_type(Some(Path::new("snapshot.png")), b"not utf8 \xFF"),
            "image/png"
        );
    }

    #[test]
    fn infers_video_page_content_type_from_file_extension() {
        assert_eq!(
            infer_page_content_type(Some(Path::new("clip.mp4")), b"mp4 bytes"),
            "video/mp4"
        );
    }
}
