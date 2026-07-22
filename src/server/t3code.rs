use std::{ffi::OsString, net::SocketAddr, path::Path, process::Stdio, time::Duration};

use axum::{
    Router,
    body::{Body, to_bytes},
    extract::{Path as AxumPath, State, WebSocketUpgrade, ws::Message as AxumMessage},
    http::{HeaderMap, Request, Response, StatusCode, Uri, header, uri::Authority},
    routing::get,
};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::{process::Command, time::sleep};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Message as TungsteniteMessage, client::IntoClientRequest},
};
use tracing::{error, info, warn};
use url::Url;

use crate::{config::T3CodeConfig, state::AppState, storage::WorktreeRecord};

use super::{
    assets::public_asset,
    auth::{
        public_api_auth_challenge, public_auth_challenge, public_headers_are_authenticated,
        public_request_is_authenticated,
    },
    constants::{AUTH_COOKIE_NAME, LOGIN_PATH},
    paths::{is_hop_by_hop_header, join_upstream_url},
    public::{get_public_login, post_public_login},
    response::{internal_response, json_error, plain_response},
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PairingCredentialOutput {
    pair_url: String,
}

#[derive(Debug, Deserialize)]
struct ProjectRegistrationOutput {
    id: String,
}

pub(super) fn t3code_gateway_router(state: AppState) -> Router {
    Router::new()
        .route("/__latitude/assets/{name}", get(t3code_gateway_asset))
        .route(LOGIN_PATH, get(get_public_login).post(post_public_login))
        .route("/ws", get(t3code_gateway_websocket))
        .fallback(t3code_gateway_http)
        .with_state(state)
}

async fn t3code_gateway_asset(path: AxumPath<String>, headers: HeaderMap) -> Response<Body> {
    public_asset(path, headers).await
}

pub(super) async fn open_t3code(
    State(state): State<AppState>,
    req: Request<Body>,
) -> Response<Body> {
    let config = state.config_snapshot().await;
    if !public_request_is_authenticated(&state, &config, &req) {
        return public_api_auth_challenge();
    }
    if !config.t3code.enabled {
        return plain_response(
            StatusCode::NOT_FOUND,
            "T3 Code integration is not enabled\n",
        );
    }

    let pairing_base_url = match pairing_base_url(&config.t3code, &req) {
        Ok(base_url) => base_url,
        Err(message) => {
            error!(%message, "T3 Code gateway URL could not be determined");
            return plain_response(StatusCode::INTERNAL_SERVER_ERROR, format!("{message}\n"));
        }
    };
    if let Err(message) = ensure_server(&state, &config.t3code).await {
        error!(%message, "T3 Code server startup failed");
        return plain_response(StatusCode::BAD_GATEWAY, format!("{message}\n"));
    }
    let pairing_url = match create_pairing_url(&config.t3code, pairing_base_url, "Latitude").await {
        Ok(url) => url,
        Err(message) => {
            error!(%message, "T3 Code pairing credential failed");
            return plain_response(StatusCode::BAD_GATEWAY, format!("{message}\n"));
        }
    };

    info!("opening T3 Code");
    pairing_redirect(pairing_url)
}

pub(super) async fn open_project_in_t3code(
    State(state): State<AppState>,
    AxumPath(project_name): AxumPath<String>,
    req: Request<Body>,
) -> Response<Body> {
    let config = state.config_snapshot().await;
    if !public_request_is_authenticated(&state, &config, &req) {
        return public_api_auth_challenge();
    }
    if !config.t3code.enabled {
        return plain_response(
            StatusCode::NOT_FOUND,
            "T3 Code integration is not enabled\n",
        );
    }

    let pairing_base_url = match pairing_base_url(&config.t3code, &req) {
        Ok(base_url) => base_url,
        Err(message) => {
            error!(project = %project_name, %message, "T3 Code gateway URL could not be determined");
            return plain_response(StatusCode::INTERNAL_SERVER_ERROR, format!("{message}\n"));
        }
    };

    let project = match state.catalog().get_project(&project_name).await {
        Ok(Some(project)) if project.enabled => project,
        Ok(_) => {
            return plain_response(
                StatusCode::NOT_FOUND,
                format!("Project '{project_name}' was not found\n"),
            );
        }
        Err(error) => {
            error!(%error, project = %project_name, "T3 Code project lookup failed");
            return plain_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Latitude could not load this project\n",
            );
        }
    };

    let worktrees = match state.catalog().list_worktrees().await {
        Ok(worktrees) => worktrees,
        Err(error) => {
            error!(%error, project = %project_name, "T3 Code worktree lookup failed");
            return plain_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Latitude could not resolve this project's worktree\n",
            );
        }
    };
    let (root_project_name, launch_worktree) =
        resolve_t3code_launch_mapping(&project.name, &worktrees);
    let root_project = if root_project_name == project.name {
        project.clone()
    } else {
        match state.catalog().get_project(&root_project_name).await {
            Ok(Some(root_project)) if root_project.enabled => root_project,
            Ok(_) => {
                return plain_response(
                    StatusCode::NOT_FOUND,
                    format!("Root project '{root_project_name}' was not found\n"),
                );
            }
            Err(error) => {
                error!(%error, project = %project_name, root_project = %root_project_name, "T3 Code root project lookup failed");
                return plain_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Latitude could not load this project's repository root\n",
                );
            }
        }
    };

    if let Err(message) = ensure_server(&state, &config.t3code).await {
        error!(project = %project_name, %message, "T3 Code server startup failed");
        return plain_response(StatusCode::BAD_GATEWAY, format!("{message}\n"));
    }

    // Revalidate the cached mapping against the live server on every launch. T3 Code
    // can replace or rebuild its state while Latitude stays running, leaving a cached
    // project id that makes the pairing route wait forever for a project that no
    // longer exists. `project add --if-missing` is idempotent and returns the current
    // live id in both cases.
    let project_args = project_registration_args(
        &config.t3code,
        &root_project.project_dir,
        &root_project.name,
    );
    let output = match run_cli(&config.t3code, project_args).await {
        Ok(output) => output,
        Err(message) => {
            error!(project = %project_name, %message, "T3 Code project registration failed");
            return plain_response(StatusCode::BAD_GATEWAY, format!("{message}\n"));
        }
    };
    let registration: ProjectRegistrationOutput = match serde_json::from_slice(&output) {
        Ok(registration) => registration,
        Err(error) => {
            error!(%error, project = %project_name, "T3 Code project output was invalid");
            return plain_response(
                StatusCode::BAD_GATEWAY,
                "T3 Code returned an invalid project response\n",
            );
        }
    };
    let t3code_project_id = registration.id;

    let mut pairing_url = match create_pairing_url(
        &config.t3code,
        pairing_base_url,
        &format!("Latitude: {}", project.name),
    )
    .await
    {
        Ok(url) => url,
        Err(message) => {
            error!(project = %project_name, %message, "T3 Code pairing credential failed");
            return plain_response(StatusCode::BAD_GATEWAY, format!("{message}\n"));
        }
    };
    let mut fragment =
        url::form_urlencoded::parse(pairing_url.fragment().unwrap_or_default().as_bytes())
            .into_owned()
            .collect::<Vec<_>>();
    fragment.retain(|(name, _)| !matches!(name.as_str(), "project" | "worktree" | "branch"));
    fragment.push(("project".to_string(), t3code_project_id));
    if let Some(worktree) = &launch_worktree {
        fragment.push((
            "worktree".to_string(),
            worktree.worktree_dir.to_string_lossy().into_owned(),
        ));
        if let Some(branch) = &worktree.branch {
            fragment.push(("branch".to_string(), branch.clone()));
        }
    }
    pairing_url.set_fragment(Some(
        &url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(fragment)
            .finish(),
    ));

    info!(project = %project_name, "opening project in T3 Code");
    pairing_redirect(pairing_url)
}

