use std::{path::Path, time::Duration};

use axum::{
    Json,
    body::{Body, to_bytes},
    extract::State,
    http::{HeaderValue, Method, Request, Response, StatusCode, header},
    response::IntoResponse,
};
use tokio::fs;
use tracing::error;

use crate::{
    config::{
        ApplicationConfig, ApplicationTarget, BootConfig, DeploymentShareConfig, PageFormat,
        ProjectConfig, current_unix_timestamp, is_binary_document_media_type,
    },
    desktop::desktop_info_response,
    state::AppState,
    storage::PageContent,
};

use super::super::{
    auth::{
        clean_next_path, header_cookie_value, parse_public_login_form, public_auth_challenge,
        public_login_success_response, public_password_matches, public_request_is_authenticated,
        request_bearer_token,
    },
    constants::{
        AUTH_COOKIE_MAX_AGE_SECONDS, AUTH_COOKIE_NAME, DESKTOP_ROUTE_SEGMENT, DIFF_ROUTE_SEGMENT,
        FILES_ROUTE_SEGMENT, MAX_LOGIN_PAYLOAD_BYTES, MAX_TERMINAL_COMMAND_BYTES,
        PUBLIC_ROOT_DESKTOP_WS_PATH, PUBLIC_SHARE_BASE_PATH, TERMINAL_ROUTE_SEGMENT,
    },
    desktop_api::execute_desktop_action_request,
    git::{GitActionResponse, collect_project_diff, handle_git_action_request},
    page::{page_theme_from_headers, render_project_page_content},
    paths::{
        ProjectPath, is_hop_by_hop_header, join_upstream_url, resolve_project_path,
        sanitized_relative_path, split_project_path,
    },
    render::{
        render_diff_workspace_fragment, render_project_diff, render_project_files,
        render_project_home, render_project_terminal, render_root_desktop, render_root_terminal,
        render_server_home, render_share_login,
    },
    response::{internal_response, json_error, plain_response},
    terminal_api::{
        execute_root_terminal_command, execute_terminal_command, parse_terminal_command_payload,
        root_terminal_info_response, terminal_info_response,
    },
};

pub(in crate::server) async fn public_entry(
    State(state): State<AppState>,
    req: Request<Body>,
) -> Response<Body> {
    let original_path = req.uri().path().to_string();
    let config = state.config_snapshot().await;
    let device_hostname = state.device_hostname().to_string();

    if original_path == PUBLIC_SHARE_BASE_PATH
        || original_path.starts_with(&format!("{PUBLIC_SHARE_BASE_PATH}/"))
    {
        let Some(share_path) = split_share_path(&original_path) else {
            return plain_response(StatusCode::NOT_FOUND, "share link was not found\n");
        };
        return serve_shared_deployment(state, req, share_path, &device_hostname).await;
    }

    if !public_request_is_authenticated(&state, &config, &req) {
        return public_auth_challenge(&state, &req, false);
    }

    if original_path == "/" {
        return serve_server_home(req, &state, &config, &device_hostname).await;
    }

    if let Some(remainder) = root_terminal_remainder(&original_path) {
        return serve_root_terminal(req, remainder, &device_hostname).await;
    }

    if let Some(remainder) = root_desktop_remainder(&original_path) {
        return serve_root_desktop(req, &state, &config, remainder, &device_hostname).await;
    }

    let Some(public_path) = split_project_path(&original_path) else {
        return plain_response(
            StatusCode::NOT_FOUND,
            "Latitude is running. Mount a deployment at /{project}/{name} to serve traffic.\n",
        );
    };
    let project_mount = public_path.project_name().to_string();

    let project = match load_enabled_project(&state, &project_mount).await {
        Ok(Some(project)) => project,
        Ok(None) => {
            return plain_response(
                StatusCode::NOT_FOUND,
                format!("No enabled project is mounted at /{project_mount}\n"),
            );
        }
        Err(response) => return response,
    };

    let ProjectPath::Deployment {
        deployment: app_mount,
        remainder,
        ..
    } = public_path
    else {
        return serve_project_home(req, &project, &device_hostname).await;
    };

    if app_mount == DIFF_ROUTE_SEGMENT {
        return serve_project_diff(req, &project, remainder.as_str(), &device_hostname).await;
    }
    if app_mount == FILES_ROUTE_SEGMENT {
        if remainder.as_str() != "/" {
            return plain_response(
                StatusCode::NOT_FOUND,
                "file viewer only serves one document\n",
            );
        }
        if req.method() != Method::GET && req.method() != Method::HEAD {
            return plain_response(
                StatusCode::METHOD_NOT_ALLOWED,
                "file viewer supports GET and HEAD\n",
            );
        }
        return html_response(
            req.method(),
            render_project_files(&project, &device_hostname),
        );
    }
    if app_mount == TERMINAL_ROUTE_SEGMENT {
        return serve_project_terminal(req, &project, remainder.as_str(), &device_hostname).await;
    }

    let Some(app) = project
        .deployments
        .iter()
        .find(|app| app.enabled && app.name == app_mount)
        .cloned()
    else {
        return plain_response(
            StatusCode::NOT_FOUND,
            format!("No enabled deployment is mounted at /{project_mount}/{app_mount}\n"),
        );
    };

    let mount_path = format!("/{}/{}", project.name, app.name);
    serve_deployment_target(
        state,
        req,
        &project,
        &app,
        remainder.as_str(),
        &mount_path,
        None,
        &device_hostname,
    )
    .await
}

