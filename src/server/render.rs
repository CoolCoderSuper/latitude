use std::path::Path;

use crate::config::{ApplicationConfig, ApplicationTarget, LatitudeConfig, ProjectConfig};

use super::{
    assets::{
        AUTH_PAGE_STYLE, DIFF_VIEWER_SCRIPT, DIFF_VIEWER_STYLE, PROJECT_HOME_STYLE,
        TERMINAL_VIEWER_SCRIPT, TERMINAL_VIEWER_STYLE,
    },
    constants::{DIFF_ROUTE_SEGMENT, LOGIN_PATH, TERMINAL_ROUTE_SEGMENT, TERMINAL_WS_SUFFIX},
    git::{FileSectionKind, GitDiffReport, GitFileChange},
    terminal_api::PublicTerminalInfoResponse,
};

pub(super) fn render_project_home(project: &ProjectConfig) -> String {
    let page_title = format!("{} - Latitude Project", project.name);
    let mut output = String::new();
    output.push_str("<!doctype html>\n<html lang=\"en\">\n<head>\n");
    output.push_str("<meta charset=\"utf-8\" />\n");
    output.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\" />\n");
    output.push_str("<title>");
    output.push_str(&escape_html_text(&page_title));
    output.push_str("</title>\n<style>\n");
    output.push_str(PROJECT_HOME_STYLE);
    output.push_str("\n</style>\n</head>\n<body>\n<main>\n<h1>");
    output.push_str(&escape_html_text(&project.name));
    output.push_str("</h1>\n<p>Project tools and deployments</p>\n");

    let enabled_deployments = project
        .deployments
        .iter()
        .filter(|deployment| deployment.enabled)
        .collect::<Vec<_>>();

    output.push_str("<ul>\n");
    output.push_str("<li><a href=\"/");
    output.push_str(&escape_html_text(&project.name));
    output.push('/');
    output.push_str(DIFF_ROUTE_SEGMENT);
    output.push_str(
        "\"><strong>Code changes</strong><span>Review staged and unstaged files</span></a></li>\n",
    );
    output.push_str("<li><a href=\"/");
    output.push_str(&escape_html_text(&project.name));
    output.push('/');
    output.push_str(TERMINAL_ROUTE_SEGMENT);
    output.push_str(
        "\"><strong>Terminal</strong><span>Run commands in the project directory</span></a></li>\n",
    );

    for deployment in enabled_deployments {
        output.push_str("<li><a href=\"/");
        output.push_str(&escape_html_text(&project.name));
        output.push('/');
        output.push_str(&escape_html_text(&deployment.name));
        output.push_str("\"><strong>");
        output.push_str(&escape_html_text(&deployment.name));
        output.push_str("</strong><span>");
        output.push_str(deployment_home_label(deployment));
        if let Some(title) = deployment_page_title(deployment) {
            output.push_str(": ");
            output.push_str(&escape_html_text(title));
        }
        output.push_str("</span></a></li>\n");
    }

    if project
        .deployments
        .iter()
        .all(|deployment| !deployment.enabled)
    {
        output.push_str("<li class=\"empty\">No enabled deployments yet.</li>\n");
    }

    output.push_str("</ul>\n");
    output.push_str("</main>\n</body>\n</html>\n");
    output
}

pub(super) fn render_project_diff(project: &ProjectConfig, report: &GitDiffReport) -> String {
    let page_title = format!("{} code changes - Latitude", project.name);
    let mut output = String::new();
    output.push_str("<!doctype html>\n<html lang=\"en\">\n<head>\n");
    output.push_str("<meta charset=\"utf-8\" />\n");
    output.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\" />\n");
    output.push_str("<title>");
    output.push_str(&escape_html_text(&page_title));
    output.push_str("</title>\n<style>\n");
    output.push_str(DIFF_VIEWER_STYLE);
    output.push_str("\n</style>\n</head>\n<body>\n<main>\n<header>\n<a href=\"/");
    output.push_str(&escape_html_text(&project.name));
    output.push_str("\">Back to project</a>\n<h1>Code changes</h1>\n<p>");
    output.push_str(&escape_html_text(&project.name));
    output.push_str("</p>\n<p class=\"project-path\">");
    output.push_str(&escape_html_text(&display_path(&report.repo_dir)));
    output.push_str("</p>\n</header>\n");

    render_diff_workspace(&mut output, project, report);

    output.push_str("<script>\n");
    output.push_str(DIFF_VIEWER_SCRIPT);
    output.push_str("\n</script>\n</main>\n</body>\n</html>\n");
    output
}

