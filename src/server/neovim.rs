use axum::{
    body::Body,
    extract::{Path, State, WebSocketUpgrade},
    http::{HeaderMap, Method, Response, StatusCode},
    response::IntoResponse,
};
use maud::html;

use super::{
    html as html_page,
    public::enabled_project,
    response::{ApiError, html_response},
};
use crate::state::AppState;

pub(super) async fn page(
    Path(name): Path<String>,
    State(state): State<AppState>,
    method: Method,
) -> Result<Response<Body>, ApiError> {
    let project = enabled_project(&state, &name).await?;
    Ok(html_response(
        &method,
        html_page::document(
            &format!("{} Neovim - Latitude", project.name),
            state.device_hostname(),
            "/__latitude/assets/neovim.css",
            html! {},
            html! {
                main class="project-tool-page" data-neovim data-ws-path=(format!("/{}/_neovim/ws", project.name)) {
                    (html_page::page_header(html_page::PageHeader {
                        class_name: Some("project-tool-header"), back_href: &format!("/{}", project.name),
                        back_label: "Back to project", heading: "Neovim",
                        description: &format!("{} on {}", project.name, state.device_hostname()),
                        path: Some(&super::paths::display_path(&project.project_dir)),
                    }))
                    div class="neovim-editor" data-neovim-editor {
                        canvas data-neovim-canvas aria-label="Neovim editor" {}
                        textarea data-neovim-input aria-label="Neovim keyboard input" autocomplete="off" autocapitalize="off" spellcheck="false" {}
                        div class="neovim-notice" data-neovim-notice {
                            span role="status" data-neovim-status { "Connecting to Neovim…" }
                            button type="button" data-neovim-restart hidden { "Reconnect" }
                            button type="button" data-neovim-dismiss hidden { "Dismiss" }
                        }
                    }
                    script type="module" src="/__latitude/assets/neovim.bundle.js" {}
                }
            },
        ),
    ))
}

pub(super) async fn websocket(
    Path(name): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Result<Response<Body>, ApiError> {
    // Cookie authentication is enforced by public_pages_router. Reject cross-origin
    // upgrades too, since an editor can execute commands with the user's privileges.
    if !same_origin(&headers) {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "Neovim requires a same-origin connection",
        ));
    }
    let project = enabled_project(&state, &name).await?;
    Ok(ws
        .max_message_size(1024 * 1024)
        .on_upgrade(move |socket| async move {
            state
                .workspace()
                .run_neovim(socket, project.project_dir)
                .await;
        })
        .into_response())
}

fn same_origin(headers: &HeaderMap) -> bool {
    headers
        .get("origin")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| url::Url::parse(v).ok())
        .zip(headers.get("host").and_then(|v| v.to_str().ok()))
        .is_some_and(|(origin, host)| {
            matches!(origin.scheme(), "http" | "https")
                && origin[url::Position::BeforeHost..url::Position::AfterPort]
                    .eq_ignore_ascii_case(host)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neovim_rejects_cross_origin_upgrades() {
        let mut headers = HeaderMap::new();
        headers.insert("host", "localhost:7601".parse().unwrap());
        assert!(!same_origin(&headers));
        for origin in ["null", "https://untrusted.example", "http://localhost:7602"] {
            headers.insert("origin", origin.parse().unwrap());
            assert!(!same_origin(&headers));
        }
        headers.insert("origin", "http://localhost:7601".parse().unwrap());
        assert!(same_origin(&headers));
    }
}
