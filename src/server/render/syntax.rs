use std::path::Path;

pub(in crate::server) fn render_diff_code_output(output: &mut String, content: &str, path: &str) {
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

pub(in crate::server) fn diff_line_class(line: &str) -> Option<&'static str> {
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
pub(in crate::server) enum SyntaxLanguage {
    Plain,
    Rust,
    JavaScript,
    Css,
    Html,
    Json,
    Config,
}

pub(in crate::server) fn syntax_language_for_path(path: &str) -> SyntaxLanguage {
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

fn escape_html_text(value: &str) -> String {
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