pub(super) fn render_project_terminal(
    project: &ProjectConfig,
    info: &PublicTerminalInfoResponse,
    websocket_token: Option<&str>,
) -> String {
    let page_title = format!("{} terminal - Latitude", project.name);
    let mut output = String::new();
    output.push_str("<!doctype html>\n<html lang=\"en\">\n<head>\n");
    output.push_str("<meta charset=\"utf-8\" />\n");
    output.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\" />\n");
    output.push_str("<title>");
    output.push_str(&escape_html_text(&page_title));
    output.push_str("</title>\n<style>\n");
    output.push_str(TERMINAL_VIEWER_STYLE);
    output.push_str(
        "\n</style>\n<link rel=\"stylesheet\" href=\"https://cdn.jsdelivr.net/npm/@xterm/xterm@5.5.0/css/xterm.css\" />\n",
    );
    output.push_str("</head>\n<body>\n<main>\n<header>\n<a href=\"/");
    output.push_str(&escape_html_text(&project.name));
    output.push_str("\">Back to project</a>\n<h1>Terminal</h1>\n<p>");
    output.push_str(&escape_html_text(&project.name));
    output.push_str("</p>\n<p class=\"project-path\">");
    output.push_str(&escape_html_text(&info.cwd));
    output.push_str("</p>\n</header>\n");
    output.push_str(
        "<section class=\"terminal-workspace\" data-terminal-workspace data-sessions-path=\"",
    );
    output.push_str(&escape_html_text(&info.sessions_href));
    output.push_str("\" data-ws-path=\"/");
    output.push_str(&escape_html_text(&project.name));
    output.push('/');
    output.push_str(TERMINAL_ROUTE_SEGMENT);
    output.push('/');
    output.push_str(TERMINAL_WS_SUFFIX);
    output.push('"');
    if let Some(token) = websocket_token {
        output.push_str(" data-ws-token=\"");
        output.push_str(&escape_html_text(token));
        output.push('"');
    }
    output.push_str(">\n");
    output.push_str("<div class=\"action-status\" data-terminal-status hidden></div>\n");
    output.push_str("<div class=\"terminal-session-bar\"><div class=\"terminal-session-list\" data-terminal-sessions></div><button class=\"terminal-new-button\" type=\"button\" data-terminal-new aria-label=\"New terminal\" title=\"New terminal\">+</button></div>\n");
    output.push_str("<div class=\"terminal-stack\" data-terminal-stack><div class=\"terminal-empty\" data-terminal-empty hidden>No terminals. Use + to create one.</div></div>\n");
    output.push_str("</section>\n<script src=\"https://cdn.jsdelivr.net/npm/@xterm/xterm@5.5.0/lib/xterm.min.js\"></script>\n");
    output.push_str("<script src=\"https://cdn.jsdelivr.net/npm/@xterm/addon-fit@0.10.0/lib/addon-fit.min.js\"></script>\n<script>\n");
    output.push_str(TERMINAL_VIEWER_SCRIPT);
    output.push_str("\n</script>\n</main>\n</body>\n</html>\n");
    output
}

fn render_diff_workspace(output: &mut String, project: &ProjectConfig, report: &GitDiffReport) {
    output.push_str("<div class=\"diff-workspace\" data-diff-workspace data-action-url=\"/");
    output.push_str(&escape_html_text(&project.name));
    output.push('/');
    output.push_str(DIFF_ROUTE_SEGMENT);
    output.push_str("\">\n");
    render_diff_workspace_inner(output, report);
    output.push_str("</div>\n");
}

