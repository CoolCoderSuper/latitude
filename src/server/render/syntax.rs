use std::{path::Path, str::FromStr, sync::LazyLock};

use serde::Serialize;
use syntect::{
    easy::HighlightLines,
    highlighting::{Color, FontStyle, ScopeSelectors, Style, StyleModifier, Theme, ThemeItem},
    parsing::{SyntaxDefinition, SyntaxReference, SyntaxSet},
};

static SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(syntax_set);
static TOKEN_THEME: LazyLock<Theme> = LazyLock::new(token_theme);

const VB_NET_SYNTAX: &str = include_str!("../syntaxes/vbnet.sublime-syntax");

const DEFAULT_COLOR: Color = Color {
    r: 0,
    g: 0,
    b: 0,
    a: 255,
};

pub(in crate::server) fn warm_syntax_highlighter() {
    LazyLock::force(&SYNTAX_SET);
    LazyLock::force(&TOKEN_THEME);
}

fn syntax_set() -> SyntaxSet {
    let mut builder = two_face::syntax::extra_no_newlines().into_builder();
    builder.add(
        SyntaxDefinition::load_from_str(VB_NET_SYNTAX, false, Some("vbnet.sublime-syntax"))
            .expect("bundled VB.NET syntax is valid"),
    );
    builder.build()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::server) enum SyntaxTokenKind {
    Comment,
    Keyword,
    Number,
    Property,
    Punctuation,
    String,
    Type,
}

impl SyntaxTokenKind {
    fn class_name(self) -> &'static str {
        match self {
            Self::Comment => "tok-comment",
            Self::Keyword => "tok-keyword",
            Self::Number => "tok-number",
            Self::Property => "tok-property",
            Self::Punctuation => "tok-punctuation",
            Self::String => "tok-string",
            Self::Type => "tok-type",
        }
    }

    fn marker_color(self) -> Color {
        match self {
            Self::Comment => color(1),
            Self::Keyword => color(2),
            Self::Number => color(3),
            Self::Property => color(4),
            Self::Punctuation => color(5),
            Self::String => color(6),
            Self::Type => color(7),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(in crate::server) struct SyntaxToken {
    pub(in crate::server) text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::server) kind: Option<SyntaxTokenKind>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(in crate::server) struct HighlightedDiffLine {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::server) kind: Option<&'static str>,
    pub(in crate::server) tokens: Vec<SyntaxToken>,
}

pub(in crate::server) fn render_diff_code_output(output: &mut String, content: &str, path: &str) {
    output.push_str("<pre>");
    for line in highlight_diff_lines(content, path) {
        output.push_str("<span class=\"line");
        if let Some(class) = line.kind {
            output.push(' ');
            output.push_str(class);
        }
        output.push_str("\">");
        render_tokens(output, &line.tokens);
        output.push_str("</span>");
    }
    output.push_str("</pre>\n");
}

pub(in crate::server) fn highlight_diff_lines(
    content: &str,
    path: &str,
) -> Vec<HighlightedDiffLine> {
    let syntax_set = &SYNTAX_SET;
    let syntax = syntax_for_diff(syntax_set, path, content);
    let is_rust = syntax.name == "Rust";
    let mut highlighter = HighlightLines::new(syntax, &TOKEN_THEME);
    let mut lines = Vec::new();

    for line in content.lines() {
        let kind = diff_line_class(line);
        if matches!(kind, Some("file" | "hunk")) {
            highlighter = HighlightLines::new(syntax, &TOKEN_THEME);
            lines.push(HighlightedDiffLine {
                kind,
                tokens: vec![plain_token(line)],
            });
            continue;
        }

        let tokens = if let Some(first) = line.chars().next() {
            if matches!(first, '+' | '-' | ' ') {
                let mut marker = [0; 4];
                let mut tokens = vec![plain_token(first.encode_utf8(&mut marker))];
                tokens.extend(highlight_syntax_line(
                    &mut highlighter,
                    syntax_set,
                    is_rust,
                    &line[first.len_utf8()..],
                ));
                tokens
            } else {
                highlight_syntax_line(&mut highlighter, syntax_set, is_rust, line)
            }
        } else {
            vec![plain_token(" ")]
        };

        lines.push(HighlightedDiffLine { kind, tokens });
    }

    lines
}

pub(in crate::server) fn highlight_source_lines(
    content: &str,
    path: &str,
) -> Vec<HighlightedDiffLine> {
    let syntax_set = &SYNTAX_SET;
    let syntax = syntax_for_file_name(syntax_set, path)
        .or_else(|| {
            content
                .lines()
                .next()
                .and_then(|line| syntax_set.find_syntax_by_first_line(line))
        })
        .unwrap_or_else(|| syntax_set.find_syntax_plain_text());
    let is_rust = syntax.name == "Rust";
    let mut highlighter = HighlightLines::new(syntax, &TOKEN_THEME);

    content
        .split('\n')
        .map(|line| HighlightedDiffLine {
            kind: None,
            tokens: highlight_syntax_line(&mut highlighter, syntax_set, is_rust, line),
        })
        .collect()
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

#[cfg(test)]
pub(in crate::server) fn syntax_name_for_path(path: &str) -> &'static str {
    syntax_for_path(&SYNTAX_SET, path).name.as_str()
}

fn syntax_for_diff<'a>(
    syntax_set: &'a SyntaxSet,
    path: &str,
    content: &str,
) -> &'a SyntaxReference {
    syntax_for_file_name(syntax_set, path)
        .or_else(|| {
            first_source_line(content).and_then(|line| syntax_set.find_syntax_by_first_line(line))
        })
        .unwrap_or_else(|| syntax_set.find_syntax_plain_text())
}

