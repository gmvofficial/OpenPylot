//! Lightweight syntax highlighting for code shown in the terminal.
//!
//! Deliberately not `syntect`: a full grammar engine would add a heavy
//! dependency and seconds of build time to colour a handful of lines in a chat
//! transcript. A token-level pass over strings, comments, numbers and a
//! per-language keyword set covers what actually reads as "highlighted" at this
//! size, and degrades to plain text for anything it does not know.
//!
//! Correctness bar: never mangle the text. Every input byte appears in the
//! output exactly once, in order — only the styling varies. The tests enforce
//! that for every supported language.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

/// The token classes the highlighter distinguishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    Plain,
    Keyword,
    Str,
    Comment,
    Number,
    Type,
}

impl Class {
    fn style(self) -> Style {
        match self {
            Class::Plain => Style::default(),
            Class::Keyword => Style::default().fg(Color::Magenta),
            Class::Str => Style::default().fg(Color::Green),
            Class::Comment => Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
            Class::Number => Style::default().fg(Color::Yellow),
            Class::Type => Style::default().fg(Color::Cyan),
        }
    }
}

/// A language's lexical rules.
struct Syntax {
    keywords: &'static [&'static str],
    types: &'static [&'static str],
    /// Sequences that start a comment running to end of line.
    line_comments: &'static [&'static str],
    /// Quote characters that delimit strings.
    quotes: &'static [char],
}

const RUST: Syntax = Syntax {
    keywords: &[
        "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum",
        "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move",
        "mut", "pub", "ref", "return", "self", "Self", "static", "struct", "super", "trait",
        "true", "type", "unsafe", "use", "where", "while",
    ],
    types: &[
        "bool", "char", "f32", "f64", "i8", "i16", "i32", "i64", "i128", "isize", "str", "u8",
        "u16", "u32", "u64", "u128", "usize", "String", "Vec", "Option", "Result", "Box", "Arc",
        "Rc", "HashMap", "HashSet",
    ],
    line_comments: &["//"],
    quotes: &['"'],
};

const PYTHON: Syntax = Syntax {
    keywords: &[
        "and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del",
        "elif", "else", "except", "False", "finally", "for", "from", "global", "if", "import",
        "in", "is", "lambda", "None", "nonlocal", "not", "or", "pass", "raise", "return", "True",
        "try", "while", "with", "yield",
    ],
    types: &["bool", "bytes", "dict", "float", "int", "list", "set", "str", "tuple"],
    line_comments: &["#"],
    quotes: &['"', '\''],
};

const JAVASCRIPT: Syntax = Syntax {
    keywords: &[
        "async", "await", "break", "case", "catch", "class", "const", "continue", "default",
        "delete", "do", "else", "export", "extends", "false", "finally", "for", "from", "function",
        "if", "import", "in", "instanceof", "let", "new", "null", "of", "return", "static",
        "super", "switch", "this", "throw", "true", "try", "typeof", "undefined", "var", "void",
        "while", "yield",
    ],
    types: &[
        "Array", "Boolean", "Number", "Object", "Promise", "String", "Symbol", "any", "boolean",
        "number", "string", "unknown", "void",
    ],
    line_comments: &["//"],
    quotes: &['"', '\'', '`'],
};

const SHELL: Syntax = Syntax {
    keywords: &[
        "case", "do", "done", "elif", "else", "esac", "export", "fi", "for", "function", "if",
        "in", "local", "return", "then", "until", "while",
    ],
    types: &[],
    line_comments: &["#"],
    quotes: &['"', '\''],
};

const SQL: Syntax = Syntax {
    keywords: &[
        "AND", "AS", "ASC", "BY", "CASE", "CREATE", "CROSS", "DELETE", "DESC", "DISTINCT", "DROP",
        "ELSE", "END", "FROM", "FULL", "GROUP", "HAVING", "IN", "INNER", "INSERT", "INTO", "JOIN",
        "LEFT", "LIKE", "LIMIT", "NOT", "NULL", "OFFSET", "ON", "OR", "ORDER", "OUTER", "RIGHT",
        "SELECT", "SET", "THEN", "UNION", "UPDATE", "VALUES", "WHEN", "WHERE", "WITH",
    ],
    types: &["BOOLEAN", "DATE", "FLOAT", "INT", "INTEGER", "REAL", "TEXT", "TIMESTAMP", "VARCHAR"],
    line_comments: &["--"],
    quotes: &['\'', '"'],
};

const JSON_LIKE: Syntax = Syntax {
    keywords: &["true", "false", "null"],
    types: &[],
    line_comments: &[],
    quotes: &['"'],
};

