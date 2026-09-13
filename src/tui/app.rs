//! The TUI application: state, key handling, layout and the event loop.
//!
//! # Shape
//!
//! The terminal is driven in **inline** mode, not full-screen. Finished
//! transcript entries are pushed into the terminal's own scrollback with
//! `insert_before`, and only the live part — a streaming reply, the completion
//! popup, the composer and the status line — occupies the managed viewport.
//!
//! That is what lets you scroll back through a session with your terminal's
//! own scrollbar, copy text with the mouse, and leave the conversation on
//! screen after quitting. A full-screen alternate-screen TUI gives all of that
//! up, which is why this does not use one.
//!
//! ```text
//!   › refactor the parser                      ← scrollback (real history)
//!   ⏺ read_file  src/parse.rs            ▸
//!   ● Here is what I found …
//!   ────────────────────────────────────────
//!   ● streaming reply …                        ← viewport (redrawn each frame)
//!   ╭──────────────────────────────────────╮
//!   │ › ▏                                  │
//!   ╰──────────────────────────────────────╯
//!    claude-opus-5 · workspace-write   ctx 4%
//! ```

use std::io;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::Result;
use ratatui::crossterm::event::{
    DisableBracketedPaste, EnableBracketedPaste, Event, EventStream, KeyCode, KeyEvent,
    KeyEventKind, KeyModifiers,
};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Widget, Wrap};
use ratatui::{Frame, Terminal, TerminalOptions, Viewport};
use tokio_stream::StreamExt;

use crate::agent::Agent;
use crate::config::AppConfig;
use crate::permissions::PermissionMode;
use crate::streaming::{stream_channel, StreamEvent};
use crate::usage::{TokenUsage, UsageTracker};

use super::commands::{self, Args};
use super::completion::{self, Completion, Kind};
use super::composer::Composer;
use super::history::History;
use super::status::{self, Activity, Status};
use super::theme::{glyph, Theme};
use super::transcript::{Entry, Level, ToolCall, ToolStatus, Transcript};

/// Spinner tick. Fast enough to look alive, slow enough to cost nothing.
const TICK: Duration = Duration::from_millis(80);

/// Rows the composer area occupies, excluding its border.
const COMPOSER_MIN_ROWS: u16 = 1;
const COMPOSER_MAX_ROWS: u16 = 10;

/// What keystrokes currently mean.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Focus {
    /// Editing the composer.
    Composer,
    /// Ctrl+R reverse history search.
    HistorySearch,
}

/// Outcome of handling one key.
enum Action {
    /// Keep going.
    Continue,
    /// Send this text to the agent.
    Submit(String),
    /// Leave.
    Quit,
}

pub struct App {
    agent: Agent,
    transcript: Transcript,
    composer: Composer,
    history: History,
    completion: Option<Completion>,
    status: Status,
    usage: UsageTracker,
    theme: Theme,
    focus: Focus,

    /// Ctrl+R query and its results.
    search_query: String,
    search_selected: usize,

    /// Assistant text accumulated during the current turn.
    streaming: String,
    /// Spinner frame index.
    frame: usize,

    /// Entries already flushed into terminal scrollback.
    flushed: usize,

    workspace: PathBuf,
    agent_name: String,
    /// Models offered by `/model` completion.
    models: Vec<String>,
    /// Whether the user has been shown the Ctrl+C hint this session.
    quit_armed: Option<Instant>,
}

impl App {
    pub fn new(agent: Agent, config: &AppConfig) -> Self {
        let mut status = Status {
            model: config.llm_model.clone(),
            mode: PermissionMode::WorkspaceWrite,
            ..Default::default()
        };
        status.mcp_servers = 0;

        Self {
            agent,
            transcript: Transcript::new(),
            composer: Composer::new(),
            history: History::load(&config.data_dir),
            completion: None,
            status,
            usage: UsageTracker::new(&config.llm_model),
            theme: Theme::dark(),
            focus: Focus::Composer,
            search_query: String::new(),
            search_selected: 0,
            streaming: String::new(),
            frame: 0,
            flushed: 0,
            workspace: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            agent_name: config.agent_name.clone(),
            models: known_models(&config.llm_provider),
            quit_armed: None,
        }
    }

    /// Record how many MCP servers answered, for the status-line badge.
    pub fn set_mcp_servers(&mut self, count: usize) {
        self.status.mcp_servers = count;
    }

    /// Run until the user quits.
    pub async fn run(mut self) -> Result<()> {
        let mut terminal = setup()?;
        let result = self.event_loop(&mut terminal).await;
        restore(&mut terminal)?;
        result
    }

    async fn event_loop(&mut self, terminal: &mut Tui) -> Result<()> {
        self.greet();

        let mut events = EventStream::new();
        let mut ticker = tokio::time::interval(TICK);

        loop {
            self.flush_completed(terminal)?;
            self.draw(terminal)?;

            let submitted = tokio::select! {
                maybe = events.next() => {
                    match maybe {
                        Some(Ok(Event::Key(key))) => match self.on_key(key) {
                            Action::Quit => return Ok(()),
                            Action::Submit(text) => Some(text),
                            Action::Continue => None,
                        },
                        Some(Ok(Event::Paste(text))) => {
                            self.composer.insert_str(&text);
                            self.refresh_completion();
                            None
                        }
                        Some(Ok(Event::Resize(_, _))) => None,
                        Some(Ok(_)) => None,
                        Some(Err(e)) => return Err(anyhow::Error::from(e)),
                        None => return Ok(()),
                    }
                }
                _ = ticker.tick() => {
                    self.frame = self.frame.wrapping_add(1);
                    None
                }
            };

            if let Some(text) = submitted {
                self.run_turn(terminal, &mut events, text).await?;
            }
        }
    }

    // ── One agent turn ───────────────────────────────────────────────

