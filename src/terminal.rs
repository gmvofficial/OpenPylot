use anyhow::Result;
use colored::Colorize;

use crate::agent::Agent;
use crate::config::AppConfig;

/// Terminal REPL interface for the GMV Agent.
pub struct Terminal {
    agent: Agent,
}

impl Terminal {
    pub fn new(agent: Agent) -> Self {
        Self { agent }
    }

    /// Print the welcome banner.
    fn print_banner(&self, config: &AppConfig) {
        let border = "═".repeat(56);
        println!("\n  ╔{}╗", border);
        println!(
            "  ║{:^56}║",
            format!("🤖 {} v0.1.0", config.agent_name)
        );
        println!(
            "  ║{:^56}║",
            "Personal AI Assistant — Powered by Rust"
        );
        println!("  ╚{}╝\n", border);

        println!(
            "  {} {} ({})",
            "LLM:".bright_blue().bold(),
            config.llm_model,
            config.llm_provider
        );

        let tool_names = self.agent.tool_names();
        if tool_names.is_empty() {
            println!("  {} none", "Tools:".bright_blue().bold());
        } else {
            println!(
                "  {} {} tool(s) loaded",
                "Tools:".bright_blue().bold(),
                tool_names.len()
            );
            for name in &tool_names {
                println!("    • {}", name.bright_cyan());
            }
        }

        println!();
        println!("  {}", "Commands:".bright_yellow().bold());
        println!("    {}  — Start a fresh conversation", "/clear".bright_green());
        println!("    {}   — Show loaded tools", "/tools".bright_green());
        println!("    {}   — Show this help", "/help".bright_green());
        println!("    {}   — Exit the agent", "/quit".bright_green());
        println!();
    }

    /// Run the interactive REPL loop.
    pub async fn run(&mut self, config: &AppConfig) -> Result<()> {
        self.print_banner(config);

        let mut editor = rustyline::DefaultEditor::new()?;

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

            // Handle commands
            match input {
                "/quit" | "/exit" | "/q" => {
                    println!("\n{}", "Goodbye! 👋".bright_cyan());
                    break;
                }
                "/clear" => {
                    self.agent.clear_context();
                    println!("{}", "Conversation cleared.".bright_yellow());
                    continue;
                }
                "/tools" => {
                    let names = self.agent.tool_names();
                    if names.is_empty() {
                        println!("{}", "No tools loaded.".bright_yellow());
                    } else {
                        println!("{}", "Loaded tools:".bright_blue().bold());
                        for name in &names {
                            println!("  • {}", name.bright_cyan());
                        }
                    }
                    continue;
                }
                "/help" => {
                    self.print_help();
                    continue;
                }
                _ if input.starts_with('/') => {
                    println!(
                        "{} Unknown command: {}. Type /help for commands.",
                        "⚠".bright_yellow(),
                        input.bright_red()
                    );
                    continue;
                }
                _ => {}
            }

            // Send to agent
            match self.agent.chat(input).await {
                Ok(response) => {
                    println!(
                        "\n{} {}\n",
                        format!("{}:", config.agent_name).bright_cyan().bold(),
                        response
                    );
                }
                Err(e) => {
                    eprintln!(
                        "\n{} {}\n",
                        "Error:".bright_red().bold(),
                        e
                    );
                }
            }
        }

        // Save history
        let _ = editor.save_history(&history_path);
        Ok(())
    }

    fn print_help(&self) {
        println!("\n{}", "GMV Agent Help".bright_blue().bold());
        println!("{}", "─".repeat(40));
        println!("Just type a message to chat with your AI assistant.");
        println!("The agent can use tools to help you:\n");
        println!("  {} — Create, list, search, delete notes", "Notes".bright_cyan());
        println!("  {} — Create events, meetings, list calendar", "Calendar".bright_cyan());
        println!("  {} — Send messages via Telegram", "Telegram".bright_cyan());
        println!("  {} — Send messages via WhatsApp", "WhatsApp".bright_cyan());
        println!("  {} — Set, list, complete reminders", "Reminders".bright_cyan());
        println!("\n{}", "Commands:".bright_yellow().bold());
        println!("  /clear  — Start a fresh conversation");
        println!("  /tools  — Show loaded tools");
        println!("  /help   — Show this help");
        println!("  /quit   — Exit the agent\n");
    }
}