/// Resolve a fenced-code-block language tag to a syntax, if we know it.
fn syntax_for(lang: &str) -> Option<&'static Syntax> {
    let lang = lang.trim().to_ascii_lowercase();
    // Strip anything after the first non-identifier char, so ```rust,ignore works.
    let lang = lang
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '+' && c != '#')
        .next()
        .unwrap_or("");

    Some(match lang {
        "rust" | "rs" => &RUST,
        "python" | "py" => &PYTHON,
        "javascript" | "js" | "jsx" | "typescript" | "ts" | "tsx" => &JAVASCRIPT,
        "bash" | "sh" | "shell" | "zsh" | "console" => &SHELL,
        "sql" => &SQL,
        "json" | "json5" | "jsonc" => &JSON_LIKE,
        _ => return None,
    })
}

/// Whether a language tag is one we can highlight.
pub fn is_supported(lang: &str) -> bool {
    syntax_for(lang).is_some()
}

/// Highlight one line of code, returning styled spans.
///
/// For an unknown language every character comes back as one plain span, so
/// callers never need a separate "no highlighting" path.
pub fn line<'a>(code: &'a str, lang: &str) -> Line<'a> {
    let Some(syntax) = syntax_for(lang) else {
        return Line::from(code);
    };
    Line::from(spans(code, syntax))
}

/// Tokenize one line into styled spans.
///
/// Strings are not continued across lines: a transcript renders line by line,
/// and mis-tracking an unterminated quote would tint the rest of the block.
/// Treating each line independently is the more forgiving failure.
fn spans<'a>(code: &'a str, syntax: &Syntax) -> Vec<Span<'a>> {
    let mut out: Vec<Span<'a>> = Vec::new();
    let bytes = code.as_bytes();
    let mut i = 0usize;
    // Start of the current run of plain text not yet flushed.
    let mut plain_start = 0usize;

    let flush = |out: &mut Vec<Span<'a>>, from: usize, to: usize| {
        if to > from {
            out.push(Span::raw(&code[from..to]));
        }
    };

    while i < bytes.len() {
        // Comments run to end of line — emit and stop.
        if let Some(marker) = syntax
            .line_comments
            .iter()
            .find(|m| code[i..].starts_with(**m))
        {
            let _ = marker;
            flush(&mut out, plain_start, i);
            out.push(Span::styled(&code[i..], Class::Comment.style()));
            return out;
        }

        let ch = code[i..].chars().next().unwrap_or('\0');

        // Strings.
        if syntax.quotes.contains(&ch) {
            flush(&mut out, plain_start, i);
            let end = string_end(code, i, ch);
            out.push(Span::styled(&code[i..end], Class::Str.style()));
            i = end;
            plain_start = i;
            continue;
        }

        // Numbers: only when they start a token, so `x1` is not "x" + number 1.
        if ch.is_ascii_digit() && !preceded_by_ident_char(code, i) {
            flush(&mut out, plain_start, i);
            let end = number_end(code, i);
            out.push(Span::styled(&code[i..end], Class::Number.style()));
            i = end;
            plain_start = i;
            continue;
        }

        // Identifiers → keyword / type / plain.
        if is_ident_start(ch) {
            let end = ident_end(code, i);
            let word = &code[i..end];
            let class = classify(word, syntax);
            if class != Class::Plain {
                flush(&mut out, plain_start, i);
                out.push(Span::styled(word, class.style()));
                plain_start = end;
            }
            i = end;
            continue;
        }

        i += ch.len_utf8();
    }

    flush(&mut out, plain_start, bytes.len());
    out
}

fn classify(word: &str, syntax: &Syntax) -> Class {
    if syntax.keywords.contains(&word) {
        return Class::Keyword;
    }
    if syntax.types.contains(&word) {
        return Class::Type;
    }
    // SQL is conventionally written in either case; match keywords case-insensitively.
    if std::ptr::eq(syntax, &SQL) {
        let upper = word.to_ascii_uppercase();
        if syntax.keywords.contains(&upper.as_str()) {
            return Class::Keyword;
        }
        if syntax.types.contains(&upper.as_str()) {
            return Class::Type;
        }
    }
    Class::Plain
}

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn preceded_by_ident_char(code: &str, i: usize) -> bool {
    code[..i].chars().next_back().is_some_and(is_ident_char)
}

fn ident_end(code: &str, start: usize) -> usize {
    let mut end = start;
    for c in code[start..].chars() {
        if !is_ident_char(c) {
            break;
        }
        end += c.len_utf8();
    }
    end
}

fn number_end(code: &str, start: usize) -> usize {
    let mut end = start;
    for c in code[start..].chars() {
        // Accept hex digits, separators and suffixes so `0xFF_u8` stays one token.
        if c.is_ascii_alphanumeric() || c == '.' || c == '_' {
            end += c.len_utf8();
        } else {
            break;
        }
    }
    end
}