struct SharePath {
    token: String,
    mount_path: String,
    remainder: String,
}

async fn serve_shared_deployment(
    state: AppState,
    req: Request<Body>,
    share_path: SharePath,
    device_hostname: &str,
) -> Response<Body> {
    let share = match state.catalog().get_share(&share_path.token).await {
        Ok(Some(share)) => share,
        Ok(None) => return plain_response(StatusCode::NOT_FOUND, "share link was not found\n"),
        Err(error) => {
            error!(%error, token = %share_path.token, "share link lookup failed");
            return plain_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "catalog could not be read\n",
            );
        }
    };

    let now = current_unix_timestamp();
    if share.is_expired(now) {
        return plain_response(StatusCode::GONE, "share link has expired\n");
    }

    if !share_request_is_authenticated(&state, &req, &share) {
        if req.method() == Method::POST {
            return handle_share_login_post(state, req, &share, &share_path, now, device_hostname)
                .await;
        }
        return share_auth_challenge(&req, &share_path, false, device_hostname);
    }

    let project = match load_enabled_project(&state, &share.project).await {
        Ok(Some(project)) => project,
        Ok(None) => {
            return plain_response(
                StatusCode::NOT_FOUND,
                format!("No enabled project is mounted at {}\n", share.project),
            );
        }
        Err(response) => return response,
    };

    let Some(app) = project
        .deployments
        .iter()
        .find(|app| app.enabled && app.name == share.deployment)
        .cloned()
    else {
        return plain_response(
            StatusCode::NOT_FOUND,
            format!(
                "No enabled deployment is mounted at {}/{}\n",
                share.project, share.deployment
            ),
        );
    };

    let share_cookie_name = share
        .password
        .as_ref()
        .map(|_| share_auth_cookie_name(&share.token));
    serve_deployment_target(
        state,
        req,
        &project,
        &app,
        &share_path.remainder,
        &share_path.mount_path,
        share_cookie_name.as_deref(),
        device_hostname,
    )
    .await
}

async fn handle_share_login_post(
    state: AppState,
    req: Request<Body>,
    share: &DeploymentShareConfig,
    share_path: &SharePath,
    now: u64,
    device_hostname: &str,
) -> Response<Body> {
    let query_next = req
        .uri()
        .path_and_query()
        .map(|path_and_query| path_and_query.as_str().to_string());
    let (_parts, body) = req.into_parts();
    let body = match to_bytes(body, MAX_LOGIN_PAYLOAD_BYTES).await {
        Ok(body) => body,
        Err(error) => {
            return plain_response(
                StatusCode::BAD_REQUEST,
                format!("share login payload could not be read: {error}\n"),
            );
        }
    };

    let form = parse_public_login_form(&body);
    let next = clean_next_path(form.next.or(query_next));

    if share
        .password
        .as_deref()
        .is_some_and(|password| public_password_matches(&form.password, password))
    {
        return public_login_success_response(&next, share_auth_set_cookie(&state, share, now));
    }

    share_auth_challenge_for_path(&share_path.mount_path, &next, true, false, device_hostname)
}

