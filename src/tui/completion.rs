//! Completion: fuzzy slash commands, their arguments, and `@` file mentions.
//!
//! The old REPL did prefix matching against a flat list of command strings —
//! `/mod` found `/model`, `/mdl` found nothing, and no command could offer its
//! own arguments. This does subsequence matching with a relevance score, and
//! knows the difference between completing a command name, a command argument,
//! and a path.

use std::path::{Path, PathBuf};

use super::commands::{self, Args};

/// What the word under the cursor is asking for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    /// A slash command name.
    Command,
    /// An argument to `command`.
    Argument(&'static str),
    /// A file path after `@`.
    File,
}

/// One offered completion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// Text to insert, replacing the word under the cursor.
    pub replacement: String,
    /// What to show in the popup.
    pub label: String,
    /// Secondary text (a command summary, or "dir").
    pub detail: String,
}

/// The active completion popup.
#[derive(Debug, Clone)]
pub struct Completion {
    pub kind: Kind,
    pub candidates: Vec<Candidate>,
    pub selected: usize,
    /// Character range in the composer that a chosen candidate replaces.
    pub range: std::ops::Range<usize>,
}

impl Completion {
    pub fn selected(&self) -> Option<&Candidate> {
        self.candidates.get(self.selected)
    }

    pub fn next(&mut self) {
        if !self.candidates.is_empty() {
            self.selected = (self.selected + 1) % self.candidates.len();
        }
    }

    pub fn previous(&mut self) {
        if !self.candidates.is_empty() {
            self.selected = self
                .selected
                .checked_sub(1)
                .unwrap_or(self.candidates.len() - 1);
        }
    }
}

/// Maximum candidates shown, so the popup never eats the screen.
const MAX_CANDIDATES: usize = 12;

/// Build a completion for `line` with the cursor at character `cursor`.
///
/// `workspace` roots file completion. `sessions` and `models` supply the
/// runtime-dependent argument lists; empty slices simply mean no suggestions.
pub fn compute(
    line: &str,
    cursor: usize,
    range: std::ops::Range<usize>,
    word: &str,
    workspace: &Path,
    sessions: &[String],
    models: &[String],
) -> Option<Completion> {
    let _ = cursor;

    // `@path` — file mention.
    if let Some(prefix) = word.strip_prefix('@') {
        let candidates = complete_path(prefix, workspace);
        return non_empty(Completion {
            kind: Kind::File,
            candidates,
            selected: 0,
            range,
        });
    }

    // `/command` — but only when it is the first word on the line.
    if word.starts_with('/') && line.trim_start().starts_with('/') && is_first_word(line, &range) {
        let candidates = complete_command(word);
        return non_empty(Completion {
            kind: Kind::Command,
            candidates,
            selected: 0,
            range,
        });
    }

    // An argument to a slash command already typed on this line.
    if let Some(command) = leading_command(line) {
        let candidates = match command.args {
            Args::None => Vec::new(),
            Args::Free(_) => Vec::new(),
            Args::Choice(options) => {
                let owned: Vec<String> = options.iter().map(|s| s.to_string()).collect();
                rank(word, &owned, "")
            }
            Args::Session => rank(word, sessions, "session"),
            Args::Model => rank(word, models, "model"),
        };
        return non_empty(Completion {
            kind: Kind::Argument(command.name),
            candidates,
            selected: 0,
            range,
        });
    }

    None
}

fn non_empty(completion: Completion) -> Option<Completion> {
    (!completion.candidates.is_empty()).then_some(completion)
}

/// Whether `range` covers the first whitespace-delimited word of `line`.
fn is_first_word(line: &str, range: &std::ops::Range<usize>) -> bool {
    let leading = line.chars().take_while(|c| c.is_whitespace()).count();
    range.start <= leading
}

/// The slash command at the start of `line`, if any.
fn leading_command(line: &str) -> Option<&'static commands::Command> {
    let first = line.split_whitespace().next()?;
    commands::lookup(first)
}

