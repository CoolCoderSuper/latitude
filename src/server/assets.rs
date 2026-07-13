use axum::{
    body::Body,
    extract::Path as AxumPath,
    http::{HeaderMap, Response, StatusCode, header},
    response::IntoResponse,
};
use sha2::{Digest, Sha256};

pub(super) const ASSET_BASE_PATH: &str = "/__latitude/assets";

macro_rules! asset_href {
    ($name:literal) => {
        concat!("/__latitude/assets/", $name, "?v=2")
    };
}

pub(super) const COMMON_THEME_STYLE_HREF: &str = asset_href!("common-theme.css");
pub(super) const THEME_BOOTSTRAP_SCRIPT_SRC: &str = asset_href!("theme-bootstrap.js");
pub(super) const THEME_TOGGLE_SCRIPT_SRC: &str = asset_href!("theme-toggle.js");
pub(super) const HTMX_SCRIPT_SRC: &str = asset_href!("htmx.min.js");
pub(super) const AUTH_PAGE_STYLE_HREF: &str = asset_href!("auth.css");
pub(super) const PROJECT_HOME_STYLE_HREF: &str = asset_href!("project-home.css");
pub(super) const PROJECT_HOME_SCRIPT_SRC: &str = asset_href!("project-home.js");
pub(super) const DIFF_VIEWER_STYLE_HREF: &str = asset_href!("diff-viewer.css");
pub(super) const DIFF_VIEWER_SCRIPT_SRC: &str = asset_href!("diff-viewer.js");
pub(super) const FILE_VIEWER_STYLE_HREF: &str = asset_href!("file-viewer.css");
pub(super) const FILE_VIEWER_SCRIPT_SRC: &str = asset_href!("file-viewer.js");
pub(super) const TERMINAL_VIEWER_STYLE_HREF: &str = asset_href!("terminal-viewer.css");
pub(super) const TERMINAL_VIEWER_SCRIPT_SRC: &str = asset_href!("terminal-viewer.js");
pub(super) const DESKTOP_VIEWER_STYLE_HREF: &str = asset_href!("desktop-viewer.css");
pub(super) const DESKTOP_VIEWER_SCRIPT_SRC: &str = asset_href!("desktop-viewer.js");
pub(super) const PAGE_STYLE_HREF: &str = asset_href!("page.css");

struct EmbeddedAsset {
    content_type: &'static str,
    bytes: &'static [u8],
}

pub(super) async fn public_asset(
    AxumPath(name): AxumPath<String>,
    headers: HeaderMap,
) -> Response<Body> {
    let Some(asset) = embedded_asset(&name) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let etag = format!("\"{:x}\"", Sha256::digest(asset.bytes));
    let not_modified = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.split(',').any(|candidate| candidate.trim() == etag));
    let builder = Response::builder()
        .status(if not_modified {
            StatusCode::NOT_MODIFIED
        } else {
            StatusCode::OK
        })
        .header(header::CONTENT_TYPE, asset.content_type)
        .header(header::CACHE_CONTROL, "public, no-cache")
        .header(header::ETAG, etag)
        .header("x-content-type-options", "nosniff");

    if not_modified {
        builder.body(Body::empty()).expect("static asset response")
    } else {
        builder
            .header(header::CONTENT_LENGTH, asset.bytes.len())
            .body(Body::from(asset.bytes))
            .expect("static asset response")
    }
}

fn embedded_asset(name: &str) -> Option<EmbeddedAsset> {
    let (content_type, bytes): (&str, &[u8]) = match name {
        "common-theme.css" => (
            "text/css; charset=utf-8",
            include_bytes!("assets/common-theme.css"),
        ),
        "theme-bootstrap.js" => (
            "text/javascript; charset=utf-8",
            include_bytes!("assets/theme-bootstrap.js"),
        ),
        "theme-toggle.js" => (
            "text/javascript; charset=utf-8",
            include_bytes!("assets/theme-toggle.js"),
        ),
        "htmx.min.js" => (
            "text/javascript; charset=utf-8",
            include_bytes!("assets/htmx.min.js"),
        ),
        "auth.css" => ("text/css; charset=utf-8", include_bytes!("assets/auth.css")),
        "project-home.css" => (
            "text/css; charset=utf-8",
            include_bytes!("assets/project-home.css"),
        ),
        "project-home.js" => (
            "text/javascript; charset=utf-8",
            include_bytes!("assets/project-home.js"),
        ),
        "diff-viewer.css" => (
            "text/css; charset=utf-8",
            include_bytes!("assets/diff-viewer.css"),
        ),
        "diff-viewer.js" => (
            "text/javascript; charset=utf-8",
            include_bytes!("assets/diff-viewer.js"),
        ),
        "file-viewer.css" => (
            "text/css; charset=utf-8",
            include_bytes!("assets/file-viewer.css"),
        ),
        "file-viewer.js" => (
            "text/javascript; charset=utf-8",
            include_bytes!("assets/file-viewer.js"),
        ),
        "terminal-viewer.css" => (
            "text/css; charset=utf-8",
            include_bytes!("assets/terminal-viewer.css"),
        ),
        "terminal-viewer.js" => (
            "text/javascript; charset=utf-8",
            include_bytes!("assets/terminal-viewer.js"),
        ),
        "desktop-viewer.css" => (
            "text/css; charset=utf-8",
            include_bytes!("assets/desktop-viewer.css"),
        ),
        "desktop-viewer.js" => (
            "text/javascript; charset=utf-8",
            include_bytes!("assets/desktop-viewer.js"),
        ),
        "page.css" => ("text/css; charset=utf-8", include_bytes!("assets/page.css")),
        _ => return None,
    };
    Some(EmbeddedAsset {
        content_type,
        bytes,
    })
}

#[cfg(test)]
pub(super) fn embedded_asset_names() -> &'static [&'static str] {
    &[
        "common-theme.css",
        "theme-bootstrap.js",
        "theme-toggle.js",
        "htmx.min.js",
        "auth.css",
        "project-home.css",
        "project-home.js",
        "diff-viewer.css",
        "diff-viewer.js",
        "file-viewer.css",
        "file-viewer.js",
        "terminal-viewer.css",
        "terminal-viewer.js",
        "desktop-viewer.css",
        "desktop-viewer.js",
        "page.css",
    ]
}