fn share_auth_challenge(
    req: &Request<Body>,
    share_path: &SharePath,
    login_failed: bool,
    device_hostname: &str,
) -> Response<Body> {
    if req.method() != Method::GET && req.method() != Method::HEAD {
        return plain_response(StatusCode::UNAUTHORIZED, "share password required\n");
    }

    let next = clean_next_path(
        req.uri()
            .path_and_query()
            .map(|path_and_query| path_and_query.as_str().to_string()),
    );

    share_auth_challenge_for_path(
        &share_path.mount_path,
        &next,
        login_failed,
        req.method() == Method::HEAD,
        device_hostname,
    )
}

fn share_auth_challenge_for_path(
    action: &str,
    next: &str,
    login_failed: bool,
    head: bool,
    device_hostname: &str,
) -> Response<Body> {
    let method = if head { Method::HEAD } else { Method::GET };
    html_status_response(
        StatusCode::UNAUTHORIZED,
        &method,
        render_share_login(action, next, login_failed, device_hostname),
    )
}

fn share_request_is_authenticated(
    state: &AppState,
    req: &Request<Body>,
    share: &DeploymentShareConfig,
) -> bool {
    if share.password.is_none() {
        return true;
    }

    header_cookie_value(req.headers(), &share_auth_cookie_name(&share.token))
        .as_deref()
        .is_some_and(|value| state.verify_public_auth_cookie(&share_auth_key(share), value))
}

fn share_auth_set_cookie(state: &AppState, share: &DeploymentShareConfig, now: u64) -> String {
    let value = state.public_auth_cookie_value(&share_auth_key(share));
    let cookie_name = share_auth_cookie_name(&share.token);
    let max_age = share
        .expires_at
        .map(|expires_at| expires_at.saturating_sub(now))
        .filter(|seconds| *seconds > 0)
        .unwrap_or(AUTH_COOKIE_MAX_AGE_SECONDS)
        .min(AUTH_COOKIE_MAX_AGE_SECONDS);
    format!(
        "{cookie_name}={value}; HttpOnly; SameSite=Lax; Path={PUBLIC_SHARE_BASE_PATH}/{}; Max-Age={max_age}",
        share.token
    )
}

fn share_auth_key(share: &DeploymentShareConfig) -> String {
    format!(
        "share:{}:{}:{}:{}",
        share.token,
        share.project,
        share.deployment,
        share.password.as_deref().unwrap_or("")
    )
}

fn share_auth_cookie_name(token: &str) -> String {
    format!("latitude_share_{token}")
}

fn split_share_path(path: &str) -> Option<SharePath> {
    let prefix = format!("{PUBLIC_SHARE_BASE_PATH}/");
    let rest = path.strip_prefix(&prefix)?;
    let mut segments = rest.splitn(2, '/');
    let token = segments.next()?.to_string();
    if token.is_empty() {
        return None;
    }

    let remainder = segments
        .next()
        .map(|rest| format!("/{rest}"))
        .unwrap_or_else(|| "/".to_string());

    Some(SharePath {
        mount_path: format!("{PUBLIC_SHARE_BASE_PATH}/{token}"),
        token,
        remainder,
    })
}

async fn serve_deployment_target(
    state: AppState,
    req: Request<Body>,
    project: &ProjectConfig,
    app: &ApplicationConfig,
    remainder: &str,
    mount_path: &str,
    extra_excluded_cookie_name: Option<&str>,
    device_hostname: &str,
) -> Response<Body> {
    match &app.target {
        ApplicationTarget::ReverseProxy {
            upstream,
            strip_prefix,
        } => {
            proxy_request(
                state,
                req,
                upstream,
                *strip_prefix,
                remainder,
                mount_path,
                extra_excluded_cookie_name,
            )
            .await
        }
        ApplicationTarget::Static {
            root,
            index_file,
            spa_fallback,
        } => {
            let root = resolve_project_path(&project.project_dir, root);
            serve_static(
                req,
                &project.name,
                &app.name,
                &root,
                index_file,
                *spa_fallback,
                remainder,
                device_hostname,
            )
            .await
        }
        ApplicationTarget::Page { .. } => {
            match state
                .catalog()
                .get_page_content(&project.name, &app.name)
                .await
            {
                Ok(Some(content)) => {
                    serve_page(req, &project.name, content, remainder, device_hostname).await
                }
                Ok(None) => plain_response(
                    StatusCode::NOT_FOUND,
                    format!(
                        "page deployment '{}' was not found in project '{}'\n",
                        app.name, project.name
                    ),
                ),
                Err(error) => json_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("page content could not be read: {error}"),
                ),
            }
        }
    }
}

