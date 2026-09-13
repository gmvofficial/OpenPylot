//! Markdown → styled terminal lines.
//!
//! Models drive this output, so the renderer has to be forgiving: unbalanced
//! emphasis, an unclosed code fence, a table with a ragged row. Nothing here
//! may panic or lose text on malformed input — a half-streamed response is
//! malformed by definition, since it is rendered before it is finished.
//!
//! Scope is what actually shows up in a chat transcript: headings, fenced and
//! inline code, lists, block quotes, bold/italic, links and horizontal rules.
//! Tables are rendered as their raw source; aligning them properly inside a
//! wrapping transcript costs more than it returns at this size.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use super::highlight;
use super::theme::Theme;

/// Render markdown into styled lines, wrapped to `width` columns.
///
/// `width` is the full text column; the caller has already subtracted any
/// gutter. A width of 0 is treated as unbounded rather than panicking, because
/// a terminal can legitimately report a zero-width area mid-resize.
pub fn render(source: &str, width: usize, theme: &Theme) -> Vec<Line<'static>> {
    let width = if width == 0 { usize::MAX } else { width };
    let mut out = Vec::new();
    let mut fence: Option<Fence> = None;

    for raw in source.split('\n') {
        // Inside a fence, everything is code until the fence closes.
        if let Some(open) = &fence {
            if is_closing_fence(raw, open) {
                fence = None;
                continue;
            }
            out.push(code_line(raw, &open.lang, theme));
            continue;
        }

        if let Some(open) = opening_fence(raw) {
            fence = Some(open);
            continue;
        }

        render_block_line(raw, width, theme, &mut out);
    }

    // An unclosed fence is normal while streaming — the lines already emitted
    // stand on their own, so there is nothing to fix up here.
    out
}

struct Fence {
    marker: char,
    len: usize,
    lang: String,
}

fn opening_fence(line: &str) -> Option<Fence> {
    let trimmed = line.trim_start();
    let marker = trimmed.chars().next()?;
    if marker != '`' && marker != '~' {
        return None;
    }
    let len = trimmed.chars().take_while(|c| *c == marker).count();
    if len < 3 {
        return None;
    }
    let lang = trimmed[len..].trim().to_string();
    Some(Fence { marker, len, lang })
}

fn is_closing_fence(line: &str, open: &Fence) -> bool {
    let trimmed = line.trim();
    let len = trimmed.chars().take_while(|c| *c == open.marker).count();
    len >= open.len && trimmed.chars().all(|c| c == open.marker)
}

fn code_line(text: &str, lang: &str, theme: &Theme) -> Line<'static> {
    let mut spans = vec![Span::styled("  │ ".to_string(), theme.dim())];
    if highlight::is_supported(lang) {
        for span in highlight::line(text, lang).spans {
            spans.push(Span::styled(span.content.into_owned(), span.style));
        }
    } else {
        spans.push(Span::styled(text.to_string(), theme.dim()));
    }
    Line::from(spans)
}

