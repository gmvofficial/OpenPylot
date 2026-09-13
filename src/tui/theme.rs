//! Terminal palette.
//!
//! Everything the TUI draws takes its colour from here, so the whole interface
//! can be retuned in one place and so no widget hardcodes a colour that only
//! reads on one terminal background.
//!
//! Colours are indexed ANSI where a reasonable equivalent exists and RGB only
//! where the difference matters. Indexed colours inherit the user's terminal
//! theme, which is why a well-behaved TUI looks at home in both a light and a
//! dark terminal without knowing which one it is in.

use ratatui::style::{Color, Modifier, Style};

/// Named roles, so call sites say what a colour *means* rather than what it is.
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    /// Primary accent — the assistant, focus rings, active affordances.
    pub accent: Color,
    /// Secondary accent for the user's own turns.
    pub user: Color,
    /// Tool activity.
    pub tool: Color,
    /// Success / completion.
    pub success: Color,
    /// Warnings and pending approval.
    pub warning: Color,
    /// Errors and denials.
    pub error: Color,
    /// De-emphasised text: hints, timestamps, counts.
    pub muted: Color,
    /// Borders and rules.
    pub border: Color,
    /// Ordinary body text. `Reset` lets the terminal decide, which is what
    /// keeps the UI legible on a light background.
    pub text: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

impl Theme {
    /// The default palette. Indexed ANSI throughout so it adapts to the user's
    /// terminal colours rather than fighting them.
    pub const fn dark() -> Self {
        Self {
            accent: Color::Cyan,
            user: Color::Blue,
            tool: Color::Yellow,
            success: Color::Green,
            warning: Color::Yellow,
            error: Color::Red,
            muted: Color::DarkGray,
            border: Color::DarkGray,
            text: Color::Reset,
        }
    }

    pub fn body(&self) -> Style {
        Style::default().fg(self.text)
    }

    pub fn dim(&self) -> Style {
        Style::default().fg(self.muted)
    }

    pub fn heading(&self) -> Style {
        Style::default()
            .fg(self.text)
            .add_modifier(Modifier::BOLD)
    }

    pub fn accented(&self) -> Style {
        Style::default().fg(self.accent)
    }

    pub fn label(&self) -> Style {
        Style::default()
            .fg(self.muted)
            .add_modifier(Modifier::DIM)
    }
}

/// Glyphs used as leading markers. Grouped here so the visual language stays
/// consistent and so a future `--ascii` mode has one place to swap them.
pub mod glyph {
    /// The user's own turn.
    pub const USER: &str = "›";
    /// The assistant's turn.
    pub const ASSISTANT: &str = "●";
    /// A tool call.
    pub const TOOL: &str = "⏺";
    /// A tool that succeeded.
    pub const OK: &str = "✓";
    /// A tool that failed.
    pub const FAIL: &str = "✗";
    /// A tool awaiting approval.
    pub const PENDING: &str = "◆";
    /// Collapsed detail available.
    pub const COLLAPSED: &str = "▸";
    /// Expanded detail.
    pub const EXPANDED: &str = "▾";
    /// Reasoning / thinking output.
    pub const THINKING: &str = "✻";
    /// A system notice.
    pub const NOTICE: &str = "•";

    /// Spinner frames, in order.
    pub const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_text_defers_to_the_terminal_foreground() {
        // Hardcoding white here would make the TUI unreadable in a light
        // terminal — a classic and very visible bug.
        assert_eq!(Theme::dark().text, Color::Reset);
    }

    #[test]
    fn every_spinner_frame_is_one_column_wide() {
        // A wider frame would shift the text after it on every tick.
        for frame in glyph::SPINNER {
            assert_eq!(
                unicode_width::UnicodeWidthStr::width(frame),
                1,
                "frame {frame:?} would make the spinner jitter"
            );
        }
    }

    #[test]
    fn markers_are_single_width() {
        for marker in [
            glyph::USER,
            glyph::ASSISTANT,
            glyph::TOOL,
            glyph::OK,
            glyph::FAIL,
            glyph::PENDING,
            glyph::COLLAPSED,
            glyph::EXPANDED,
            glyph::THINKING,
            glyph::NOTICE,
        ] {
            assert_eq!(
                unicode_width::UnicodeWidthStr::width(marker),
                1,
                "marker {marker:?} breaks gutter alignment"
            );
        }
    }
}