    /// Drive one message through the agent, streaming into the transcript.
    ///
    /// Keys are still handled while this runs, which is what makes Esc able to
    /// interrupt: the old REPL blocked on `agent.chat().await` with no way to
    /// get a keystroke in, so Ctrl+C killed the whole process.
    async fn run_turn(
        &mut self,
        terminal: &mut Tui,
        events: &mut EventStream,
        input: String,
    ) -> Result<()> {
        self.history.push(&input);
        self.transcript.push_user(input.clone());

        if let Some(action) = self.try_slash_command(&input) {
            match action {
                SlashOutcome::Handled => return Ok(()),
                SlashOutcome::Quit => {
                    self.flush_completed(terminal)?;
                    return Err(Quit.into());
                }
                SlashOutcome::SendToAgent => {}
            }
        }

        let (tx, mut rx) = stream_channel();

        // The agent future borrows `self.agent` for as long as it is alive, so
        // it lives in its own scope: everything that needs `&mut self` again —
        // clearing the sender, folding the result into the transcript — happens
        // after the block, once the future has been dropped. Dropping it is
        // also what cancels the in-flight HTTP request on an interrupt.
        let (outcome, interrupted) = {
            let Self {
                agent,
                transcript,
                status,
                usage,
                streaming,
                frame,
                theme,
                flushed,
                ..
            } = self;

            agent.set_stream_sender(tx);
            agent.set_streaming(true);
            status.activity = Activity::Thinking;
            streaming.clear();

            let chat = agent.chat(&input);
            tokio::pin!(chat);

            let mut ticker = tokio::time::interval(TICK);
            let mut outcome: Option<Result<String>> = None;
            let mut interrupted = false;

            loop {
                // Move anything finished into scrollback, then redraw the live part.
                flush_entries(terminal, transcript, flushed, theme)?;
                draw_frame(
                    terminal,
                    transcript,
                    streaming,
                    status,
                    theme,
                    *frame,
                    &Composer::new(),
                    &None,
                    &Focus::Composer,
                    "",
                    true,
                )?;

                tokio::select! {
                    biased;

                    // Drain stream events first so the display stays ahead of the model.
                    event = rx.recv() => {
                        if let Some(event) = event {
                            apply_stream_event(event, transcript, status, usage, streaming);
                        }
                    }

                    result = &mut chat, if outcome.is_none() => {
                        outcome = Some(result);
                    }

                    maybe = events.next() => {
                        if let Some(Ok(Event::Key(key))) = maybe {
                            if is_interrupt(&key) {
                                interrupted = true;
                                status.activity = Activity::Cancelling;
                                break;
                            }
                            if is_toggle_fold(&key) {
                                transcript.toggle_last_tool();
                            }
                        }
                    }

                    _ = ticker.tick() => {
                        *frame = frame.wrapping_add(1);
                    }
                }

                // Done once the agent has returned and the event channel has
                // drained — otherwise the last few deltas would be dropped.
                if outcome.is_some() && rx.is_empty() {
                    break;
                }
            }

            (outcome, interrupted)
        };

        self.agent.clear_stream_sender();
        self.finish_turn(outcome, interrupted);
        Ok(())
    }

    /// Fold the turn's result into the transcript.
    fn finish_turn(&mut self, outcome: Option<Result<String>>, interrupted: bool) {
        self.status.activity = Activity::Idle;
        fold_turn_result(
            &mut self.transcript,
            &mut self.streaming,
            outcome,
            interrupted,
        );
        self.status.context_used = estimate_context_tokens(&self.transcript);
    }

    // ── Key handling ─────────────────────────────────────────────────

    fn on_key(&mut self, key: KeyEvent) -> Action {
        // Windows sends both press and release; only act on press.
        if key.kind == KeyEventKind::Release {
            return Action::Continue;
        }

        if self.focus == Focus::HistorySearch {
            return self.on_search_key(key);
        }

        // The completion popup takes precedence over the composer for the keys
        // it owns, so Tab/Enter/arrows do the expected thing while it is open.
        if self.completion.is_some() {
            match key.code {
                KeyCode::Tab | KeyCode::Enter => {
                    self.accept_completion();
                    return Action::Continue;
                }
                KeyCode::Esc => {
                    self.completion = None;
                    return Action::Continue;
                }
                KeyCode::Up => {
                    if let Some(c) = &mut self.completion {
                        c.previous();
                    }
                    return Action::Continue;
                }
                KeyCode::Down => {
                    if let Some(c) = &mut self.completion {
                        c.next();
                    }
                    return Action::Continue;
                }
                _ => {}
            }
        }

        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);

        match key.code {
            // ── Submit / newline ─────────────────────────────────────
            KeyCode::Enter if shift || alt => {
                self.composer.insert_char('\n');
            }
            KeyCode::Enter if ctrl => {
                self.composer.insert_char('\n');
            }
            KeyCode::Enter => {
                let text = self.composer.take();
                self.completion = None;
                self.history.reset_cursor();
                if text.trim().is_empty() {
                    return Action::Continue;
                }
                return Action::Submit(text);
            }

            // ── Quitting ─────────────────────────────────────────────
            KeyCode::Char('d') if ctrl && self.composer.is_empty() => return Action::Quit,
            KeyCode::Char('c') if ctrl => {
                if !self.composer.is_empty() {
                    self.composer.clear();
                    self.completion = None;
                    self.quit_armed = None;
                } else if self.quit_armed.is_some_and(|t| t.elapsed() < Duration::from_secs(2)) {
                    return Action::Quit;
                } else {
                    // First press only arms it, so a stray Ctrl+C does not
                    // discard the session.
                    self.quit_armed = Some(Instant::now());
                    self.transcript
                        .push_notice("Press Ctrl+C again to exit.", Level::Info);
                }
                return Action::Continue;
            }

            // ── Editing ──────────────────────────────────────────────
            KeyCode::Char(c) if ctrl || alt => {
                self.on_control_char(c, ctrl, alt);
                return Action::Continue;
            }
            KeyCode::Char(c) => {
                self.composer.insert_char(c);
                self.quit_armed = None;
            }
            KeyCode::Backspace if alt => self.composer.delete_word_backward(),
            KeyCode::Backspace => self.composer.delete_backward(),
            KeyCode::Delete => self.composer.delete_forward(),

            // ── Movement ─────────────────────────────────────────────
            KeyCode::Left if alt || ctrl => self.composer.move_word_left(),
            KeyCode::Left => self.composer.move_left(),
            KeyCode::Right if alt || ctrl => self.composer.move_word_right(),
            KeyCode::Right => self.composer.move_right(),
            KeyCode::Home => self.composer.move_to_line_start(),
            KeyCode::End => self.composer.move_to_line_end(),

            // ↑ moves within a multi-line draft; at the top it recalls history,
            // which is what a shell user expects.
            KeyCode::Up => {
                if !self.composer.move_up() {
                    if let Some(entry) = self.history.previous(&self.composer.text()) {
                        self.composer.set_text(&entry);
                    }
                }
            }
            KeyCode::Down => {
                if !self.composer.move_down() {
                    if let Some(entry) = self.history.next() {
                        self.composer.set_text(&entry);
                    }
                }
            }

            KeyCode::Tab => {
                self.refresh_completion();
                if self.completion.is_some() {
                    self.accept_completion();
                }
                return Action::Continue;
            }

            KeyCode::Esc => {
                self.completion = None;
                self.history.reset_cursor();
                return Action::Continue;
            }

            _ => return Action::Continue,
        }