pub(super) fn render_diff_workspace_fragment(report: &GitDiffReport) -> String {
    let mut output = String::new();
    render_diff_workspace_inner(&mut output, report);
    output
}

fn render_diff_workspace_inner(output: &mut String, report: &GitDiffReport) {
    output.push_str("<div class=\"action-status\" data-action-status hidden></div>\n");
    render_git_action_panel(output);
    render_git_file_panel(output, &report.file_changes);
}

fn render_git_action_panel(output: &mut String) {
    output.push_str("<section class=\"action-panel\">\n");
    render_git_action_button(output, "stage_all", "Stage all");
    render_git_action_button(output, "unstage_all", "Unstage all");
    output.push_str("<div class=\"commit-form\">");
    output.push_str(
        "<input data-commit-message type=\"text\" required placeholder=\"Commit message\" />",
    );
    output.push_str(
        "<button type=\"button\" data-git-action=\"commit\">Commit staged</button></div>\n",
    );
    render_git_action_button(output, "push", "Push");
    output.push_str("</section>\n");
}

fn render_git_action_button(output: &mut String, action: &str, label: &str) {
    output.push_str("<button type=\"button\" data-git-action=\"");
    output.push_str(&escape_html_text(action));
    output.push_str("\">");
    output.push_str(&escape_html_text(label));
    output.push_str("</button>\n");
}

fn render_git_file_panel(output: &mut String, changes: &[GitFileChange]) {
    render_git_file_section(
        output,
        "Unstaged files",
        "No unstaged files.",
        changes,
        FileSectionKind::Unstaged,
    );
    render_git_file_section(
        output,
        "Staged files",
        "No staged files.",
        changes,
        FileSectionKind::Staged,
    );
}

fn render_git_file_section(
    output: &mut String,
    title: &str,
    empty_message: &str,
    changes: &[GitFileChange],
    kind: FileSectionKind,
) {
    let section_changes = changes
        .iter()
        .filter(|change| kind.includes(change))
        .collect::<Vec<_>>();

    output.push_str("<section class=\"file-panel\">\n<div class=\"section-heading\"><h2>");
    output.push_str(&escape_html_text(title));
    output.push_str("</h2><code>");
    output.push_str(&section_changes.len().to_string());
    output.push_str(match section_changes.len() {
        1 => " file",
        _ => " files",
    });
    output.push_str("</code></div>\n");

    if section_changes.is_empty() {
        output.push_str("<div class=\"empty\">");
        output.push_str(&escape_html_text(empty_message));
        output.push_str("</div>\n</section>\n");
        return;
    }

    output.push_str("<div class=\"file-list\">\n");
    for change in section_changes {
        let visible_diffs = change
            .diffs
            .iter()
            .filter(|diff| kind.includes_diff(diff))
            .collect::<Vec<_>>();

        output.push_str("<details class=\"file-card\" data-file-path=\"");
        output.push_str(&escape_html_text(&change.path));
        output.push_str("\"><summary class=\"file-summary\"><div class=\"status-code\">");
        output.push_str(&escape_html_text(&change.status_label()));
        output.push_str("</div><div class=\"file-path\">");
        output.push_str(&escape_html_text(&change.path));
        if let Some(original_path) = &change.original_path {
            output.push_str("<span> from ");
            output.push_str(&escape_html_text(original_path));
            output.push_str("</span>");
        }
        output.push_str("</div><div class=\"file-count\">");
        output.push_str(match visible_diffs.len() {
            0 => "status only",
            1 => "1 diff",
            _ => "diffs",
        });
        if visible_diffs.len() > 1 {
            output.push_str(": ");
            output.push_str(&visible_diffs.len().to_string());
        }
        output.push_str("</div></summary><div class=\"file-content\"><div class=\"file-actions\">");

        match kind {
            FileSectionKind::Unstaged => {
                render_git_file_action_button(output, "stage_file", "Stage", &change.path);
            }
            FileSectionKind::Staged => {
                render_git_file_action_button(output, "unstage_file", "Unstage", &change.path);
            }
        }

        output.push_str("</div>");

        if visible_diffs.is_empty() {
            output.push_str("<div class=\"empty\">No inline diff for this file.</div>");
        } else {
            for diff in visible_diffs {
                output.push_str("<div class=\"file-diff\"><div class=\"file-diff-title\"><strong>");
                output.push_str(&escape_html_text(&diff.label));
                output.push_str("</strong><code>");
                output.push_str(&escape_html_text(&diff.command));
                output.push_str("</code></div>");
                render_diff_code_output(output, &diff.content, &diff.path);
                output.push_str("</div>");
            }
        }

        output.push_str("</div></details>\n");
    }
    output.push_str("</div>\n</section>\n");
}