/// Render one non-code line, wrapping as needed and appending to `out`.
fn render_block_line(raw: &str, width: usize, theme: &Theme, out: &mut Vec<Line<'static>>) {
    let trimmed = raw.trim_end();

    if trimmed.trim().is_empty() {
        out.push(Line::from(""));
        return;
    }

    // Horizontal rule.
    let bare = trimmed.trim();
    if bare.len() >= 3 && (bare.chars().all(|c| c == '-') || bare.chars().all(|c| c == '*')) {
        let n = width.min(60);
        out.push(Line::from(Span::styled("─".repeat(n), theme.dim())));
        return;
    }

    // Heading.
    if let Some(rest) = bare.strip_prefix('#') {
        let level = 1 + rest.chars().take_while(|c| *c == '#').count();
        let text = rest.trim_start_matches('#').trim();
        if level <= 6 && !text.is_empty() {
            let style = theme.heading();
            for line in wrap_spans(inline(text, style, theme), width, "") {
                out.push(line);
            }
            return;
        }
    }

    // Block quote.
    if let Some(rest) = bare.strip_prefix('>') {
        let inner = rest.trim_start();
        let prefix = "▏ ";
        let avail = width.saturating_sub(prefix.chars().count()).max(8);
        for (i, line) in wrap_spans(inline(inner, theme.dim(), theme), avail, "").into_iter().enumerate() {
            let mut spans = vec![Span::styled(prefix.to_string(), theme.dim())];
            spans.extend(line.spans);
            let _ = i;
            out.push(Line::from(spans));
        }
        return;
    }

    // List item — preserve the author's indentation, swap the bullet glyph.
    if let Some((marker, content, indent)) = list_item(trimmed) {
        let prefix = format!("{indent}{marker} ");
        let continuation = " ".repeat(prefix.chars().count());
        let avail = width.saturating_sub(prefix.chars().count()).max(8);

        let wrapped = wrap_spans(inline(content, theme.body(), theme), avail, "");
        for (i, line) in wrapped.into_iter().enumerate() {
            let lead = if i == 0 { prefix.clone() } else { continuation.clone() };
            let mut spans = vec![Span::styled(lead, if i == 0 { theme.accented() } else { theme.body() })];
            spans.extend(line.spans);
            out.push(Line::from(spans));
        }
        return;
    }

    for line in wrap_spans(inline(trimmed, theme.body(), theme), width, "") {
        out.push(line);
    }
}

/// Detect a list item, returning (bullet to draw, content, leading indent).
fn list_item(line: &str) -> Option<(String, &str, String)> {
    let indent: String = line.chars().take_while(|c| *c == ' ').collect();
    let rest = &line[indent.len()..];

    for marker in ["- ", "* ", "+ "] {
        if let Some(content) = rest.strip_prefix(marker) {
            return Some(("•".to_string(), content, indent));
        }
    }

    // Ordered: `1. ` / `12) `
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if !digits.is_empty() && digits.len() <= 3 {
        let after = &rest[digits.len()..];
        for sep in [". ", ") "] {
            if let Some(content) = after.strip_prefix(sep) {
                return Some((format!("{digits}."), content, indent));
            }
        }
    }
    None
}

/// Parse inline markdown (code, bold, italic, links) into styled spans.
///
/// Unbalanced markers are emitted verbatim rather than swallowed — while a
/// response streams, every `**` is unbalanced for a moment.
fn inline(text: &str, base: Style, theme: &Theme) -> Vec<Span<'static>> {
    let mut out: Vec<Span<'static>> = Vec::new();
    let mut buf = String::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0usize;

    let flush = |buf: &mut String, out: &mut Vec<Span<'static>>| {
        if !buf.is_empty() {
            out.push(Span::styled(std::mem::take(buf), base));
        }
    };

    while i < chars.len() {
        let c = chars[i];

        // Inline code.
        if c == '`' {
            if let Some(end) = find(&chars, i + 1, '`') {
                flush(&mut buf, &mut out);
                let code: String = chars[i + 1..end].iter().collect();
                out.push(Span::styled(code, Style::default().fg(theme.accent)));
                i = end + 1;
                continue;
            }
        }

        // Bold (** or __), same word-boundary rule as italics.
        if (c == '*' || c == '_')
            && at_word_boundary(&chars, i)
            && i + 1 < chars.len()
            && chars[i + 1] == c
        {
            if let Some(end) = find_double(&chars, i + 2, c) {
                flush(&mut buf, &mut out);
                let inner: String = chars[i + 2..end].iter().collect();
                out.push(Span::styled(inner, base.add_modifier(Modifier::BOLD)));
                i = end + 2;
                continue;
            }
        }

        // Italic (single * or _). The opener must sit at a word boundary and be
        // followed by a non-space, so `a * b` stays literal and — importantly —
        // `some_function_name` is not read as emphasis around "function".
        if (c == '*' || c == '_')
            && at_word_boundary(&chars, i)
            && chars.get(i + 1).is_some_and(|n| !n.is_whitespace() && *n != c)
        {
            if let Some(end) = find(&chars, i + 1, c) {
                flush(&mut buf, &mut out);
                let inner: String = chars[i + 1..end].iter().collect();
                out.push(Span::styled(inner, base.add_modifier(Modifier::ITALIC)));
                i = end + 1;
                continue;
            }
        }

        // Link: [text](url) — show the text, underlined, and drop the URL.
        if c == '[' {
            if let Some(close) = find(&chars, i + 1, ']') {
                if chars.get(close + 1) == Some(&'(') {
                    if let Some(paren) = find(&chars, close + 2, ')') {
                        flush(&mut buf, &mut out);
                        let label: String = chars[i + 1..close].iter().collect();
                        out.push(Span::styled(
                            label,
                            base.fg(theme.accent).add_modifier(Modifier::UNDERLINED),
                        ));
                        i = paren + 1;
                        continue;
                    }
                }
            }
        }

        buf.push(c);
        i += 1;
    }

    flush(&mut buf, &mut out);
    out
}