        self.refresh_completion();
        Action::Continue
    }

    fn on_control_char(&mut self, c: char, ctrl: bool, alt: bool) {
        match c {
            'a' if ctrl => self.composer.move_to_line_start(),
            'e' if ctrl => self.composer.move_to_line_end(),
            'b' if ctrl => self.composer.move_left(),
            'f' if ctrl => self.composer.move_right(),
            'b' if alt => self.composer.move_word_left(),
            'f' if alt => self.composer.move_word_right(),
            'w' if ctrl => self.composer.delete_word_backward(),
            'u' if ctrl => self.composer.delete_to_line_start(),
            'k' if ctrl => self.composer.delete_to_line_end(),
            'h' if ctrl => self.composer.delete_backward(),
            'l' if ctrl => self.transcript.clear(),
            'o' if ctrl => {
                self.transcript.toggle_last_tool();
            }
            'r' if ctrl => {
                self.focus = Focus::HistorySearch;
                self.search_query.clear();
                self.search_selected = 0;
            }
            't' if ctrl => {
                self.transcript.show_thinking = !self.transcript.show_thinking;
            }
            _ => {}
        }
        self.refresh_completion();
    }

    fn on_search_key(&mut self, key: KeyEvent) -> Action {
        match key.code {
            KeyCode::Esc => {
                self.focus = Focus::Composer;
                self.search_query.clear();
            }
            KeyCode::Enter | KeyCode::Tab => {
                let hit = self
                    .history
                    .search(&self.search_query)
                    .get(self.search_selected)
                    .map(|s| s.to_string());
                if let Some(text) = hit {
                    self.composer.set_text(&text);
                }
                self.focus = Focus::Composer;
                self.search_query.clear();
            }
            KeyCode::Up => {
                self.search_selected = self.search_selected.saturating_sub(1);
            }
            KeyCode::Down => {
                let n = self.history.search(&self.search_query).len();
                if n > 0 {
                    self.search_selected = (self.search_selected + 1).min(n - 1);
                }
            }
            KeyCode::Backspace => {
                self.search_query.pop();
                self.search_selected = 0;
            }
            KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Repeated Ctrl+R walks down the result list, like a shell.
                let n = self.history.search(&self.search_query).len();
                if n > 0 {
                    self.search_selected = (self.search_selected + 1) % n;
                }
            }
            KeyCode::Char('c') | KeyCode::Char('g')
                if key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                self.focus = Focus::Composer;
                self.search_query.clear();
            }
            KeyCode::Char(c) => {
                self.search_query.push(c);
                self.search_selected = 0;
            }
            _ => {}
        }
        Action::Continue
    }

    // ── Completion ───────────────────────────────────────────────────

    fn refresh_completion(&mut self) {
        let line = self.composer.text();
        let (range, word) = self.composer.word_before_cursor();

        // Nothing to complete unless the word looks like a trigger.
        self.completion = completion::compute(
            &line,
            self.composer.cursor(),
            range,
            &word,
            &self.workspace,
            &self.session_names(),
            &self.models,
        );
    }

    fn accept_completion(&mut self) {
        let Some(completion) = self.completion.take() else {
            return;
        };
        let Some(candidate) = completion.selected().cloned() else {
            return;
        };
        self.composer
            .replace_range(completion.range.clone(), &candidate.replacement);

        // Completing a directory leaves the popup open so the next Tab descends.
        if completion.kind == Kind::File && candidate.replacement.ends_with('/') {
            self.refresh_completion();
        }
    }

    fn session_names(&self) -> Vec<String> {
        Vec::new()
    }

    // ── Slash commands ───────────────────────────────────────────────

    fn try_slash_command(&mut self, input: &str) -> Option<SlashOutcome> {
        let trimmed = input.trim();
        if !trimmed.starts_with('/') {
            return None;
        }
        let mut parts = trimmed.splitn(2, char::is_whitespace);
        let name = parts.next().unwrap_or_default();
        let arg = parts.next().unwrap_or("").trim();

        let Some(command) = commands::lookup(name) else {
            self.transcript.push_notice(
                format!("Unknown command {name}. Try /help."),
                Level::Error,
            );
            return Some(SlashOutcome::Handled);
        };

        match command.name {
            "/help" => self.show_help(),
            "/quit" => return Some(SlashOutcome::Quit),
            "/new" => {
                self.agent.clear_context();
                self.transcript.clear();
                self.flushed = 0;
                self.status.context_used = 0;
                self.transcript
                    .push_notice("Started a new conversation.", Level::Success);
            }
            "/mode" => self.set_mode(arg),
            "/model" => self.show_or_set_model(arg),
            "/thinking" => {
                self.transcript.show_thinking = !self.transcript.show_thinking;
                self.transcript.push_notice(
                    format!(
                        "Reasoning display {}.",
                        on_off(self.transcript.show_thinking)
                    ),
                    Level::Info,
                );
            }
            "/verbose" => {
                self.transcript.verbose = !self.transcript.verbose;
                self.transcript.push_notice(
                    format!("Full tool output {}.", on_off(self.transcript.verbose)),
                    Level::Info,
                );
            }
            "/cost" => {
                self.transcript
                    .push_notice(self.usage.summary(), Level::Info);
            }
            "/context" => self.show_context(),
            "/tools" => self.show_tools(),
            "/theme" => {
                self.transcript
                    .push_notice("Theme follows your terminal colours.", Level::Info);
            }
            _ => {
                self.transcript.push_notice(
                    format!("{} is not wired up yet.", command.name),
                    Level::Warning,
                );
            }
        }
        Some(SlashOutcome::Handled)
    }

    fn set_mode(&mut self, arg: &str) {
        if arg.is_empty() {
            self.transcript.push_notice(
                format!("Permission mode: {}", status::mode_label(self.status.mode)),
                Level::Info,
            );
            return;
        }
        match status::parse_mode(arg) {
            Some(mode) => {
                self.status.mode = mode;
                self.transcript.push_notice(
                    format!("Permission mode set to {}.", status::mode_label(mode)),
                    Level::Success,
                );
            }
            None => self.transcript.push_notice(
                format!(
                    "Unknown mode '{arg}'. Choose one of: {}.",
                    commands::MODES.join(", ")
                ),
                Level::Error,
            ),
        }
    }

    fn show_or_set_model(&mut self, arg: &str) {
        if arg.is_empty() {
            self.transcript.push_notice(
                format!("Model: {}", self.status.model),
                Level::Info,
            );
            return;
        }
        self.status.model = arg.to_string();
        self.usage.set_model(arg);
        self.transcript.push_notice(
            format!("Model set to {arg} for cost accounting. Restart to switch the provider."),
            Level::Warning,
        );
    }

    fn show_context(&mut self) {
        let used = self.status.context_used;
        let window = status::context_window(&self.status.model);
        self.transcript.push_notice(
            format!(
                "{} of ~{} context tokens ({}%), {} messages.",
                status::compact_count(used),
                status::compact_count(window),
                (self.status.context_fraction() * 100.0).round() as u32,
                self.agent.context_len(),
            ),
            Level::Info,
        );
    }

    fn show_tools(&mut self) {
        let names = self.agent.tool_names();
        let body = if names.is_empty() {
            "No tools registered.".to_string()
        } else {
            format!("{} tools:\n{}", names.len(), bullet_list(&names))
        };
        self.transcript.push_notice(body, Level::Info);
    }

    fn show_help(&mut self) {
        let mut body = String::from("Commands\n");
        for command in commands::COMMANDS {
            let arg = match command.args {
                Args::None => String::new(),
                Args::Free(name) => format!(" <{name}>"),
                Args::Choice(options) => format!(" <{}>", options.join("|")),
                Args::Session => " <session>".to_string(),
                Args::Model => " <model>".to_string(),
            };
            body.push_str(&format!(
                "- `{}{}` — {}\n",
                command.name, arg, command.summary
            ));
        }
        body.push_str(
            "\nKeys\n\
             - `Enter` send · `Shift+Enter` newline\n\
             - `Tab` complete · `@` mention a file · `/` command\n\
             - `↑`/`↓` history · `Ctrl+R` search history\n\
             - `Ctrl+O` fold or unfold the last tool output\n\
             - `Ctrl+T` toggle reasoning · `Ctrl+L` clear the screen\n\
             - `Esc` interrupt the agent · `Ctrl+C` twice to exit\n",
        );
        self.transcript.push_notice(body, Level::Info);
    }

    fn greet(&mut self) {
        self.transcript.push_notice(
            format!(
                "{} ready. `/help` for commands, `@` to mention a file.",
                self.agent_name
            ),
            Level::Info,
        );
    }

    // ── Rendering ────────────────────────────────────────────────────

    /// Push finished transcript entries into the terminal's scrollback.
    fn flush_completed(&mut self, terminal: &mut Tui) -> Result<()> {
        let Self {
            transcript,
            flushed,
            theme,
            ..
        } = self;
        flush_entries(terminal, transcript, flushed, theme)
    }

    fn draw(&mut self, terminal: &mut Tui) -> Result<()> {
        draw_frame(
            terminal,
            &self.transcript,
            &self.streaming,
            &self.status,
            &self.theme,
            self.frame,
            &self.composer,
            &self.completion,
            &self.focus,
            &self.search_query,
            false,
        )
    }
}