#[cfg(test)]
fn syntax_for_path<'a>(syntax_set: &'a SyntaxSet, path: &str) -> &'a SyntaxReference {
    syntax_for_file_name(syntax_set, path).unwrap_or_else(|| syntax_set.find_syntax_plain_text())
}

fn syntax_for_file_name<'a>(syntax_set: &'a SyntaxSet, path: &str) -> Option<&'a SyntaxReference> {
    let path = Path::new(path);
    let file_name = path.file_name().and_then(|name| name.to_str())?;
    let extension = path.extension().and_then(|extension| extension.to_str());

    syntax_set
        .find_syntax_by_extension(file_name)
        .or_else(|| extension.and_then(|extension| syntax_set.find_syntax_by_extension(extension)))
}

fn first_source_line(content: &str) -> Option<&str> {
    content.lines().find_map(|line| {
        if matches!(diff_line_class(line), Some("file" | "hunk")) {
            return None;
        }

        let first = line.chars().next()?;
        Some(if matches!(first, '+' | '-' | ' ') {
            &line[first.len_utf8()..]
        } else {
            line
        })
        .filter(|line| !line.trim().is_empty())
    })
}

fn highlight_syntax_line(
    highlighter: &mut HighlightLines<'_>,
    syntax_set: &SyntaxSet,
    is_rust: bool,
    line: &str,
) -> Vec<SyntaxToken> {
    if line.is_empty() {
        return Vec::new();
    }

    match highlighter.highlight_line(line, syntax_set) {
        Ok(ranges) => tokens_from_ranges(ranges, is_rust),
        Err(_) => vec![plain_token(line)],
    }
}

fn tokens_from_ranges(ranges: Vec<(Style, &str)>, is_rust: bool) -> Vec<SyntaxToken> {
    let mut tokens = Vec::<SyntaxToken>::new();
    for (style, text) in ranges {
        if text.is_empty() {
            continue;
        }

        push_token(
            &mut tokens,
            SyntaxToken {
                text: text.to_string(),
                kind: token_kind_for_style(style, text, is_rust),
            },
        );
    }
    tokens
}

fn push_token(tokens: &mut Vec<SyntaxToken>, token: SyntaxToken) {
    if let Some(previous) = tokens.last_mut() {
        if previous.kind == token.kind {
            previous.text.push_str(&token.text);
            return;
        }
    }

    tokens.push(token);
}

fn plain_token(text: &str) -> SyntaxToken {
    SyntaxToken {
        text: text.to_string(),
        kind: None,
    }
}