/// Whether index `i` begins a word — i.e. the preceding character is not
/// alphanumeric. Emphasis markers only open at a boundary, which is what keeps
/// `snake_case_names` and `a*b` from being read as emphasis.
fn at_word_boundary(chars: &[char], i: usize) -> bool {
    match i.checked_sub(1).and_then(|p| chars.get(p)) {
        None => true,
        Some(prev) => !prev.is_alphanumeric(),
    }
}

fn find(chars: &[char], from: usize, target: char) -> Option<usize> {
    (from..chars.len()).find(|&i| chars[i] == target)
}

fn find_double(chars: &[char], from: usize, target: char) -> Option<usize> {
    (from..chars.len().saturating_sub(1))
        .find(|&i| chars[i] == target && chars[i + 1] == target)
}

/// Wrap styled spans to `width` display columns, breaking at word boundaries
/// and falling back to a hard break for a word longer than the line.
///
/// Width is measured with `unicode-width`, so CJK and emoji occupy the columns
/// they actually occupy rather than one each.
pub fn wrap_spans(spans: Vec<Span<'static>>, width: usize, indent: &str) -> Vec<Line<'static>> {
    use unicode_width::UnicodeWidthStr;

    if width == usize::MAX || width == 0 {
        return vec![Line::from(spans)];
    }

    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut current: Vec<Span<'static>> = Vec::new();
    let mut used = 0usize;

    let push_line = |lines: &mut Vec<Line<'static>>, current: &mut Vec<Span<'static>>| {
        lines.push(Line::from(std::mem::take(current)));
    };

    for span in spans {
        let style = span.style;
        // Split into words while keeping the whitespace that follows each, so
        // spacing survives the round trip.
        for chunk in split_keeping_spaces(span.content.as_ref()) {
            let chunk_width = UnicodeWidthStr::width(chunk.as_str());

            if used + chunk_width <= width {
                used += chunk_width;
                current.push(Span::styled(chunk, style));
                continue;
            }

            // Does not fit. A trailing space at a wrap point is dropped.
            if chunk.trim().is_empty() {
                push_line(&mut lines, &mut current);
                used = indent.width();
                if !indent.is_empty() {
                    current.push(Span::raw(indent.to_string()));
                }
                continue;
            }

            if !current.is_empty() {
                push_line(&mut lines, &mut current);
                used = indent.width();
                if !indent.is_empty() {
                    current.push(Span::raw(indent.to_string()));
                }
            }

            // A single word wider than the line: hard-break it by columns.
            if chunk_width > width.saturating_sub(indent.width()) {
                for piece in hard_break(&chunk, width.saturating_sub(indent.width()).max(1)) {
                    let piece_width = UnicodeWidthStr::width(piece.as_str());
                    current.push(Span::styled(piece, style));
                    used += piece_width;
                    if used >= width {
                        push_line(&mut lines, &mut current);
                        used = indent.width();
                        if !indent.is_empty() {
                            current.push(Span::raw(indent.to_string()));
                        }
                    }
                }
            } else {
                used += chunk_width;
                current.push(Span::styled(chunk, style));
            }
        }
    }

    if !current.is_empty() || lines.is_empty() {
        push_line(&mut lines, &mut current);
    }
    lines
}

/// Split into alternating word / whitespace chunks, losing nothing.
fn split_keeping_spaces(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut in_space: Option<bool> = None;

    for c in text.chars() {
        let is_space = c.is_whitespace();
        match in_space {
            Some(prev) if prev == is_space => buf.push(c),
            Some(_) => {
                out.push(std::mem::take(&mut buf));
                buf.push(c);
                in_space = Some(is_space);
            }
            None => {
                buf.push(c);
                in_space = Some(is_space);
            }
        }
    }
    if !buf.is_empty() {
        out.push(buf);
    }
    out
}

/// Break a too-long word into pieces of at most `width` display columns.
fn hard_break(word: &str, width: usize) -> Vec<String> {
    use unicode_width::UnicodeWidthChar;

    let mut out = Vec::new();
    let mut buf = String::new();
    let mut used = 0usize;

    for c in word.chars() {
        let w = UnicodeWidthChar::width(c).unwrap_or(0);
        if used + w > width && !buf.is_empty() {
            out.push(std::mem::take(&mut buf));
            used = 0;
        }
        buf.push(c);
        used += w;
    }
    if !buf.is_empty() {
        out.push(buf);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use unicode_width::UnicodeWidthStr;

    fn theme() -> Theme {
        Theme::dark()
    }

    fn text_of(lines: &[Line<'static>]) -> Vec<String> {
        lines
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect::<String>())
            .collect()
    }

    #[test]
    fn plain_text_survives_intact() {
        let out = render("hello world", 40, &theme());
        assert_eq!(text_of(&out), vec!["hello world"]);
    }

    #[test]
    fn a_zero_width_area_does_not_panic() {
        // Terminals report width 0 mid-resize.
        let out = render("# heading\n- item\n```rust\nfn f() {}\n```", 0, &theme());
        assert!(!out.is_empty());
    }

    #[test]
    fn headings_are_bold_and_lose_their_hashes() {
        let out = render("## Section", 40, &theme());
        assert_eq!(text_of(&out), vec!["Section"]);
        assert!(out[0].spans.iter().any(|s| s.style.add_modifier.contains(Modifier::BOLD)));
    }

    #[test]
    fn bullets_are_normalised_and_indentation_is_kept() {
        let out = render("- one\n  - nested", 40, &theme());
        let lines = text_of(&out);
        assert_eq!(lines[0], "• one");
        assert_eq!(lines[1], "  • nested");
    }

    #[test]
    fn ordered_lists_keep_their_numbers() {
        let out = render("1. first\n2. second", 40, &theme());
        assert_eq!(text_of(&out), vec!["1. first", "2. second"]);
    }

    #[test]
    fn a_number_that_is_not_a_list_is_left_alone() {
        let out = render("1984 was a year", 40, &theme());
        assert_eq!(text_of(&out), vec!["1984 was a year"]);
    }

    #[test]
    fn fenced_code_is_gutter_marked_and_the_fence_lines_vanish() {
        let out = render("```rust\nfn main() {}\n```", 40, &theme());
        let lines = text_of(&out);
        assert_eq!(lines.len(), 1, "fence markers should not render: {lines:?}");
        assert_eq!(lines[0], "  │ fn main() {}");
    }

    #[test]
    fn an_unclosed_fence_still_renders_its_content() {
        // Every streaming response passes through this state.
        let out = render("```python\nprint(1)", 40, &theme());
        assert_eq!(text_of(&out), vec!["  │ print(1)"]);
    }

    #[test]
    fn markdown_inside_a_fence_is_not_interpreted() {
        let out = render("```\n# not a heading\n- not a bullet\n```", 40, &theme());
        let lines = text_of(&out);
        assert_eq!(lines[0], "  │ # not a heading");
        assert_eq!(lines[1], "  │ - not a bullet");
    }

    #[test]
    fn inline_code_is_accented_and_loses_its_backticks() {
        let out = render("use `cargo test` now", 40, &theme());
        assert_eq!(text_of(&out), vec!["use cargo test now"]);
    }

    #[test]
    fn bold_and_italic_lose_their_markers() {
        assert_eq!(text_of(&render("**bold** text", 40, &theme())), vec!["bold text"]);
        assert_eq!(text_of(&render("*italic* text", 40, &theme())), vec!["italic text"]);
    }

    #[test]
    fn unbalanced_emphasis_is_shown_verbatim() {
        // Mid-stream this happens constantly; swallowing the text would make
        // the response flicker as tokens arrive.
        assert_eq!(text_of(&render("**half", 40, &theme())), vec!["**half"]);
        assert_eq!(text_of(&render("a * b", 40, &theme())), vec!["a * b"]);
    }

    #[test]
    fn snake_case_is_not_treated_as_italics() {
        assert_eq!(
            text_of(&render("call some_function_name here", 40, &theme())),
            vec!["call some_function_name here"]
        );
    }

    #[test]
    fn links_show_their_label_and_hide_the_url() {
        let out = render("see [the docs](https://example.com/x) now", 60, &theme());
        assert_eq!(text_of(&out), vec!["see the docs now"]);
    }

    #[test]
    fn a_bare_bracket_is_left_alone() {
        assert_eq!(text_of(&render("array[0] = 1", 40, &theme())), vec!["array[0] = 1"]);
    }

    #[test]
    fn long_text_wraps_within_the_width() {
        let source = "the quick brown fox jumps over the lazy dog and keeps running";
        let out = render(source, 20, &theme());
        assert!(out.len() > 1);
        for line in text_of(&out) {
            assert!(
                UnicodeWidthStr::width(line.as_str()) <= 20,
                "line exceeds width: {line:?}"
            );
        }
    }

    #[test]
    fn wrapping_preserves_every_word() {
        let source = "alpha beta gamma delta epsilon zeta eta theta iota kappa";
        let out = text_of(&render(source, 12, &theme())).join(" ");
        for word in source.split(' ') {
            assert!(out.contains(word), "lost {word:?} in {out:?}");
        }
    }

    #[test]
    fn a_word_longer_than_the_line_is_hard_broken() {
        let out = render(&"x".repeat(50), 10, &theme());
        assert!(out.len() >= 5);
        for line in text_of(&out) {
            assert!(UnicodeWidthStr::width(line.as_str()) <= 10);
        }
    }

    #[test]
    fn wide_characters_are_measured_by_display_width() {
        // Each CJK char is 2 columns, so 10 of them need at least 2 lines at width 12.
        let out = render(&"漢".repeat(10), 12, &theme());
        for line in text_of(&out) {
            assert!(
                UnicodeWidthStr::width(line.as_str()) <= 12,
                "wide chars overflowed: {line:?}"
            );
        }
    }

    #[test]
    fn blank_lines_are_preserved_as_spacing() {
        let out = render("a\n\nb", 40, &theme());
        assert_eq!(text_of(&out), vec!["a", "", "b"]);
    }

    #[test]
    fn horizontal_rules_render_as_a_rule() {
        let out = render("---", 40, &theme());
        assert_eq!(out.len(), 1);
        assert!(text_of(&out)[0].starts_with('─'));
    }

    #[test]
    fn block_quotes_get_a_margin_bar() {
        let out = render("> quoted", 40, &theme());
        assert_eq!(text_of(&out), vec!["▏ quoted"]);
    }

    #[test]
    fn empty_input_yields_one_empty_line_not_a_panic() {
        assert_eq!(text_of(&render("", 40, &theme())), vec![""]);
    }
}