/// Why a slash command ended the way it did.
enum SlashOutcome {
    /// Fully handled in the TUI; do not call the agent.
    Handled,
    /// Leave the session.
    Quit,
    /// Not a command after all — send it as a message.
    #[allow(dead_code)]
    SendToAgent,
}

/// Marker error used to unwind out of a turn on `/quit`.
#[derive(Debug)]
struct Quit;

impl std::fmt::Display for Quit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "quit")
    }
}

impl std::error::Error for Quit {}

// ── Free functions (usable while `self` is partially borrowed) ───────

type Tui = Terminal<ratatui::backend::CrosstermBackend<io::Stdout>>;

fn setup() -> Result<Tui> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnableBracketedPaste)?;

    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    // Inline, not full-screen: the transcript belongs in the terminal's own
    // scrollback so it can be scrolled and copied like any other output.
    let terminal = Terminal::with_options(
        backend,
        TerminalOptions {
            viewport: Viewport::Inline(COMPOSER_MAX_ROWS + 6),
        },
    )?;
    Ok(terminal)
}

fn restore(terminal: &mut Tui) -> Result<()> {
    disable_raw_mode()?;
    execute!(io::stdout(), DisableBracketedPaste)?;
    // Leave the cursor below the viewport so the shell prompt does not land on
    // top of the last frame.
    terminal.clear()?;
    println!();
    Ok(())
}

