use std::path::{Component, Path, PathBuf};

use crate::util::strip_windows_extended_path;
use percent_encoding::percent_decode_str;

pub(super) fn resolve_project_path(project_dir: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        project_dir.join(path)
    }
}

pub(super) fn join_upstream_url(
    upstream: &str,
    forward_path: &str,
    query: Option<&str>,
) -> Result<String, url::ParseError> {
    let path = if forward_path.starts_with('/') {
        forward_path.to_string()
    } else {
        format!("/{forward_path}")
    };

    let mut target = format!("{}{}", upstream.trim_end_matches('/'), path);
    if let Some(query) = query {
        target.push('?');
        target.push_str(query);
    }

    Ok(target.parse::<url::Url>()?.to_string())
}

pub(super) fn sanitized_relative_path(path: &str) -> Option<PathBuf> {
    let mut output = PathBuf::new();

    for raw_segment in path.trim_start_matches('/').split('/') {
        if raw_segment.is_empty() {
            continue;
        }

        let decoded = percent_decode_str(raw_segment).decode_utf8().ok()?;
        let segment_path = Path::new(decoded.as_ref());
        let mut components = segment_path.components();

        match (components.next(), components.next()) {
            (Some(Component::Normal(value)), None) => output.push(value),
            _ => return None,
        }
    }

    Some(output)
}

pub(super) fn filtered_cookie_header(
    value: &axum::http::HeaderValue,
    excluded_names: &[&str],
) -> Option<String> {
    let raw = value.to_str().ok()?;
    let cookies = raw
        .split(';')
        .filter_map(|cookie| {
            let cookie = cookie.trim();
            let (name, _) = cookie.split_once('=')?;
            if excluded_names.contains(&name.trim()) {
                None
            } else {
                Some(cookie.to_string())
            }
        })
        .collect::<Vec<_>>();

    if cookies.is_empty() {
        None
    } else {
        Some(cookies.join("; "))
    }
}

pub(super) fn display_path(path: &Path) -> String {
    let path = path.display().to_string();
    strip_windows_extended_path(&path).into_owned()
}
