use anyhow::Result;
use colored::Colorize;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::agent::Agent;
use crate::config::AppConfig;
use crate::permissions::{PermissionMode, PermissionPolicy};
use crate::sessions::SessionStore;
use crate::streaming::{stream_channel, StreamEvent};
use crate::usage::UsageTracker;

// ── Slash-command autocomplete helper ──────────────────────────────

const SLASH_COMMANDS: &[&str] = &[
    "/help", "/quit", "/exit", "/clear", "/new", "/reset", "/save", "/load",
    "/sessions", "/history", "/search", "/tools", "/skills", "/verbose",
    "/quiet", "/stream", "/streaming", "/thinking", "/yolo", "/mode",
    "/context", "/status", "/compress", "/cost", "/usage", "/model", "/export",
];

#[derive(Default)]
struct SlashCompleter;

impl rustyline::completion::Completer for SlashCompleter {
    type Candidate = String;
    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &rustyline::Context<'_>,
    ) -> rustyline::Result<(usize, Vec<String>)> {
        // Only complete if the word under the cursor starts with '/'
        let prefix = &line[..pos];
        let start = prefix
            .rfind(|c: char| c.is_whitespace())
            .map(|i| i + 1)
            .unwrap_or(0);
        let word = &prefix[start..];
        if !word.starts_with('/') {
            return Ok((pos, Vec::new()));
        }
        let matches: Vec<String> = SLASH_COMMANDS
            .iter()
            .filter(|c| c.starts_with(word))
            .map(|c| c.to_string())
            .collect();
        Ok((start, matches))
    }
}

impl rustyline::hint::Hinter for SlashCompleter {
    type Hint = String;
}
impl rustyline::highlight::Highlighter for SlashCompleter {}
impl rustyline::validate::Validator for SlashCompleter {}
impl rustyline::Helper for SlashCompleter {}

// ── Spinner ────────────────────────────────────────────────────────

/// A lightweight braille spinner that runs on a background thread and
/// stops when its `stop` flag is set.
struct Spinner {
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Spinner {
    fn start(label: &str) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_clone = stop.clone();
        let label = label.to_string();
        let handle = std::thread::spawn(move || {
            let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
            let mut i = 0usize;
            while !stop_clone.load(Ordering::Relaxed) {
                let frame = frames[i % frames.len()];
                // \r to overwrite, \x1b[K to clear to end of line
                print!("\r\x1b[K{} {}", frame.bright_cyan(), label.dimmed());
                let _ = std::io::stdout().flush();
                i += 1;
                std::thread::sleep(std::time::Duration::from_millis(80));
            }
            // Clear the spinner line on exit
            print!("\r\x1b[K");
            let _ = std::io::stdout().flush();
        });
        Self {
            stop,
            handle: Some(handle),
        }
    }
}

impl Drop for Spinner {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

/// Terminal REPL interface for the Pylot agent.
/// Full-featured with streaming display, session persistence,
/// usage tracking, permission modes, and interactive approval.
pub struct Terminal {
    agent: Agent,
    /// Track session message count for display.
    message_count: usize,
    /// Whether streaming display is enabled.
    streaming_display: bool,
    /// Whether approval mode is active.
    approval_mode: bool,
    /// Whether to display thinking blocks.
    show_thinking: bool,
    /// Session persistence store.
    session_store: Option<SessionStore>,
    /// Current session ID.
    current_session_id: Option<String>,
    /// Usage/cost tracker for this session.
    usage_tracker: UsageTracker,
    /// Permission policy.
    permission_policy: PermissionPolicy,
}

impl Terminal {
    pub fn new(agent: Agent, config: &AppConfig) -> Self {
        // Open session store
        let session_store = SessionStore::open(&config.data_dir)
            .map_err(|e| tracing::warn!("Failed to open session store: {}", e))
            .ok();

        // Create a new session ID
        let session_id = uuid::Uuid::new_v4().to_string();
        if let Some(ref store) = session_store {
            let _ = store.create_session(&session_id, "cli");
        }

        let usage_tracker = UsageTracker::new(&config.llm_model);
        let permission_policy = PermissionPolicy::new(PermissionMode::WorkspaceWrite);

        Self {
            agent,
            message_count: 0,
            streaming_display: false,
            approval_mode: true,
            show_thinking: true,
            session_store,
            current_session_id: Some(session_id),
            usage_tracker,
            permission_policy,
        }
    }