fn render_git_file_action_button(output: &mut String, action: &str, label: &str, path: &str) {
    output.push_str("<button type=\"button\" data-git-action=\"");
    output.push_str(&escape_html_text(action));
    output.push_str("\" data-path=\"");
    output.push_str(&escape_html_text(path));
    output.push_str("\">");
    output.push_str(&escape_html_text(label));
    output.push_str("</button>\n");
}

pub(super) fn render_diff_code_output(output: &mut String, content: &str, path: &str) {
    let language = syntax_language_for_path(path);

    output.push_str("<pre>");
    for line in content.lines() {
        output.push_str("<span class=\"line");
        if let Some(class) = diff_line_class(line) {
            output.push(' ');
            output.push_str(class);
        }
        output.push_str("\">");
        render_diff_line_content(output, line, language);
        output.push_str("</span>\n");
    }
    output.push_str("</pre>\n");
}

fn render_diff_line_content(output: &mut String, line: &str, language: SyntaxLanguage) {
    if matches!(diff_line_class(line), Some("file" | "hunk")) {
        output.push_str(&escape_html_text(line));
        return;
    }

    if let Some(first) = line.chars().next() {
        if matches!(first, '+' | '-' | ' ') {
            output.push(first);
            render_syntax_line(output, language, &line[first.len_utf8()..]);
            return;
        }
    }

    render_syntax_line(output, language, line);
}

pub(super) fn diff_line_class(line: &str) -> Option<&'static str> {
    if line.starts_with("diff --git")
        || line.starts_with("index ")
        || line.starts_with("new file mode ")
        || line.starts_with("deleted file mode ")
        || line.starts_with("--- ")
        || line.starts_with("+++ ")
    {
        Some("file")
    } else if line.starts_with("@@") {
        Some("hunk")
    } else if line.starts_with('+') {
        Some("add")
    } else if line.starts_with('-') {
        Some("remove")
    } else {
        None
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SyntaxLanguage {
    Plain,
    Rust,
    JavaScript,
    Css,
    Html,
    Json,
    Config,
}

pub(super) fn syntax_language_for_path(path: &str) -> SyntaxLanguage {
    let path = Path::new(path);
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    match file_name.as_str() {
        "cargo.toml" | "cargo.lock" | "package.json" | "tsconfig.json" | "vite.config.js"
        | "vite.config.ts" | "svelte.config.js" | "svelte.config.ts" => {
            return match file_name.rsplit_once('.').map(|(_, ext)| ext) {
                Some("json") => SyntaxLanguage::Json,
                Some("js") | Some("ts") => SyntaxLanguage::JavaScript,
                _ => SyntaxLanguage::Config,
            };
        }
        _ => {}
    }

    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "rs" => SyntaxLanguage::Rust,
        "js" | "jsx" | "mjs" | "cjs" | "ts" | "tsx" | "svelte" => SyntaxLanguage::JavaScript,
        "css" | "scss" | "sass" => SyntaxLanguage::Css,
        "html" | "htm" | "xml" | "svg" => SyntaxLanguage::Html,
        "json" => SyntaxLanguage::Json,
        "toml" | "yaml" | "yml" | "env" | "ini" | "conf" | "lock" => SyntaxLanguage::Config,
        _ => SyntaxLanguage::Plain,
    }
}