/// Move any transcript entries not yet in scrollback out of the viewport.
fn flush_entries(
    terminal: &mut Tui,
    transcript: &Transcript,
    flushed: &mut usize,
    theme: &Theme,
) -> Result<()> {
    let width = terminal.size()?.width as usize;
    // The last entry stays in the viewport while a turn is live, since a tool
    // card can still change (status, output, fold state) after it appears.
    let flushable = transcript.entries().len().saturating_sub(1);

    while *flushed < flushable {
        let entry = &transcript.entries()[*flushed];
        let lines = transcript.render_entry(entry, width, theme);
        *flushed += 1;

        if lines.is_empty() {
            continue;
        }
        // A blank line after each entry keeps the transcript breathing.
        let height = lines.len() as u16 + 1;
        terminal.insert_before(height, |buf| {
            for (i, line) in lines.into_iter().enumerate() {
                let area = Rect::new(0, i as u16, buf.area.width, 1);
                Paragraph::new(line).render(area, buf);
            }
        })?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn draw_frame(
    terminal: &mut Tui,
    transcript: &Transcript,
    streaming: &str,
    status: &Status,
    theme: &Theme,
    frame: usize,
    composer: &Composer,
    completion: &Option<Completion>,
    focus: &Focus,
    search_query: &str,
    busy: bool,
) -> Result<()> {
    terminal.draw(|f| {
        render_viewport(
            f, transcript, streaming, status, theme, frame, composer, completion, focus,
            search_query, busy,
        );
    })?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn render_viewport(
    f: &mut Frame,
    transcript: &Transcript,
    streaming: &str,
    status: &Status,
    theme: &Theme,
    frame: usize,
    composer: &Composer,
    completion: &Option<Completion>,
    focus: &Focus,
    search_query: &str,
    busy: bool,
) {
    let area = f.area();
    if area.height == 0 || area.width == 0 {
        return;
    }

    let spinner = glyph::SPINNER[frame % glyph::SPINNER.len()];

    // The live region: the tail of the transcript still in the viewport, plus
    // whatever is streaming right now.
    let mut live: Vec<Line<'static>> = Vec::new();
    if let Some(last) = transcript.entries().last() {
        live.extend(transcript.render_entry(last, area.width as usize, theme));
    }
    if !streaming.is_empty() {
        live.extend(transcript.render_entry(
            &Entry::Assistant(streaming.to_string()),
            area.width as usize,
            theme,
        ));
    }

    let composer_rows = if busy {
        0
    } else {
        (composer.lines().len() as u16)
            .clamp(COMPOSER_MIN_ROWS, COMPOSER_MAX_ROWS)
            + 2 // borders
    };

    let popup_rows = match (busy, completion, focus) {
        (false, Some(c), Focus::Composer) => (c.candidates.len() as u16).min(6),
        _ => 0,
    };

    let live_rows = area
        .height
        .saturating_sub(composer_rows + popup_rows + 1)
        .min(live.len() as u16);

    let chunks = Layout::vertical([
        Constraint::Length(live_rows),
        Constraint::Length(popup_rows),
        Constraint::Length(composer_rows),
        Constraint::Length(1),
    ])
    .split(area);

    // Live transcript tail — show the end, which is where new text appears.
    if live_rows > 0 {
        let start = live.len().saturating_sub(live_rows as usize);
        let visible: Vec<Line<'static>> = live[start..].to_vec();
        f.render_widget(Paragraph::new(visible).wrap(Wrap { trim: false }), chunks[0]);
    }

    if popup_rows > 0 {
        if let Some(c) = completion {
            render_completion(f, chunks[1], c, theme);
        }
    }

    if composer_rows > 0 {
        if *focus == Focus::HistorySearch {
            render_search(f, chunks[2], search_query, theme);
        } else {
            render_composer(f, chunks[2], composer, theme);
        }
    }

    f.render_widget(
        Paragraph::new(status.render(chunks[3].width as usize, theme, spinner)),
        chunks[3],
    );
}

fn render_composer(f: &mut Frame, area: Rect, composer: &Composer, theme: &Theme) {
    if area.height < 3 {
        return;
    }
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let lines: Vec<Line<'static>> = composer
        .lines()
        .into_iter()
        .enumerate()
        .map(|(i, text)| {
            let lead = if i == 0 {
                Span::styled(
                    format!("{} ", glyph::USER),
                    Style::default().fg(theme.accent),
                )
            } else {
                Span::raw("  ".to_string())
            };
            Line::from(vec![lead, Span::styled(text, theme.body())])
        })
        .collect();

    let placeholder = composer.is_empty();
    if placeholder {
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!("{} ", glyph::USER), Style::default().fg(theme.accent)),
                Span::styled(
                    "Ask anything, or / for commands".to_string(),
                    theme.dim(),
                ),
            ])),
            inner,
        );
    } else {
        f.render_widget(Paragraph::new(lines), inner);
    }

    // Place the real terminal cursor, so the user sees where they are typing
    // and so screen readers and IMEs behave.
    let (line, column) = composer.cursor_position();
    let x = inner.x + 2 + column as u16;
    let y = inner.y + line as u16;
    if x < inner.right() && y < inner.bottom() {
        f.set_cursor_position((x, y));
    }
}

fn render_search(f: &mut Frame, area: Rect, query: &str, theme: &Theme) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent));
    let inner = block.inner(area);
    f.render_widget(block, area);

    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "search ".to_string(),
                Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
            ),
            Span::styled(query.to_string(), theme.body()),
            Span::styled("▏".to_string(), Style::default().fg(theme.accent)),
        ])),
        inner,
    );
}

fn render_completion(f: &mut Frame, area: Rect, completion: &Completion, theme: &Theme) {
    f.render_widget(Clear, area);

    let rows: Vec<Line<'static>> = completion
        .candidates
        .iter()
        .take(area.height as usize)
        .enumerate()
        .map(|(i, candidate)| {
            let selected = i == completion.selected;
            let label_style = if selected {
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                theme.body()
            };
            let marker = if selected { "▸ " } else { "  " };
            let mut spans = vec![
                Span::styled(marker.to_string(), Style::default().fg(theme.accent)),
                Span::styled(candidate.label.clone(), label_style),
            ];
            if !candidate.detail.is_empty() {
                spans.push(Span::raw("  ".to_string()));
                spans.push(Span::styled(candidate.detail.clone(), theme.dim()));
            }
            Line::from(spans)
        })
        .collect();

    f.render_widget(Paragraph::new(rows), area);
}

// ── Stream handling ──────────────────────────────────────────────────

