use maud::{DOCTYPE, Markup, PreEscaped, html};

pub(super) fn document(
    title: &str,
    device_hostname: &str,
    style: &str,
    head_extra: Markup,
    body: Markup,
) -> String {
    document_with_theme(title, device_hostname, style, None, head_extra, body)
}

pub(super) fn document_with_theme(
    title: &str,
    device_hostname: &str,
    style: &str,
    theme: Option<&str>,
    head_extra: Markup,
    body: Markup,
) -> String {
    let document = if let Some(theme) = theme {
        html! {
            (DOCTYPE)
            html lang="en" data-latitude-theme=(theme) {
                (document_head(title, device_hostname, style, head_extra))
                body { (body) }
            }
        }
    } else {
        html! {
            (DOCTYPE)
            html lang="en" {
                (document_head(title, device_hostname, style, head_extra))
                body { (body) }
            }
        }
    };

    document.into_string()
}

fn document_head(title: &str, device_hostname: &str, style: &str, head_extra: Markup) -> Markup {
    let document_title = format!("{title} - {device_hostname}");

    html! {
        head {
            meta charset="utf-8";
            meta name="viewport" content="width=device-width, initial-scale=1";
            title { (document_title) }
            style { (PreEscaped(style)) }
            (head_extra)
        }
    }
}