fn render_syntax_line(output: &mut String, language: SyntaxLanguage, line: &str) {
    if language == SyntaxLanguage::Plain {
        output.push_str(&escape_html_text(line));
        return;
    }

    let mut index = 0;
    while index < line.len() {
        let rest = &line[index..];

        if let Some(length) = comment_token_len(rest, language) {
            render_token_span(output, "tok-comment", &rest[..length]);
            index += length;
            continue;
        }

        let ch = rest.chars().next().expect("rest is not empty");

        if matches!(ch, '"' | '\'' | '`') {
            let length = string_token_len(rest, ch);
            let class = if language == SyntaxLanguage::Json && followed_by_colon(&rest[length..]) {
                "tok-property"
            } else {
                "tok-string"
            };
            render_token_span(output, class, &rest[..length]);
            index += length;
            continue;
        }

        if language == SyntaxLanguage::Css && rest.starts_with('#') {
            let length = css_color_token_len(rest);
            if length > 1 {
                render_token_span(output, "tok-number", &rest[..length]);
                index += length;
                continue;
            }
        }

        if ch.is_ascii_digit() {
            let length = number_token_len(rest);
            render_token_span(output, "tok-number", &rest[..length]);
            index += length;
            continue;
        }

        if is_identifier_start(ch) {
            let length = identifier_token_len(rest);
            let token = &rest[..length];
            if let Some(class) = syntax_identifier_class(language, token, &rest[length..]) {
                render_token_span(output, class, token);
            } else {
                output.push_str(&escape_html_text(token));
            }
            index += length;
            continue;
        }

        if is_punctuation(ch) {
            render_token_span(output, "tok-punctuation", &rest[..ch.len_utf8()]);
            index += ch.len_utf8();
            continue;
        }

        output.push_str(&escape_html_text(&rest[..ch.len_utf8()]));
        index += ch.len_utf8();
    }
}

fn render_token_span(output: &mut String, class: &str, token: &str) {
    output.push_str("<span class=\"");
    output.push_str(class);
    output.push_str("\">");
    output.push_str(&escape_html_text(token));
    output.push_str("</span>");
}

fn comment_token_len(rest: &str, language: SyntaxLanguage) -> Option<usize> {
    match language {
        SyntaxLanguage::Rust | SyntaxLanguage::JavaScript if rest.starts_with("//") => {
            Some(rest.len())
        }
        SyntaxLanguage::Css if rest.starts_with("/*") => {
            Some(rest.find("*/").map(|index| index + 2).unwrap_or(rest.len()))
        }
        SyntaxLanguage::Html if rest.starts_with("<!--") => Some(
            rest.find("-->")
                .map(|index| index + 3)
                .unwrap_or(rest.len()),
        ),
        SyntaxLanguage::Config if rest.starts_with('#') => Some(rest.len()),
        _ => None,
    }
}

fn string_token_len(rest: &str, quote: char) -> usize {
    let mut escaped = false;
    for (index, ch) in rest.char_indices().skip(1) {
        if escaped {
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == quote {
            return index + ch.len_utf8();
        }
    }
    rest.len()
}

fn css_color_token_len(rest: &str) -> usize {
    let mut length = 1;
    for (index, ch) in rest.char_indices().skip(1) {
        if ch.is_ascii_hexdigit() {
            length = index + ch.len_utf8();
        } else {
            break;
        }
    }
    length
}

fn number_token_len(rest: &str) -> usize {
    let mut length = 0;
    for (index, ch) in rest.char_indices() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_') {
            length = index + ch.len_utf8();
        } else {
            break;
        }
    }
    length
}

fn identifier_token_len(rest: &str) -> usize {
    let mut length = 0;
    for (index, ch) in rest.char_indices() {
        if is_identifier_continue(ch) {
            length = index + ch.len_utf8();
        } else {
            break;
        }
    }
    length
}