/// Fold one streamed event into the transcript and status.
fn apply_stream_event(
    event: StreamEvent,
    transcript: &mut Transcript,
    status: &mut Status,
    usage: &mut UsageTracker,
    streaming: &mut String,
) {
    match event {
        StreamEvent::TextDelta { text } => {
            streaming.push_str(&text);
        }
        StreamEvent::Thinking { text } => {
            transcript.push(Entry::Thinking(text));
        }
        StreamEvent::ToolUseStart { id, name } => {
            // Any assistant text before the tool call is its own entry, so the
            // tool card appears after the prose that introduced it.
            if !streaming.trim().is_empty() {
                transcript.push_assistant(std::mem::take(streaming));
            }
            streaming.clear();

            let mut tool = ToolCall::new(id, name.clone());
            tool.status = ToolStatus::Running;
            transcript.push(Entry::Tool(tool));
            status.activity = Activity::Running(name);
        }
        StreamEvent::ToolInputDelta { id, delta } => {
            if let Some(tool) = transcript.tool_mut(&id) {
                tool.input.push_str(&delta);
            }
        }
        StreamEvent::ToolResult {
            id,
            success,
            output,
            ..
        } => {
            if let Some(tool) = transcript.tool_mut(&id) {
                tool.output = output;
                tool.status = if success { ToolStatus::Ok } else { ToolStatus::Failed };
                // Failures are worth seeing without a keystroke.
                if !success {
                    tool.expanded = true;
                }
            }
            status.activity = Activity::Thinking;
        }
        StreamEvent::Usage {
            input_tokens,
            output_tokens,
        } => {
            usage.record(
                TokenUsage {
                    input_tokens: input_tokens as u64,
                    output_tokens: output_tokens as u64,
                    ..Default::default()
                },
                None,
            );
            let cumulative = usage.cumulative();
            status.input_tokens = cumulative.input_tokens;
            status.output_tokens = cumulative.output_tokens;
            status.cost_usd = usage.total_cost();
            // The input token count of the latest request is exactly how full
            // the context is — better than any estimate.
            status.context_used = input_tokens as u64;
        }
        StreamEvent::MessageStop => {}
        StreamEvent::Error { message } => {
            transcript.push_notice(message, Level::Error);
        }
    }
}

/// Fold a finished turn into the transcript.
///
/// Free-standing so the outcome matrix — success, error, interrupt, and an
/// agent that returned nothing — can be tested without constructing an Agent.
fn fold_turn_result(
    transcript: &mut Transcript,
    streaming: &mut String,
    outcome: Option<Result<String>>,
    interrupted: bool,
) {
    // A tool still mid-flight when the turn ended must not be left spinning.
    for entry in transcript.entries_mut() {
        if let Entry::Tool(tool) = entry {
            if !tool.status.is_terminal() {
                tool.status = if interrupted {
                    ToolStatus::Denied
                } else {
                    ToolStatus::Failed
                };
            }
        }
    }

    // Whatever streamed before the turn ended is real output the user watched
    // arrive, so it is kept in every branch rather than discarded.
    let partial = std::mem::take(streaming);
    let has_partial = !partial.trim().is_empty();

    if interrupted {
        if has_partial {
            transcript.push_assistant(partial);
        }
        transcript.push_notice("Interrupted.", Level::Warning);
        return;
    }

    match outcome {
        Some(Ok(text)) => {
            // Prefer the streamed text: it is what was on screen.
            let body = if has_partial { partial } else { text };
            if !body.trim().is_empty() {
                transcript.push_assistant(body);
            }
        }
        Some(Err(e)) => {
            if has_partial {
                transcript.push_assistant(partial);
            }
            transcript.push_notice(format!("{e:#}"), Level::Error);
        }
        None => {
            transcript.push_notice("The agent stopped without answering.", Level::Error);
        }
    }
}

/// Whether this key means "stop what you are doing".
fn is_interrupt(key: &KeyEvent) -> bool {
    if key.kind == KeyEventKind::Release {
        return false;
    }
    matches!(key.code, KeyCode::Esc)
        || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL))
}

fn is_toggle_fold(key: &KeyEvent) -> bool {
    key.kind != KeyEventKind::Release
        && key.code == KeyCode::Char('o')
        && key.modifiers.contains(KeyModifiers::CONTROL)
}

/// Rough token estimate for the transcript, used until a real count arrives.
fn estimate_context_tokens(transcript: &Transcript) -> u64 {
    let chars: usize = transcript
        .entries()
        .iter()
        .map(|e| match e {
            Entry::User(t) | Entry::Assistant(t) | Entry::Thinking(t) => t.len(),
            Entry::Tool(tool) => tool.input.len() + tool.output.len(),
            Entry::Notice { .. } => 0,
        })
        .sum();
    (chars / 4) as u64
}

fn on_off(value: bool) -> &'static str {
    if value {
        "on"
    } else {
        "off"
    }
}

fn bullet_list(items: &[String]) -> String {
    items
        .iter()
        .map(|i| format!("- {i}\n"))
        .collect::<String>()
}