fn resolve_t3code_launch_mapping(
    project_name: &str,
    worktrees: &[WorktreeRecord],
) -> (String, Option<WorktreeRecord>) {
    let Some(selected) = worktrees
        .iter()
        .find(|worktree| worktree.project_name == project_name)
    else {
        return (project_name.to_string(), None);
    };
    let repository_root = selected.common_git_dir.parent();
    let root = worktrees
        .iter()
        .filter(|candidate| candidate.common_git_dir == selected.common_git_dir)
        .find(|candidate| {
            repository_root.is_some_and(|root| same_t3code_path(&candidate.worktree_dir, root))
        })
        .or_else(|| {
            worktrees.iter().find(|candidate| {
                candidate.common_git_dir == selected.common_git_dir && !candidate.discovered
            })
        });

    match root {
        Some(root) => (root.project_name.clone(), Some(selected.clone())),
        None => (project_name.to_string(), Some(selected.clone())),
    }
}

fn same_t3code_path(left: &Path, right: &Path) -> bool {
    if cfg!(windows) {
        left.to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy())
    } else {
        left == right
    }
}

async fn create_pairing_url(
    config: &T3CodeConfig,
    base_url: String,
    label: &str,
) -> Result<Url, String> {
    let mut args = vec![
        OsString::from("auth"),
        OsString::from("pairing"),
        OsString::from("create"),
        OsString::from("--ttl"),
        OsString::from("5m"),
        OsString::from("--label"),
        OsString::from(label),
        OsString::from("--base-url"),
        OsString::from(base_url),
        OsString::from("--json"),
    ];
    append_base_dir(&mut args, config);
    let output = run_cli(config, args).await?;
    let pairing: PairingCredentialOutput = serde_json::from_slice(&output)
        .map_err(|error| format!("T3 Code returned an invalid pairing response: {error}"))?;
    Url::parse(&pairing.pair_url)
        .map_err(|error| format!("T3 Code returned an invalid pairing URL: {error}"))
}

