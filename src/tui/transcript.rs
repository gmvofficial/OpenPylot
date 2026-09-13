//! The conversation transcript: what has been said, and how it draws.
//!
//! Entries are kept as structured data rather than pre-rendered text, so the
//! same transcript can be re-laid-out on a terminal resize, folded and
//! unfolded, and exported — none of which is possible once content has been
//! flattened to styled lines.
//!
//! The old REPL had no model at all: it `print!`ed as events arrived, so a tool
//! call was one `🔧 name` line with no status, no output, and nothing to fold.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use super::markdown;
use super::theme::{glyph, Theme};

/// How a tool call ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolStatus {
    /// Arguments are still streaming in.
    Pending,
    /// Executing.
    Running,
    /// Finished successfully.
    Ok,
    /// Finished with an error.
    Failed,
    /// Refused by the permission policy.
    Denied,
}

impl ToolStatus {
    fn glyph(&self) -> &'static str {
        match self {
            ToolStatus::Pending => glyph::PENDING,
            ToolStatus::Running => glyph::TOOL,
            ToolStatus::Ok => glyph::OK,
            ToolStatus::Failed | ToolStatus::Denied => glyph::FAIL,
        }
    }

    fn style(&self, theme: &Theme) -> Style {
        match self {
            ToolStatus::Pending => Style::default().fg(theme.muted),
            ToolStatus::Running => Style::default().fg(theme.tool),
            ToolStatus::Ok => Style::default().fg(theme.success),
            ToolStatus::Failed => Style::default().fg(theme.error),
            ToolStatus::Denied => Style::default().fg(theme.warning),
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, ToolStatus::Ok | ToolStatus::Failed | ToolStatus::Denied)
    }
}

/// A tool call and its result.
#[derive(Debug, Clone)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    /// Raw JSON arguments, accumulated from `tool_input_delta` events.
    pub input: String,
    pub output: String,
    pub status: ToolStatus,
    /// Whether the detail rows are shown.
    pub expanded: bool,
}

impl ToolCall {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            input: String::new(),
            output: String::new(),
            status: ToolStatus::Pending,
            expanded: false,
        }
    }

    /// A one-line summary of the arguments, for the collapsed row.
    ///
    /// Picks the argument that best identifies the call — the command for
    /// `bash`, the path for a file tool — and falls back to compacted JSON.
    pub fn summary(&self) -> String {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&self.input) else {
            // Still streaming, or not JSON at all.
            return compact(&self.input, 72);
        };

        for key in ["command", "file_path", "path", "pattern", "query", "url", "sql"] {
            if let Some(v) = value.get(key).and_then(|v| v.as_str()) {
                return compact(v, 72);
            }
        }

        match &value {
            serde_json::Value::Object(map) if map.is_empty() => String::new(),
            other => compact(&other.to_string(), 72),
        }
    }

    /// Lines of output, for the expanded view.
    pub fn output_lines(&self) -> Vec<&str> {
        if self.output.is_empty() {
            return Vec::new();
        }
        self.output.lines().collect()
    }
}

/// One thing in the transcript.
#[derive(Debug, Clone)]
pub enum Entry {
    /// Something the user said.
    User(String),
    /// Something the assistant said, as markdown.
    Assistant(String),
    /// The model's reasoning, shown only when `/thinking` is on.
    Thinking(String),
    /// A tool call.
    Tool(ToolCall),
    /// A system notice: a mode change, an error, a slash-command result.
    Notice { text: String, level: Level },
}

/// Severity of a notice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Info,
    Success,
    Warning,
    Error,
}

impl Level {
    fn style(&self, theme: &Theme) -> Style {
        match self {
            Level::Info => Style::default().fg(theme.muted),
            Level::Success => Style::default().fg(theme.success),
            Level::Warning => Style::default().fg(theme.warning),
            Level::Error => Style::default().fg(theme.error),
        }
    }
}

/// The conversation so far.
#[derive(Debug, Default)]
pub struct Transcript {
    entries: Vec<Entry>,
    /// Whether tool output is shown in full rather than summarised.
    pub verbose: bool,
    /// Whether reasoning entries are rendered at all.
    pub show_thinking: bool,
}

