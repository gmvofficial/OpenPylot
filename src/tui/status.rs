//! The status line: what the agent is, what it is doing, and what it is costing.
//!
//! The old REPL surfaced none of this. Model, permission mode, token spend and
//! how close the conversation is to filling the context window are exactly the
//! facts a user needs in order to decide whether to compact, switch models, or
//! loosen permissions — so they belong on screen at all times, not behind a
//! slash command.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use super::theme::Theme;
use crate::permissions::PermissionMode;

/// Context-window sizes for the models we know about, in tokens.
///
/// An unknown model falls back to a conservative 128k rather than claiming a
/// window it may not have — over-reporting headroom is the harmful direction,
/// since it invites the user to keep going until the request fails.
pub fn context_window(model: &str) -> u64 {
    let m = model.to_ascii_lowercase();

    // Anthropic
    if m.contains("opus-5") || m.contains("sonnet-5") {
        return if m.contains("[1m]") || m.contains("-1m") { 1_000_000 } else { 200_000 };
    }
    if m.contains("claude") {
        return 200_000;
    }
    // OpenAI
    if m.contains("gpt-5") || m.contains("gpt-4.1") {
        return 1_000_000;
    }
    if m.contains("gpt-4o") || m.contains("gpt-4-turbo") {
        return 128_000;
    }
    if m.contains("gpt-4") {
        return 8_192;
    }
    if m.contains("gpt-3.5") {
        return 16_385;
    }
    128_000
}

/// What the agent is doing right now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Activity {
    /// Waiting for input.
    Idle,
    /// Waiting on the model.
    Thinking,
    /// Running a tool.
    Running(String),
    /// Cancelling an in-flight request.
    Cancelling,
}

impl Activity {
    pub fn is_busy(&self) -> bool {
        !matches!(self, Activity::Idle)
    }

    /// The verb shown next to the spinner.
    pub fn label(&self) -> String {
        match self {
            Activity::Idle => String::new(),
            Activity::Thinking => "Thinking".to_string(),
            Activity::Running(tool) => format!("Running {tool}"),
            Activity::Cancelling => "Cancelling".to_string(),
        }
    }
}

/// Everything the status line reports.
#[derive(Debug, Clone)]
pub struct Status {
    pub model: String,
    pub mode: PermissionMode,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost_usd: f64,
    /// Tokens currently occupied by the conversation.
    pub context_used: u64,
    pub activity: Activity,
    /// Connected MCP servers, for the badge.
    pub mcp_servers: usize,
}

impl Default for Status {
    fn default() -> Self {
        Self {
            model: String::new(),
            mode: PermissionMode::WorkspaceWrite,
            input_tokens: 0,
            output_tokens: 0,
            cost_usd: 0.0,
            context_used: 0,
            activity: Activity::Idle,
            mcp_servers: 0,
        }
    }
}

impl Status {
    /// Fraction of the context window in use, clamped to 0..=1.
    pub fn context_fraction(&self) -> f64 {
        let window = context_window(&self.model);
        if window == 0 {
            return 0.0;
        }
        (self.context_used as f64 / window as f64).clamp(0.0, 1.0)
    }

    /// Whether the context is full enough to be worth warning about.
    pub fn context_is_tight(&self) -> bool {
        self.context_fraction() >= 0.75
    }

    /// Render the status line at `width` columns.
    pub fn render(&self, width: usize, theme: &Theme, spinner: &str) -> Line<'static> {
        let mut spans: Vec<Span<'static>> = Vec::new();

        if self.activity.is_busy() {
            spans.push(Span::styled(
                format!("{spinner} "),
                Style::default().fg(theme.accent),
            ));
            spans.push(Span::styled(
                self.activity.label(),
                Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::styled("  ·  ".to_string(), theme.dim()));
            spans.push(Span::styled("esc to interrupt".to_string(), theme.dim()));
        } else {
            spans.push(Span::styled(short_model(&self.model), theme.dim()));
            spans.push(Span::styled("  ·  ".to_string(), theme.dim()));
            spans.push(Span::styled(mode_label(self.mode).to_string(), mode_style(self.mode, theme)));

            if self.mcp_servers > 0 {
                spans.push(Span::styled("  ·  ".to_string(), theme.dim()));
                spans.push(Span::styled(
                    format!("{} mcp", self.mcp_servers),
                    Style::default().fg(theme.success),
                ));
            }
        }

        // Right-aligned: context pressure and spend.
        let mut right: Vec<Span<'static>> = Vec::new();
        if self.context_used > 0 {
            let pct = (self.context_fraction() * 100.0).round() as u32;
            right.push(Span::styled(
                format!("ctx {pct}%"),
                if self.context_is_tight() {
                    Style::default().fg(theme.warning)
                } else {
                    theme.dim()
                },
            ));
        }
        let total = self.input_tokens + self.output_tokens;
        if total > 0 {
            if !right.is_empty() {
                right.push(Span::styled("  ·  ".to_string(), theme.dim()));
            }
            right.push(Span::styled(format!("{} tok", compact_count(total)), theme.dim()));
        }
        if self.cost_usd > 0.0 {
            if !right.is_empty() {
                right.push(Span::styled("  ·  ".to_string(), theme.dim()));
            }
            right.push(Span::styled(format!("${:.3}", self.cost_usd), theme.dim()));
        }

        // Pad between the two groups so the right side sits at the margin.
        let left_width: usize = spans.iter().map(|s| display_width(&s.content)).sum();
        let right_width: usize = right.iter().map(|s| display_width(&s.content)).sum();
        let gap = width.saturating_sub(left_width + right_width);
        if gap > 0 && !right.is_empty() {
            spans.push(Span::raw(" ".repeat(gap)));
        }
        spans.extend(right);

        Line::from(spans)
    }
}

