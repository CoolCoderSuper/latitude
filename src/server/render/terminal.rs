use maud::{Markup, html};

use super::super::terminal_api::PublicTerminalInfoResponse;

pub(super) fn terminal_workspace(
    info: &PublicTerminalInfoResponse,
    websocket_path: &str,
    websocket_token: Option<&str>,
) -> Markup {
    if let Some(token) = websocket_token {
        html! {
            section class="terminal-workspace" data-terminal-workspace data-sessions-path=(&info.sessions_href) data-ws-path=(websocket_path) data-ws-token=(token) {
                (terminal_workspace_inner())
            }
        }
    } else {
        html! {
            section class="terminal-workspace" data-terminal-workspace data-sessions-path=(&info.sessions_href) data-ws-path=(websocket_path) {
                (terminal_workspace_inner())
            }
        }
    }
}

fn terminal_workspace_inner() -> Markup {
    html! {
        div class="action-status" data-terminal-status hidden {}
        div class="terminal-session-bar" {
            div class="terminal-session-list" data-terminal-sessions {}
            button class="terminal-new-button" type="button" data-terminal-new aria-label="New terminal" title="New terminal" { "+" }
        }
        div class="terminal-stack" data-terminal-stack {
            div class="terminal-empty" data-terminal-empty hidden {
                "No terminals. Use + to create one."
            }
        }
    }
}
