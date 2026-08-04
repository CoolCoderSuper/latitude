use maud::{DOCTYPE, Markup, html};

use super::assets::{
    COMMON_THEME_STYLE_HREF, FAVICON_HREF, THEME_BOOTSTRAP_SCRIPT_SRC, THEME_TOGGLE_SCRIPT_SRC,
};

pub(super) struct PageHeader<'a> {
    pub(super) class_name: Option<&'a str>,
    pub(super) back_href: &'a str,
    pub(super) back_label: &'a str,
    pub(super) heading: &'a str,
    pub(super) description: &'a str,
    pub(super) path: Option<&'a str>,
}

pub(super) fn page_header(header: PageHeader<'_>) -> Markup {
    html! {
        header class=[header.class_name] {
            a href=(header.back_href) { (header.back_label) }
            h1 { (header.heading) }
            p { (header.description) }
            @if let Some(path) = header.path {
                p class="project-path" { (path) }
            }
        }
    }
}

pub(super) fn document(
    title: &str,
    device_hostname: &str,
    style_href: &str,
    head_extra: Markup,
    body: Markup,
) -> String {
    document_with_theme(title, device_hostname, style_href, None, head_extra, body)
}

pub(super) fn document_with_theme(
    title: &str,
    device_hostname: &str,
    style_href: &str,
    theme: Option<&str>,
    head_extra: Markup,
    body: Markup,
) -> String {
    html! {
        (DOCTYPE)
        html lang="en" data-latitude-theme=[theme] {
            (document_head(title, device_hostname, style_href, head_extra))
            body { (document_body(body)) }
        }
    }
    .into_string()
}

fn document_head(
    title: &str,
    device_hostname: &str,
    style_href: &str,
    head_extra: Markup,
) -> Markup {
    let document_title = format!("{title} - {device_hostname}");

    html! {
        head {
            meta charset="utf-8";
            meta name="viewport" content="width=device-width, initial-scale=1";
            title { (document_title) }
            link rel="icon" type="image/png" href=(FAVICON_HREF);
            script src=(THEME_BOOTSTRAP_SCRIPT_SRC) {}
            link rel="stylesheet" href=(COMMON_THEME_STYLE_HREF);
            link rel="stylesheet" href=(style_href);
            (head_extra)
        }
    }
}

fn document_body(body: Markup) -> Markup {
    html! {
        (theme_toggle())
        (body)
        script src=(THEME_TOGGLE_SCRIPT_SRC) {}
    }
}

fn theme_toggle() -> Markup {
    html! {
        button
            class="latitude-theme-toggle"
            data-latitude-theme-toggle
            type="button"
            aria-label="Toggle color theme"
            title="Toggle color theme" {
            svg class="latitude-theme-icon latitude-theme-icon-moon" aria-hidden="true" viewBox="0 0 24 24" {
                path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z" {}
            }
            svg class="latitude-theme-icon latitude-theme-icon-sun" aria-hidden="true" viewBox="0 0 24 24" {
                circle cx="12" cy="12" r="4" {}
                path d="M12 2v2M12 20v2M4.93 4.93l1.41 1.41M17.66 17.66l1.41 1.41M2 12h2M20 12h2M4.93 19.07l1.41-1.41M17.66 6.34l1.41-1.41" {}
            }
        }
    }
}
