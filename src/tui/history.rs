//! Prompt history: recall with ↑/↓ and reverse search with Ctrl+R.
//!
//! Persisted to `<data_dir>/history` so it survives restarts, the way a shell's
//! does. The previous REPL kept history only for the life of the process.

use std::path::{Path, PathBuf};

use super::completion::score;

/// How many entries to keep on disk. Enough to be useful, small enough that
/// loading it is free.
const MAX_ENTRIES: usize = 1000;

/// Prompt history with a recall cursor and a search mode.
#[derive(Debug, Default)]
pub struct History {
    /// Oldest first, so index 0 is the first thing ever typed.
    entries: Vec<String>,
    /// Recall position: `None` means "at the live buffer, below the history".
    cursor: Option<usize>,
    /// Text that was in the composer when recall started, restored on ↓ past
    /// the newest entry.
    stash: Option<String>,
    path: Option<PathBuf>,
}

impl History {
    pub fn new() -> Self {
        Self::default()
    }

    /// Load from `<data_dir>/history`, ignoring a missing or unreadable file —
    /// history is a convenience, never a reason to fail to start.
    pub fn load(data_dir: &Path) -> Self {
        let path = data_dir.join("history");
        let entries = std::fs::read_to_string(&path)
            .map(|text| {
                text.lines()
                    .map(unescape)
                    .filter(|l| !l.trim().is_empty())
                    .collect()
            })
            .unwrap_or_default();

        Self {
            entries,
            cursor: None,
            stash: None,
            path: Some(path),
        }
    }

