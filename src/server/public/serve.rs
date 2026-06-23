use std::{path::Path, time::Duration};

use axum::{
    Json,
    body::{Body, to_bytes},
    extract::State,
    http::{Method, Request, Response, StatusCode, header},
    response::IntoResponse,
};
use tokio::fs;
use tracing::error;

use crate::{
    config::{
        ApplicationTarget, LatitudeConfig, PageFormat, ProjectConfig, decode_page_binary_content,
        is_binary_document_media_type,
    },
    state::AppState,
};

use super::super::{
    auth::{public_auth_challenge, public_request_is_authenticated, request_bearer_token},
    constants::{
        AUTH_COOKIE_NAME, DIFF_ROUTE_SEGMENT, MAX_TERMINAL_COMMAND_BYTES, TERMINAL_ROUTE_SEGMENT,
    },
    git::{GitActionResponse, collect_project_diff, handle_git_action_request},
    page::{page_theme_from_headers, render_project_page_content},
    paths::{
        ProjectPath, filtered_cookie_header, is_hop_by_hop_header, join_upstream_url,
        resolve_project_path, sanitized_relative_path, split_project_path,
    },
    render::{
        render_diff_workspace_fragment, render_project_diff, render_project_home,
        render_project_terminal, render_server_home,
    },
    response::{internal_response, json_error, plain_response},
    terminal_api::{
        execute_terminal_command, parse_terminal_command_payload, terminal_info_response,
    },
};

pub(in crate::server) async fn public_entry(
    State(state): State<AppState>,
    req: Request<Body>,
) -> Response<Body> {
    let original_path = req.uri().path().to_string();
    let config = state.config_snapshot().await;
    if !public_request_is_authenticated(&state, &config, &req) {
        return public_auth_challenge(&state, &req, false);
    }
    let device_hostname = state.device_hostname().to_string();

    if original_path == "/" {
        return serve_server_home(req, &config, &device_hostname).await;
    }

    let Some(public_path) = split_project_path(&original_path) else {
        return plain_response(
            StatusCode::NOT_FOUND,
            "Latitude is running. Mount a deployment at /{project}/{name} to serve traffic.\n",
        );
    };
    let project_mount = public_path.project_name().to_string();

    let Some(project) = config
        .projects
        .iter()
        .find(|project| project.enabled && project.name == project_mount)
        .cloned()
    else {
        return plain_response(
            StatusCode::NOT_FOUND,
            format!("No enabled project is mounted at /{project_mount}\n"),
        );
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

    match &app.target {
        ApplicationTarget::ReverseProxy {
            upstream,
            strip_prefix,
        } => proxy_request(state, req, upstream, *strip_prefix, remainder.as_str()).await,
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
                remainder.as_str(),
                &device_hostname,
            )
            .await
        }
        ApplicationTarget::Page {
            content,
            format,
            media_type,
            title,
        } => {
            serve_page(
                req,
                &project.name,
                title.as_deref(),
                *format,
                media_type.as_deref(),
                content,
                remainder.as_str(),
                &device_hostname,
            )
            .await
        }
    }
}

async fn proxy_request(
    state: AppState,
    req: Request<Body>,
    upstream: &str,
    strip_prefix: bool,
    remainder: &str,
) -> Response<Body> {
    let (parts, body) = req.into_parts();
    let forward_path = if strip_prefix {
        remainder.to_string()
    } else {
        parts.uri.path().to_string()
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
            if let Some(filtered_cookie) = filtered_cookie_header(value, AUTH_COOKIE_NAME) {
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
    title: Option<&str>,
    format: PageFormat,
    media_type: Option<&str>,
    content: &str,
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

    if format == PageFormat::Binary && page_raw_requested(req.uri().query()) {
        return binary_document_response(req.method(), media_type, content);
    }

    html_response(
        req.method(),
        render_project_page_content(
            project_name,
            title,
            format,
            media_type,
            content,
            page_theme_from_headers(req.headers()),
            device_hostname,
        ),
    )
}

fn binary_document_response(
    method: &Method,
    media_type: Option<&str>,
    content: &str,
) -> Response<Body> {
    let Some(media_type) = media_type else {
        return plain_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "binary page media_type is missing\n",
        );
    };
    let bytes = match decode_page_binary_content(content) {
        Ok(bytes) => bytes,
        Err(error) => {
            return plain_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("binary page content could not be decoded: {error}\n"),
            );
        }
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

async fn serve_server_home(
    req: Request<Body>,
    config: &LatitudeConfig,
    device_hostname: &str,
) -> Response<Body> {
    if req.method() != Method::GET && req.method() != Method::HEAD {
        return plain_response(
            StatusCode::METHOD_NOT_ALLOWED,
            "server home supports GET and HEAD\n",
        );
    }

    html_response(req.method(), render_server_home(config, device_hostname))
}

fn html_response(method: &Method, html: String) -> Response<Body> {
    let bytes = html.into_bytes();
    let builder = Response::builder()
        .status(StatusCode::OK)
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
