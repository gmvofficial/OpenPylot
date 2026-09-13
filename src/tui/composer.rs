//! The input box: a multi-line text buffer with a cursor and editing commands.
//!
//! Kept free of any terminal or rendering dependency so the editing semantics
//! — which are where the fiddly bugs live — can be tested directly.
//!
//! Positions are **character** offsets, never byte offsets. A byte offset into
//! a `String` is a panic waiting for the first user who types an accent or an
//! emoji, and a chat composer sees both constantly.

/// A multi-line editable buffer.
#[derive(Debug, Clone, Default)]
pub struct Composer {
    /// The text, as characters. Stored as a `Vec<char>` so every index
    /// operation is O(1) and can never split a multi-byte character.
    chars: Vec<char>,
    /// Cursor position, in characters, in `0..=chars.len()`.
    cursor: usize,
}

impl Composer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn text(&self) -> String {
        self.chars.iter().collect()
    }

    pub fn is_empty(&self) -> bool {
        self.chars.is_empty()
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn len(&self) -> usize {
        self.chars.len()
    }

    /// Replace the whole buffer and put the cursor at the end.
    ///
    /// Used by history recall and completion acceptance.
    pub fn set_text(&mut self, text: &str) {
        self.chars = text.chars().collect();
        self.cursor = self.chars.len();
    }

    pub fn clear(&mut self) {
        self.chars.clear();
        self.cursor = 0;
    }

    /// Take the text and reset the buffer.
    pub fn take(&mut self) -> String {
        let text = self.text();
        self.clear();
        text
    }

    pub fn insert_char(&mut self, c: char) {
        self.chars.insert(self.cursor, c);
        self.cursor += 1;
    }

    pub fn insert_str(&mut self, text: &str) {
        for c in text.chars() {
            self.insert_char(c);
        }
    }

    /// Backspace. No-op at the start of the buffer.
    pub fn delete_backward(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.chars.remove(self.cursor);
        }
    }

    /// Delete forward. No-op at the end of the buffer.
    pub fn delete_forward(&mut self) {
        if self.cursor < self.chars.len() {
            self.chars.remove(self.cursor);
        }
    }

    /// Delete the word before the cursor (Ctrl+W / Alt+Backspace).
    ///
    /// Skips trailing whitespace first, so pressing it after a space removes
    /// the word rather than just the space.
    pub fn delete_word_backward(&mut self) {
        let start = self.word_start();
        self.chars.drain(start..self.cursor);
        self.cursor = start;
    }

    /// Delete from the cursor to the start of the line (Ctrl+U).
    pub fn delete_to_line_start(&mut self) {
        let start = self.line_start();
        self.chars.drain(start..self.cursor);
        self.cursor = start;
    }

    /// Delete from the cursor to the end of the line (Ctrl+K).
    pub fn delete_to_line_end(&mut self) {
        let end = self.line_end();
        self.chars.drain(self.cursor..end);
    }

    pub fn move_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn move_right(&mut self) {
        if self.cursor < self.chars.len() {
            self.cursor += 1;
        }
    }

    pub fn move_word_left(&mut self) {
        self.cursor = self.word_start();
    }

    pub fn move_word_right(&mut self) {
        let mut i = self.cursor;
        while i < self.chars.len() && self.chars[i].is_whitespace() {
            i += 1;
        }
        while i < self.chars.len() && !self.chars[i].is_whitespace() {
            i += 1;
        }
        self.cursor = i;
    }

    pub fn move_to_line_start(&mut self) {
        self.cursor = self.line_start();
    }

    pub fn move_to_line_end(&mut self) {
        self.cursor = self.line_end();
    }

    pub fn move_to_start(&mut self) {
        self.cursor = 0;
    }

    pub fn move_to_end(&mut self) {
        self.cursor = self.chars.len();
    }

    /// Move up one visual line, keeping the column where possible.
    /// Returns false when already on the first line, so the caller can fall
    /// back to history recall — the behaviour a shell user expects.
    pub fn move_up(&mut self) -> bool {
        let start = self.line_start();
        if start == 0 {
            return false;
        }
        let column = self.cursor - start;
        // The line above ends at `start - 1` (the newline itself).
        let prev_end = start - 1;
        let prev_start = self.line_start_of(prev_end);
        self.cursor = (prev_start + column).min(prev_end);
        true
    }

    /// Move down one visual line. Returns false when already on the last line.
    pub fn move_down(&mut self) -> bool {
        let end = self.line_end();
        if end >= self.chars.len() {
            return false;
        }
        let column = self.cursor - self.line_start();
        let next_start = end + 1;
        let next_end = self.line_end_of(next_start);
        self.cursor = (next_start + column).min(next_end);
        true
    }

    /// The lines of the buffer, for rendering.
    pub fn lines(&self) -> Vec<String> {
        self.text().split('\n').map(|s| s.to_string()).collect()
    }

    /// Cursor position as (line index, column), both 0-based in characters.
    pub fn cursor_position(&self) -> (usize, usize) {
        let mut line = 0usize;
        let mut column = 0usize;
        for &c in &self.chars[..self.cursor] {
            if c == '\n' {
                line += 1;
                column = 0;
            } else {
                column += 1;
            }
        }
        (line, column)
    }

    /// The word immediately before the cursor, for completion. Returns the
    /// character range and the text.
    ///
    /// A "word" here stops at whitespace only, so `/model` and `@src/main.rs`
    /// both come back whole — which is what the completion engine needs.
    pub fn word_before_cursor(&self) -> (std::ops::Range<usize>, String) {
        let mut start = self.cursor;
        while start > 0 && !self.chars[start - 1].is_whitespace() {
            start -= 1;
        }
        let text: String = self.chars[start..self.cursor].iter().collect();
        (start..self.cursor, text)
    }

    /// Replace a character range with `text`, leaving the cursor after it.
    pub fn replace_range(&mut self, range: std::ops::Range<usize>, text: &str) {
        let start = range.start.min(self.chars.len());
        let end = range.end.min(self.chars.len());
        self.chars.splice(start..end, text.chars());
        self.cursor = start + text.chars().count();
    }

    // ── Line boundaries ──────────────────────────────────────────────

    fn line_start(&self) -> usize {
        self.line_start_of(self.cursor)
    }

    fn line_start_of(&self, from: usize) -> usize {
        let mut i = from;
        while i > 0 && self.chars[i - 1] != '\n' {
            i -= 1;
        }
        i
    }

    fn line_end(&self) -> usize {
        self.line_end_of(self.cursor)
    }

    fn line_end_of(&self, from: usize) -> usize {
        let mut i = from;
        while i < self.chars.len() && self.chars[i] != '\n' {
            i += 1;
        }
        i
    }

    fn word_start(&self) -> usize {
        let mut i = self.cursor;
        while i > 0 && self.chars[i - 1].is_whitespace() {
            i -= 1;
        }
        while i > 0 && !self.chars[i - 1].is_whitespace() {
            i -= 1;
        }
        i
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn composer(text: &str) -> Composer {
        let mut c = Composer::new();
        c.set_text(text);
        c
    }

    fn at(text: &str, cursor: usize) -> Composer {
        let mut c = composer(text);
        c.cursor = cursor;
        c
    }

    #[test]
    fn starts_empty() {
        let c = Composer::new();
        assert!(c.is_empty());
        assert_eq!(c.cursor(), 0);
        assert_eq!(c.text(), "");
    }

    #[test]
    fn typing_inserts_at_the_cursor() {
        let mut c = Composer::new();
        c.insert_str("helo");
        c.move_left();
        c.insert_char('l');
        assert_eq!(c.text(), "hello");
    }

    #[test]
    fn set_text_puts_the_cursor_at_the_end() {
        let c = composer("hello");
        assert_eq!(c.cursor(), 5);
    }

    #[test]
    fn take_returns_the_text_and_empties_the_buffer() {
        let mut c = composer("send me");
        assert_eq!(c.take(), "send me");
        assert!(c.is_empty());
        assert_eq!(c.cursor(), 0);
    }

    // ── Multi-byte safety ────────────────────────────────────────────

    #[test]
    fn multibyte_characters_are_one_position_each() {
        let mut c = Composer::new();
        c.insert_str("héllo");
        assert_eq!(c.cursor(), 5, "é must count as one position, not two");
        c.delete_backward();
        assert_eq!(c.text(), "héll");
    }

    #[test]
    fn emoji_can_be_deleted_without_corrupting_the_buffer() {
        // Byte-indexed editing panics or produces mojibake here.
        let mut c = Composer::new();
        c.insert_str("hi 🎉 there");
        c.move_to_start();
        for _ in 0..3 {
            c.move_right();
        }
        c.delete_forward();
        assert_eq!(c.text(), "hi  there");
    }

    #[test]
    fn editing_in_the_middle_of_multibyte_text_is_safe() {
        let mut c = composer("日本語テキスト");
        c.move_to_start();
        c.move_right();
        c.insert_char('X');
        assert_eq!(c.text(), "日X本語テキスト");
    }

    // ── Deletion ─────────────────────────────────────────────────────

    #[test]
    fn backspace_at_the_start_is_a_no_op() {
        let mut c = at("abc", 0);
        c.delete_backward();
        assert_eq!(c.text(), "abc");
        assert_eq!(c.cursor(), 0);
    }

    #[test]
    fn delete_forward_at_the_end_is_a_no_op() {
        let mut c = composer("abc");
        c.delete_forward();
        assert_eq!(c.text(), "abc");
    }

    #[test]
    fn delete_word_backward_removes_the_whole_word() {
        let mut c = composer("hello brave world");
        c.delete_word_backward();
        assert_eq!(c.text(), "hello brave ");
    }

    #[test]
    fn delete_word_backward_skips_trailing_space_first() {
        // Pressing Ctrl+W after a space should eat the word, not just the space.
        let mut c = composer("hello world   ");
        c.delete_word_backward();
        assert_eq!(c.text(), "hello ");
    }

    #[test]
    fn delete_word_backward_on_an_empty_buffer_is_a_no_op() {
        let mut c = Composer::new();
        c.delete_word_backward();
        assert!(c.is_empty());
    }

    #[test]
    fn ctrl_u_deletes_to_the_start_of_the_current_line_only() {
        let mut c = composer("first line\nsecond line");
        c.delete_to_line_start();
        assert_eq!(c.text(), "first line\n");
    }

    #[test]
    fn ctrl_k_deletes_to_the_end_of_the_current_line_only() {
        let mut c = at("first line\nsecond line", 3);
        c.delete_to_line_end();
        assert_eq!(c.text(), "fir\nsecond line");
    }

    // ── Movement ─────────────────────────────────────────────────────

    #[test]
    fn cursor_movement_is_clamped_at_both_ends() {
        let mut c = at("ab", 0);
        c.move_left();
        assert_eq!(c.cursor(), 0);
        c.move_to_end();
        c.move_right();
        assert_eq!(c.cursor(), 2);
    }

    #[test]
    fn word_movement_crosses_words_not_characters() {
        let mut c = composer("alpha beta gamma");
        c.move_word_left();
        assert_eq!(c.cursor(), 11, "should land at the start of 'gamma'");
        c.move_word_left();
        assert_eq!(c.cursor(), 6);
    }

    #[test]
    fn move_word_right_stops_at_the_end_of_the_next_word() {
        let mut c = at("alpha beta", 0);
        c.move_word_right();
        assert_eq!(c.cursor(), 5);
        c.move_word_right();
        assert_eq!(c.cursor(), 10);
        c.move_word_right();
        assert_eq!(c.cursor(), 10, "must not run past the end");
    }

    #[test]
    fn home_and_end_act_on_the_current_line() {
        let mut c = at("one\ntwo\nthree", 5);
        c.move_to_line_start();
        assert_eq!(c.cursor(), 4);
        c.move_to_line_end();
        assert_eq!(c.cursor(), 7);
    }

    // ── Vertical movement ────────────────────────────────────────────

    #[test]
    fn move_up_reports_false_on_the_first_line() {
        // The caller uses this to fall through to history recall.
        let mut c = at("single line", 3);
        assert!(!c.move_up());
        assert_eq!(c.cursor(), 3, "a refused move must not move the cursor");
    }

    #[test]
    fn move_down_reports_false_on_the_last_line() {
        let mut c = at("single line", 3);
        assert!(!c.move_down());
        assert_eq!(c.cursor(), 3);
    }

    #[test]
    fn move_up_keeps_the_column() {
        let mut c = at("abcdef\nghijkl", 10); // line 1, column 3
        assert!(c.move_up());
        assert_eq!(c.cursor(), 3);
    }

    #[test]
    fn move_up_clamps_to_a_shorter_line() {
        let mut c = at("ab\nlonger line", 10); // line 1, column 7
        assert!(c.move_up());
        assert_eq!(c.cursor(), 2, "should clamp to the end of the shorter line");
    }

    #[test]
    fn move_down_clamps_to_a_shorter_line() {
        let mut c = at("longer line\nab", 7);
        assert!(c.move_down());
        assert_eq!(c.cursor(), 14);
    }

    // ── Reporting ────────────────────────────────────────────────────

    #[test]
    fn cursor_position_reports_line_and_column() {
        assert_eq!(at("one\ntwo", 0).cursor_position(), (0, 0));
        assert_eq!(at("one\ntwo", 3).cursor_position(), (0, 3));
        assert_eq!(at("one\ntwo", 4).cursor_position(), (1, 0));
        assert_eq!(at("one\ntwo", 7).cursor_position(), (1, 3));
    }

    #[test]
    fn lines_splits_on_newlines_and_keeps_empty_ones() {
        assert_eq!(composer("a\n\nb").lines(), vec!["a", "", "b"]);
        assert_eq!(Composer::new().lines(), vec![""]);
    }

    // ── Completion support ───────────────────────────────────────────

    #[test]
    fn word_before_cursor_returns_a_slash_command_whole() {
        let (range, word) = composer("/mod").word_before_cursor();
        assert_eq!(word, "/mod");
        assert_eq!(range, 0..4);
    }

    #[test]
    fn word_before_cursor_returns_a_file_mention_whole() {
        // A path contains slashes and dots; stopping at those would break it.
        let (_, word) = composer("look at @src/main.rs").word_before_cursor();
        assert_eq!(word, "@src/main.rs");
    }

    #[test]
    fn word_before_cursor_is_empty_after_a_space() {
        let (range, word) = composer("hello ").word_before_cursor();
        assert_eq!(word, "");
        assert_eq!(range, 6..6);
    }

    #[test]
    fn replace_range_swaps_text_and_moves_the_cursor_after_it() {
        let mut c = composer("/mod");
        let (range, _) = c.word_before_cursor();
        c.replace_range(range, "/model ");
        assert_eq!(c.text(), "/model ");
        assert_eq!(c.cursor(), 7);
    }

    #[test]
    fn replace_range_in_the_middle_keeps_the_tail() {
        let mut c = at("see @mai and stop", 8);
        let (range, word) = c.word_before_cursor();
        assert_eq!(word, "@mai");
        c.replace_range(range, "@src/main.rs");
        assert_eq!(c.text(), "see @src/main.rs and stop");
    }

    #[test]
    fn replace_range_clamps_an_out_of_bounds_range() {
        // Guards against a stale completion range after the buffer shrank.
        let mut c = composer("ab");
        c.replace_range(0..99, "xyz");
        assert_eq!(c.text(), "xyz");
    }
}