    pub fn entries(&self) -> &[String] {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Record a submitted prompt.
    ///
    /// Blank entries and an immediate repeat of the previous entry are dropped,
    /// matching shell behaviour — pressing ↑ should not walk through five
    /// copies of the same thing.
    pub fn push(&mut self, entry: &str) {
        self.reset_cursor();

        let trimmed = entry.trim();
        if trimmed.is_empty() {
            return;
        }
        if self.entries.last().map(String::as_str) == Some(trimmed) {
            return;
        }

        self.entries.push(trimmed.to_string());
        if self.entries.len() > MAX_ENTRIES {
            let overflow = self.entries.len() - MAX_ENTRIES;
            self.entries.drain(0..overflow);
        }
        self.persist();
    }

    /// Move back one entry. `current` is the live composer text, stashed on the
    /// first step so ↓ can restore it.
    pub fn previous(&mut self, current: &str) -> Option<String> {
        if self.entries.is_empty() {
            return None;
        }
        let next = match self.cursor {
            None => {
                self.stash = Some(current.to_string());
                self.entries.len() - 1
            }
            Some(0) => return None, // already at the oldest
            Some(i) => i - 1,
        };
        self.cursor = Some(next);
        self.entries.get(next).cloned()
    }

    /// Move forward one entry, returning the stashed draft past the newest.
    pub fn next(&mut self) -> Option<String> {
        match self.cursor {
            None => None,
            Some(i) if i + 1 < self.entries.len() => {
                self.cursor = Some(i + 1);
                self.entries.get(i + 1).cloned()
            }
            Some(_) => {
                self.cursor = None;
                Some(self.stash.take().unwrap_or_default())
            }
        }
    }

    /// Abandon recall, forgetting any stashed draft.
    pub fn reset_cursor(&mut self) {
        self.cursor = None;
        self.stash = None;
    }

    pub fn is_recalling(&self) -> bool {
        self.cursor.is_some()
    }

    /// Entries matching `query`, newest first, ranked by relevance.
    ///
    /// Backs Ctrl+R. An empty query returns everything, newest first, so the
    /// search opens on the full list rather than on nothing.
    pub fn search(&self, query: &str) -> Vec<&str> {
        let mut scored: Vec<(i32, usize, &str)> = self
            .entries
            .iter()
            .enumerate()
            .filter_map(|(i, entry)| score(query, entry).map(|s| (s, i, entry.as_str())))
            .collect();

        // Score first, then recency — two equally good matches should show the
        // more recent one first.
        scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
        scored.into_iter().map(|(_, _, e)| e).collect()
    }

    /// Write history to disk, best effort.
    fn persist(&self) {
        let Some(path) = &self.path else { return };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let body: String = self
            .entries
            .iter()
            .map(|e| format!("{}\n", escape(e)))
            .collect();
        if let Err(e) = std::fs::write(path, body) {
            tracing::debug!("Could not persist prompt history: {e}");
        }
    }
}

/// Encode newlines so a multi-line prompt stays one history line.
fn escape(entry: &str) -> String {
    entry.replace('\\', "\\\\").replace('\n', "\\n")
}

/// Reverse [`escape`].
fn unescape(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn history(entries: &[&str]) -> History {
        let mut h = History::new();
        for e in entries {
            h.push(e);
        }
        h
    }

    #[test]
    fn an_empty_history_recalls_nothing() {
        let mut h = History::new();
        assert_eq!(h.previous("draft"), None);
        assert_eq!(h.next(), None);
    }

    #[test]
    fn blank_entries_are_not_recorded() {
        let h = history(&["", "   ", "\n"]);
        assert!(h.is_empty());
    }

    #[test]
    fn entries_are_trimmed() {
        let h = history(&["  spaced  "]);
        assert_eq!(h.entries(), ["spaced"]);
    }

    #[test]
    fn an_immediate_repeat_is_collapsed() {
        // Otherwise ↑ walks through duplicates of the same command.
        let h = history(&["ls", "ls", "ls"]);
        assert_eq!(h.len(), 1);
    }

    #[test]
    fn a_non_adjacent_repeat_is_kept() {
        let h = history(&["a", "b", "a"]);
        assert_eq!(h.entries(), ["a", "b", "a"]);
    }

    #[test]
    fn up_walks_backwards_from_the_newest() {
        let mut h = history(&["first", "second", "third"]);
        assert_eq!(h.previous("").as_deref(), Some("third"));
        assert_eq!(h.previous("").as_deref(), Some("second"));
        assert_eq!(h.previous("").as_deref(), Some("first"));
    }

    #[test]
    fn up_stops_at_the_oldest_entry() {
        let mut h = history(&["only"]);
        assert_eq!(h.previous("").as_deref(), Some("only"));
        assert_eq!(h.previous(""), None, "must not wrap around to the newest");
    }

    #[test]
    fn down_returns_towards_the_live_draft() {
        let mut h = history(&["first", "second"]);
        h.previous("");
        h.previous("");
        assert_eq!(h.next().as_deref(), Some("second"));
    }

    #[test]
    fn the_draft_is_restored_when_stepping_past_the_newest() {
        // The classic shell behaviour: type something, press ↑, press ↓, and
        // get your half-written line back.
        let mut h = history(&["old"]);
        assert_eq!(h.previous("my draft").as_deref(), Some("old"));
        assert_eq!(h.next().as_deref(), Some("my draft"));
        assert!(!h.is_recalling());
    }

    #[test]
    fn down_without_recalling_does_nothing() {
        let mut h = history(&["a"]);
        assert_eq!(h.next(), None);
    }

    #[test]
    fn submitting_ends_recall() {
        let mut h = history(&["a", "b"]);
        h.previous("");
        assert!(h.is_recalling());
        h.push("c");
        assert!(!h.is_recalling());
    }

    // ── Search ───────────────────────────────────────────────────────

    #[test]
    fn search_finds_a_substring() {
        let h = history(&["cargo build", "cargo test", "git status"]);
        assert_eq!(h.search("test"), vec!["cargo test"]);
    }

    #[test]
    fn search_is_fuzzy() {
        let h = history(&["cargo build --release"]);
        assert_eq!(h.search("cbr"), vec!["cargo build --release"]);
    }

    #[test]
    fn search_prefers_the_more_recent_of_two_equal_matches() {
        let h = history(&["deploy", "other", "deploy"]);
        // Both entries are identical, so recency is the only tiebreak; the
        // result should still list two and start with the newer.
        let found = h.search("deploy");
        assert_eq!(found.len(), 2);
    }

    #[test]
    fn an_empty_search_returns_everything() {
        let h = history(&["a", "b", "c"]);
        assert_eq!(h.search("").len(), 3);
    }

    #[test]
    fn search_returns_nothing_when_nothing_matches() {
        let h = history(&["cargo build"]);
        assert!(h.search("zzzzz").is_empty());
    }

    // ── Persistence ──────────────────────────────────────────────────

    #[test]
    fn history_round_trips_through_disk() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = History::load(dir.path());
        h.push("first command");
        h.push("second command");

        let reloaded = History::load(dir.path());
        assert_eq!(reloaded.entries(), ["first command", "second command"]);
    }

    #[test]
    fn a_multiline_prompt_survives_the_round_trip() {
        // Stored one entry per line, so newlines must be escaped or the entry
        // comes back split in two.
        let dir = tempfile::tempdir().unwrap();
        let mut h = History::load(dir.path());
        h.push("line one\nline two");

        let reloaded = History::load(dir.path());
        assert_eq!(reloaded.entries(), ["line one\nline two"]);
    }

    #[test]
    fn a_literal_backslash_n_is_not_turned_into_a_newline() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = History::load(dir.path());
        h.push(r"grep '\n' file");

        let reloaded = History::load(dir.path());
        assert_eq!(reloaded.entries(), [r"grep '\n' file"]);
    }

    #[test]
    fn a_missing_history_file_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(History::load(&dir.path().join("nonexistent")).is_empty());
    }

    #[test]
    fn history_is_capped() {
        let dir = tempfile::tempdir().unwrap();
        let mut h = History::load(dir.path());
        for i in 0..(MAX_ENTRIES + 50) {
            h.push(&format!("command {i}"));
        }
        assert_eq!(h.len(), MAX_ENTRIES);
        assert_eq!(
            h.entries().last().unwrap(),
            &format!("command {}", MAX_ENTRIES + 49),
            "the newest entry must survive the trim"
        );
    }
}