/// Output rows shown for a collapsed tool call.
const COLLAPSED_OUTPUT_ROWS: usize = 3;
/// Output rows shown for an expanded tool call when not in verbose mode.
const EXPANDED_OUTPUT_ROWS: usize = 40;

impl Transcript {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            verbose: false,
            show_thinking: true,
        }
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Mutable access, for applying a late status change to entries already
    /// pushed (e.g. marking an in-flight tool as cancelled).
    pub fn entries_mut(&mut self) -> &mut [Entry] {
        &mut self.entries
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn push(&mut self, entry: Entry) {
        self.entries.push(entry);
    }

    pub fn push_user(&mut self, text: impl Into<String>) {
        self.entries.push(Entry::User(text.into()));
    }

    pub fn push_assistant(&mut self, text: impl Into<String>) {
        self.entries.push(Entry::Assistant(text.into()));
    }

    pub fn push_notice(&mut self, text: impl Into<String>, level: Level) {
        self.entries.push(Entry::Notice {
            text: text.into(),
            level,
        });
    }

    /// Find a tool call by its id, for applying a later event to it.
    pub fn tool_mut(&mut self, id: &str) -> Option<&mut ToolCall> {
        self.entries.iter_mut().rev().find_map(|e| match e {
            Entry::Tool(t) if t.id == id => Some(t),
            _ => None,
        })
    }

    /// The most recent tool call, expanded or collapsed by Ctrl+O.
    pub fn toggle_last_tool(&mut self) -> bool {
        for entry in self.entries.iter_mut().rev() {
            if let Entry::Tool(tool) = entry {
                tool.expanded = !tool.expanded;
                return true;
            }
        }
        false
    }

    /// Expand or collapse every tool call.
    pub fn set_all_expanded(&mut self, expanded: bool) {
        for entry in self.entries.iter_mut() {
            if let Entry::Tool(tool) = entry {
                tool.expanded = expanded;
            }
        }
    }

    /// Render the whole transcript to styled lines at `width` columns.
    pub fn render(&self, width: usize, theme: &Theme) -> Vec<Line<'static>> {
        let mut out = Vec::new();
        for entry in &self.entries {
            out.extend(self.render_entry(entry, width, theme));
        }
        out
    }

    /// Render one entry. Public so the event loop can push a finished entry
    /// straight into terminal scrollback without re-rendering everything.
    pub fn render_entry(&self, entry: &Entry, width: usize, theme: &Theme) -> Vec<Line<'static>> {
        match entry {
            Entry::User(text) => render_user(text, width, theme),
            Entry::Assistant(text) => render_assistant(text, width, theme),
            Entry::Thinking(text) => {
                if self.show_thinking {
                    render_thinking(text, width, theme)
                } else {
                    Vec::new()
                }
            }
            Entry::Tool(tool) => render_tool(tool, width, theme, self.verbose),
            Entry::Notice { text, level } => render_notice(text, *level, width, theme),
        }
    }
}

fn gutter(marker: &str, style: Style) -> Span<'static> {
    Span::styled(format!("{marker} "), style)
}

fn render_user(text: &str, width: usize, theme: &Theme) -> Vec<Line<'static>> {
    let avail = width.saturating_sub(2).max(8);
    let style = Style::default().fg(theme.user).add_modifier(Modifier::BOLD);
    let spans = vec![Span::styled(text.to_string(), style)];

    markdown::wrap_spans(spans, avail, "")
        .into_iter()
        .enumerate()
        .map(|(i, line)| {
            let lead = if i == 0 {
                gutter(glyph::USER, style)
            } else {
                Span::raw("  ".to_string())
            };
            let mut spans = vec![lead];
            spans.extend(line.spans);
            Line::from(spans)
        })
        .collect()
}

