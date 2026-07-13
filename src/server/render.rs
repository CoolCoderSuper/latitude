mod auth;
mod desktop;
mod diff;
mod files;
mod home;
mod share;
mod syntax;
mod terminal;
mod terminal_page;

pub(super) use auth::{render_public_login, render_share_login};
pub(super) use diff::{
    render_diff_file_update, render_diff_workspace_fragment, render_project_diff,
};
pub(super) use files::render_project_files;
pub(super) use home::{render_project_home, render_server_home};
pub(super) use share::render_share_dialog_shell;
pub(super) use terminal_page::{render_project_terminal, render_root_terminal};

#[cfg(test)]
pub(super) use syntax::{
    HighlightedDiffLine, diff_line_class, highlight_diff_lines, highlight_source_lines,
    render_diff_code_output, syntax_name_for_path,
};

#[cfg(not(test))]
pub(super) use syntax::{HighlightedDiffLine, highlight_diff_lines, highlight_source_lines};

use maud::html;

use crate::desktop::DesktopInfoResponse;

use super::{
    assets::{DESKTOP_VIEWER_SCRIPT_SRC, DESKTOP_VIEWER_STYLE_HREF},
    html as html_page,
};

pub(super) fn render_root_desktop(
    info: &DesktopInfoResponse,
    websocket_token: Option<&str>,
    device_hostname: &str,
) -> String {
    html_page::document(
        &format!("{} - Latitude", info.label),
        device_hostname,
        DESKTOP_VIEWER_STYLE_HREF,
        html! {},
        html! {
            main {
                header {
                    a href="/" { "Back to projects" }
                    h1 { (&info.label) }
                    p { "Desktop on " (device_hostname) }
                }
                (desktop::desktop_workspace(info, websocket_token))
                script type="module" src=(DESKTOP_VIEWER_SCRIPT_SRC) {}
            }
        },
    )
}
