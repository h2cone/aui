use gpui::{
    App, Bounds, Element, ElementId, GlobalElementId, IntoElement, LayoutId, Pixels, ShapedLine,
    SharedString, Style, TextRun, Window, point, relative, rgb,
};

pub struct HighlightedCode {
    pub text: SharedString,
    pub language: SharedString,
}

pub struct HighlightedCodeState {
    lines: Vec<ShapedLine>,
    line_height: Pixels,
}

impl IntoElement for HighlightedCode {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for HighlightedCode {
    type RequestLayoutState = ();
    type PrepaintState = HighlightedCodeState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let line_count = self.text.split('\n').count().max(1) as f32;
        let line_height = window.line_height();
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = (line_height * line_count).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        _cx: &mut App,
    ) -> Self::PrepaintState {
        let style = window.text_style();
        let font_size = style.font_size.to_pixels(window.rem_size());
        let line_height = window.line_height();
        let theme = HighlightTheme::default_with_base(style.color);
        let font = style.font();
        let mut lines = Vec::new();

        for line in self.text.split('\n') {
            let runs = highlight_runs_for_line(line, self.language.as_ref(), &theme, &font);
            lines.push(window.text_system().shape_line(
                SharedString::from(line.to_string()),
                font_size,
                &runs,
                None,
            ));
        }

        HighlightedCodeState { lines, line_height }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let lines = std::mem::take(&mut prepaint.lines);
        let line_height = prepaint.line_height;
        for (line_ix, line) in lines.iter().enumerate() {
            let y = bounds.top() + line_height * line_ix as f32;
            let _ = line.paint(point(bounds.left(), y), line_height, window, cx);
        }
    }
}

#[derive(Clone, Copy)]
struct HighlightTheme {
    base: gpui::Hsla,
    keyword: gpui::Hsla,
    string: gpui::Hsla,
    comment: gpui::Hsla,
    number: gpui::Hsla,
}

impl HighlightTheme {
    fn default_with_base(base: gpui::Hsla) -> Self {
        Self {
            base,
            keyword: rgb(0x2563eb).into(),
            string: rgb(0x15803d).into(),
            comment: rgb(0x6b7280).into(),
            number: rgb(0x0f766e).into(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SpanKind {
    Normal,
    Keyword,
    String,
    Comment,
    Number,
}

struct Span {
    text: String,
    kind: SpanKind,
}

fn highlight_runs_for_line(
    line: &str,
    language: &str,
    theme: &HighlightTheme,
    font: &gpui::Font,
) -> Vec<TextRun> {
    let spans = highlight_line(line, language);
    let mut runs = Vec::new();
    for span in spans {
        let color = match span.kind {
            SpanKind::Normal => theme.base,
            SpanKind::Keyword => theme.keyword,
            SpanKind::String => theme.string,
            SpanKind::Comment => theme.comment,
            SpanKind::Number => theme.number,
        };
        runs.push(TextRun {
            len: span.text.len(),
            font: font.clone(),
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        });
    }
    if runs.is_empty() {
        runs.push(TextRun {
            len: 0,
            font: font.clone(),
            color: theme.base,
            background_color: None,
            underline: None,
            strikethrough: None,
        });
    }
    runs
}

fn highlight_line(line: &str, language: &str) -> Vec<Span> {
    let markers = comment_markers(language);
    let allow_backtick = allow_backtick(language);
    let keywords = keywords(language);
    let mut spans = Vec::new();
    let mut index = 0;
    let mut segment_start = 0;

    while index < line.len() {
        if starts_with_marker(line, index, markers).is_some() {
            if segment_start < index {
                spans.push(Span {
                    text: line[segment_start..index].to_string(),
                    kind: SpanKind::Normal,
                });
            }
            spans.push(Span {
                text: line[index..].to_string(),
                kind: SpanKind::Comment,
            });
            return expand_keywords(spans, keywords);
        }

        let ch = line[index..].chars().next().unwrap_or('\0');
        if is_quote(ch, allow_backtick) {
            if segment_start < index {
                spans.push(Span {
                    text: line[segment_start..index].to_string(),
                    kind: SpanKind::Normal,
                });
            }
            let end = scan_string(line, index, ch);
            spans.push(Span {
                text: line[index..end].to_string(),
                kind: SpanKind::String,
            });
            index = end;
            segment_start = index;
            continue;
        }
        index += ch.len_utf8();
    }

    if segment_start < line.len() {
        spans.push(Span {
            text: line[segment_start..].to_string(),
            kind: SpanKind::Normal,
        });
    }

    expand_keywords(spans, keywords)
}

fn expand_keywords(spans: Vec<Span>, keywords: &'static [&'static str]) -> Vec<Span> {
    let mut expanded = Vec::new();
    for span in spans {
        if span.kind != SpanKind::Normal {
            expanded.push(span);
            continue;
        }
        expanded.extend(split_keywords(&span.text, keywords));
    }
    expanded
}

fn split_keywords(text: &str, keywords: &'static [&'static str]) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut buffer = String::new();
    let mut index = 0;
    while index < text.len() {
        let ch = text[index..].chars().next().unwrap_or('\0');
        if is_word_char(ch) {
            if !buffer.is_empty() {
                spans.push(Span {
                    text: buffer.clone(),
                    kind: SpanKind::Normal,
                });
                buffer.clear();
            }
            let start = index;
            let mut end = index + ch.len_utf8();
            while end < text.len() {
                let next = text[end..].chars().next().unwrap_or('\0');
                if !is_word_char(next) {
                    break;
                }
                end += next.len_utf8();
            }
            let word = &text[start..end];
            let kind = if keywords.contains(&word) {
                SpanKind::Keyword
            } else if is_number(word) {
                SpanKind::Number
            } else {
                SpanKind::Normal
            };
            spans.push(Span {
                text: word.to_string(),
                kind,
            });
            index = end;
        } else {
            buffer.push(ch);
            index += ch.len_utf8();
        }
    }
    if !buffer.is_empty() {
        spans.push(Span {
            text: buffer,
            kind: SpanKind::Normal,
        });
    }
    spans
}

fn is_number(word: &str) -> bool {
    let mut seen_digit = false;
    for ch in word.chars() {
        if ch.is_ascii_digit() {
            seen_digit = true;
            continue;
        }
        if ch == '.' || ch == '_' {
            continue;
        }
        return false;
    }
    seen_digit
}

fn is_word_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

fn is_quote(ch: char, allow_backtick: bool) -> bool {
    ch == '"' || ch == '\'' || (allow_backtick && ch == '`')
}

fn scan_string(line: &str, start: usize, quote: char) -> usize {
    let mut index = start + quote.len_utf8();
    let mut escaped = false;
    while index < line.len() {
        let ch = line[index..].chars().next().unwrap_or('\0');
        if escaped {
            escaped = false;
            index += ch.len_utf8();
            continue;
        }
        if ch == '\\' {
            escaped = true;
            index += ch.len_utf8();
            continue;
        }
        index += ch.len_utf8();
        if ch == quote {
            break;
        }
    }
    index
}

fn starts_with_marker<'a>(
    line: &'a str,
    index: usize,
    markers: &'static [&'static str],
) -> Option<&'a str> {
    let slice = &line[index..];
    for marker in markers {
        if slice.starts_with(marker) {
            return Some(slice);
        }
    }
    None
}

