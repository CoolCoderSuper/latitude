use maud::html;

use crate::config::ProjectConfig;

use super::super::{
    assets::{FILE_VIEWER_SCRIPT_SRC, FILE_VIEWER_STYLE_HREF, HTMX_SCRIPT_SRC},
    html as html_page,
    paths::display_path,
};

pub(in crate::server) fn render_project_files(
    project: &ProjectConfig,
    device_hostname: &str,
) -> String {
    html_page::document(
        &format!("{} files - Latitude", project.name),
        device_hostname,
        FILE_VIEWER_STYLE_HREF,
        html! { script src=(HTMX_SCRIPT_SRC) {} },
        html! {
            main class="files-page" data-file-workspace data-api-url=(format!("/__latitude/api/projects/{}/files", project.name)) {
                header class="files-header" {
                    a href=(format!("/{}", project.name)) { "Back to project" }
                    h1 { "Files" }
                    p { (&project.name) " on " (device_hostname) }
                    p class="project-path" { (display_path(&project.project_dir)) }
                }
                div class="file-workspace" {
                    aside class="file-sidebar" { div class="file-tree" data-file-tree { "Loading…" } }
                    div class="file-resizer" data-file-resizer role="separator" aria-orientation="vertical" aria-label="Resize file explorer" tabindex="0" {}
                    section class="file-main" {
                        span class="visually-hidden" data-file-title { "Select a file to preview" }
                        form
                            class="file-actions"
                            data-file-actions
                            hx-put=(format!("/__latitude/ui/files/{}", project.name))
                            hx-target="[data-save-state]"
                            hx-swap="innerHTML"
                            hidden {
                            span data-save-state {}
                            button class="mode-toggle" type="button" data-vim-toggle aria-pressed="false" title="Enable Vim keybindings" { "Vim" }
                            button type="submit" data-save disabled { "Save" }
                        }
                        div class="file-preview" data-file-preview {
                            div class="file-empty" { "Choose a file from the explorer." }
                        }
                    }
                }
                script type="module" src=(FILE_VIEWER_SCRIPT_SRC) {}
            }
        },
    )
}