    /// Print the welcome banner.
    fn print_banner(&self, config: &AppConfig) {
        let border = "═".repeat(56);
        println!("\n  ╔{}╗", border);
        println!(
            "  ║{:^56}║",
            format!("🤖 {} v0.3.0", config.agent_name)
        );
        println!(
            "  ║{:^56}║",
            "Personal AI Agent — Powered by Rust"
        );
        println!("  ╚{}╝\n", border);

        println!(
            "  {} {} ({})",
            "LLM:".bright_blue().bold(),
            config.llm_model,
            config.llm_provider
        );

        let tool_names = self.agent.tool_names();
        println!(
            "  {} {} tool(s) loaded",
            "Tools:".bright_blue().bold(),
            tool_names.len()
        );

        if let Some(ref sid) = self.current_session_id {
            println!(
                "  {} {}",
                "Session:".bright_blue().bold(),
                &sid[..8]
            );
        }

        println!(
            "  {} {}",
            "Mode:".bright_blue().bold(),
            match self.permission_policy.mode() {
                PermissionMode::ReadOnly => "read-only".bright_yellow(),
                PermissionMode::WorkspaceWrite => "workspace-write".bright_green(),
                PermissionMode::FullAccess => "full-access".bright_red(),
            }
        );

        println!();
        println!("  Type {} for commands, or just start chatting.\n", "/help".bright_green());
    }

