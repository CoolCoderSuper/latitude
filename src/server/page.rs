use axum::http::{HeaderMap, StatusCode};
use maud::{Markup, PreEscaped, html};
use pulldown_cmark::{Options, Parser, html::push_html};
use serde::Deserialize;

use crate::config::{PageFormat, encode_page_binary_content, is_binary_document_media_type};

use super::{
    assets::PAGE_STYLE,
    constants::{DEFAULT_PAGE_TITLE, LATITUDE_THEME_HEADER},
    html as html_page,
    response::ApiError,
};

#[derive(Debug, Clone, Copy)]
pub(super) struct PageDocumentShell<'a> {
    pub(super) project_name: &'a str,
    pub(super) raw_href: Option<&'a str>,
}

#[derive(Debug)]
pub(super) struct PagePayload {
    pub(super) content: String,
    pub(super) format: PageFormat,
    pub(super) media_type: Option<String>,
    pub(super) title: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct JsonPagePayload {
    pub(super) content: String,
    #[serde(default)]
    pub(super) format: Option<PageFormat>,
    #[serde(default)]
    pub(super) media_type: Option<String>,
    #[serde(default)]
    pub(super) title: Option<String>,
}

pub(super) fn parse_page_payload(
    content_type: Option<&str>,
    body: &[u8],
) -> Result<PagePayload, ApiError> {
    let media_type = content_type_media_type(content_type);

    if media_type.as_deref().is_some_and(is_json_media_type) {
        let payload: JsonPagePayload = serde_json::from_slice(body).map_err(|error| {
            ApiError::new(
                StatusCode::BAD_REQUEST,
                format!("page JSON payload is invalid: {error}"),
            )
        })?;
        let title = clean_page_title(payload.title);
        let media_type = clean_page_media_type(payload.media_type);
        let format = payload
            .format
            .unwrap_or_else(|| infer_page_format(media_type.as_deref(), &payload.content));

        return Ok(PagePayload {
            content: payload.content,
            format,
            media_type,
            title,
        });
    }

    if let Some(media_type) = media_type
        .as_deref()
        .filter(|media_type| is_binary_document_media_type(media_type))
    {
        return Ok(PagePayload {
            content: encode_page_binary_content(body),
            format: PageFormat::Binary,
            media_type: Some(media_type.to_string()),
            title: None,
        });
    }

    let content = std::str::from_utf8(body)
        .map_err(|error| {
            ApiError::new(
                StatusCode::BAD_REQUEST,
                format!("page payload must be UTF-8 text: {error}"),
            )
        })?
        .to_string();
    let format = infer_page_format(media_type.as_deref(), &content);

    Ok(PagePayload {
        content,
        format,
        media_type: None,
        title: None,
    })
}

pub(super) fn content_type_media_type(content_type: Option<&str>) -> Option<String> {
    content_type
        .and_then(|value| value.split(';').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_lowercase)
}

pub(super) fn is_json_media_type(media_type: &str) -> bool {
    media_type == "application/json" || media_type.ends_with("+json")
}

fn clean_page_title(title: Option<String>) -> Option<String> {
    title
        .map(|title| title.trim().to_string())
        .filter(|title| !title.is_empty())
}

fn clean_page_media_type(media_type: Option<String>) -> Option<String> {
    media_type.and_then(|media_type| content_type_media_type(Some(&media_type)))
}

fn infer_page_format(media_type: Option<&str>, content: &str) -> PageFormat {
    match media_type {
        Some(media_type) if is_binary_document_media_type(media_type) => PageFormat::Binary,
        Some("text/html") | Some("application/xhtml+xml") => PageFormat::Html,
        Some("text/markdown") | Some("text/x-markdown") | Some("text/md") => PageFormat::Markdown,
        _ if looks_like_html(content) => PageFormat::Html,
        _ => PageFormat::Markdown,
    }
}

#[cfg(test)]
pub(super) fn render_page_content(
    title: Option<&str>,
    format: PageFormat,
    content: &str,
    theme: Option<&str>,
    device_hostname: &str,
) -> String {
    render_page_document(title, format, None, content, theme, None, device_hostname)
}

pub(super) fn render_project_page_content(
    project_name: &str,
    title: Option<&str>,
    format: PageFormat,
    media_type: Option<&str>,
    content: &str,
    theme: Option<&str>,
    device_hostname: &str,
) -> String {
    render_page_document(
        title,
        format,
        media_type,
        content,
        theme,
        Some(PageDocumentShell {
            project_name,
            raw_href: (format == PageFormat::Binary).then_some("?raw=1"),
        }),
        device_hostname,
    )
}

fn render_page_document(
    title: Option<&str>,
    format: PageFormat,
    media_type: Option<&str>,
    content: &str,
    theme: Option<&str>,
    shell: Option<PageDocumentShell<'_>>,
    device_hostname: &str,
) -> String {
    match format {
        PageFormat::Html if is_full_html_document(content) => content.to_string(),
        PageFormat::Html => wrap_page_document(
            resolved_page_title(title, None),
            html! { (PreEscaped(content)) },
            theme,
            shell,
            device_hostname,
        ),
        PageFormat::Markdown => {
            let html = render_markdown(content);
            wrap_page_document(
                resolved_page_title(title, markdown_heading_title(content)),
                html! { (PreEscaped(html)) },
                theme,
                shell,
                device_hostname,
            )
        }
        PageFormat::Binary => {
            let title = resolved_page_title(title, None);
            wrap_page_document(
                title,
                render_binary_document(title, media_type, shell.and_then(|shell| shell.raw_href)),
                theme,
                shell,
                device_hostname,
            )
        }
    }
}

pub(super) fn page_theme_from_headers(headers: &HeaderMap) -> Option<&'static str> {
    headers
        .get(LATITUDE_THEME_HEADER)
        .and_then(|value| value.to_str().ok())
        .and_then(clean_page_theme)
}