fn complete_command(word: &str) -> Vec<Candidate> {
    let query = word.trim_start_matches('/');
    let mut scored: Vec<(i32, Candidate)> = Vec::new();

    for command in commands::COMMANDS {
        // Score against the name and every alias; keep the best.
        let best = std::iter::once(command.name)
            .chain(command.aliases.iter().copied())
            .filter_map(|name| score(query, name.trim_start_matches('/')).map(|s| (s, name)))
            .max_by_key(|(s, _)| *s);

        if let Some((s, matched)) = best {
            // Commands that take an argument get a trailing space, so the next
            // keystroke starts the argument rather than extending the name.
            let replacement = if matches!(command.args, Args::None) {
                matched.to_string()
            } else {
                format!("{matched} ")
            };
            scored.push((
                s,
                Candidate {
                    replacement,
                    label: matched.to_string(),
                    detail: command.summary.to_string(),
                },
            ));
        }
    }

    finish(scored)
}

fn rank(word: &str, options: &[String], detail: &str) -> Vec<Candidate> {
    let mut scored: Vec<(i32, Candidate)> = Vec::new();
    for option in options {
        if let Some(s) = score(word, option) {
            scored.push((
                s,
                Candidate {
                    replacement: option.clone(),
                    label: option.clone(),
                    detail: detail.to_string(),
                },
            ));
        }
    }
    finish(scored)
}

fn finish(mut scored: Vec<(i32, Candidate)>) -> Vec<Candidate> {
    // Highest score first; ties resolve alphabetically so the order is stable
    // between keystrokes rather than jumping around.
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.label.cmp(&b.1.label)));
    scored
        .into_iter()
        .take(MAX_CANDIDATES)
        .map(|(_, c)| c)
        .collect()
}

/// Complete a filesystem path relative to `workspace`.
fn complete_path(prefix: &str, workspace: &Path) -> Vec<Candidate> {
    // Split into "directory already typed" and "partial name being typed".
    let (dir_part, name_part) = match prefix.rfind('/') {
        Some(i) => (&prefix[..=i], &prefix[i + 1..]),
        None => ("", prefix),
    };

    let dir: PathBuf = if dir_part.is_empty() {
        workspace.to_path_buf()
    } else {
        workspace.join(dir_part)
    };

    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };

    let mut scored: Vec<(i32, Candidate)> = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();

        // Hidden files only appear once the user types the dot, and build
        // output is never worth suggesting.
        if name.starts_with('.') && !name_part.starts_with('.') {
            continue;
        }
        if matches!(name.as_str(), "node_modules" | "target" | ".git") {
            continue;
        }

        let Some(s) = score(name_part, &name) else {
            continue;
        };
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);

        // A directory completes to itself plus a slash, so the next Tab
        // descends into it instead of accepting it as a final answer.
        let suffix = if is_dir { "/" } else { "" };
        scored.push((
            // Directories rank above files at equal score: you are usually on
            // the way somewhere.
            s + if is_dir { 1 } else { 0 },
            Candidate {
                replacement: format!("@{dir_part}{name}{suffix}"),
                label: format!("{dir_part}{name}{suffix}"),
                detail: if is_dir { "dir".into() } else { String::new() },
            },
        ));
    }

    finish(scored)
}

/// Subsequence match with a relevance score, or `None` if `query` does not
/// match `candidate` at all.
///
/// Scoring, highest first:
///   - exact match
///   - candidate starts with the query
///   - every query character appears in order, with a bonus for adjacency and
///     for matching at a word boundary (`-`, `_`, `/`, `.` or a case change)
///
/// Matching is case-insensitive; an exact-case hit scores slightly higher.
pub fn score(query: &str, candidate: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }

    let q: Vec<char> = query.chars().flat_map(|c| c.to_lowercase()).collect();
    let c: Vec<char> = candidate.chars().flat_map(|ch| ch.to_lowercase()).collect();

    if q.len() > c.len() {
        return None;
    }
    if q == c {
        return Some(1000 + if query == candidate { 10 } else { 0 });
    }
    if c.starts_with(&q[..]) {
        // Shorter candidates are better prefix matches: `/new` beats `/newish`.
        return Some(500 - c.len().min(400) as i32);
    }

    let mut score = 0i32;
    let mut ci = 0usize;
    let mut last_match: Option<usize> = None;

    for &qc in &q {
        let found = (ci..c.len()).find(|&i| c[i] == qc)?;
        score += 1;
        if last_match == Some(found.saturating_sub(1)) {
            score += 3; // adjacent characters
        }
        if found == 0 || is_boundary(&c, found) {
            score += 2; // start of a word
        }
        last_match = Some(found);
        ci = found + 1;
    }

    // Prefer tighter matches: the fewer characters skipped, the better.
    let span = last_match.unwrap_or(0) + 1;
    Some(score - (span.saturating_sub(q.len())) as i32)
}