fn render_assistant(text: &str, width: usize, theme: &Theme) -> Vec<Line<'static>> {
    let avail = width.saturating_sub(2).max(8);
    let body = markdown::render(text, avail, theme);

    body.into_iter()
        .enumerate()
        .map(|(i, line)| {
            let lead = if i == 0 {
                gutter(glyph::ASSISTANT, Style::default().fg(theme.accent))
            } else {
                Span::raw("  ".to_string())
            };
            let mut spans = vec![lead];
            spans.extend(line.spans);
            Line::from(spans)
        })
        .collect()
}

fn render_thinking(text: &str, width: usize, theme: &Theme) -> Vec<Line<'static>> {
    let avail = width.saturating_sub(2).max(8);
    let style = theme.dim().add_modifier(Modifier::ITALIC);
    let spans = vec![Span::styled(text.to_string(), style)];

    markdown::wrap_spans(spans, avail, "")
        .into_iter()
        .enumerate()
        .map(|(i, line)| {
            let lead = if i == 0 {
                gutter(glyph::THINKING, theme.dim())
            } else {
                Span::raw("  ".to_string())
            };
            let mut spans = vec![lead];
            spans.extend(line.spans);
            Line::from(spans)
        })
        .collect()
}

fn render_notice(text: &str, level: Level, width: usize, theme: &Theme) -> Vec<Line<'static>> {
    let avail = width.saturating_sub(2).max(8);
    let style = level.style(theme);

    // A notice may be a whole document — `/help` and `/tools` both are — so
    // anything with a line break goes through the markdown renderer. Wrapping
    // it as one run would reflow the entire command reference into a paragraph.
    let body = if text.contains('\n') {
        markdown::render(text, avail, theme)
    } else {
        markdown::wrap_spans(vec![Span::styled(text.to_string(), style)], avail, "")
    };

    body.into_iter()
        .enumerate()
        .map(|(i, line)| {
            let lead = if i == 0 {
                gutter(glyph::NOTICE, style)
            } else {
                Span::raw("  ".to_string())
            };
            let mut spans = vec![lead];
            spans.extend(line.spans);
            Line::from(spans)
        })
        .collect()
}

/// A tool call: one header row, plus folded detail.
///
/// ```text
/// ⏺ bash  ls -la                              ▸
///   │ total 48
///   │ drwxr-xr-x  12 user  staff   384 …
///   … 14 more lines
/// ```
fn render_tool(tool: &ToolCall, width: usize, theme: &Theme, verbose: bool) -> Vec<Line<'static>> {
    let status_style = tool.status.style(theme);
    let mut out = Vec::new();

    // Header.
    let summary = tool.summary();
    let mut header = vec![
        gutter(tool.status.glyph(), status_style),
        Span::styled(
            tool.name.clone(),
            Style::default().fg(theme.tool).add_modifier(Modifier::BOLD),
        ),
    ];
    if !summary.is_empty() {
        header.push(Span::raw("  ".to_string()));
        // Truncate to what is left on the row, leaving space for the fold marker.
        let used = 2 + tool.name.chars().count() + 2;
        let room = width.saturating_sub(used + 2);
        header.push(Span::styled(compact(&summary, room), theme.dim()));
    }

    let lines = tool.output_lines();
    let foldable = lines.len() > COLLAPSED_OUTPUT_ROWS;
    if foldable {
        header.push(Span::raw(" ".to_string()));
        header.push(Span::styled(
            if tool.expanded { glyph::EXPANDED } else { glyph::COLLAPSED }.to_string(),
            theme.dim(),
        ));
    }
    out.push(Line::from(header));

    if tool.status == ToolStatus::Pending || tool.status == ToolStatus::Running {
        return out;
    }

    // Detail.
    let limit = if verbose {
        usize::MAX
    } else if tool.expanded {
        EXPANDED_OUTPUT_ROWS
    } else {
        COLLAPSED_OUTPUT_ROWS
    };

    let output_style = if tool.status == ToolStatus::Failed {
        Style::default().fg(theme.error)
    } else {
        theme.dim()
    };

    let shown = lines.len().min(limit);
    for line in lines.iter().take(shown) {
        let room = width.saturating_sub(4).max(8);
        out.push(Line::from(vec![
            Span::styled("  │ ".to_string(), theme.dim()),
            Span::styled(compact(line, room), output_style),
        ]));
    }

    if lines.len() > shown {
        let hidden = lines.len() - shown;
        out.push(Line::from(Span::styled(
            format!("  … {hidden} more line{}", if hidden == 1 { "" } else { "s" }),
            theme.dim(),
        )));
    }

    out
}

