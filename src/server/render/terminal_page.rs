use maud::html;

use crate::config::ProjectConfig;

use super::{
    super::{
        assets::{TERMINAL_VIEWER_SCRIPT_SRC, TERMINAL_VIEWER_STYLE_HREF},
        constants::{TERMINAL_ROUTE_SEGMENT, TERMINAL_WS_SUFFIX},
        html as html_page,
        terminal_api::PublicTerminalInfoResponse,
    },
    terminal::terminal_workspace,
};

pub(in crate::server) fn render_project_terminal(
    project: &ProjectConfig,
    info: &PublicTerminalInfoResponse,
    websocket_token: Option<&str>,
    device_hostname: &str,
) -> String {
    let websocket_path = format!(
        "/{}/{}/{}",
        project.name, TERMINAL_ROUTE_SEGMENT, TERMINAL_WS_SUFFIX
    );
    render_terminal_page(TerminalPage {
        title: &format!("{} terminal - Latitude", project.name),
        heading: "Terminal",
        back_href: &format!("/{}", project.name),
        back_label: "Back to project",
        description: &format!("{} on {device_hostname}", project.name),
        info,
        websocket_path: &websocket_path,
        websocket_token,
        device_hostname,
    })
}

pub(in crate::server) fn render_root_terminal(
    info: &PublicTerminalInfoResponse,
    websocket_token: Option<&str>,
    device_hostname: &str,
) -> String {
    let websocket_path = format!("/{TERMINAL_ROUTE_SEGMENT}/{TERMINAL_WS_SUFFIX}");
    render_terminal_page(TerminalPage {
        title: "Root terminal - Latitude",
        heading: "Root Terminal",
        back_href: "/",
        back_label: "Back to projects",
        description: &format!("User directory on {device_hostname}"),
        info,
        websocket_path: &websocket_path,
        websocket_token,
        device_hostname,
    })
}

struct TerminalPage<'a> {
    title: &'a str,
    heading: &'a str,
    back_href: &'a str,
    back_label: &'a str,
    description: &'a str,
    info: &'a PublicTerminalInfoResponse,
    websocket_path: &'a str,
    websocket_token: Option<&'a str>,
    device_hostname: &'a str,
}

fn render_terminal_page(page: TerminalPage<'_>) -> String {
    html_page::document(
        page.title,
        page.device_hostname,
        TERMINAL_VIEWER_STYLE_HREF,
        html! { link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/@xterm/xterm@5.5.0/css/xterm.css"; },
        html! {
            main {
                header {
                    a href=(page.back_href) { (page.back_label) }
                    h1 { (page.heading) }
                    p { (page.description) }
                    p class="project-path" { (&page.info.cwd) }
                }
                (terminal_workspace(page.info, page.websocket_path, page.websocket_token))
                script src="https://cdn.jsdelivr.net/npm/@xterm/xterm@5.5.0/lib/xterm.min.js" {}
                script src="https://cdn.jsdelivr.net/npm/@xterm/addon-fit@0.10.0/lib/addon-fit.min.js" {}
                script src=(TERMINAL_VIEWER_SCRIPT_SRC) {}
            }
        },
    )
}