fn pairing_redirect(pairing_url: Url) -> Response<Body> {
    Response::builder()
        .status(StatusCode::SEE_OTHER)
        .header(header::LOCATION, pairing_url.as_str())
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::empty())
        .unwrap_or_else(internal_response)
}

fn pairing_base_url(config: &T3CodeConfig, req: &Request<Body>) -> Result<String, String> {
    if config.base_url != "auto" {
        return Ok(config.base_url.clone());
    }

    let bind = config
        .gateway_bind
        .as_deref()
        .ok_or_else(|| "T3 Code gateway_bind is required when base_url is auto".to_string())?
        .parse::<SocketAddr>()
        .map_err(|error| format!("T3 Code gateway_bind is invalid: {error}"))?;
    let scheme = forwarded_header(req.headers(), "x-forwarded-proto")
        .or_else(|| req.uri().scheme_str())
        .unwrap_or("http");
    if !matches!(scheme, "http" | "https") {
        return Err("T3 Code gateway request used an unsupported URL scheme".to_string());
    }
    let raw_authority = forwarded_header(req.headers(), "x-forwarded-host")
        .or_else(|| {
            req.headers()
                .get(header::HOST)
                .and_then(|value| value.to_str().ok())
        })
        .ok_or_else(|| "T3 Code gateway request did not include a host".to_string())?;
    let authority = raw_authority
        .parse::<Authority>()
        .map_err(|error| format!("T3 Code gateway request host is invalid: {error}"))?;
    let host = authority.host();
    let host = if host.contains(':') {
        format!("[{host}]")
    } else {
        host.to_string()
    };
    Ok(format!("{scheme}://{host}:{}", bind.port()))
}

fn forwarded_header<'a>(headers: &'a HeaderMap, name: &'static str) -> Option<&'a str> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