fn display_width(s: &str) -> usize {
    unicode_width::UnicodeWidthStr::width(s)
}

/// Drop the provider prefix and date suffix so the model fits the status line.
pub fn short_model(model: &str) -> String {
    if model.is_empty() {
        return "no model".to_string();
    }
    // `us.anthropic.claude-opus-5-v1:0` → `claude-opus-5`
    let after_provider = model.rsplit('.').next().unwrap_or(model);
    let without_version = after_provider.split(':').next().unwrap_or(after_provider);

    // Strip a trailing -YYYYMMDD date stamp.
    let parts: Vec<&str> = without_version.split('-').collect();
    if let Some(last) = parts.last() {
        if last.len() == 8 && last.chars().all(|c| c.is_ascii_digit()) {
            return parts[..parts.len() - 1].join("-");
        }
    }
    without_version.to_string()
}

pub fn mode_label(mode: PermissionMode) -> &'static str {
    match mode {
        PermissionMode::ReadOnly => "read-only",
        PermissionMode::WorkspaceWrite => "workspace-write",
        PermissionMode::FullAccess => "full-access",
    }
}

/// Parse a `/mode` argument. Accepts the hyphenated spelling the completion
/// offers as well as the shorter forms people actually type.
pub fn parse_mode(text: &str) -> Option<PermissionMode> {
    match text.trim().to_ascii_lowercase().replace('_', "-").as_str() {
        "read-only" | "readonly" | "read" | "ro" => Some(PermissionMode::ReadOnly),
        "workspace-write" | "workspace" | "write" | "ws" => Some(PermissionMode::WorkspaceWrite),
        "full-access" | "full" | "all" | "yolo" => Some(PermissionMode::FullAccess),
        _ => None,
    }
}

fn mode_style(mode: PermissionMode, theme: &Theme) -> Style {
    match mode {
        // Full access is the one worth noticing at a glance.
        PermissionMode::FullAccess => Style::default().fg(theme.warning),
        PermissionMode::ReadOnly => Style::default().fg(theme.success),
        PermissionMode::WorkspaceWrite => theme.dim(),
    }
}