async fn proxy_request(
    state: AppState,
    req: Request<Body>,
    upstream: &str,
    strip_prefix: bool,
    remainder: &str,
    mount_path: &str,
    extra_excluded_cookie_name: Option<&str>,
) -> Response<Body> {
    let (parts, body) = req.into_parts();
    let forward_path = if strip_prefix {
        remainder.to_string()
    } else {
        format!("{}{}", mount_path.trim_end_matches('/'), remainder)
    };

    let target_url = match join_upstream_url(upstream, &forward_path, parts.uri.query()) {
        Ok(url) => url,
        Err(error) => {
            return json_error(
                StatusCode::BAD_GATEWAY,
                format!("upstream URL could not be built: {error}"),
            );
        }
    };

    let body_bytes = match to_bytes(body, usize::MAX).await {
        Ok(bytes) => bytes,
        Err(error) => {
            return json_error(
                StatusCode::BAD_REQUEST,
                format!("request body could not be read: {error}"),
            );
        }
    };

    let mut builder = state.client().request(parts.method, target_url);
    for (name, value) in &parts.headers {
        if is_hop_by_hop_header(name.as_str()) || *name == header::HOST {
            continue;
        }
        if *name == header::COOKIE {
            let excluded_cookie_names = [Some(AUTH_COOKIE_NAME), extra_excluded_cookie_name];
            if let Some(filtered_cookie) =
                filtered_cookie_header_except(value, &excluded_cookie_names)
            {
                builder = builder.header(name, filtered_cookie);
            }
            continue;
        }
        builder = builder.header(name, value);
    }

    match builder
        .timeout(Duration::from_secs(60))
        .body(body_bytes)
        .send()
        .await
    {
        Ok(response) => {
            let status = response.status();
            let mut response_builder = Response::builder().status(status);

            for (name, value) in response.headers() {
                if is_hop_by_hop_header(name.as_str()) {
                    continue;
                }
                response_builder = response_builder.header(name, value);
            }

            match response.bytes().await {
                Ok(bytes) => response_builder
                    .body(Body::from(bytes))
                    .unwrap_or_else(internal_response),
                Err(error) => json_error(
                    StatusCode::BAD_GATEWAY,
                    format!("upstream body could not be read: {error}"),
                ),
            }
        }
        Err(error) => json_error(
            StatusCode::BAD_GATEWAY,
            format!("upstream request failed: {error}"),
        ),
    }
}

fn filtered_cookie_header_except(
    value: &HeaderValue,
    excluded_names: &[Option<&str>],
) -> Option<String> {
    let raw = value.to_str().ok()?;
    let cookies = raw
        .split(';')
        .filter_map(|cookie| {
            let cookie = cookie.trim();
            let (name, _) = cookie.split_once('=')?;
            let should_exclude = excluded_names
                .iter()
                .flatten()
                .any(|excluded_name| name.trim() == *excluded_name);
            (!should_exclude).then(|| cookie.to_string())
        })
        .collect::<Vec<_>>();

    if cookies.is_empty() {
        None
    } else {
        Some(cookies.join("; "))
    }
}