async fn t3code_gateway_http(State(state): State<AppState>, req: Request<Body>) -> Response<Body> {
    let config = state.config_snapshot().await;
    if !config.t3code.enabled || config.t3code.gateway_bind.is_none() {
        return plain_response(StatusCode::NOT_FOUND, "T3 Code gateway is not enabled\n");
    }
    if !public_request_is_authenticated(&state, &config, &req) {
        return public_auth_challenge(&state, &req, false);
    }

    let target_url = match join_upstream_url(
        &config.t3code.server_url,
        req.uri().path(),
        req.uri().query(),
    ) {
        Ok(url) => url,
        Err(error) => {
            return json_error(
                StatusCode::BAD_GATEWAY,
                format!("T3 Code upstream URL is invalid: {error}"),
            );
        }
    };
    let (parts, body) = req.into_parts();
    let body = match to_bytes(body, usize::MAX).await {
        Ok(body) => body,
        Err(error) => {
            return json_error(
                StatusCode::BAD_REQUEST,
                format!("T3 Code request body could not be read: {error}"),
            );
        }
    };

    let mut request = state.client().request(parts.method, target_url);
    for (name, value) in &parts.headers {
        if is_hop_by_hop_header(name.as_str()) || *name == header::HOST {
            continue;
        }
        if *name == header::COOKIE {
            if let Some(value) = cookie_header_without(value, AUTH_COOKIE_NAME) {
                request = request.header(name, value);
            }
            continue;
        }
        request = request.header(name, value);
    }

    let upstream = match request
        .timeout(Duration::from_secs(60))
        .body(body)
        .send()
        .await
    {
        Ok(response) => response,
        Err(error) => {
            return json_error(
                StatusCode::BAD_GATEWAY,
                format!("T3 Code upstream request failed: {error}"),
            );
        }
    };
    let status = upstream.status();
    let mut response = Response::builder().status(status);
    for (name, value) in upstream.headers() {
        if !is_hop_by_hop_header(name.as_str()) {
            response = response.header(name, value);
        }
    }
    match upstream.bytes().await {
        Ok(body) => response
            .body(Body::from(body))
            .unwrap_or_else(internal_response),
        Err(error) => json_error(
            StatusCode::BAD_GATEWAY,
            format!("T3 Code upstream response could not be read: {error}"),
        ),
    }
}

async fn t3code_gateway_websocket(
    State(state): State<AppState>,
    ws: WebSocketUpgrade,
    headers: HeaderMap,
    uri: Uri,
) -> Response<Body> {
    let config = state.config_snapshot().await;
    if !config.t3code.enabled || config.t3code.gateway_bind.is_none() {
        return plain_response(StatusCode::NOT_FOUND, "T3 Code gateway is not enabled\n");
    }
    if !public_headers_are_authenticated(&state, &config, &headers, None) {
        return public_api_auth_challenge();
    }

    let mut upstream_url = match Url::parse(&config.t3code.server_url) {
        Ok(url) => url,
        Err(error) => {
            return json_error(
                StatusCode::BAD_GATEWAY,
                format!("T3 Code upstream URL is invalid: {error}"),
            );
        }
    };
    if upstream_url.set_scheme("ws").is_err() {
        return json_error(StatusCode::BAD_GATEWAY, "T3 Code upstream URL is invalid");
    }
    upstream_url.set_path(uri.path());
    upstream_url.set_query(uri.query());
    let mut upstream_request = match upstream_url.as_str().into_client_request() {
        Ok(request) => request,
        Err(error) => {
            return json_error(
                StatusCode::BAD_GATEWAY,
                format!("T3 Code websocket request could not be created: {error}"),
            );
        }
    };
    if let Some(cookie) = headers
        .get(header::COOKIE)
        .and_then(|value| cookie_header_without(value, AUTH_COOKIE_NAME))
        .and_then(|value| value.parse().ok())
    {
        upstream_request
            .headers_mut()
            .insert(header::COOKIE, cookie);
    }
    if let Some(authorization) = headers.get(header::AUTHORIZATION) {
        upstream_request
            .headers_mut()
            .insert(header::AUTHORIZATION, authorization.clone());
    }
    if let Some(user_agent) = headers.get(header::USER_AGENT) {
        upstream_request
            .headers_mut()
            .insert(header::USER_AGENT, user_agent.clone());
    }

    let upstream = match connect_async(upstream_request).await {
        Ok((socket, _response)) => socket,
        Err(error) => {
            warn!(%error, "T3 Code upstream websocket connection failed");
            return json_error(
                StatusCode::BAD_GATEWAY,
                format!("T3 Code websocket connection failed: {error}"),
            );
        }
    };

    ws.on_upgrade(move |client| proxy_websocket(client, upstream))
}

