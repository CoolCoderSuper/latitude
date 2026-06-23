use std::path::{Component, Path, PathBuf};

use axum::http::HeaderValue;
use percent_encoding::percent_decode_str;

#[derive(Debug, PartialEq, Eq)]
pub(super) enum ProjectPath {
    Project {
        project: String,
    },
    Deployment {
        project: String,
        deployment: String,
        remainder: String,
    },
}

impl ProjectPath {
    pub(super) fn project_name(&self) -> &str {
        match self {
            Self::Project { project } | Self::Deployment { project, .. } => project,
        }
    }
}

pub(super) fn resolve_project_path(project_dir: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        project_dir.join(path)
    }
}

pub(super) fn split_project_path(path: &str) -> Option<ProjectPath> {
    let path = path.trim_start_matches('/');
    if path.is_empty() {
        return None;
    }

    let mut segments = path.splitn(3, '/');
    let project = segments.next()?.to_string();
    if project.is_empty() {
        return None;
    }

    let Some(deployment) = segments.next() else {
        return Some(ProjectPath::Project { project });
    };
    if deployment.is_empty() {
        return if segments.next().is_some() {
            None
        } else {
            Some(ProjectPath::Project { project })
        };
    }

    let remainder = segments
        .next()
        .map(|rest| format!("/{rest}"))
        .unwrap_or_else(|| "/".to_string());

    Some(ProjectPath::Deployment {
        project,
        deployment: deployment.to_string(),
        remainder,
    })
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

pub(super) fn is_hop_by_hop_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
    )
}

pub(super) fn filtered_cookie_header(value: &HeaderValue, excluded_name: &str) -> Option<String> {
    let raw = value.to_str().ok()?;
    let cookies = raw
        .split(';')
        .filter_map(|cookie| {
            let cookie = cookie.trim();
            let (name, _) = cookie.split_once('=')?;
            if name.trim() == excluded_name {
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
    path.strip_prefix(r"\\?\").unwrap_or(&path).to_string()
}