async fn serve_static(
    req: Request<Body>,
    project_name: &str,
    deployment_name: &str,
    root: &Path,
    index_file: &str,
    spa_fallback: bool,
    remainder: &str,
    device_hostname: &str,
) -> Response<Body> {
    if req.method() != Method::GET && req.method() != Method::HEAD {
        return plain_response(
            StatusCode::METHOD_NOT_ALLOWED,
            "static deployments support GET and HEAD\n",
        );
    }

    if remainder == "/"
        && !page_raw_requested(req.uri().query())
        && let Some(media_type) = static_document_media_type(index_file)
    {
        return html_response(
            req.method(),
            render_project_page_content(
                project_name,
                Some(deployment_name),
                PageFormat::Binary,
                Some(&media_type),
                "",
                page_theme_from_headers(req.headers()),
                device_hostname,
            ),
        );
    }

    let relative_path = match sanitized_relative_path(remainder) {
        Some(path) => path,
        None => return plain_response(StatusCode::BAD_REQUEST, "invalid static path\n"),
    };

    let mut candidate = root.join(relative_path);
    match fs::metadata(&candidate).await {
        Ok(metadata) if metadata.is_dir() => {
            candidate = candidate.join(index_file);
        }
        Ok(_) => {}
        Err(_) if spa_fallback => {
            candidate = root.join(index_file);
        }
        Err(_) => return plain_response(StatusCode::NOT_FOUND, "file not found\n"),
    }

    let metadata = match fs::metadata(&candidate).await {
        Ok(metadata) if metadata.is_file() => metadata,
        _ if spa_fallback => match fs::metadata(root.join(index_file)).await {
            Ok(metadata) if metadata.is_file() => {
                candidate = root.join(index_file);
                metadata
            }
            _ => return plain_response(StatusCode::NOT_FOUND, "file not found\n"),
        },
        _ => return plain_response(StatusCode::NOT_FOUND, "file not found\n"),
    };

    let content_type = mime_guess::from_path(&candidate)
        .first_or_octet_stream()
        .to_string();

    if req.method() == Method::HEAD {
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, content_type)
            .header(header::CONTENT_LENGTH, metadata.len())
            .body(Body::empty())
            .unwrap_or_else(internal_response);
    }

    match fs::read(&candidate).await {
        Ok(bytes) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, content_type)
            .header(header::CONTENT_LENGTH, bytes.len())
            .body(Body::from(bytes))
            .unwrap_or_else(internal_response),
        Err(error) => json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("file could not be read: {error}"),
        ),
    }
}

async fn serve_page(
    req: Request<Body>,
    project_name: &str,
    content: PageContent,
    remainder: &str,
    device_hostname: &str,
) -> Response<Body> {
    if req.method() != Method::GET && req.method() != Method::HEAD {
        return plain_response(
            StatusCode::METHOD_NOT_ALLOWED,
            "page deployments support GET and HEAD\n",
        );
    }

    if remainder != "/" {
        return plain_response(
            StatusCode::NOT_FOUND,
            "page deployments only serve one document\n",
        );
    }

    if content.format == PageFormat::Binary && page_raw_requested(req.uri().query()) {
        return binary_document_response(
            req.method(),
            content.media_type.as_deref(),
            content.bytes,
        );
    }

    let rendered_content = match content.format {
        PageFormat::Binary => String::new(),
        PageFormat::Html | PageFormat::Markdown => match String::from_utf8(content.bytes) {
            Ok(content) => content,
            Err(error) => {
                return plain_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("page content could not be decoded as UTF-8: {error}\n"),
                );
            }
        },
    };

    html_response(
        req.method(),
        render_project_page_content(
            project_name,
            content.title.as_deref(),
            content.format,
            content.media_type.as_deref(),
            &rendered_content,
            page_theme_from_headers(req.headers()),
            device_hostname,
        ),
    )
}

fn binary_document_response(
    method: &Method,
    media_type: Option<&str>,
    bytes: Vec<u8>,
) -> Response<Body> {
    let Some(media_type) = media_type else {
        return plain_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "binary page media_type is missing\n",
        );
    };
    let builder = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, media_type)
        .header(header::CONTENT_LENGTH, bytes.len());

    if method == Method::HEAD {
        builder
            .body(Body::empty())
            .unwrap_or_else(internal_response)
    } else {
        builder
            .body(Body::from(bytes))
            .unwrap_or_else(internal_response)
    }
}

fn page_raw_requested(query: Option<&str>) -> bool {
    query.is_some_and(|query| {
        url::form_urlencoded::parse(query.as_bytes()).any(|(name, value)| {
            name.eq_ignore_ascii_case("raw") && value != "0" && !value.eq_ignore_ascii_case("false")
        })
    })
}

fn static_document_media_type(index_file: &str) -> Option<String> {
    mime_guess::from_path(index_file)
        .first()
        .map(|mime| mime.essence_str().to_string())
        .filter(|media_type| is_binary_document_media_type(media_type))
}

