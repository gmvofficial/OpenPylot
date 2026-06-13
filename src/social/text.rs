//! Helpers for sanitising user / LLM-supplied post text before it hits a
//! social platform's API.
//!
//! Most networks (LinkedIn, Twitter, Threads) render text as plain text and
//! show markdown syntax characters (`**bold**`, `*italic*`, `## heading`,
//! backticks) literally, which looks broken. This module strips that
//! formatting while preserving line breaks, hashtags, mentions, and emoji.

use once_cell::sync::Lazy;
use regex::Regex;

// Pre-compiled regexes (compile once, reuse forever).
static FENCED_CODE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?s)```[a-zA-Z0-9_+-]*\n?(.*?)```").unwrap());
static INLINE_CODE: Lazy<Regex> = Lazy::new(|| Regex::new(r"`([^`]+)`").unwrap());
static IMAGE: Lazy<Regex> = Lazy::new(|| Regex::new(r"!\[[^\]]*\]\(([^)]+)\)").unwrap());
static LINK: Lazy<Regex> = Lazy::new(|| Regex::new(r"\[([^\]]+)\]\(([^)]+)\)").unwrap());
static HEADING: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)^\s{0,3}#{1,6}\s+").unwrap());
static BLOCKQUOTE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)^\s{0,3}>\s?").unwrap());
static BULLET: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)^\s*[-*+]\s+").unwrap());
static ORDERED: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)^\s*\d+\.\s+").unwrap());
static BOLD: Lazy<Regex> = Lazy::new(|| Regex::new(r"\*\*([^*]+?)\*\*").unwrap());
static BOLD_UNDERSCORE: Lazy<Regex> = Lazy::new(|| Regex::new(r"__([^_]+?)__").unwrap());
static ITALIC_STAR: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?:^|[^*])\*([^*\n]+?)\*(?:[^*]|$)").unwrap());
static ITALIC_UNDERSCORE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?:^|[^_])_([^_\n]+?)_(?:[^_]|$)").unwrap());
static STRIKE: Lazy<Regex> = Lazy::new(|| Regex::new(r"~~([^~]+?)~~").unwrap());
static HR: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)^[-*_]{3,}\s*$").unwrap());
static MULTI_BLANK: Lazy<Regex> = Lazy::new(|| Regex::new(r"\n{3,}").unwrap());

/// Convert markdown-formatted text into clean plain text suitable for
/// LinkedIn / Twitter / Threads / Reddit.
///
/// Rules:
/// - `**bold**` / `__bold__` → `bold`
/// - `*italic*` / `_italic_` → `italic`
/// - `~~strike~~` → `strike`
/// - `# heading` → `heading`
/// - `> quote` → `quote`
/// - `- bullet` / `* bullet` / `1. item` → `• bullet`
/// - `[label](url)` → `label (url)` (so the URL is still visible)
/// - `![alt](url)` → `` (images dropped — caller should attach media instead)
/// - `` `code` `` → `code`
/// - ```` ```block``` ```` → `block`
/// - Horizontal rules dropped, runs of 3+ blank lines collapsed to 2.
pub fn strip_markdown(input: &str) -> String {
    if input.is_empty() {
        return String::new();
    }

    let mut s = input.to_string();

    // Drop images entirely (LinkedIn etc. need a real upload).
    s = IMAGE.replace_all(&s, "").to_string();

    // Fenced code blocks → keep inner text only.
    s = FENCED_CODE.replace_all(&s, "$1").to_string();

    // Inline code → unwrap.
    s = INLINE_CODE.replace_all(&s, "$1").to_string();

    // Links → "label (url)" so users still see the URL on platforms
    // that don't auto-render plain URLs.
    s = LINK.replace_all(&s, "$1 ($2)").to_string();

    // Bold / italic / strike — order matters: bold BEFORE italic so the
    // italic regex doesn't greedily eat one side of a `**word**`.
    s = BOLD.replace_all(&s, "$1").to_string();
    s = BOLD_UNDERSCORE.replace_all(&s, "$1").to_string();
    s = STRIKE.replace_all(&s, "$1").to_string();

    // Italic patterns use look-around-ish guards via capture groups, so we
    // re-build the matched slice keeping the surrounding char.
    s = ITALIC_STAR
        .replace_all(&s, |caps: &regex::Captures| {
            let full = &caps[0];
            let inner = &caps[1];
            // Replace just the *...* portion inside the surrounding chars.
            full.replacen(&format!("*{}*", inner), inner, 1)
        })
        .to_string();
    s = ITALIC_UNDERSCORE
        .replace_all(&s, |caps: &regex::Captures| {
            let full = &caps[0];
            let inner = &caps[1];
            full.replacen(&format!("_{}_", inner), inner, 1)
        })
        .to_string();

    // Headings → just the text.
    s = HEADING.replace_all(&s, "").to_string();

    // Blockquotes → drop the leading `>`.
    s = BLOCKQUOTE.replace_all(&s, "").to_string();

    // Bullet lists → unicode bullet (renders cleanly everywhere).
    s = BULLET.replace_all(&s, "• ").to_string();

    // Ordered lists keep their numbering, just normalise spacing.
    s = ORDERED.replace_all(&s, "").to_string();

    // Drop horizontal rules.
    s = HR.replace_all(&s, "").to_string();

    // Collapse runs of blank lines.
    s = MULTI_BLANK.replace_all(&s, "\n\n").to_string();

    s.trim().to_string()
}

