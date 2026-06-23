use axum::http::{HeaderMap, StatusCode};
use maud::{PreEscaped, html};
use pulldown_cmark::{Options, Parser, html::push_html};
use serde::Deserialize;

use crate::config::PageFormat;

use super::{
    assets::PAGE_STYLE,
    constants::{DEFAULT_PAGE_TITLE, LATITUDE_THEME_HEADER},
    html as html_page,
    response::ApiError,
};

#[derive(Debug)]
pub(super) struct PagePayload {
    pub(super) content: String,
    pub(super) format: PageFormat,
    pub(super) title: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct JsonPagePayload {
    pub(super) content: String,
    #[serde(default)]
    pub(super) format: Option<PageFormat>,
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
        let format = payload
            .format
            .unwrap_or_else(|| infer_page_format(None, &payload.content));

        return Ok(PagePayload {
            content: payload.content,
            format,
            title,
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

fn infer_page_format(media_type: Option<&str>, content: &str) -> PageFormat {
    match media_type {
        Some("text/html") | Some("application/xhtml+xml") => PageFormat::Html,
        Some("text/markdown") | Some("text/x-markdown") | Some("text/md") => PageFormat::Markdown,
        _ if looks_like_html(content) => PageFormat::Html,
        _ => PageFormat::Markdown,
    }
}

pub(super) fn render_page_content(
    title: Option<&str>,
    format: PageFormat,
    content: &str,
    theme: Option<&str>,
) -> String {
    match format {
        PageFormat::Html if is_full_html_document(content) => content.to_string(),
        PageFormat::Html => wrap_page_document(resolved_page_title(title, None), content, theme),
        PageFormat::Markdown => {
            let html = render_markdown(content);
            wrap_page_document(
                resolved_page_title(title, markdown_heading_title(content)),
                &html,
                theme,
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

fn wrap_page_document(title: &str, body_html: &str, theme: Option<&str>) -> String {
    html_page::document_with_theme(
        title,
        PAGE_STYLE,
        theme.and_then(clean_page_theme),
        html! {},
        html! {
            main class="latitude-page" {
                (PreEscaped(body_html))
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