fn is_identifier_start(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphabetic()
}

fn is_identifier_continue(ch: char) -> bool {
    ch == '_' || ch == '-' || ch.is_ascii_alphanumeric()
}

fn is_punctuation(ch: char) -> bool {
    matches!(
        ch,
        '{' | '}'
            | '['
            | ']'
            | '('
            | ')'
            | '<'
            | '>'
            | ';'
            | ':'
            | ','
            | '.'
            | '='
            | '+'
            | '-'
            | '*'
            | '/'
            | '!'
            | '?'
            | '|'
            | '&'
            | '%'
    )
}

fn followed_by_colon(rest: &str) -> bool {
    rest.trim_start().starts_with(':')
}

fn syntax_identifier_class(
    language: SyntaxLanguage,
    token: &str,
    following: &str,
) -> Option<&'static str> {
    if is_keyword(language, token) {
        Some("tok-keyword")
    } else if is_type_token(language, token) {
        Some("tok-type")
    } else if language == SyntaxLanguage::Css && followed_by_colon(following) {
        Some("tok-property")
    } else {
        None
    }
}

fn is_keyword(language: SyntaxLanguage, token: &str) -> bool {
    match language {
        SyntaxLanguage::Rust => matches!(
            token,
            "as" | "async"
                | "await"
                | "break"
                | "const"
                | "continue"
                | "crate"
                | "else"
                | "enum"
                | "extern"
                | "false"
                | "fn"
                | "for"
                | "if"
                | "impl"
                | "in"
                | "let"
                | "loop"
                | "match"
                | "mod"
                | "move"
                | "mut"
                | "pub"
                | "ref"
                | "return"
                | "self"
                | "Self"
                | "static"
                | "struct"
                | "super"
                | "trait"
                | "true"
                | "type"
                | "unsafe"
                | "use"
                | "where"
                | "while"
        ),
        SyntaxLanguage::JavaScript => matches!(
            token,
            "as" | "async"
                | "await"
                | "break"
                | "case"
                | "catch"
                | "class"
                | "const"
                | "continue"
                | "default"
                | "else"
                | "export"
                | "extends"
                | "false"
                | "finally"
                | "for"
                | "from"
                | "function"
                | "if"
                | "import"
                | "in"
                | "interface"
                | "let"
                | "new"
                | "null"
                | "return"
                | "switch"
                | "this"
                | "throw"
                | "true"
                | "try"
                | "type"
                | "typeof"
                | "var"
                | "while"
        ),
        SyntaxLanguage::Css => matches!(
            token,
            "and"
                | "from"
                | "important"
                | "keyframes"
                | "media"
                | "not"
                | "only"
                | "supports"
                | "to"
        ),
        SyntaxLanguage::Json => matches!(token, "false" | "null" | "true"),
        SyntaxLanguage::Html => matches!(token, "DOCTYPE"),
        SyntaxLanguage::Config | SyntaxLanguage::Plain => false,
    }
}

fn is_type_token(language: SyntaxLanguage, token: &str) -> bool {
    match language {
        SyntaxLanguage::Rust => {
            matches!(
                token,
                "bool"
                    | "char"
                    | "f32"
                    | "f64"
                    | "i8"
                    | "i16"
                    | "i32"
                    | "i64"
                    | "i128"
                    | "isize"
                    | "str"
                    | "String"
                    | "u8"
                    | "u16"
                    | "u32"
                    | "u64"
                    | "u128"
                    | "usize"
            ) || token
                .chars()
                .next()
                .map(|ch| ch.is_ascii_uppercase())
                .unwrap_or(false)
        }
        SyntaxLanguage::JavaScript | SyntaxLanguage::Html => token
            .chars()
            .next()
            .map(|ch| ch.is_ascii_uppercase())
            .unwrap_or(false),
        _ => false,
    }
}