async fn serve_project_home(
    req: Request<Body>,
    project: &ProjectConfig,
    device_hostname: &str,
) -> Response<Body> {
    if req.method() != Method::GET && req.method() != Method::HEAD {
        return plain_response(
            StatusCode::METHOD_NOT_ALLOWED,
            "project homes support GET and HEAD\n",
        );
    }

    html_response(req.method(), render_project_home(project, device_hostname))
}

async fn serve_project_diff(
    req: Request<Body>,
    project: &ProjectConfig,
    remainder: &str,
    device_hostname: &str,
) -> Response<Body> {
    let method = req.method().clone();
    if method != Method::GET && method != Method::HEAD && method != Method::PATCH {
        return plain_response(
            StatusCode::METHOD_NOT_ALLOWED,
            "diff viewers support GET, HEAD, and PATCH\n",
        );
    }

    if remainder != "/" {
        return plain_response(
            StatusCode::NOT_FOUND,
            "diff viewers only serve one document\n",
        );
    }

    if method == Method::PATCH {
        let action_result = handle_git_action_request(req, &project.project_dir).await;
        if let Err(error) = &action_result {
            error!(%error, project = %project.name, "git action failed");
        }

        let report = collect_project_diff(&project.project_dir).await;
        return (
            StatusCode::OK,
            Json(GitActionResponse {
                ok: action_result.is_ok(),
                error: action_result.err(),
                workspace_html: render_diff_workspace_fragment(&report),
            }),
        )
            .into_response();
    }

    let report = collect_project_diff(&project.project_dir).await;
    html_response(
        &method,
        render_project_diff(project, &report, device_hostname),
    )
}

async fn serve_project_terminal(
    req: Request<Body>,
    project: &ProjectConfig,
    remainder: &str,
    device_hostname: &str,
) -> Response<Body> {
    let method = req.method().clone();
    if method != Method::GET && method != Method::HEAD && method != Method::POST {
        return plain_response(
            StatusCode::METHOD_NOT_ALLOWED,
            "terminal viewers support GET, HEAD, and POST\n",
        );
    }

    if remainder != "/" {
        return plain_response(
            StatusCode::NOT_FOUND,
            "terminal viewers only serve one document\n",
        );
    }

    if method == Method::POST {
        let content_type = req
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let (_parts, body) = req.into_parts();
        let body = match to_bytes(body, MAX_TERMINAL_COMMAND_BYTES + 1024).await {
            Ok(body) => body,
            Err(error) => {
                return json_error(
                    StatusCode::BAD_REQUEST,
                    format!("terminal payload could not be read: {error}"),
                );
            }
        };
        let command = match parse_terminal_command_payload(content_type.as_deref(), &body) {
            Ok(command) => command,
            Err(error) => return json_error(StatusCode::BAD_REQUEST, error),
        };

        return Json(execute_terminal_command(&project.project_dir, command).await).into_response();
    }

    let websocket_token = request_bearer_token(&req);
    let info = terminal_info_response(&project.name, &project.project_dir).await;
    html_response(
        &method,
        render_project_terminal(project, &info, websocket_token.as_deref(), device_hostname),
    )
}

async fn serve_root_terminal(
    req: Request<Body>,
    remainder: &str,
    device_hostname: &str,
) -> Response<Body> {
    let method = req.method().clone();
    if method != Method::GET && method != Method::HEAD && method != Method::POST {
        return plain_response(
            StatusCode::METHOD_NOT_ALLOWED,
            "terminal viewers support GET, HEAD, and POST\n",
        );
    }

    if remainder != "/" {
        return plain_response(
            StatusCode::NOT_FOUND,
            "terminal viewers only serve one document\n",
        );
    }

    if method == Method::POST {
        let content_type = req
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let (_parts, body) = req.into_parts();
        let body = match to_bytes(body, MAX_TERMINAL_COMMAND_BYTES + 1024).await {
            Ok(body) => body,
            Err(error) => {
                return json_error(
                    StatusCode::BAD_REQUEST,
                    format!("terminal payload could not be read: {error}"),
                );
            }
        };
        let command = match parse_terminal_command_payload(content_type.as_deref(), &body) {
            Ok(command) => command,
            Err(error) => return json_error(StatusCode::BAD_REQUEST, error),
        };

        return Json(execute_root_terminal_command(command).await).into_response();
    }

    let websocket_token = request_bearer_token(&req);
    let info = root_terminal_info_response().await;
    html_response(
        &method,
        render_root_terminal(&info, websocket_token.as_deref(), device_hostname),
    )
}