fn allow_backtick(language: &str) -> bool {
    is_language(
        language,
        &["js", "javascript", "ts", "typescript", "jsx", "tsx"],
    )
}

fn comment_markers(language: &str) -> &'static [&'static str] {
    if is_language(
        language,
        &[
            "py", "python", "sh", "bash", "zsh", "toml", "yaml", "yml", "ini", "cfg",
        ],
    ) {
        &["#"]
    } else if is_language(language, &["sql", "lua"]) {
        &["--"]
    } else {
        &["//"]
    }
}

fn keywords(language: &str) -> &'static [&'static str] {
    if is_language(language, &["rust", "rs"]) {
        RUST_KEYWORDS
    } else if is_language(
        language,
        &["js", "javascript", "ts", "typescript", "jsx", "tsx"],
    ) {
        JS_KEYWORDS
    } else if is_language(language, &["python", "py"]) {
        PY_KEYWORDS
    } else if is_language(language, &["go", "golang"]) {
        GO_KEYWORDS
    } else if is_language(language, &["sh", "bash", "zsh", "shell"]) {
        SH_KEYWORDS
    } else {
        GENERIC_KEYWORDS
    }
}

fn is_language(language: &str, candidates: &[&str]) -> bool {
    candidates
        .iter()
        .any(|candidate| language.eq_ignore_ascii_case(candidate))
}

const RUST_KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern",
    "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub",
    "ref", "return", "self", "Self", "static", "struct", "super", "trait", "true", "type",
    "unsafe", "use", "where", "while",
];

const JS_KEYWORDS: &[&str] = &[
    "async",
    "await",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "default",
    "delete",
    "do",
    "else",
    "export",
    "extends",
    "false",
    "finally",
    "for",
    "function",
    "if",
    "import",
    "in",
    "instanceof",
    "let",
    "new",
    "null",
    "return",
    "super",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "typeof",
    "var",
    "void",
    "while",
    "with",
    "yield",
];

const PY_KEYWORDS: &[&str] = &[
    "and", "as", "assert", "break", "class", "continue", "def", "del", "elif", "else", "except",
    "False", "finally", "for", "from", "global", "if", "import", "in", "is", "lambda", "None",
    "nonlocal", "not", "or", "pass", "raise", "return", "True", "try", "while", "with", "yield",
];

const GO_KEYWORDS: &[&str] = &[
    "break",
    "case",
    "chan",
    "const",
    "continue",
    "default",
    "defer",
    "else",
    "fallthrough",
    "for",
    "func",
    "go",
    "goto",
    "if",
    "import",
    "interface",
    "map",
    "package",
    "range",
    "return",
    "select",
    "struct",
    "switch",
    "type",
    "var",
];

const SH_KEYWORDS: &[&str] = &[
    "case", "do", "done", "elif", "else", "esac", "exit", "export", "fi", "for", "function", "if",
    "in", "local", "return", "then", "until", "while",
];

const GENERIC_KEYWORDS: &[&str] = &[
    "async", "await", "break", "case", "catch", "class", "const", "continue", "def", "else",
    "enum", "false", "fn", "for", "if", "impl", "import", "let", "match", "new", "null", "return",
    "struct", "switch", "this", "true", "type", "use", "while",
];