async fn proxy_websocket(
    client: axum::extract::ws::WebSocket,
    upstream: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) {
    let (mut client_tx, mut client_rx) = client.split();
    let (mut upstream_tx, mut upstream_rx) = upstream.split();
    loop {
        tokio::select! {
            message = client_rx.next() => match message {
                Some(Ok(message)) => {
                    let close = matches!(message, AxumMessage::Close(_));
                    if let Err(error) = upstream_tx.send(to_upstream_message(message)).await {
                        warn!(%error, "T3 Code gateway could not forward browser message");
                        break;
                    }
                    if close {
                        break;
                    }
                }
                Some(Err(error)) => {
                    warn!(%error, "T3 Code gateway browser websocket failed");
                    break;
                }
                None => break,
            },
            message = upstream_rx.next() => match message {
                Some(Ok(TungsteniteMessage::Frame(_))) => {}
                Some(Ok(message)) => {
                    let close = matches!(message, TungsteniteMessage::Close(_));
                    if let Err(error) = client_tx.send(to_client_message(message)).await {
                        warn!(%error, "T3 Code gateway could not forward upstream message");
                        break;
                    }
                    if close {
                        break;
                    }
                }
                Some(Err(error)) => {
                    warn!(%error, "T3 Code gateway upstream websocket failed");
                    break;
                }
                None => break,
            },
        }
    }
}

fn to_upstream_message(message: AxumMessage) -> TungsteniteMessage {
    match message {
        AxumMessage::Text(value) => TungsteniteMessage::Text(value.to_string().into()),
        AxumMessage::Binary(value) => TungsteniteMessage::Binary(value),
        AxumMessage::Ping(value) => TungsteniteMessage::Ping(value),
        AxumMessage::Pong(value) => TungsteniteMessage::Pong(value),
        AxumMessage::Close(_) => TungsteniteMessage::Close(None),
    }
}

fn to_client_message(message: TungsteniteMessage) -> AxumMessage {
    match message {
        TungsteniteMessage::Text(value) => AxumMessage::Text(value.to_string().into()),
        TungsteniteMessage::Binary(value) => AxumMessage::Binary(value),
        TungsteniteMessage::Ping(value) => AxumMessage::Ping(value),
        TungsteniteMessage::Pong(value) => AxumMessage::Pong(value),
        TungsteniteMessage::Close(_) | TungsteniteMessage::Frame(_) => AxumMessage::Close(None),
    }
}

fn cookie_header_without(value: &header::HeaderValue, excluded_name: &str) -> Option<String> {
    let raw = value.to_str().ok()?;
    let cookies = raw
        .split(';')
        .map(str::trim)
        .filter(|cookie| {
            cookie
                .split_once('=')
                .is_some_and(|(name, _)| name.trim() != excluded_name)
        })
        .collect::<Vec<_>>();
    (!cookies.is_empty()).then(|| cookies.join("; "))
}