fn token_kind_for_style(style: Style, text: &str, is_rust: bool) -> Option<SyntaxTokenKind> {
    if is_rust && is_rust_primitive_type(text) {
        return Some(SyntaxTokenKind::Type);
    }

    match style.foreground {
        color if color == SyntaxTokenKind::Comment.marker_color() => Some(SyntaxTokenKind::Comment),
        color if color == SyntaxTokenKind::Keyword.marker_color() => Some(SyntaxTokenKind::Keyword),
        color if color == SyntaxTokenKind::Number.marker_color() => Some(SyntaxTokenKind::Number),
        color if color == SyntaxTokenKind::Property.marker_color() => {
            Some(SyntaxTokenKind::Property)
        }
        color if color == SyntaxTokenKind::Punctuation.marker_color() => {
            Some(SyntaxTokenKind::Punctuation)
        }
        color if color == SyntaxTokenKind::String.marker_color() => Some(SyntaxTokenKind::String),
        color if color == SyntaxTokenKind::Type.marker_color() => Some(SyntaxTokenKind::Type),
        _ => None,
    }
}

fn is_rust_primitive_type(text: &str) -> bool {
    matches!(
        text,
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
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "usize"
    )
}

fn render_tokens(output: &mut String, tokens: &[SyntaxToken]) {
    for token in tokens {
        if let Some(kind) = token.kind {
            render_token_span(output, kind.class_name(), &token.text);
        } else {
            output.push_str(&escape_html_text(&token.text));
        }
    }
}

fn render_token_span(output: &mut String, class: &str, token: &str) {
    output.push_str("<span class=\"");
    output.push_str(class);
    output.push_str("\">");
    output.push_str(&escape_html_text(token));
    output.push_str("</span>");
}

fn token_theme() -> Theme {
    Theme {
        name: Some("Latitude token classes".to_string()),
        settings: syntect::highlighting::ThemeSettings {
            foreground: Some(DEFAULT_COLOR),
            ..Default::default()
        },
        scopes: vec![
            theme_item("comment", SyntaxTokenKind::Comment, Some(FontStyle::ITALIC)),
            theme_item(
                "comment punctuation",
                SyntaxTokenKind::Comment,
                Some(FontStyle::ITALIC),
            ),
            theme_item("string", SyntaxTokenKind::String, None),
            theme_item("string punctuation", SyntaxTokenKind::String, None),
            theme_item("constant.character.escape", SyntaxTokenKind::String, None),
            theme_item("constant.numeric", SyntaxTokenKind::Number, None),
            theme_item("constant.language", SyntaxTokenKind::Keyword, None),
            theme_item("keyword", SyntaxTokenKind::Keyword, Some(FontStyle::BOLD)),
            theme_item("storage", SyntaxTokenKind::Keyword, Some(FontStyle::BOLD)),
            theme_item(
                "storage.type.function",
                SyntaxTokenKind::Keyword,
                Some(FontStyle::BOLD),
            ),
            theme_item("variable.language", SyntaxTokenKind::Keyword, None),
            theme_item("support.type", SyntaxTokenKind::Type, None),
            theme_item("support.class", SyntaxTokenKind::Type, None),
            theme_item("entity.name.type", SyntaxTokenKind::Type, None),
            theme_item("entity.name.class", SyntaxTokenKind::Type, None),
            theme_item("entity.name.enum", SyntaxTokenKind::Type, None),
            theme_item("entity.name.struct", SyntaxTokenKind::Type, None),
            theme_item("entity.name.trait", SyntaxTokenKind::Type, None),
            theme_item("entity.name.tag", SyntaxTokenKind::Keyword, None),
            theme_item(
                "entity.other.attribute-name",
                SyntaxTokenKind::Property,
                None,
            ),
            theme_item(
                "support.type.property-name",
                SyntaxTokenKind::Property,
                None,
            ),
            theme_item("variable.other.member", SyntaxTokenKind::Property, None),
            theme_item("meta.mapping.key", SyntaxTokenKind::Property, None),
            theme_item("meta.object-literal.key", SyntaxTokenKind::Property, None),
            theme_item("punctuation", SyntaxTokenKind::Punctuation, None),
        ],
        ..Default::default()
    }
}

fn theme_item(scope: &str, kind: SyntaxTokenKind, font_style: Option<FontStyle>) -> ThemeItem {
    ThemeItem {
        scope: ScopeSelectors::from_str(scope).expect("token theme scope selector is valid"),
        style: StyleModifier {
            foreground: Some(kind.marker_color()),
            background: None,
            font_style,
        },
    }
}

const fn color(index: u8) -> Color {
    Color {
        r: index,
        g: 0,
        b: 0,
        a: 255,
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