/// Models offered by `/model` completion, for the active provider.
///
/// The provider's catalogue default always leads, so an Ollama or Groq user is
/// not offered GPT model names — which is what the old `_ =>` arm did.
fn known_models(provider: &str) -> Vec<String> {
    let Some(spec) = crate::llm::providers::find(provider) else {
        return Vec::new();
    };

    let mut models = vec![spec.default_model.to_string()];
    // A short hand-maintained list for the providers whose model names people
    // type often. Anything not listed still works — this only drives
    // completion, never what can be set.
    let extra: &[&str] = match spec.id {
        "anthropic" => &["claude-opus-5", "claude-sonnet-5", "claude-haiku-4-5-20251001"],
        "openai" => &["gpt-5", "gpt-4o", "gpt-4o-mini"],
        "ollama" => &["llama3.1", "qwen2.5-coder", "mistral", "phi4"],
        "groq" => &["llama-3.3-70b-versatile", "llama-3.1-8b-instant"],
        "deepseek" => &["deepseek-chat", "deepseek-reasoner"],
        "mistral" => &["mistral-large-latest", "mistral-small-latest"],
        "openrouter" => &["anthropic/claude-sonnet-4.5", "openai/gpt-4o", "google/gemini-2.0-flash"],
        _ => &[],
    };
    for model in extra {
        if !models.iter().any(|m| m == model) {
            models.push(model.to_string());
        }
    }
    models
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    // ── Interrupt detection ──────────────────────────────────────────

    #[test]
    fn esc_and_ctrl_c_both_interrupt() {
        assert!(is_interrupt(&key(KeyCode::Esc)));
        assert!(is_interrupt(&ctrl('c')));
    }

    #[test]
    fn an_ordinary_key_does_not_interrupt() {
        assert!(!is_interrupt(&key(KeyCode::Char('a'))));
        assert!(!is_interrupt(&key(KeyCode::Enter)));
    }

    #[test]
    fn a_key_release_never_interrupts() {
        // Windows delivers press and release; acting on both would interrupt
        // twice from one physical keypress.
        let mut released = key(KeyCode::Esc);
        released.kind = KeyEventKind::Release;
        assert!(!is_interrupt(&released));
    }

    #[test]
    fn ctrl_o_toggles_folding_but_other_keys_do_not() {
        assert!(is_toggle_fold(&ctrl('o')));
        assert!(!is_toggle_fold(&key(KeyCode::Char('o'))));
        assert!(!is_toggle_fold(&ctrl('p')));
    }

    // ── Stream folding ───────────────────────────────────────────────

    struct Harness {
        transcript: Transcript,
        status: Status,
        usage: UsageTracker,
        streaming: String,
    }

    impl Harness {
        fn new() -> Self {
            Self {
                transcript: Transcript::new(),
                status: Status::default(),
                usage: UsageTracker::new("gpt-4o"),
                streaming: String::new(),
            }
        }

        fn feed(&mut self, event: StreamEvent) {
            apply_stream_event(
                event,
                &mut self.transcript,
                &mut self.status,
                &mut self.usage,
                &mut self.streaming,
            );
        }
    }

    #[test]
    fn text_deltas_accumulate_into_one_message() {
        let mut h = Harness::new();
        h.feed(StreamEvent::TextDelta { text: "Hel".into() });
        h.feed(StreamEvent::TextDelta { text: "lo".into() });
        assert_eq!(h.streaming, "Hello");
        assert!(h.transcript.is_empty(), "nothing is committed until the turn ends");
    }

    #[test]
    fn a_tool_call_commits_the_prose_that_preceded_it() {
        // Otherwise the explanation and the tool card render out of order.
        let mut h = Harness::new();
        h.feed(StreamEvent::TextDelta {
            text: "Let me look.".into(),
        });
        h.feed(StreamEvent::ToolUseStart {
            id: "t1".into(),
            name: "read_file".into(),
        });

        assert!(h.streaming.is_empty());
        assert_eq!(h.transcript.entries().len(), 2);
        assert!(matches!(h.transcript.entries()[0], Entry::Assistant(_)));
        assert!(matches!(h.transcript.entries()[1], Entry::Tool(_)));
    }

    #[test]
    fn tool_input_deltas_land_on_the_right_call() {
        let mut h = Harness::new();
        h.feed(StreamEvent::ToolUseStart { id: "a".into(), name: "bash".into() });
        h.feed(StreamEvent::ToolUseStart { id: "b".into(), name: "grep".into() });
        h.feed(StreamEvent::ToolInputDelta { id: "a".into(), delta: "{\"x\"".into() });

        let inputs: Vec<String> = h
            .transcript
            .entries()
            .iter()
            .filter_map(|e| match e {
                Entry::Tool(t) => Some(t.input.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(inputs, vec!["{\"x\"".to_string(), String::new()]);
    }

    #[test]
    fn a_tool_result_sets_the_status_and_output() {
        let mut h = Harness::new();
        h.feed(StreamEvent::ToolUseStart { id: "t".into(), name: "bash".into() });
        h.feed(StreamEvent::ToolResult {
            id: "t".into(),
            name: "bash".into(),
            success: true,
            output: "done".into(),
        });

        let Entry::Tool(tool) = &h.transcript.entries()[0] else {
            panic!("expected a tool entry");
        };
        assert_eq!(tool.status, ToolStatus::Ok);
        assert_eq!(tool.output, "done");
        assert!(!tool.expanded, "a success stays folded");
    }

    #[test]
    fn a_failed_tool_expands_itself() {
        // You almost always want to see why it failed.
        let mut h = Harness::new();
        h.feed(StreamEvent::ToolUseStart { id: "t".into(), name: "bash".into() });
        h.feed(StreamEvent::ToolResult {
            id: "t".into(),
            name: "bash".into(),
            success: false,
            output: "command not found".into(),
        });

        let Entry::Tool(tool) = &h.transcript.entries()[0] else {
            panic!("expected a tool entry");
        };
        assert_eq!(tool.status, ToolStatus::Failed);
        assert!(tool.expanded);
    }

    #[test]
    fn a_result_for_an_unknown_id_is_ignored_rather_than_panicking() {
        let mut h = Harness::new();
        h.feed(StreamEvent::ToolResult {
            id: "never-started".into(),
            name: "bash".into(),
            success: true,
            output: "x".into(),
        });
        assert!(h.transcript.is_empty());
    }

    #[test]
    fn usage_events_update_the_status_line() {
        let mut h = Harness::new();
        h.feed(StreamEvent::Usage {
            input_tokens: 1_000,
            output_tokens: 250,
        });
        assert_eq!(h.status.input_tokens, 1_000);
        assert_eq!(h.status.output_tokens, 250);
        assert_eq!(
            h.status.context_used, 1_000,
            "the request's input count is the real context size"
        );
    }

    #[test]
    fn activity_follows_the_tool_lifecycle() {
        let mut h = Harness::new();
        assert_eq!(h.status.activity, Activity::Idle);

        h.feed(StreamEvent::ToolUseStart { id: "t".into(), name: "bash".into() });
        assert_eq!(h.status.activity, Activity::Running("bash".into()));

        h.feed(StreamEvent::ToolResult {
            id: "t".into(),
            name: "bash".into(),
            success: true,
            output: String::new(),
        });
        assert_eq!(h.status.activity, Activity::Thinking);
    }

    #[test]
    fn a_stream_error_becomes_a_visible_notice() {
        let mut h = Harness::new();
        h.feed(StreamEvent::Error {
            message: "rate limited".into(),
        });
        let Entry::Notice { text, level } = &h.transcript.entries()[0] else {
            panic!("expected a notice");
        };
        assert_eq!(text, "rate limited");
        assert_eq!(*level, Level::Error);
    }

    #[test]
    fn thinking_events_become_their_own_entries() {
        let mut h = Harness::new();
        h.feed(StreamEvent::Thinking { text: "hmm".into() });
        assert!(matches!(h.transcript.entries()[0], Entry::Thinking(_)));
    }

    // ── Helpers ──────────────────────────────────────────────────────

    #[test]
    fn context_estimation_counts_tool_traffic_too() {
        let mut t = Transcript::new();
        t.push_user(&"x".repeat(400));
        assert_eq!(estimate_context_tokens(&t), 100);

        let mut tool = ToolCall::new("t", "bash");
        tool.output = "y".repeat(400);
        t.push(Entry::Tool(tool));
        assert_eq!(estimate_context_tokens(&t), 200);
    }

    #[test]
    fn notices_do_not_count_against_the_context() {
        // They are local UI, never sent to the model.
        let mut t = Transcript::new();
        t.push_notice("x".repeat(4000), Level::Info);
        assert_eq!(estimate_context_tokens(&t), 0);
    }

    #[test]
    fn known_models_differ_by_provider() {
        assert!(known_models("anthropic").iter().any(|m| m.contains("claude")));
        assert!(known_models("openai").iter().any(|m| m.contains("gpt")));
    }

    #[test]
    fn a_local_provider_is_not_offered_hosted_model_names() {
        // The old `_ =>` arm handed an Ollama user a list of GPT models.
        let models = known_models("ollama");
        assert!(models.iter().any(|m| m.contains("llama")), "{models:?}");
        assert!(!models.iter().any(|m| m.starts_with("gpt-")), "{models:?}");
    }

    #[test]
    fn the_catalogue_default_leads_the_list() {
        for provider in ["anthropic", "openai", "ollama", "groq"] {
            let spec = crate::llm::providers::find(provider).unwrap();
            assert_eq!(known_models(provider)[0], spec.default_model);
        }
    }

    #[test]
    fn an_unknown_provider_offers_nothing_rather_than_guessing() {
        assert!(known_models("not-a-provider").is_empty());
    }

    #[test]
    fn the_model_list_has_no_duplicates() {
        for provider in crate::llm::providers::ids() {
            let models = known_models(provider);
            let mut unique = models.clone();
            unique.sort();
            unique.dedup();
            assert_eq!(unique.len(), models.len(), "{provider}: {models:?}");
        }
    }

    // ── Turn completion ──────────────────────────────────────────────

    fn fold(streamed: &str, outcome: Option<Result<String>>, interrupted: bool) -> Transcript {
        let mut t = Transcript::new();
        let mut streaming = streamed.to_string();
        fold_turn_result(&mut t, &mut streaming, outcome, interrupted);
        assert!(streaming.is_empty(), "the streaming buffer must be drained");
        t
    }

    fn kinds(t: &Transcript) -> Vec<&'static str> {
        t.entries()
            .iter()
            .map(|e| match e {
                Entry::User(_) => "user",
                Entry::Assistant(_) => "assistant",
                Entry::Thinking(_) => "thinking",
                Entry::Tool(_) => "tool",
                Entry::Notice { .. } => "notice",
            })
            .collect()
    }

    #[test]
    fn a_successful_turn_commits_the_streamed_text() {
        let t = fold("streamed answer", Some(Ok("returned answer".into())), false);
        assert_eq!(kinds(&t), vec!["assistant"]);
        let Entry::Assistant(text) = &t.entries()[0] else { unreachable!() };
        assert_eq!(
            text, "streamed answer",
            "what the user watched arrive wins over the returned string"
        );
    }

    #[test]
    fn a_non_streaming_turn_falls_back_to_the_returned_text() {
        let t = fold("", Some(Ok("returned answer".into())), false);
        let Entry::Assistant(text) = &t.entries()[0] else { unreachable!() };
        assert_eq!(text, "returned answer");
    }

    #[test]
    fn an_empty_answer_adds_nothing() {
        let t = fold("   ", Some(Ok("  ".into())), false);
        assert!(t.is_empty(), "a blank reply should not leave an empty bubble");
    }

    #[test]
    fn an_interrupt_keeps_the_partial_reply_and_says_so() {
        // Discarding the partial would throw away output the user already read.
        let t = fold("half an ans", None, true);
        assert_eq!(kinds(&t), vec!["assistant", "notice"]);
        let Entry::Notice { text, level } = &t.entries()[1] else { unreachable!() };
        assert_eq!(text, "Interrupted.");
        assert_eq!(*level, Level::Warning);
    }

    #[test]
    fn an_interrupt_with_nothing_streamed_only_notes_it() {
        let t = fold("", None, true);
        assert_eq!(kinds(&t), vec!["notice"]);
    }

    #[test]
    fn an_error_keeps_the_partial_reply_and_reports_the_error() {
        let t = fold("some text", Some(Err(anyhow::anyhow!("rate limited"))), false);
        assert_eq!(kinds(&t), vec!["assistant", "notice"]);
        let Entry::Notice { text, level } = &t.entries()[1] else { unreachable!() };
        assert!(text.contains("rate limited"));
        assert_eq!(*level, Level::Error);
    }

    #[test]
    fn an_agent_that_returned_nothing_is_reported_rather_than_ignored() {
        // Silence here would look like a successful empty answer.
        let t = fold("", None, false);
        assert_eq!(kinds(&t), vec!["notice"]);
        let Entry::Notice { level, .. } = &t.entries()[0] else { unreachable!() };
        assert_eq!(*level, Level::Error);
    }

    #[test]
    fn an_interrupt_marks_in_flight_tools_as_denied() {
        let mut t = Transcript::new();
        let mut running = ToolCall::new("t", "bash");
        running.status = ToolStatus::Running;
        t.push(Entry::Tool(running));

        let mut streaming = String::new();
        fold_turn_result(&mut t, &mut streaming, None, true);

        let Entry::Tool(tool) = &t.entries()[0] else { unreachable!() };
        assert_eq!(
            tool.status,
            ToolStatus::Denied,
            "a cancelled tool must not stay spinning forever"
        );
    }

    #[test]
    fn an_error_marks_in_flight_tools_as_failed() {
        let mut t = Transcript::new();
        let mut running = ToolCall::new("t", "bash");
        running.status = ToolStatus::Running;
        t.push(Entry::Tool(running));

        let mut streaming = String::new();
        fold_turn_result(&mut t, &mut streaming, Some(Err(anyhow::anyhow!("boom"))), false);

        let Entry::Tool(tool) = &t.entries()[0] else { unreachable!() };
        assert_eq!(tool.status, ToolStatus::Failed);
    }

    #[test]
    fn tools_that_already_finished_are_left_alone() {
        let mut t = Transcript::new();
        let mut done = ToolCall::new("t", "bash");
        done.status = ToolStatus::Ok;
        t.push(Entry::Tool(done));

        let mut streaming = String::new();
        fold_turn_result(&mut t, &mut streaming, None, true);

        let Entry::Tool(tool) = &t.entries()[0] else { unreachable!() };
        assert_eq!(tool.status, ToolStatus::Ok, "a completed tool keeps its result");
    }
}
