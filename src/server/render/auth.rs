use maud::html;

use super::super::{assets::AUTH_PAGE_STYLE_HREF, constants::LOGIN_PATH, html as html_page};

pub(in crate::server) fn render_public_login(
    next: &str,
    login_failed: bool,
    device_hostname: &str,
) -> String {
    render_login_page(
        "Sign in - Latitude",
        LOGIN_PATH,
        next,
        login_failed,
        "Sign in to",
        "Sign in",
        device_hostname,
    )
}

pub(in crate::server) fn render_share_login(
    action: &str,
    next: &str,
    login_failed: bool,
    device_hostname: &str,
) -> String {
    render_login_page(
        "Open share link - Latitude",
        action,
        next,
        login_failed,
        "Open shared deployment on",
        "Open share",
        device_hostname,
    )
}

fn render_login_page(
    title: &str,
    action: &str,
    next: &str,
    login_failed: bool,
    prompt: &str,
    submit_label: &str,
    device_hostname: &str,
) -> String {
    html_page::document(
        title,
        device_hostname,
        AUTH_PAGE_STYLE_HREF,
        html! {},
        html! {
            main {
                header { h1 { "Latitude" } p { (prompt) " " (device_hostname) } }
                @if login_failed { div class="error" { "Incorrect password." } }
                form method="post" action=(action) {
                    input type="hidden" name="next" value=(next);
                    label {
                        "Password"
                        input name="password" type="password" required autofocus autocomplete="current-password";
                    }
                    button type="submit" { (submit_label) }
                }
            }
        },
    )
}
