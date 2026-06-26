use maud::{Markup, html};

use crate::desktop::DesktopInfoResponse;

pub(super) fn desktop_workspace(
    info: &DesktopInfoResponse,
    websocket_token: Option<&str>,
) -> Markup {
    let view_only = if info.view_only { "true" } else { "false" };

    if let Some(token) = websocket_token {
        html! {
            (workspace_markup(info, view_only, Some(token)))
        }
    } else {
        html! {
            (workspace_markup(info, view_only, None))
        }
    }
}

fn workspace_markup(
    info: &DesktopInfoResponse,
    view_only: &str,
    websocket_token: Option<&str>,
) -> Markup {
    let screen_layout = serde_json::to_string(&info.screens).unwrap_or_else(|_| "[]".to_string());
    let resolution_options =
        serde_json::to_string(&info.resolutions).unwrap_or_else(|_| "[]".to_string());
    let action_path = info
        .websocket_href
        .strip_suffix("/ws")
        .unwrap_or(&info.websocket_href);

    html! {
        section
            class="desktop-workspace"
            data-desktop-workspace
            data-action-path=(action_path)
            data-ws-path=(&info.websocket_href)
            data-view-only=(view_only)
            data-screen-layout=(screen_layout)
            data-resolution-options=(resolution_options)
            data-ws-token=[websocket_token] {
            div class="desktop-toolbar" {
                div class="desktop-mode" {
                    @if info.view_only {
                        "View only"
                    } @else {
                        "Control enabled"
                    }
                }
                div class="desktop-screen-switcher" data-desktop-screens hidden aria-label="Screens" {}
                select class="desktop-resolution-select" data-desktop-resolution hidden aria-label="Resolution" title="Change desktop resolution" {}
                button class="desktop-control-button active" data-desktop-scale type="button" aria-pressed="true" title="Toggle auto scale" { "Fit" }
                button class="desktop-control-button" data-desktop-fullscreen type="button" aria-pressed="false" title="Toggle fullscreen" { "Full" }
                div class="desktop-status" data-desktop-status { "Connecting" }
            }
            div class="desktop-frame" {
                form class="desktop-credentials" data-desktop-credentials hidden {
                    label data-desktop-credential-user hidden {
                        "Username"
                        input type="text" autocomplete="username";
                    }
                    label data-desktop-credential-password {
                        "Password"
                        input type="password" autocomplete="current-password";
                    }
                    label data-desktop-credential-target hidden {
                        "Target"
                        input type="text";
                    }
                    button type="submit" { "Connect" }
                }
                div class="desktop-target" data-desktop-target tabindex="0" {}
            }
        }
    }
}
