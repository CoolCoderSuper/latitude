use std::path::Path;

use axum::{
    body::Body,
    http::{Method, Request, Response, StatusCode, header},
};
use tokio::fs;

use crate::{
    config::{
        ApplicationConfig, ApplicationTarget, PageFormat, ProjectConfig,
        is_binary_document_media_type,
    },
    http_stream::file_response,
    state::AppState,
    storage::PageContent,
};

use super::super::{
    constants::AUTH_COOKIE_NAME,
    page::{page_theme_from_headers, render_project_page_content},
    paths::{join_upstream_url, resolve_project_path, sanitized_relative_path},
    proxy::{HttpProxyError, proxy_http},
    response::{html_response, internal_response, json_error, plain_response},
};

pub(super) struct DeploymentRequest<'a> {
    pub(super) project: &'a ProjectConfig,
    pub(super) deployment: &'a ApplicationConfig,
    pub(super) remainder: &'a str,
    pub(super) mount_path: &'a str,
    pub(super) extra_excluded_cookie_name: Option<&'a str>,
    pub(super) device_hostname: &'a str,
}

pub(super) async fn serve_deployment_target(
    state: AppState,
    req: Request<Body>,
    target: DeploymentRequest<'_>,
) -> Response<Body> {
    match &target.deployment.target {
        ApplicationTarget::ReverseProxy {
            upstream,
            strip_prefix,
        } => {
            proxy_request(
                state,
                req,
                upstream,
                *strip_prefix,
                target.remainder,
                target.mount_path,
                target.extra_excluded_cookie_name,
            )
            .await
        }
        ApplicationTarget::Static {
            root,
            index_file,
            spa_fallback,
        } => {
            let root = resolve_project_path(&target.project.project_dir, root);
            serve_static(
                req,
                StaticDeployment {
                    project_name: &target.project.name,
                    deployment_name: &target.deployment.name,
                    root: &root,
                    index_file,
                    spa_fallback: *spa_fallback,
                    remainder: target.remainder,
                    device_hostname: target.device_hostname,
                },
            )
            .await
        }
        ApplicationTarget::Page { .. } => {
            match state
                .catalog()
                .get_page_content(&target.project.name, &target.deployment.name)
                .await
            {
                Ok(Some(content)) => serve_page(
                    req,
                    &target.project.name,
                    content,
                    target.remainder,
                    target.device_hostname,
                ),
                Ok(None) => plain_response(
                    StatusCode::NOT_FOUND,
                    format!(
                        "page deployment '{}' was not found in project '{}'\n",
                        target.deployment.name, target.project.name
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
    let forward_path = if strip_prefix {
        remainder.to_string()
    } else {
        format!("{}{}", mount_path.trim_end_matches('/'), remainder)
    };

    let target_url = match join_upstream_url(upstream, &forward_path, req.uri().query()) {
        Ok(url) => url,
        Err(error) => {
            return json_error(
                StatusCode::BAD_GATEWAY,
                format!("upstream URL could not be built: {error}"),
            );
        }
    };

    let mut excluded_cookie_names = vec![AUTH_COOKIE_NAME];
    excluded_cookie_names.extend(extra_excluded_cookie_name);
    match proxy_http(state.client(), req, target_url, &excluded_cookie_names).await {
        Ok(response) => response,
        Err(HttpProxyError::Request(error)) => json_error(
            StatusCode::BAD_GATEWAY,
            format!("upstream request failed: {error}"),
        ),
        Err(HttpProxyError::Timeout) => json_error(
            StatusCode::GATEWAY_TIMEOUT,
            "upstream did not return response headers within 60 seconds",
        ),
    }
}

struct StaticDeployment<'a> {
    project_name: &'a str,
    deployment_name: &'a str,
    root: &'a Path,
    index_file: &'a str,
    spa_fallback: bool,
    remainder: &'a str,
    device_hostname: &'a str,
}

async fn serve_static(req: Request<Body>, target: StaticDeployment<'_>) -> Response<Body> {
    if req.method() != Method::GET && req.method() != Method::HEAD {
        return plain_response(
            StatusCode::METHOD_NOT_ALLOWED,
            "static deployments support GET and HEAD\n",
        );
    }

    if target.remainder == "/"
        && !page_raw_requested(req.uri().query())
        && let Some(media_type) = static_document_media_type(target.index_file)
    {
        return html_response(
            req.method(),
            render_project_page_content(
                target.project_name,
                Some(target.deployment_name),
                PageFormat::Binary,
                Some(&media_type),
                "",
                page_theme_from_headers(req.headers()),
                target.device_hostname,
            ),
        );
    }

    let relative_path = match sanitized_relative_path(target.remainder) {
        Some(path) => path,
        None => return plain_response(StatusCode::BAD_REQUEST, "invalid static path\n"),
    };

    let mut candidate = target.root.join(relative_path);
    match fs::metadata(&candidate).await {
        Ok(metadata) if metadata.is_dir() => {
            candidate = candidate.join(target.index_file);
        }
        Ok(_) => {}
        Err(_) if target.spa_fallback => {
            candidate = target.root.join(target.index_file);
        }
        Err(_) => return plain_response(StatusCode::NOT_FOUND, "file not found\n"),
    }

    match fs::metadata(&candidate).await {
        Ok(metadata) if metadata.is_file() => {}
        _ if target.spa_fallback => match fs::metadata(target.root.join(target.index_file)).await {
            Ok(metadata) if metadata.is_file() => {
                candidate = target.root.join(target.index_file);
            }
            _ => return plain_response(StatusCode::NOT_FOUND, "file not found\n"),
        },
        _ => return plain_response(StatusCode::NOT_FOUND, "file not found\n"),
    }

    let content_type = mime_guess::from_path(&candidate)
        .first_or_octet_stream()
        .to_string();

    match file_response(
        req.method(),
        req.headers()
            .get(header::RANGE)
            .and_then(|value| value.to_str().ok()),
        &candidate,
        &content_type,
    )
    .await
    {
        Ok(response) => response,
        Err(error) => json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("file could not be read: {error}"),
        ),
    }
}

fn serve_page(
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