fn is_boundary(chars: &[char], i: usize) -> bool {
    i > 0 && matches!(chars[i - 1], '-' | '_' | '/' | '.' | ' ')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn complete(line: &str, workspace: &Path) -> Option<Completion> {
        // Mirror Composer::word_before_cursor with the cursor at the end.
        let chars: Vec<char> = line.chars().collect();
        let cursor = chars.len();
        let mut start = cursor;
        while start > 0 && !chars[start - 1].is_whitespace() {
            start -= 1;
        }
        let word: String = chars[start..cursor].iter().collect();
        compute(line, cursor, start..cursor, &word, workspace, &[], &[])
    }

    fn labels(c: &Completion) -> Vec<String> {
        c.candidates.iter().map(|x| x.label.clone()).collect()
    }

    // ── Scoring ──────────────────────────────────────────────────────

    #[test]
    fn an_exact_match_outranks_a_prefix_match() {
        assert!(score("new", "new").unwrap() > score("new", "newish").unwrap());
    }

    #[test]
    fn a_prefix_match_outranks_a_scattered_subsequence() {
        assert!(score("mod", "model").unwrap() > score("mdl", "model").unwrap());
    }

    #[test]
    fn a_non_subsequence_does_not_match() {
        assert!(score("xyz", "model").is_none());
        assert!(score("longer than", "short").is_none());
    }

    #[test]
    fn matching_is_case_insensitive() {
        assert!(score("MOD", "model").is_some());
        assert!(score("mod", "MODEL").is_some());
    }

    #[test]
    fn an_empty_query_matches_everything() {
        assert_eq!(score("", "anything"), Some(0));
    }

    #[test]
    fn a_shorter_prefix_match_ranks_higher() {
        // `/new` should beat `/newish` when the user typed `new`.
        assert!(score("new", "new").unwrap() > score("new", "newer").unwrap());
    }

    // ── Command completion ───────────────────────────────────────────

    #[test]
    fn prefix_completion_still_works() {
        // `/mod` is a prefix of both `/mode` and `/model`; the closer (shorter)
        // one leads, and both are offered.
        let c = complete("/mod", Path::new(".")).unwrap();
        assert_eq!(c.kind, Kind::Command);
        let labels = labels(&c);
        assert_eq!(labels[0], "/mode");
        assert!(labels.contains(&"/model".to_string()), "{labels:?}");
    }

    #[test]
    fn fuzzy_completion_finds_what_prefix_matching_could_not() {
        // The old REPL returned nothing for this.
        let c = complete("/mdl", Path::new(".")).unwrap();
        assert!(labels(&c).contains(&"/model".to_string()), "{:?}", labels(&c));
    }

    #[test]
    fn aliases_are_offered() {
        let c = complete("/exi", Path::new(".")).unwrap();
        assert!(labels(&c).contains(&"/exit".to_string()));
    }

    #[test]
    fn a_bare_slash_offers_every_command() {
        let c = complete("/", Path::new(".")).unwrap();
        assert_eq!(c.candidates.len(), MAX_CANDIDATES.min(commands::COMMANDS.len()));
    }

    #[test]
    fn commands_taking_an_argument_complete_with_a_trailing_space() {
        let c = complete("/mode", Path::new(".")).unwrap();
        let mode = c.candidates.iter().find(|x| x.label == "/mode").unwrap();
        assert_eq!(mode.replacement, "/mode ", "so the next key starts the argument");
    }

    #[test]
    fn commands_taking_nothing_complete_without_a_trailing_space() {
        let c = complete("/help", Path::new(".")).unwrap();
        let help = c.candidates.iter().find(|x| x.label == "/help").unwrap();
        assert_eq!(help.replacement, "/help");
    }

    #[test]
    fn a_slash_that_is_not_the_first_word_is_not_a_command() {
        // "what does /usr/bin do" must not open the command popup.
        let c = complete("what does /usr", Path::new("."));
        assert!(c.is_none() || c.unwrap().kind != Kind::Command);
    }

    #[test]
    fn an_unmatchable_command_offers_nothing() {
        assert!(complete("/zzzzzzzz", Path::new(".")).is_none());
    }

    // ── Argument completion ──────────────────────────────────────────

    #[test]
    fn mode_arguments_complete_from_the_choice_list() {
        let c = complete("/mode work", Path::new(".")).unwrap();
        assert_eq!(c.kind, Kind::Argument("/mode"));
        assert_eq!(labels(&c), vec!["workspace-write"]);
    }

    #[test]
    fn export_arguments_complete_from_the_format_list() {
        let c = complete("/export ma", Path::new(".")).unwrap();
        assert_eq!(labels(&c), vec!["markdown"]);
    }

    #[test]
    fn an_argument_list_supplied_at_runtime_is_used() {
        let sessions = vec!["monday-refactor".to_string(), "tuesday-bug".to_string()];
        let c = compute(
            "/resume mon",
            11,
            8..11,
            "mon",
            Path::new("."),
            &sessions,
            &[],
        )
        .unwrap();
        assert_eq!(labels(&c), vec!["monday-refactor"]);
    }

    #[test]
    fn a_command_taking_free_text_offers_nothing() {
        assert!(complete("/search some query", Path::new(".")).is_none());
    }

    #[test]
    fn a_command_taking_no_argument_offers_nothing_after_it() {
        assert!(complete("/help anything", Path::new(".")).is_none());
    }

    // ── File completion ──────────────────────────────────────────────

    fn workspace() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("main.rs"), "").unwrap();
        std::fs::write(dir.path().join("mod.rs"), "").unwrap();
        std::fs::write(dir.path().join(".hidden"), "").unwrap();
        std::fs::create_dir(dir.path().join("src")).unwrap();
        std::fs::create_dir(dir.path().join("target")).unwrap();
        std::fs::write(dir.path().join("src").join("lib.rs"), "").unwrap();
        dir
    }

    #[test]
    fn an_at_sign_completes_workspace_files() {
        let ws = workspace();
        let c = complete("look at @ma", ws.path()).unwrap();
        assert_eq!(c.kind, Kind::File);
        assert_eq!(labels(&c), vec!["main.rs"]);
    }

    #[test]
    fn directories_complete_with_a_trailing_slash() {
        let ws = workspace();
        let c = complete("@sr", ws.path()).unwrap();
        let src = c.candidates.iter().find(|x| x.label.starts_with("src")).unwrap();
        assert_eq!(src.replacement, "@src/", "so the next Tab descends into it");
    }

    #[test]
    fn completion_descends_into_a_typed_directory() {
        let ws = workspace();
        let c = complete("@src/l", ws.path()).unwrap();
        assert_eq!(labels(&c), vec!["src/lib.rs"]);
        assert_eq!(c.candidates[0].replacement, "@src/lib.rs");
    }

    #[test]
    fn build_output_directories_are_never_suggested() {
        let ws = workspace();
        let c = complete("@t", ws.path());
        let labels = c.map(|c| labels(&c)).unwrap_or_default();
        assert!(
            !labels.iter().any(|l| l.starts_with("target")),
            "target/ is noise in a file picker: {labels:?}"
        );
    }

    #[test]
    fn hidden_files_appear_only_once_the_dot_is_typed() {
        let ws = workspace();
        let without = complete("@", ws.path()).map(|c| labels(&c)).unwrap_or_default();
        assert!(!without.iter().any(|l| l.starts_with(".hidden")));

        let with = complete("@.h", ws.path()).map(|c| labels(&c)).unwrap_or_default();
        assert!(with.iter().any(|l| l.starts_with(".hidden")), "{with:?}");
    }

    #[test]
    fn a_nonexistent_directory_offers_nothing_rather_than_erroring() {
        let ws = workspace();
        assert!(complete("@no/such/dir/x", ws.path()).is_none());
    }

    // ── Popup navigation ─────────────────────────────────────────────

    #[test]
    fn selection_wraps_in_both_directions() {
        let ws = workspace();
        let mut c = complete("/", ws.path()).unwrap();
        let n = c.candidates.len();

        c.previous();
        assert_eq!(c.selected, n - 1, "up from the top wraps to the bottom");
        c.next();
        assert_eq!(c.selected, 0, "down from the bottom wraps to the top");
    }

    #[test]
    fn the_candidate_list_is_capped() {
        let ws = workspace();
        let c = complete("/", ws.path()).unwrap();
        assert!(c.candidates.len() <= MAX_CANDIDATES);
    }

    #[test]
    fn ordering_is_stable_for_the_same_input() {
        let ws = workspace();
        let first = labels(&complete("/s", ws.path()).unwrap());
        let second = labels(&complete("/s", ws.path()).unwrap());
        assert_eq!(first, second, "a jumping popup is unusable");
    }
}