/// `1234` → `1.2k`, `1234567` → `1.2M`.
pub fn compact_count(n: u64) -> String {
    if n < 1_000 {
        return n.to_string();
    }
    if n < 1_000_000 {
        return format!("{:.1}k", n as f64 / 1_000.0);
    }
    format!("{:.1}M", n as f64 / 1_000_000.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme() -> Theme {
        Theme::dark()
    }

    fn text_of(line: &Line<'static>) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    // ── Context windows ──────────────────────────────────────────────

    #[test]
    fn known_models_report_their_real_window() {
        assert_eq!(context_window("claude-sonnet-5"), 200_000);
        assert_eq!(context_window("claude-opus-5[1m]"), 1_000_000);
        assert_eq!(context_window("gpt-4o"), 128_000);
        assert_eq!(context_window("gpt-4"), 8_192);
    }

    #[test]
    fn an_unknown_model_gets_a_conservative_window() {
        // Over-reporting headroom would invite the user to keep going until the
        // request fails, so the fallback must not be generous.
        assert_eq!(context_window("some-new-model"), 128_000);
    }

    #[test]
    fn the_context_fraction_is_clamped() {
        let mut s = Status { model: "gpt-4".into(), ..Default::default() };
        s.context_used = 99_999_999;
        assert_eq!(s.context_fraction(), 1.0, "must never exceed 100%");

        s.context_used = 0;
        assert_eq!(s.context_fraction(), 0.0);
    }

    #[test]
    fn a_tight_context_is_flagged_at_three_quarters() {
        let mut s = Status { model: "gpt-4o".into(), ..Default::default() };
        s.context_used = 90_000; // 70%
        assert!(!s.context_is_tight());
        s.context_used = 100_000; // 78%
        assert!(s.context_is_tight());
    }

    // ── Model names ──────────────────────────────────────────────────

    #[test]
    fn model_names_are_shortened_for_display() {
        assert_eq!(short_model("us.anthropic.claude-opus-5-v1:0"), "claude-opus-5-v1");
        assert_eq!(short_model("claude-haiku-4-5-20251001"), "claude-haiku-4-5");
        assert_eq!(short_model("gpt-4o"), "gpt-4o");
    }

    #[test]
    fn an_empty_model_says_so_rather_than_showing_blank() {
        assert_eq!(short_model(""), "no model");
    }

    #[test]
    fn a_version_that_is_not_a_date_is_kept() {
        assert_eq!(short_model("model-12345"), "model-12345");
    }

    // ── Modes ────────────────────────────────────────────────────────

    #[test]
    fn every_mode_label_round_trips_through_the_parser() {
        for mode in [
            PermissionMode::ReadOnly,
            PermissionMode::WorkspaceWrite,
            PermissionMode::FullAccess,
        ] {
            assert_eq!(parse_mode(mode_label(mode)), Some(mode));
        }
    }

    #[test]
    fn the_parser_accepts_the_short_forms_people_type() {
        assert_eq!(parse_mode("ro"), Some(PermissionMode::ReadOnly));
        assert_eq!(parse_mode("WRITE"), Some(PermissionMode::WorkspaceWrite));
        assert_eq!(parse_mode("read_only"), Some(PermissionMode::ReadOnly));
        assert_eq!(parse_mode("  full  "), Some(PermissionMode::FullAccess));
    }

    #[test]
    fn the_parser_rejects_nonsense_rather_than_guessing() {
        // Silently defaulting here would change what the agent is allowed to do.
        assert_eq!(parse_mode("whatever"), None);
        assert_eq!(parse_mode(""), None);
    }

    // ── Counts ───────────────────────────────────────────────────────

    #[test]
    fn token_counts_are_abbreviated() {
        assert_eq!(compact_count(0), "0");
        assert_eq!(compact_count(999), "999");
        assert_eq!(compact_count(1_500), "1.5k");
        assert_eq!(compact_count(1_500_000), "1.5M");
    }

    // ── Rendering ────────────────────────────────────────────────────

    #[test]
    fn an_idle_line_shows_the_model_and_mode() {
        let s = Status { model: "gpt-4o".into(), ..Default::default() };
        let text = text_of(&s.render(80, &theme(), "⠋"));
        assert!(text.contains("gpt-4o"));
        assert!(text.contains("workspace-write"));
    }

    #[test]
    fn a_busy_line_shows_the_activity_and_how_to_stop_it() {
        let s = Status {
            activity: Activity::Running("bash".into()),
            ..Default::default()
        };
        let text = text_of(&s.render(80, &theme(), "⠋"));
        assert!(text.contains("⠋"));
        assert!(text.contains("Running bash"));
        assert!(text.contains("esc to interrupt"), "the way out must be visible");
    }

    #[test]
    fn spend_is_shown_once_there_is_any() {
        let s = Status {
            model: "gpt-4o".into(),
            input_tokens: 1_200,
            output_tokens: 800,
            cost_usd: 0.0123,
            context_used: 2_000,
            ..Default::default()
        };
        let text = text_of(&s.render(100, &theme(), "⠋"));
        assert!(text.contains("2.0k tok"), "{text:?}");
        assert!(text.contains("$0.012"), "{text:?}");
        assert!(text.contains("ctx 2%"), "{text:?}");
    }

    #[test]
    fn a_fresh_session_shows_no_zero_counters() {
        // "0 tok · $0.000" is noise before anything has happened.
        let s = Status { model: "gpt-4o".into(), ..Default::default() };
        let text = text_of(&s.render(80, &theme(), "⠋"));
        assert!(!text.contains("tok"), "{text:?}");
        assert!(!text.contains('$'), "{text:?}");
    }

    #[test]
    fn the_mcp_badge_appears_only_when_servers_are_connected() {
        let mut s = Status { model: "gpt-4o".into(), ..Default::default() };
        assert!(!text_of(&s.render(80, &theme(), "⠋")).contains("mcp"));

        s.mcp_servers = 2;
        assert!(text_of(&s.render(80, &theme(), "⠋")).contains("2 mcp"));
    }

    #[test]
    fn a_narrow_terminal_does_not_panic_or_overflow() {
        let s = Status {
            model: "claude-sonnet-5".into(),
            input_tokens: 100_000,
            output_tokens: 50_000,
            cost_usd: 12.345,
            context_used: 150_000,
            ..Default::default()
        };
        for width in [0, 1, 5, 20, 200] {
            let _ = s.render(width, &theme(), "⠋");
        }
    }
}