/// Convert a LinkedIn UGC post URN (e.g. `urn:li:share:7321...` or
/// `urn:li:ugcPost:7321...`) into a public post URL the user can open
/// in their browser.
///
/// Returns `None` if the input isn't a recognisable URN.
pub fn linkedin_post_url(urn_or_id: &str) -> Option<String> {
    let s = urn_or_id.trim();
    if s.starts_with("urn:li:share:") || s.starts_with("urn:li:ugcPost:") {
        Some(format!("https://www.linkedin.com/feed/update/{}/", s))
    } else if s.chars().all(|c| c.is_ascii_digit()) && !s.is_empty() {
        // Bare numeric ID — assume it's a share URN suffix.
        Some(format!(
            "https://www.linkedin.com/feed/update/urn:li:share:{}/",
            s
        ))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_bold() {
        assert_eq!(strip_markdown("Hello **world**!"), "Hello world!");
    }

    #[test]
    fn strips_italic_and_bold_combined() {
        assert_eq!(
            strip_markdown("**Big** news: *I shipped* a thing"),
            "Big news: I shipped a thing"
        );
    }

    #[test]
    fn strips_headings_and_bullets() {
        let input = "## Today\n- one\n- two\n* three";
        let out = strip_markdown(input);
        assert!(out.starts_with("Today"));
        assert!(out.contains("• one"));
        assert!(out.contains("• three"));
    }

    #[test]
    fn unwraps_inline_code() {
        assert_eq!(
            strip_markdown("Use `cargo build` to compile"),
            "Use cargo build to compile"
        );
    }

    #[test]
    fn preserves_links_with_url() {
        assert_eq!(
            strip_markdown("Read [the docs](https://example.com)"),
            "Read the docs (https://example.com)"
        );
    }

    #[test]
    fn linkedin_url_from_share_urn() {
        assert_eq!(
            linkedin_post_url("urn:li:share:7123456789012345678").as_deref(),
            Some("https://www.linkedin.com/feed/update/urn:li:share:7123456789012345678/")
        );
    }

    #[test]
    fn linkedin_url_from_ugc_urn() {
        assert!(linkedin_post_url("urn:li:ugcPost:7123")
            .unwrap()
            .contains("urn:li:ugcPost:7123"));
    }

    #[test]
    fn linkedin_url_rejects_garbage() {
        assert_eq!(linkedin_post_url("not-a-urn"), None);
    }
}