fn display_path(path: &Path) -> String {
    let path = path.display().to_string();
    path.strip_prefix(r"\\?\").unwrap_or(&path).to_string()
}

pub(super) fn render_public_login(next: &str, login_failed: bool) -> String {
    let mut output = String::new();
    output.push_str("<!doctype html>\n<html lang=\"en\">\n<head>\n");
    output.push_str("<meta charset=\"utf-8\" />\n");
    output.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\" />\n");
    output.push_str("<title>Sign in - Latitude</title>\n<style>\n");
    output.push_str(AUTH_PAGE_STYLE);
    output.push_str("\n</style>\n</head>\n<body>\n<main>\n");
    output.push_str("<h1>Latitude</h1>\n<p>Sign in to continue</p>\n");
    if login_failed {
        output.push_str("<div class=\"error\">Incorrect password.</div>\n");
    }
    output.push_str("<form method=\"post\" action=\"");
    output.push_str(LOGIN_PATH);
    output.push_str("\">\n<input type=\"hidden\" name=\"next\" value=\"");
    output.push_str(&escape_html_text(next));
    output.push_str("\" />\n<label>Password<input name=\"password\" type=\"password\" required autofocus autocomplete=\"current-password\" /></label>\n");
    output.push_str("<button type=\"submit\">Sign in</button>\n</form>\n");
    output.push_str("</main>\n</body>\n</html>\n");
    output
}

pub(super) fn render_server_home(config: &LatitudeConfig) -> String {
    let mut output = String::new();
    output.push_str("<!doctype html>\n<html lang=\"en\">\n<head>\n");
    output.push_str("<meta charset=\"utf-8\" />\n");
    output.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\" />\n");
    output.push_str("<title>Latitude Projects</title>\n<style>\n");
    output.push_str(PROJECT_HOME_STYLE);
    output.push_str("\n</style>\n</head>\n<body>\n<main>\n<h1>Latitude</h1>\n");
    output.push_str("<p>Available projects</p>\n");

    let enabled_projects = config
        .projects
        .iter()
        .filter(|project| project.enabled)
        .collect::<Vec<_>>();

    if enabled_projects.is_empty() {
        output.push_str("<div class=\"empty\">No enabled projects yet.</div>\n");
    } else {
        output.push_str("<ul>\n");
        for project in enabled_projects {
            output.push_str("<li><a href=\"/");
            output.push_str(&escape_html_text(&project.name));
            output.push_str("\"><strong>");
            output.push_str(&escape_html_text(&project.name));
            output.push_str("</strong><span>");
            output.push_str(&project_summary(project));
            output.push_str("</span></a></li>\n");
        }
        output.push_str("</ul>\n");
    }

    output.push_str("</main>\n</body>\n</html>\n");
    output
}

pub(super) fn project_summary(project: &ProjectConfig) -> String {
    let enabled_deployment_count = enabled_deployment_count(project);

    match enabled_deployment_count {
        1 => "1 deployment".to_string(),
        count => format!("{count} deployments"),
    }
}

pub(super) fn enabled_deployment_count(project: &ProjectConfig) -> usize {
    project
        .deployments
        .iter()
        .filter(|deployment| deployment.enabled)
        .count()
}

pub(super) fn deployment_kind(deployment: &ApplicationConfig) -> &'static str {
    match &deployment.target {
        ApplicationTarget::ReverseProxy { .. } => "reverse_proxy",
        ApplicationTarget::Static { .. } => "static",
        ApplicationTarget::Page { .. } => "page",
    }
}

pub(super) fn deployment_home_label(deployment: &ApplicationConfig) -> &'static str {
    match &deployment.target {
        ApplicationTarget::ReverseProxy { .. } => "Website",
        ApplicationTarget::Static { .. } => "Static website",
        ApplicationTarget::Page { .. } => "Page",
    }
}

pub(super) fn deployment_page_title(deployment: &ApplicationConfig) -> Option<&str> {
    match &deployment.target {
        ApplicationTarget::Page { title, .. } => title.as_deref(),
        _ => None,
    }
}

pub(super) fn escape_html_text(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\'' => output.push_str("&#39;"),
            _ => output.push(ch),
        }
    }
    output
}
