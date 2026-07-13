use maud::{Markup, html};

use crate::config::{DeploymentShareConfig, current_unix_timestamp};

use super::super::constants::PUBLIC_SHARE_BASE_PATH;

pub(in crate::server) fn render_share_dialog_shell(
    project: &str,
    deployment: &str,
    shares: &[DeploymentShareConfig],
    status: Option<(&str, bool)>,
) -> Markup {
    let endpoint = format!("/__latitude/ui/shares/{project}/{deployment}");
    let now = current_unix_timestamp();
    let mut matching = shares
        .iter()
        .filter(|share| share.project == project && share.deployment == deployment)
        .collect::<Vec<_>>();
    matching.sort_by_key(|share| share.is_expired(now));

    html! {
        div class="share-dialog-shell" data-share-dialog-shell {
            div class="share-dialog-header" {
                div { h2 { "Share " (deployment) } }
                button class="share-close" type="button" data-share-close aria-label="Close share manager" { "×" }
            }
            @if let Some((message, is_error)) = status {
                div
                    class="share-status"
                    data-share-status
                    data-tone=[is_error.then_some("error")]
                    aria-live="polite" { (message) }
            }
            form
                class="share-form"
                hx-post=(&endpoint)
                hx-target="closest [data-share-dialog-shell]"
                hx-swap="outerHTML" {
                label {
                    "Password (optional)"
                    input type="password" name="password" autocomplete="new-password" placeholder="Open link when blank";
                }
                label {
                    "Expires"
                    select name="expiry" {
                        option value="" { "Never" }
                        option value="3600" { "1 hour" }
                        option value="86400" { "1 day" }
                        option value="604800" { "7 days" }
                    }
                }
                button class="share-create" type="submit" { "Create link" }
            }
            section class="share-existing" {
                div class="share-section-heading" {
                    h3 { "Links" }
                    button
                        class="share-refresh"
                        type="button"
                        hx-get=(&endpoint)
                        hx-target="closest [data-share-dialog-shell]"
                        hx-swap="outerHTML" { "Refresh" }
                }
                div class="share-list" data-share-list {
                    @if matching.is_empty() {
                        div class="share-empty" {
                            span class="share-empty-icon" { "↗" }
                            div {
                                strong { "No links yet" }
                                span { "Create a link above to share this deployment." }
                            }
                        }
                    } @else {
                        @for share in matching {
                            (share_card(share, now, &endpoint))
                        }
                    }
                }
            }
        }
    }
}

fn share_card(share: &DeploymentShareConfig, now: u64, endpoint: &str) -> Markup {
    let expired = share.is_expired(now);
    let href = format!("{PUBLIC_SHARE_BASE_PATH}/{}/", share.token);
    let delete_endpoint = format!("{endpoint}/{}", share.token);

    html! {
        article class="share-card" {
            div class="share-card-details" {
                strong { (&share.token) }
                span class=[expired.then_some("share-expired")] {
                    @if expired {
                        "Expired"
                    } @else if let Some(expires_at) = share.expires_at {
                        span data-share-expires-at=(expires_at) { "Expires soon" }
                    } @else {
                        "Never expires"
                    }
                    " · "
                    @if share.password.is_some() { "Password protected" } @else { "Open link" }
                }
            }
            div class="share-card-actions" {
                button
                    class="share-send"
                    type="button"
                    data-share-url=(&href)
                    disabled[expired] { "Share" }
                button
                    class="share-revoke"
                    type="button"
                    hx-delete=(&delete_endpoint)
                    hx-target="closest [data-share-dialog-shell]"
                    hx-swap="outerHTML"
                    hx-confirm="Revoke this share link? Anyone using it will lose access immediately." { "Revoke" }
            }
        }
    }
}