/// Collapse whitespace and truncate to `max` display columns, with an ellipsis.
fn compact(text: &str, max: usize) -> String {
    use unicode_width::UnicodeWidthChar;

    let single: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if max == 0 {
        return String::new();
    }

    let mut out = String::new();
    let mut used = 0usize;
    for c in single.chars() {
        let w = UnicodeWidthChar::width(c).unwrap_or(0);
        if used + w > max.saturating_sub(1) {
            out.push('…');
            return out;
        }
        out.push(c);
        used += w;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme() -> Theme {
        Theme::dark()
    }

    fn text_of(lines: &[Line<'static>]) -> Vec<String> {
        lines
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect::<String>())
            .collect()
    }

    fn tool_with_output(lines: usize) -> ToolCall {
        let mut tool = ToolCall::new("t1", "bash");
        tool.input = r#"{"command":"ls -la"}"#.into();
        tool.output = (0..lines).map(|i| format!("line {i}\n")).collect();
        tool.status = ToolStatus::Ok;
        tool
    }

    // ── Summaries ────────────────────────────────────────────────────

    #[test]
    fn a_bash_call_summarises_as_its_command() {
        let mut tool = ToolCall::new("t", "bash");
        tool.input = r#"{"command":"cargo test --all"}"#.into();
        assert_eq!(tool.summary(), "cargo test --all");
    }

    #[test]
    fn a_file_call_summarises_as_its_path() {
        let mut tool = ToolCall::new("t", "read_file");
        tool.input = r#"{"file_path":"src/main.rs"}"#.into();
        assert_eq!(tool.summary(), "src/main.rs");
    }

    #[test]
    fn partial_json_does_not_break_the_summary() {
        // Arguments arrive as deltas, so the header renders against fragments.
        let mut tool = ToolCall::new("t", "bash");
        tool.input = r#"{"comm"#.into();
        assert_eq!(tool.summary(), r#"{"comm"#);
    }

    #[test]
    fn an_empty_argument_object_summarises_as_nothing() {
        let mut tool = ToolCall::new("t", "status");
        tool.input = "{}".into();
        assert_eq!(tool.summary(), "");
    }

    #[test]
    fn a_long_summary_is_truncated_with_an_ellipsis() {
        let mut tool = ToolCall::new("t", "bash");
        tool.input = format!(r#"{{"command":"{}"}}"#, "x".repeat(200));
        let summary = tool.summary();
        assert!(summary.chars().count() <= 72);
        assert!(summary.ends_with('…'));
    }

    #[test]
    fn a_multiline_command_summarises_to_one_line() {
        let mut tool = ToolCall::new("t", "bash");
        tool.input = r#"{"command":"line one\nline two"}"#.into();
        assert_eq!(tool.summary(), "line one line two");
    }

    // ── Folding ──────────────────────────────────────────────────────

    #[test]
    fn short_output_shows_in_full_with_no_fold_marker() {
        let tool = tool_with_output(2);
        let lines = text_of(&render_tool(&tool, 80, &theme(), false));
        assert!(!lines[0].contains(glyph::COLLAPSED));
        assert_eq!(lines.len(), 3, "header plus both output lines: {lines:?}");
    }

    #[test]
    fn long_output_is_folded_and_says_how_much_is_hidden() {
        let tool = tool_with_output(20);
        let lines = text_of(&render_tool(&tool, 80, &theme(), false));
        assert!(lines[0].contains(glyph::COLLAPSED), "expected a fold marker");
        assert_eq!(lines.len(), COLLAPSED_OUTPUT_ROWS + 2);
        assert!(lines.last().unwrap().contains("17 more lines"), "{lines:?}");
    }

    #[test]
    fn expanding_shows_more_and_flips_the_marker() {
        let mut tool = tool_with_output(20);
        tool.expanded = true;
        let lines = text_of(&render_tool(&tool, 80, &theme(), false));
        assert!(lines[0].contains(glyph::EXPANDED));
        assert_eq!(lines.len(), 21, "header plus all 20 lines: {}", lines.len());
    }

    #[test]
    fn one_hidden_line_is_singular() {
        let tool = tool_with_output(COLLAPSED_OUTPUT_ROWS + 1);
        let lines = text_of(&render_tool(&tool, 80, &theme(), false));
        assert!(lines.last().unwrap().contains("1 more line"));
        assert!(!lines.last().unwrap().contains("lines"));
    }

    #[test]
    fn verbose_mode_shows_everything_even_when_collapsed() {
        let tool = tool_with_output(100);
        let lines = text_of(&render_tool(&tool, 80, &theme(), true));
        assert_eq!(lines.len(), 101);
    }

    #[test]
    fn a_running_tool_shows_no_output_yet() {
        let mut tool = tool_with_output(20);
        tool.status = ToolStatus::Running;
        let lines = render_tool(&tool, 80, &theme(), false);
        assert_eq!(lines.len(), 1, "just the header while it runs");
    }

    #[test]
    fn toggling_affects_the_most_recent_tool_only() {
        let mut t = Transcript::new();
        t.push(Entry::Tool(ToolCall::new("a", "first")));
        t.push(Entry::Tool(ToolCall::new("b", "second")));

        assert!(t.toggle_last_tool());

        let expanded: Vec<bool> = t
            .entries()
            .iter()
            .filter_map(|e| match e {
                Entry::Tool(tool) => Some(tool.expanded),
                _ => None,
            })
            .collect();
        assert_eq!(expanded, vec![false, true]);
    }

    #[test]
    fn toggling_with_no_tools_reports_false() {
        let mut t = Transcript::new();
        t.push_user("hello");
        assert!(!t.toggle_last_tool());
    }

    #[test]
    fn set_all_expanded_affects_every_tool() {
        let mut t = Transcript::new();
        t.push(Entry::Tool(ToolCall::new("a", "first")));
        t.push(Entry::Tool(ToolCall::new("b", "second")));
        t.set_all_expanded(true);
        assert!(t.entries().iter().all(|e| match e {
            Entry::Tool(tool) => tool.expanded,
            _ => true,
        }));
    }

    // ── Lookup ───────────────────────────────────────────────────────

    #[test]
    fn a_tool_can_be_found_by_id_to_apply_later_events() {
        let mut t = Transcript::new();
        t.push(Entry::Tool(ToolCall::new("call_1", "bash")));
        t.push(Entry::Tool(ToolCall::new("call_2", "read_file")));

        let tool = t.tool_mut("call_1").expect("call_1 should be found");
        tool.status = ToolStatus::Ok;

        assert!(t.tool_mut("missing").is_none());
    }

    #[test]
    fn tool_lookup_finds_the_most_recent_of_a_repeated_id() {
        // Ids repeat across turns for some providers; the live one is the last.
        let mut t = Transcript::new();
        let mut first = ToolCall::new("dup", "bash");
        first.output = "old".into();
        t.push(Entry::Tool(first));
        t.push(Entry::Tool(ToolCall::new("dup", "bash")));

        t.tool_mut("dup").unwrap().output = "new".into();

        let outputs: Vec<String> = t
            .entries()
            .iter()
            .filter_map(|e| match e {
                Entry::Tool(tool) => Some(tool.output.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(outputs, vec!["old", "new"]);
    }

    // ── Gutters and layout ───────────────────────────────────────────

    #[test]
    fn each_entry_type_gets_its_own_marker() {
        let t = Transcript::new();
        let user = text_of(&t.render_entry(&Entry::User("hi".into()), 40, &theme()));
        let assistant = text_of(&t.render_entry(&Entry::Assistant("hello".into()), 40, &theme()));

        assert!(user[0].starts_with(glyph::USER));
        assert!(assistant[0].starts_with(glyph::ASSISTANT));
    }

    #[test]
    fn wrapped_lines_align_under_the_gutter() {
        let t = Transcript::new();
        let long = "word ".repeat(40);
        let lines = text_of(&t.render_entry(&Entry::User(long), 30, &theme()));
        assert!(lines.len() > 1);
        for line in &lines[1..] {
            assert!(line.starts_with("  "), "continuation not indented: {line:?}");
        }
    }

    #[test]
    fn thinking_is_hidden_when_the_toggle_is_off() {
        let mut t = Transcript::new();
        t.show_thinking = false;
        let lines = t.render_entry(&Entry::Thinking("reasoning".into()), 40, &theme());
        assert!(lines.is_empty());

        t.show_thinking = true;
        assert!(!t.render_entry(&Entry::Thinking("reasoning".into()), 40, &theme()).is_empty());
    }

    #[test]
    fn a_narrow_terminal_does_not_panic() {
        let t = Transcript::new();
        for width in [0, 1, 2, 3, 4, 5] {
            let _ = t.render_entry(&Entry::User("some text here".into()), width, &theme());
            let _ = t.render_entry(&Entry::Tool(tool_with_output(5)), width, &theme());
            let _ = t.render_entry(
                &Entry::Notice { text: "note".into(), level: Level::Error },
                width,
                &theme(),
            );
        }
    }

    #[test]
    fn rendering_the_whole_transcript_covers_every_entry() {
        let mut t = Transcript::new();
        t.push_user("question");
        t.push(Entry::Tool(tool_with_output(1)));
        t.push_assistant("answer");
        t.push_notice("done", Level::Success);

        let text = text_of(&t.render(60, &theme())).join("\n");
        assert!(text.contains("question"));
        assert!(text.contains("bash"));
        assert!(text.contains("answer"));
        assert!(text.contains("done"));
    }

    // ── compact() ────────────────────────────────────────────────────

    #[test]
    fn compact_collapses_whitespace() {
        assert_eq!(compact("a   b\n\tc", 40), "a b c");
    }

    #[test]
    fn compact_handles_a_zero_budget() {
        assert_eq!(compact("anything", 0), "");
    }

    #[test]
    fn compact_measures_wide_characters_correctly() {
        // Four CJK chars are 8 columns; a budget of 6 fits two plus the ellipsis.
        let out = compact("漢字漢字", 6);
        assert!(unicode_width::UnicodeWidthStr::width(out.as_str()) <= 6, "{out:?}");
    }
}

#[cfg(test)]
mod notice_tests {
    use super::*;

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
    fn a_multiline_notice_keeps_its_structure() {
        // `/help` is a whole document; reflowing it into one paragraph makes it
        // unreadable, which is exactly what happened before markdown rendering
        // was applied here.
        let notice = Entry::Notice {
            text: "Commands\n- `/help` — show help\n- `/quit` — leave".into(),
            level: Level::Info,
        };
        let t = Transcript::new();
        let lines = text_of(&t.render_entry(&notice, 60, &theme()));

        assert_eq!(lines.len(), 3, "one line per source line: {lines:?}");
        assert!(lines[1].contains("/help"));
        assert!(lines[2].contains("/quit"));
    }

    #[test]
    fn a_single_line_notice_still_wraps_normally() {
        let notice = Entry::Notice {
            text: "word ".repeat(30),
            level: Level::Warning,
        };
        let t = Transcript::new();
        let lines = text_of(&t.render_entry(&notice, 30, &theme()));
        assert!(lines.len() > 1, "long text should still wrap");
        for line in &lines {
            assert!(unicode_width::UnicodeWidthStr::width(line.as_str()) <= 30);
        }
    }

    #[test]
    fn a_multiline_notice_indents_under_its_marker() {
        let notice = Entry::Notice {
            text: "first\nsecond".into(),
            level: Level::Info,
        };
        let t = Transcript::new();
        let lines = text_of(&t.render_entry(&notice, 40, &theme()));
        assert!(lines[0].starts_with(glyph::NOTICE));
        assert!(lines[1].starts_with("  "), "continuation must align: {lines:?}");
    }
}