/// Byte index just past the closing quote, or end of line if unterminated.
///
/// Honours backslash escapes so `"a\"b"` is one string.
fn string_end(code: &str, start: usize, quote: char) -> usize {
    let mut chars = code[start..].char_indices();
    // Consume the opening quote.
    chars.next();

    let mut escaped = false;
    for (offset, c) in chars {
        if escaped {
            escaped = false;
            continue;
        }
        if c == '\\' {
            escaped = true;
            continue;
        }
        if c == quote {
            return start + offset + c.len_utf8();
        }
    }
    code.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The invariant that matters most: highlighting must never change the text.
    fn assert_text_preserved(code: &str, lang: &str) {
        let rendered: String = line(code, lang)
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert_eq!(rendered, code, "highlighting altered the text for lang={lang}");
    }

    #[test]
    fn text_is_preserved_across_every_language() {
        let samples = [
            ("rust", r#"pub fn main() { let x: u32 = 0xFF_u8 as u32; // note"#),
            ("python", "def f(a, b=1):\n    return 'x' + \"y\"  # c"),
            ("ts", "const x: Promise<string> = await fetch(`/a/${b}`);"),
            ("bash", "for f in *.rs; do echo \"$f\"; done # loop"),
            ("sql", "SELECT a, COUNT(*) FROM t WHERE b = 'x' -- c"),
            ("json", r#"{"a": 1, "b": [true, null]}"#),
            ("brainfuck", "+++[->+<]"),
            ("", "no language tag at all"),
        ];
        for (lang, code) in samples {
            assert_text_preserved(code, lang);
        }
    }

    #[test]
    fn text_is_preserved_for_pathological_input() {
        for code in [
            "",
            "\"unterminated",
            "'",
            "\\",
            "\"a\\\"b\"",
            "héllo wörld 123",
            "🎉 emoji \"str\" 42",
            "////////",
            "0x",
            "1.2.3.4",
        ] {
            for lang in ["rust", "python", "sql", "bash", "js", "json"] {
                assert_text_preserved(code, lang);
            }
        }
    }

    fn classes(code: &str, lang: &str) -> Vec<(String, Style)> {
        line(code, lang)
            .spans
            .iter()
            .map(|s| (s.content.to_string(), s.style))
            .collect()
    }

    fn has_styled(code: &str, lang: &str, text: &str, class: Class) -> bool {
        classes(code, lang)
            .iter()
            .any(|(c, s)| c == text && *s == class.style())
    }

    #[test]
    fn rust_keywords_and_types_are_distinguished() {
        let code = "pub fn f(v: Vec<u8>) -> Option<String> {}";
        assert!(has_styled(code, "rust", "fn", Class::Keyword));
        assert!(has_styled(code, "rust", "Vec", Class::Type));
        assert!(has_styled(code, "rust", "u8", Class::Type));
    }

    #[test]
    fn comments_swallow_the_rest_of_the_line() {
        let spans = classes("let x = 1; // set x to 1", "rust");
        let comment = spans
            .iter()
            .find(|(_, s)| *s == Class::Comment.style())
            .expect("a comment span");
        assert_eq!(comment.0, "// set x to 1");
    }

    #[test]
    fn a_hash_inside_a_string_is_not_a_comment() {
        // The string is scanned before the comment marker is considered.
        let spans = classes("x = '# not a comment'", "python");
        assert!(spans.iter().all(|(_, s)| *s != Class::Comment.style()));
    }

    #[test]
    fn escaped_quotes_do_not_end_a_string() {
        let code = r#"let s = "a\"b"; let t = 1;"#;
        assert!(has_styled(code, "rust", r#""a\"b""#, Class::Str));
        // The trailing code is still tokenized, which proves the string closed.
        assert!(has_styled(code, "rust", "let", Class::Keyword));
    }

    #[test]
    fn an_unterminated_string_stops_at_end_of_line() {
        // Rather than leaking into the next line of the transcript.
        assert_text_preserved("let s = \"oops", "rust");
        assert!(has_styled("let s = \"oops", "rust", "\"oops", Class::Str));
    }

    #[test]
    fn digits_inside_an_identifier_are_not_numbers() {
        let spans = classes("let x1 = 2;", "rust");
        assert!(
            !spans.iter().any(|(c, s)| c == "1" && *s == Class::Number.style()),
            "x1 must stay one identifier: {spans:?}"
        );
        assert!(has_styled("let x1 = 2;", "rust", "2", Class::Number));
    }

    #[test]
    fn sql_keywords_match_in_either_case() {
        assert!(has_styled("select * from t", "sql", "select", Class::Keyword));
        assert!(has_styled("SELECT * FROM t", "sql", "SELECT", Class::Keyword));
    }

    #[test]
    fn unknown_languages_come_back_as_a_single_plain_span() {
        let l = line("anything at all", "cobol");
        assert_eq!(l.spans.len(), 1);
        assert_eq!(l.spans[0].content.as_ref(), "anything at all");
    }

    #[test]
    fn language_tags_with_attributes_still_resolve() {
        assert!(is_supported("rust,ignore"));
        assert!(is_supported("rust ignore"));
        assert!(is_supported("PYTHON"));
        assert!(!is_supported("cobol"));
        assert!(!is_supported(""));
    }
}
