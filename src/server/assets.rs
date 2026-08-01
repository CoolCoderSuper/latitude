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
        concat!("/__latitude/assets/", $name)
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
pub(super) const FILE_VIEWER_SCRIPT_SRC: &str = asset_href!("file-viewer.bundle.js");
pub(super) const TERMINAL_VIEWER_STYLE_HREF: &str = asset_href!("terminal-viewer.bundle.css");
pub(super) const TERMINAL_VIEWER_SCRIPT_SRC: &str = asset_href!("terminal-viewer.bundle.js");
pub(super) const DESKTOP_VIEWER_STYLE_HREF: &str = asset_href!("desktop-viewer.css");
pub(super) const DESKTOP_VIEWER_SCRIPT_SRC: &str = asset_href!("desktop-viewer.js");
pub(super) const PAGE_STYLE_HREF: &str = asset_href!("page.css");

struct EmbeddedAsset {
    name: &'static str,
    content_type: &'static str,
    bytes: &'static [u8],
}

macro_rules! embedded_assets {
    ($(($name:literal, $content_type:literal)),+ $(,)?) => {
        const EMBEDDED_ASSETS: &[EmbeddedAsset] = &[
            $(EmbeddedAsset {
                name: $name,
                content_type: $content_type,
                bytes: include_bytes!(concat!("assets/", $name)),
            }),+
        ];
    };
}

embedded_assets!(
    ("common-theme.css", "text/css; charset=utf-8"),
    ("theme-bootstrap.js", "text/javascript; charset=utf-8"),
    ("theme-toggle.js", "text/javascript; charset=utf-8"),
    ("htmx.min.js", "text/javascript; charset=utf-8"),
    ("auth.css", "text/css; charset=utf-8"),
    ("project-home.css", "text/css; charset=utf-8"),
    ("project-home.js", "text/javascript; charset=utf-8"),
    ("polling.js", "text/javascript; charset=utf-8"),
    ("diff-viewer.css", "text/css; charset=utf-8"),
    ("diff-viewer.js", "text/javascript; charset=utf-8"),
    ("file-viewer.css", "text/css; charset=utf-8"),
    ("file-viewer.bundle.js", "text/javascript; charset=utf-8"),
    ("terminal-viewer.bundle.css", "text/css; charset=utf-8"),
    (
        "terminal-viewer.bundle.js",
        "text/javascript; charset=utf-8"
    ),
    ("desktop-viewer.css", "text/css; charset=utf-8"),
    ("desktop-viewer.js", "text/javascript; charset=utf-8"),
    ("desktop-input.js", "text/javascript; charset=utf-8"),
    ("desktop-peer.js", "text/javascript; charset=utf-8"),
    ("page.css", "text/css; charset=utf-8"),
);

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

fn embedded_asset(name: &str) -> Option<&'static EmbeddedAsset> {
    EMBEDDED_ASSETS.iter().find(|asset| asset.name == name)
}

#[cfg(test)]
pub(super) fn embedded_asset_names() -> impl Iterator<Item = &'static str> {
    EMBEDDED_ASSETS.iter().map(|asset| asset.name)
}