async fn serve_root_desktop(
    req: Request<Body>,
    state: &AppState,
    config: &BootConfig,
    remainder: &str,
    device_hostname: &str,
) -> Response<Body> {
    let method = req.method().clone();
    if method != Method::GET && method != Method::HEAD && method != Method::PATCH {
        return plain_response(
            StatusCode::METHOD_NOT_ALLOWED,
            "desktop viewers support GET, HEAD, and PATCH\n",
        );
    }

    if remainder != "/" {
        return plain_response(
            StatusCode::NOT_FOUND,
            "desktop viewers only serve one document\n",
        );
    }

    if !config.desktop.enabled {
        return plain_response(StatusCode::NOT_FOUND, "desktop is not enabled\n");
    }

    if method == Method::PATCH {
        return execute_desktop_action_request(req).await;
    }

    let target = match state.desktop_manager().target_for(&config.desktop).await {
        Ok(target) => target,
        Err(error) => {
            return plain_response(
                StatusCode::BAD_GATEWAY,
                format!("desktop target could not be prepared: {error}\n"),
            );
        }
    };

    let websocket_token = request_bearer_token(&req);
    let info = desktop_info_response(
        &config.desktop,
        &target,
        PUBLIC_ROOT_DESKTOP_WS_PATH.to_string(),
    );
    html_response(
        &method,
        render_root_desktop(&info, websocket_token.as_deref(), device_hostname),
    )
}

async fn serve_server_home(
    req: Request<Body>,
    state: &AppState,
    config: &BootConfig,
    device_hostname: &str,
) -> Response<Body> {
    if req.method() != Method::GET && req.method() != Method::HEAD {
        return plain_response(
            StatusCode::METHOD_NOT_ALLOWED,
            "server home supports GET and HEAD\n",
        );
    }

    let projects = match state.catalog().list_projects().await {
        Ok(projects) => projects,
        Err(error) => {
            error!(%error, "project list failed");
            return plain_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "catalog could not be read\n",
            );
        }
    };

    html_response(
        req.method(),
        render_server_home(config, &projects, device_hostname),
    )
}

async fn load_enabled_project(
    state: &AppState,
    name: &str,
) -> Result<Option<ProjectConfig>, Response<Body>> {
    state
        .catalog()
        .get_project(name)
        .await
        .map(|project| project.filter(|project| project.enabled))
        .map_err(|error| {
            error!(%error, project = %name, "project lookup failed");
            plain_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "catalog could not be read\n",
            )
        })
}

fn root_terminal_remainder(path: &str) -> Option<&str> {
    let root_terminal_path = format!("/{TERMINAL_ROUTE_SEGMENT}");
    if path == root_terminal_path {
        return Some("/");
    }

    let root_terminal_prefix = format!("{root_terminal_path}/");
    path.strip_prefix(&root_terminal_prefix).map(
        |remainder| {
            if remainder.is_empty() { "/" } else { remainder }
        },
    )
}

fn root_desktop_remainder(path: &str) -> Option<&str> {
    let root_desktop_path = format!("/{DESKTOP_ROUTE_SEGMENT}");
    if path == root_desktop_path {
        return Some("/");
    }

    let root_desktop_prefix = format!("{root_desktop_path}/");
    path.strip_prefix(&root_desktop_prefix).map(
        |remainder| {
            if remainder.is_empty() { "/" } else { remainder }
        },
    )
}

fn html_response(method: &Method, html: String) -> Response<Body> {
    html_status_response(StatusCode::OK, method, html)
}

fn html_status_response(status: StatusCode, method: &Method, html: String) -> Response<Body> {
    let bytes = html.into_bytes();
    let builder = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .header(header::CONTENT_LENGTH, bytes.len());

    if method == Method::HEAD {
        builder
            .body(Body::empty())
            .unwrap_or_else(internal_response)
    } else {
        builder
            .body(Body::from(bytes))
            .unwrap_or_else(internal_response)
    }
}