    /// Run the interactive REPL loop.
    pub async fn run(&mut self, config: &AppConfig) -> Result<()> {
        self.print_banner(config);

        // Set up approval callback for dangerous commands
        if self.approval_mode {
            self.agent.set_approval_enabled(true);
            self.agent.set_approval_fn(|tool_name, _args| {
                print!(
                    "\n  {} Approve '{}' ? [y/N]: ",
                    "⚠️".bright_yellow(),
                    tool_name.bright_red()
                );
                std::io::stdout().flush().unwrap_or_default();
                let mut input = String::new();
                std::io::stdin().read_line(&mut input).unwrap_or_default();
                input.trim().to_lowercase() == "y" || input.trim().to_lowercase() == "yes"
            });
        }

        let mut editor: rustyline::Editor<SlashCompleter, rustyline::history::DefaultHistory> =
            rustyline::Editor::new()?;
        editor.set_helper(Some(SlashCompleter));

        // Load history
        let history_path = config.data_dir.join("history.txt");
        let _ = editor.load_history(&history_path);

        loop {
            let prompt = format!("{} ", "You>".bright_green().bold());
            let line = match editor.readline(&prompt) {
                Ok(line) => line,
                Err(rustyline::error::ReadlineError::Interrupted) => {
                    println!("\n{}", "Use /quit to exit.".bright_yellow());
                    continue;
                }
                Err(rustyline::error::ReadlineError::Eof) => break,
                Err(e) => {
                    eprintln!("Input error: {}", e);
                    break;
                }
            };

            let input = line.trim();
            if input.is_empty() {
                continue;
            }

            let _ = editor.add_history_entry(input);

            // Handle slash commands
            if input.starts_with('/') {
                if self.handle_command(input, config).await {
                    continue;
                }
            }

            // Persist user message
            if let (Some(ref store), Some(ref sid)) = (&self.session_store, &self.current_session_id) {
                let _ = store.add_message(sid, "user", input, None, None, None);
            }

            // Send to agent (with streaming if enabled)
            self.message_count += 1;

            if self.streaming_display {
                match self.chat_streaming(input, config).await {
                    Ok(_) => {}
                    Err(e) => {
                        eprintln!("\n{} {}\n", "Error:".bright_red().bold(), e);
                    }
                }
            } else {
                let spinner = Spinner::start("thinking…");
                let result = self.agent.chat(input).await;
                drop(spinner);
                match result {
                    Ok(response) => {
                        println!(
                            "\n{} {}\n",
                            format!("{}:", config.agent_name).bright_cyan().bold(),
                            response
                        );

                        // Persist assistant response
                        if let (Some(ref store), Some(ref sid)) = (&self.session_store, &self.current_session_id) {
                            let _ = store.add_message(sid, "assistant", &response, None, None, None);
                            // Auto-title after first exchange
                            if self.message_count == 1 {
                                let title: String = input.chars().take(60).collect();
                                let _ = store.update_title(sid, &title);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("\n{} {}\n", "Error:".bright_red().bold(), e);
                    }
                }
            }
        }

        // Save history
        let _ = editor.save_history(&history_path);
        Ok(())
    }

    /// Chat with streaming — display tokens as they arrive.
    async fn chat_streaming(&mut self, input: &str, config: &AppConfig) -> Result<()> {
        let (tx, mut rx) = stream_channel();
        self.agent.set_stream_sender(tx);
        self.agent.set_streaming(true);

        // Print prefix before any tokens arrive.
        print!(
            "\n{} ",
            format!("{}:", config.agent_name).bright_cyan().bold()
        );
        std::io::stdout().flush().unwrap_or_default();

        // We need to run the agent and consume stream events concurrently.
        // Wrap agent.chat() in a block that drops nothing early.
        let agent_fut = self.agent.chat(input);

        // Pin the future so we can poll it inside select!
        tokio::pin!(agent_fut);

        let mut agent_result: Option<Result<String>> = None;

        loop {
            tokio::select! {
                biased;
                // Prefer draining events so the UI stays responsive.
                event = rx.recv() => {
                    match event {
                        Some(StreamEvent::TextDelta { text }) => {
                            print!("{}", text);
                            std::io::stdout().flush().unwrap_or_default();
                        }
                        Some(StreamEvent::Thinking { text }) => {
                            if self.show_thinking {
                                println!(
                                    "\n  {} {}",
                                    "💭".dimmed(),
                                    text.chars().take(200).collect::<String>().dimmed()
                                );
                            }
                        }
                        Some(StreamEvent::ToolUseStart { name, .. }) => {
                            print!("\n  {} {}", "🔧".bright_yellow(), name.bright_yellow());
                            std::io::stdout().flush().unwrap_or_default();
                        }
                        Some(StreamEvent::ToolResult { success, .. }) => {
                            let icon = if success { "✅" } else { "❌" };
                            println!(" {}", icon);
                        }
                        Some(StreamEvent::Usage { input_tokens, output_tokens }) => {
                            self.usage_tracker.record(
                                crate::usage::TokenUsage {
                                    input_tokens: input_tokens as u64,
                                    output_tokens: output_tokens as u64,
                                    ..Default::default()
                                },
                                None,
                            );
                        }
                        Some(StreamEvent::MessageStop) => {
                            // Stream finished; agent future should resolve shortly.
                        }
                        Some(StreamEvent::Error { message }) => {
                            eprintln!("\n{}", message.bright_red());
                        }
                        Some(_) => {}
                        None => {
                            // Channel closed — agent is done.
                            break;
                        }
                    }
                }
                result = &mut agent_fut, if agent_result.is_none() => {
                    agent_result = Some(result);
                    // Don't break — keep draining remaining events.
                    // stream_tx is cleared by agent.chat() internally,
                    // so rx.recv() will return None once events drain.
                }
            }

            // If the agent is done and the channel is closed, exit.
            if agent_result.is_some() && rx.is_closed() {
                break;
            }
        }

        println!("\n");

        match agent_result.unwrap_or_else(|| Ok(String::new())) {
            Ok(response) => {
                // Persist
                if let (Some(ref store), Some(ref sid)) = (&self.session_store, &self.current_session_id) {
                    let _ = store.add_message(sid, "assistant", &response, None, None, None);
                    if self.message_count == 1 {
                        let title: String = input.chars().take(60).collect();
                        let _ = store.update_title(sid, &title);
                    }
                }
            }
            Err(e) => {
                return Err(e);
            }
        }

        Ok(())
    }

    /// Handle a slash command. Returns true if the command was handled.
    async fn handle_command(&mut self, input: &str, config: &AppConfig) -> bool {
        let parts: Vec<&str> = input.splitn(2, ' ').collect();
        let cmd = parts[0];
        let args = parts.get(1).copied().unwrap_or("");

        match cmd {
            // === Session management ===
            "/quit" | "/exit" | "/q" => {
                // Print cost summary on exit
                if self.usage_tracker.turn_count() > 0 {
                    println!("\n{}", self.usage_tracker.summary().dimmed());
                }
                println!("\n{}", "Goodbye! 👋".bright_cyan());
                std::process::exit(0);
            }
            "/clear" | "/new" | "/reset" => {
                // Start new session
                let new_id = uuid::Uuid::new_v4().to_string();
                if let Some(ref store) = self.session_store {
                    let _ = store.create_session(&new_id, "cli");
                }
                self.current_session_id = Some(new_id.clone());
                self.agent.clear_context();
                self.message_count = 0;
                println!(
                    "{} (session: {})",
                    "✨ Conversation cleared. Starting fresh.".bright_yellow(),
                    &new_id[..8]
                );
            }

            // === Session persistence ===
            "/save" => {
                let title = if args.is_empty() {
                    format!("Session {}", chrono::Local::now().format("%Y-%m-%d %H:%M"))
                } else {
                    args.to_string()
                };
                if let (Some(ref store), Some(ref sid)) = (&self.session_store, &self.current_session_id) {
                    match store.update_title(sid, &title) {
                        Ok(_) => println!(
                            "  {} Saved session as '{}'  (id: {})",
                            "💾".bright_green(),
                            title.bright_cyan(),
                            &sid[..8]
                        ),
                        Err(e) => println!("  {} Save failed: {}", "❌", e),
                    }
                } else {
                    println!("  {} No session store available", "⚠".bright_yellow());
                }
            }
            "/sessions" | "/history" => {
                if let Some(ref store) = self.session_store {
                    match store.list_sessions(15) {
                        Ok(sessions) if sessions.is_empty() => {
                            println!("  {} No saved sessions", "ℹ".bright_blue());
                        }
                        Ok(sessions) => {
                            println!("\n{} ({} sessions):", "Recent Sessions".bright_blue().bold(), sessions.len());
                            for s in &sessions {
                                let marker = if self.current_session_id.as_deref() == Some(&s.id) {
                                    "▸".bright_green().to_string()
                                } else {
                                    " ".to_string()
                                };
                                println!(
                                    "  {} {} — {} ({} msgs, {})",
                                    marker,
                                    s.id[..8].bright_cyan(),
                                    s.title.bright_white(),
                                    s.message_count,
                                    s.updated_at.format("%b %d %H:%M")
                                );
                            }
                            println!("\n  Use {} to resume a session", "/load <id>".bright_green());
                            println!();
                        }
                        Err(e) => println!("  {} Failed to list sessions: {}", "❌", e),
                    }
                }
            }
            "/load" => {
                if args.is_empty() {
                    println!("  {} Usage: /load <session-id-prefix>", "⚠".bright_yellow());
                    return true;
                }
                if let Some(ref store) = self.session_store {
                    match store.list_sessions(100) {
                        Ok(sessions) => {
                            let found = sessions.iter().find(|s| s.id.starts_with(args));
                            if let Some(session) = found {
                                // Load messages and rebuild context
                                self.agent.clear_context();
                                match store.get_messages(&session.id, None) {
                                    Ok(messages) => {
                                        for msg in &messages {
                                            let m = match msg.role.as_str() {
                                                "user" => crate::llm::Message::user(&msg.content),
                                                "assistant" => crate::llm::Message::assistant(&msg.content),
                                                _ => continue,
                                            };
                                            self.agent.push_context_message(m);
                                        }
                                        self.current_session_id = Some(session.id.clone());
                                        self.message_count = messages.iter().filter(|m| m.role == "user").count();
                                        println!(
                                            "  {} Loaded session '{}' ({} messages)",
                                            "📂".bright_green(),
                                            session.title.bright_cyan(),
                                            messages.len()
                                        );
                                    }
                                    Err(e) => println!("  {} Failed to load messages: {}", "❌", e),
                                }
                            } else {
                                println!("  {} No session found matching '{}'", "⚠".bright_yellow(), args);
                            }
                        }
                        Err(e) => println!("  {} Failed to search sessions: {}", "❌", e),
                    }
                }
            }
            "/search" => {
                if args.is_empty() {
                    println!("  {} Usage: /search <query>", "⚠".bright_yellow());
                    return true;
                }
                if let Some(ref store) = self.session_store {
                    match store.search(args, 10) {
                        Ok(results) if results.is_empty() => {
                            println!("  {} No matches for '{}'", "ℹ".bright_blue(), args);
                        }
                        Ok(results) => {
                            println!("\n{} for '{}':", "Search Results".bright_blue().bold(), args);
                            for (msg, _score) in &results {
                                let preview: String = msg.content.chars().take(80).collect();
                                println!(
                                    "  {} [{}] {} — {}",
                                    "•".bright_cyan(),
                                    msg.role.bright_yellow(),
                                    msg.session_id[..8].dimmed(),
                                    preview
                                );
                            }
                            println!();
                        }
                        Err(e) => println!("  {} Search failed: {}", "❌", e),
                    }
                }
            }

            // === Tool & Skill info ===
            "/tools" => {
                let names = self.agent.tool_names();
                if names.is_empty() {
                    println!("{}", "No tools loaded.".bright_yellow());
                } else {
                    println!("\n{} ({} loaded):", "Tools".bright_blue().bold(), names.len());
                    for name in &names {
                        println!("  • {}", name.bright_cyan());
                    }
                    println!();
                }
            }
            "/skills" => {
                println!("{}", "Use `pylot skills list` for full skill listing.".bright_yellow());
            }

            // === Display settings ===
            "/verbose" => {
                self.agent.set_quiet_mode(false);
                println!("{}", "Verbose mode: ON (showing tool calls)".bright_green());
            }
            "/quiet" => {
                self.agent.set_quiet_mode(true);
                println!("{}", "Quiet mode: ON (hiding tool calls)".bright_green());
            }
            "/stream" | "/streaming" => {
                self.streaming_display = !self.streaming_display;
                self.agent.set_streaming(self.streaming_display);
                println!(
                    "{} Streaming: {}",
                    "⚡".bright_yellow(),
                    if self.streaming_display { "ON".bright_green() } else { "OFF".bright_red() }
                );
            }
            "/thinking" => {
                self.show_thinking = !self.show_thinking;
                println!(
                    "  {} Thinking display: {}",
                    "💭",
                    if self.show_thinking { "ON".bright_green() } else { "OFF".bright_red() }
                );
            }

            // === Approval system ===
            "/yolo" => {
                self.approval_mode = !self.approval_mode;
                self.agent.set_approval_enabled(self.approval_mode);
                if self.approval_mode {
                    println!("{}", "⚠️  Approval mode: ON (dangerous commands need confirmation)".bright_yellow());
                } else {
                    println!("{}", "🔓 YOLO mode: ON (all commands auto-approved — be careful!)".bright_red());
                }
            }

            // === Permission modes ===
            "/mode" => {
                if args.is_empty() {
                    println!(
                        "  Current mode: {}",
                        match self.permission_policy.mode() {
                            PermissionMode::ReadOnly => "read-only".bright_yellow(),
                            PermissionMode::WorkspaceWrite => "workspace-write".bright_green(),
                            PermissionMode::FullAccess => "full-access".bright_red(),
                        }
                    );
                    println!("  Usage: /mode <read-only|write|full>");
                } else {
                    match args {
                        "read-only" | "readonly" | "ro" => {
                            self.permission_policy.set_mode(PermissionMode::ReadOnly);
                            println!("  {} Mode: {} (only read tools allowed)", "🔒", "read-only".bright_yellow());
                        }
                        "write" | "workspace-write" | "ws" => {
                            self.permission_policy.set_mode(PermissionMode::WorkspaceWrite);
                            println!("  {} Mode: {} (writes limited to workspace)", "📝", "workspace-write".bright_green());
                        }
                        "full" | "full-access" | "fa" => {
                            self.permission_policy.set_mode(PermissionMode::FullAccess);
                            println!("  {} Mode: {} (all operations allowed)", "⚡", "full-access".bright_red());
                        }
                        _ => {
                            println!("  {} Unknown mode. Use: read-only, write, full", "⚠".bright_yellow());
                        }
                    }
                }
            }

            // === Context & Memory ===
            "/context" | "/status" => {
                let ctx_len = self.agent.context_len();
                println!("\n{}", "Session Status".bright_blue().bold());
                println!("  Messages in context: {}", ctx_len);
                println!("  Messages this session: {}", self.message_count);
                println!("  Model: {} ({})", config.llm_model, config.llm_provider);
                println!("  Streaming: {}", if self.streaming_display { "ON".bright_green() } else { "OFF".bright_red() });
                println!("  Thinking: {}", if self.show_thinking { "ON".bright_green() } else { "OFF".bright_red() });
                println!("  Approval: {}", if self.approval_mode { "ON".bright_green() } else { "OFF (YOLO)".bright_red() });
                println!(
                    "  Permission: {}",
                    match self.permission_policy.mode() {
                        PermissionMode::ReadOnly => "read-only".bright_yellow(),
                        PermissionMode::WorkspaceWrite => "workspace-write".bright_green(),
                        PermissionMode::FullAccess => "full-access".bright_red(),
                    }
                );
                if let Some(ref sid) = self.current_session_id {
                    println!("  Session: {}", &sid[..8]);
                }
                if self.usage_tracker.turn_count() > 0 {
                    println!("  Tokens: {} (${:.4})",
                        self.usage_tracker.cumulative().total(),
                        self.usage_tracker.total_cost()
                    );
                }
                println!();
            }
            "/compress" => {
                let ctx_len = self.agent.context_len();
                if ctx_len < 10 {
                    println!("  {} Context too short to compress ({} messages)", "ℹ".bright_blue(), ctx_len);
                } else {
                    println!("{}", "Context compression triggered.".bright_yellow());
                    println!("  Context has {} messages — older messages will be summarized on next LLM call.", ctx_len);
                }
            }

            // === Usage & Cost ===
            "/cost" | "/usage" => {
                if self.usage_tracker.turn_count() == 0 {
                    println!("  {} No usage recorded yet", "ℹ".bright_blue());
                } else {
                    println!("\n{}", self.usage_tracker.summary());
                    println!();
                }
            }

            // === Model info ===
            "/model" => {
                println!(
                    "  {} {} ({})",
                    "Model:".bright_blue().bold(),
                    config.llm_model,
                    config.llm_provider
                );
            }

            // === Export conversation ===
            "/export" => {
                let format = if args.is_empty() { "md" } else { args };
                if let (Some(ref store), Some(ref sid)) = (&self.session_store, &self.current_session_id) {
                    match store.get_messages(sid, None) {
                        Ok(messages) if messages.is_empty() => {
                            println!("  {} No messages to export", "ℹ".bright_blue());
                        }
                        Ok(messages) => {
                            let filename = format!("conversation-{}.{}", &sid[..8], format);
                            let content = match format {
                                "json" => serde_json::to_string_pretty(&messages).unwrap_or_default(),
                                _ => {
                                    // Markdown format
                                    let mut md = format!("# Conversation {}\n\n", &sid[..8]);
                                    for msg in &messages {
                                        let role = match msg.role.as_str() {
                                            "user" => "**You**",
                                            "assistant" => "**Assistant**",
                                            _ => &msg.role,
                                        };
                                        md.push_str(&format!("### {}\n\n{}\n\n---\n\n", role, msg.content));
                                    }
                                    md
                                }
                            };
                            match std::fs::write(&filename, &content) {
                                Ok(_) => println!("  {} Exported to {}", "📄".bright_green(), filename.bright_cyan()),
                                Err(e) => println!("  {} Export failed: {}", "❌", e),
                            }
                        }
                        Err(e) => println!("  {} Export failed: {}", "❌", e),
                    }
                } else {
                    println!("  {} No session to export", "⚠".bright_yellow());
                }
            }

            // === Help ===
            "/help" | "/h" => {
                self.print_help();
            }

            _ => {
                println!(
                    "{} Unknown command: {}. Type /help for commands.",
                    "⚠".bright_yellow(),
                    input.bright_red()
                );
            }
        }
        true
    }

    fn print_help(&self) {
        println!("\n{}", "OpenPylot Help".bright_blue().bold());
        println!("{}", "─".repeat(55));
        println!("Just type a message to chat with your AI agent.\n");

        println!("{}", "Session Commands:".bright_yellow().bold());
        println!("  {}        — Start a fresh conversation", "/clear".bright_green());
        println!("  {} — Save session with optional name", "/save [name]".bright_green());
        println!("  {}    — Load a saved session", "/load <id>".bright_green());
        println!("  {}     — List recent sessions", "/sessions".bright_green());
        println!("  {} — Search across all sessions", "/search <q>".bright_green());
        println!("  {} — Export conversation (md/json)", "/export [fmt]".bright_green());
        println!("  {}         — Exit the agent", "/quit".bright_green());

        println!("\n{}", "Display Commands:".bright_yellow().bold());
        println!("  {}      — Show tool call details", "/verbose".bright_green());
        println!("  {}        — Hide tool call details", "/quiet".bright_green());
        println!("  {}       — Toggle streaming mode", "/stream".bright_green());
        println!("  {}     — Toggle thinking display", "/thinking".bright_green());

        println!("\n{}", "Info Commands:".bright_yellow().bold());
        println!("  {}       — Show session info & stats", "/status".bright_green());
        println!("  {}        — Show LLM model info", "/model".bright_green());
        println!("  {}        — List loaded tools", "/tools".bright_green());
        println!("  {}       — Show skills info", "/skills".bright_green());
        println!("  {}    — Show token usage & cost", "/cost".bright_green());

        println!("\n{}", "Safety & Mode Commands:".bright_yellow().bold());
        println!("  {}         — Toggle YOLO mode (skip approval)", "/yolo".bright_green());
        println!("  {} — Set permission (ro/write/full)", "/mode <m>".bright_green());
        println!("  {}     — Trigger context compression", "/compress".bright_green());

        println!("\n{}", "Capabilities:".bright_cyan().bold());
        println!("  • {} — Run shell commands", "Bash".bright_cyan());
        println!("  • {} — Read, write, edit files", "Files".bright_cyan());
        println!("  • {} — Search files and code", "Search".bright_cyan());
        println!("  • {} — Calendar, Gmail, Telegram, WhatsApp", "Integrations".bright_cyan());
        println!("  • {} — Notes, reminders, documents", "Productivity".bright_cyan());
        println!("  • {} — Web search & content extraction", "Web".bright_cyan());
        println!("  • {} — Persistent knowledge base", "Memory".bright_cyan());
        println!();
    }
}