pub(super) fn clean_page_theme(theme: &str) -> Option<&'static str> {
    match theme.trim() {
        "light" => Some("light"),
        "dark" => Some("dark"),
        _ => None,
    }
}

fn render_markdown(content: &str) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);

    let parser = Parser::new_ext(content, options);
    let mut output = String::new();
    push_html(&mut output, parser);
    output
}

fn render_binary_document(title: &str, media_type: Option<&str>, raw_href: Option<&str>) -> Markup {
    let Some(raw_href) = raw_href else {
        return html! {};
    };

    match media_type {
        Some(media_type) if is_image_media_type(media_type) => html! {
            figure class="latitude-media-document" {
                img src=(raw_href) alt=(title);
            }
        },
        Some(media_type) if is_video_media_type(media_type) => html! {
            figure class="latitude-media-document" {
                video controls preload="metadata" src=(raw_href) {
                    "Your browser cannot play this video."
                }
            }
        },
        _ => html! {
            p {
                a href=(raw_href) { "Open media document" }
            }
        },
    }
}

fn wrap_page_document(
    title: &str,
    body: Markup,
    theme: Option<&str>,
    shell: Option<PageDocumentShell<'_>>,
    device_hostname: &str,
) -> String {
    html_page::document_with_theme(
        title,
        device_hostname,
        PAGE_STYLE,
        theme.and_then(clean_page_theme),
        html! {},
        html! {
            @if let Some(shell) = shell {
                header class="latitude-page-header" {
                    a href=(format!("/{}", shell.project_name)) { "Back to project" }
                    p class="latitude-page-hostname" { (shell.project_name) " on " (device_hostname) }
                }
            }
            main class="latitude-page" {
                (body)
            }
        },
    )
}

fn resolved_page_title<'a>(explicit: Option<&'a str>, derived: Option<&'a str>) -> &'a str {
    explicit
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .or_else(|| derived.map(str::trim).filter(|title| !title.is_empty()))
        .unwrap_or(DEFAULT_PAGE_TITLE)
}

fn markdown_heading_title(content: &str) -> Option<&str> {
    for line in content.lines() {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix("# ") else {
            continue;
        };
        let title = rest.trim().trim_end_matches('#').trim();
        if !title.is_empty() {
            return Some(title);
        }
    }

    None
}

fn is_full_html_document(content: &str) -> bool {
    let trimmed = content.trim_start().to_ascii_lowercase();
    trimmed.starts_with("<!doctype html") || trimmed.starts_with("<html")
}

fn looks_like_html(content: &str) -> bool {
    let trimmed = content.trim_start();
    trimmed.starts_with('<') && trimmed.contains('>')
}

fn is_image_media_type(media_type: &str) -> bool {
    media_type.starts_with("image/")
}

fn is_video_media_type(media_type: &str) -> bool {
    media_type.starts_with("video/")
}