fn append_base_dir(args: &mut Vec<OsString>, config: &T3CodeConfig) {
    if let Some(base_dir) = &config.base_dir {
        args.push(OsString::from("--base-dir"));
        args.push(base_dir.as_os_str().to_owned());
    }
}

fn project_registration_args(
    config: &T3CodeConfig,
    project_dir: &Path,
    project_name: &str,
) -> Vec<OsString> {
    let mut args = vec![
        OsString::from("project"),
        OsString::from("add"),
        project_dir.as_os_str().to_owned(),
        OsString::from("--title"),
        OsString::from(project_name),
        OsString::from("--if-missing"),
        OsString::from("--json"),
    ];
    append_base_dir(&mut args, config);
    args
}

async fn ensure_server(state: &AppState, config: &T3CodeConfig) -> Result<(), String> {
    if !config.start_if_needed {
        return Ok(());
    }
    if server_is_ready(state, config).await {
        return Ok(());
    }

    let server_url = Url::parse(&config.server_url)
        .map_err(|error| format!("T3 Code server URL is invalid: {error}"))?;
    let host = server_url
        .host_str()
        .ok_or_else(|| "T3 Code server URL has no host".to_string())?;
    let port = server_url
        .port_or_known_default()
        .ok_or_else(|| "T3 Code server URL has no port".to_string())?;

    let mut command = cli_command(config);
    command
        .arg("serve")
        .arg("--host")
        .arg(host)
        .arg("--port")
        .arg(port.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(false);
    if let Some(base_dir) = &config.base_dir {
        command.arg("--base-dir").arg(base_dir);
    }
    command
        .spawn()
        .map_err(|error| format!("Latitude could not start T3 Code: {error}"))?;

    for _ in 0..40 {
        sleep(Duration::from_millis(250)).await;
        if server_is_ready(state, config).await {
            return Ok(());
        }
    }
    Err("T3 Code did not become ready within 10 seconds".to_string())
}

async fn server_is_ready(state: &AppState, config: &T3CodeConfig) -> bool {
    let Ok(url) = Url::parse(&config.server_url).and_then(|url| url.join("/api/auth/session"))
    else {
        return false;
    };
    state
        .client()
        .get(url)
        .timeout(Duration::from_secs(1))
        .send()
        .await
        .is_ok()
}

async fn run_cli(config: &T3CodeConfig, args: Vec<OsString>) -> Result<Vec<u8>, String> {
    let output = cli_command(config)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .await
        .map_err(|error| format!("Latitude could not run the T3 Code CLI: {error}"))?;
    if output.status.success() {
        return Ok(output.stdout);
    }

    let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Err(if detail.is_empty() {
        format!("T3 Code exited with {}", output.status)
    } else {
        format!("T3 Code failed: {detail}")
    })
}

fn cli_command(config: &T3CodeConfig) -> Command {
    let mut command = Command::new(&config.command);
    command.args(&config.command_args);
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_gateway_url_uses_request_host_and_gateway_port() {
        let config = T3CodeConfig {
            enabled: true,
            base_url: "auto".to_string(),
            gateway_bind: Some("0.0.0.0:5598".to_string()),
            ..T3CodeConfig::default()
        };
        let request = Request::builder()
            .uri("/__latitude/t3code/fabricore")
            .header(header::HOST, "fabricore-vm:5597")
            .body(Body::empty())
            .unwrap();

        assert_eq!(
            pairing_base_url(&config, &request).unwrap(),
            "http://fabricore-vm:5598"
        );
    }

    #[test]
    fn automatic_gateway_url_honors_forwarded_https_host() {
        let config = T3CodeConfig {
            enabled: true,
            base_url: "auto".to_string(),
            gateway_bind: Some("0.0.0.0:5598".to_string()),
            ..T3CodeConfig::default()
        };
        let request = Request::builder()
            .uri("/__latitude/t3code/fabricore")
            .header(header::HOST, "127.0.0.1:5597")
            .header("x-forwarded-host", "latitude.example.com")
            .header("x-forwarded-proto", "https")
            .body(Body::empty())
            .unwrap();

        assert_eq!(
            pairing_base_url(&config, &request).unwrap(),
            "https://latitude.example.com:5598"
        );
    }

    #[test]
    fn gateway_does_not_forward_latitude_session_cookie() {
        let value = header::HeaderValue::from_static(
            "theme=dark; latitude_public_session=secret; t3_session=allowed",
        );

        assert_eq!(
            cookie_header_without(&value, AUTH_COOKIE_NAME).as_deref(),
            Some("theme=dark; t3_session=allowed")
        );
    }

    #[tokio::test]
    async fn gateway_serves_login_assets_without_authentication() {
        let response =
            t3code_gateway_asset(AxumPath("auth.css".to_string()), HeaderMap::new()).await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok()),
            Some("text/css; charset=utf-8")
        );
    }

    #[test]
    fn project_registration_uses_the_configured_t3code_home() {
        let config = T3CodeConfig {
            server_url: "http://127.0.0.1:4773".to_string(),
            base_dir: Some("C:/Users/test/.t3".into()),
            ..T3CodeConfig::default()
        };
        let args = project_registration_args(&config, Path::new("C:/work/demo"), "demo")
            .into_iter()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert_eq!(
            args,
            vec![
                "project",
                "add",
                "C:/work/demo",
                "--title",
                "demo",
                "--if-missing",
                "--json",
                "--base-dir",
                "C:/Users/test/.t3",
            ]
        );
    }

    #[test]
    fn linked_worktree_launch_uses_root_project_and_preserves_worktree_context() {
        let common_git_dir = Path::new("C:/work/demo/.git").to_path_buf();
        let worktrees = vec![
            WorktreeRecord {
                project_name: "demo".to_string(),
                common_git_dir: common_git_dir.clone(),
                worktree_dir: Path::new("C:/work/demo").to_path_buf(),
                branch: Some("main".to_string()),
                head: "abc123".to_string(),
                discovered: false,
                archived: false,
            },
            WorktreeRecord {
                project_name: "demo--feature-fix".to_string(),
                common_git_dir,
                worktree_dir: Path::new("C:/worktrees/demo-feature-fix").to_path_buf(),
                branch: Some("feature/fix".to_string()),
                head: "def456".to_string(),
                discovered: true,
                archived: false,
            },
        ];

        let (root_project_name, launch_worktree) =
            resolve_t3code_launch_mapping("demo--feature-fix", &worktrees);

        assert_eq!(root_project_name, "demo");
        assert_eq!(
            launch_worktree
                .as_ref()
                .map(|worktree| &worktree.worktree_dir),
            Some(&Path::new("C:/worktrees/demo-feature-fix").to_path_buf())
        );
        assert_eq!(
            launch_worktree.and_then(|worktree| worktree.branch),
            Some("feature/fix".to_string())
        );
    }

    #[test]
    fn primary_worktree_launch_preserves_its_branch_context() {
        let worktrees = vec![WorktreeRecord {
            project_name: "demo".to_string(),
            common_git_dir: Path::new("C:/work/demo/.git").to_path_buf(),
            worktree_dir: Path::new("C:/work/demo").to_path_buf(),
            branch: Some("feature/current-checkout".to_string()),
            head: "abc123".to_string(),
            discovered: false,
            archived: false,
        }];

        let (root_project_name, launch_worktree) =
            resolve_t3code_launch_mapping("demo", &worktrees);

        assert_eq!(root_project_name, "demo");
        assert_eq!(
            launch_worktree
                .as_ref()
                .map(|worktree| &worktree.worktree_dir),
            Some(&Path::new("C:/work/demo").to_path_buf())
        );
        assert_eq!(
            launch_worktree.and_then(|worktree| worktree.branch),
            Some("feature/current-checkout".to_string())
        );
    }
}
