mod agent;
mod api;
mod config;
mod context;
mod document_chunker;
mod frontend_assets;
mod hooks;
mod init;
mod jobs;
mod learning;
mod llm;
mod marketing;
mod mcp;
mod memory;
mod memory_v2;
mod oauth;
mod permissions;
mod scheduler;
mod secrets;
mod sessions;
mod skills;
mod smart_memory;
mod social;
mod streaming;
mod sub_agents;
mod telegram_bot;
mod terminal;
mod tools;
mod traits;
mod usage;
mod webhooks;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use colored::Colorize;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::agent::Agent;
use crate::config::AppConfig;
use crate::init::InitWizard;
use crate::llm::anthropic::AnthropicProvider;
use crate::llm::openai::OpenAIProvider;
use crate::llm::LlmProvider;
use crate::scheduler::AgentScheduler;
use crate::smart_memory::SmartMemory;
use crate::terminal::Terminal;
use crate::tools::calendar::{
    authorize_google, CalendarConfig, CreateCalendarEvent, CreateMeeting, ListCalendarEvents,
};
use crate::tools::document_loader::DocumentLoaderTool;
use crate::tools::gmail::{
    authorize_gmail, GmailConfig, GmailDraftCreateTool, GmailDraftGetTool, GmailDraftListTool,
    GmailDraftSendTool, GmailGetTool, GmailReplyTool, GmailSearchTool, GmailSendTool,
};
use crate::tools::notes::{CreateNote, DeleteNote, ListNotes, SearchNotes};
use crate::tools::reminder::{CompleteReminder, ListReminders, SetReminder};
use crate::tools::telegram::{GetTelegramUpdates, SendTelegramMessage};
use crate::tools::whatsapp::SendWhatsAppMessage;
use crate::tools::ToolRegistry;

// ── CLI definition ───────────────────────────────────────────────────

#[derive(Parser)]
#[command(
    name = "pylot",
    about = "OpenPylot — A Rust-powered personal AI assistant",
    version = env!("CARGO_PKG_VERSION")
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Interactive setup wizard (replaces the old 'setup' command)
    Init {
        /// Reset all configuration and start fresh
        #[arg(long)]
        reset: bool,
        /// Set up only a specific service (e.g., google-calendar, telegram, openai)
        #[arg(long = "only")]
        only: Option<String>,
    },
    /// Add a new integration
    Add {
        /// Service to add (e.g., google-calendar, telegram, whatsapp, github, slack, openai, anthropic)
        service: String,
    },
    /// Remove an integration
    Remove {
        /// Service to remove
        service: String,
    },
    /// Legacy setup command (use 'init' instead)
    Setup {
        /// Service to set up (e.g., google-calendar)
        service: Option<String>,
    },
    /// Send a one-shot message (non-interactive)
    Chat {
        /// The message to send
        message: String,
    },
    /// List configured tools
    Tools,
    /// List loaded skills
    Skills,
    /// Start Telegram bot mode (responds to messages on Telegram)
    TelegramBot,
    /// Start the agent daemon with scheduler and background jobs
    Serve {
        /// Run in the foreground instead of as a daemon
        #[arg(long)]
        foreground: bool,
        #[command(subcommand)]
        action: Option<ServeAction>,
    },
    /// Manage scheduled background jobs
    Jobs {
        #[command(subcommand)]
        action: JobsAction,
    },
    /// Diagnose configuration issues and test connections
    Doctor,
    /// Show agent status and connected services
    Status,
    /// View and manage configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Tail agent logs
    Logs {
        /// Show scheduler logs instead of agent logs
        #[arg(long)]
        scheduler: bool,
    },
    /// Manage memory system
    Memory {
        #[command(subcommand)]
        action: MemoryAction,
    },
    /// Manage sub-agents
    Agents {
        #[command(subcommand)]
        action: AgentsAction,
    },
    /// Manage MCP server connections
    Mcp {
        #[command(subcommand)]
        action: McpAction,
    },
    /// Manage social media accounts and posts
    Social {
        #[command(subcommand)]
        action: SocialAction,
    },
    /// Manage learning rules and insights
    Learn {
        #[command(subcommand)]
        action: LearnAction,
    },
    /// Generate shell completions
    Completion {
        /// Shell to generate completions for
        shell: String,
    },
}

#[derive(Subcommand)]
enum ServeAction {
    /// Install as a system service (launchd/systemd)
    Install,
    /// Remove the system service
    Uninstall,
}

#[derive(Subcommand)]
enum JobsAction {
    /// List all scheduled jobs
    List,
    /// Run a specific job immediately
    Run {
        /// Job name to run
        job_name: String,
    },
    /// Enable a scheduled job
    Enable {
        /// Job name to enable
        job_name: String,
    },
    /// Disable a scheduled job
    Disable {
        /// Job name to disable
        job_name: String,
    },
}

#[derive(Subcommand)]
enum ConfigAction {
    /// List current configuration
    List,
    /// Set a configuration value
    Set {
        /// Configuration key
        key: String,
        /// Configuration value
        value: String,
    },
}

#[derive(Subcommand)]
enum MemoryAction {
    /// Search memories
    Search {
        /// Search query
        query: String,
    },
    /// Show memory statistics
    Stats,
    /// Consolidate and clean up memories
    Consolidate,
}

#[derive(Subcommand)]
enum AgentsAction {
    /// List running sub-agents
    List,
    /// Show running sub-agent status
    Status {
        /// Agent ID
        id: String,
    },
    /// List available agent presets (manifests in ~/.pylot/agents/, ./agents/, bundled)
    Presets,
    /// Show a preset manifest's full details
    Show {
        /// Preset name (as defined by `name = "..."` in the .toml)
        name: String,
    },
    /// Print the user-level agent manifests directory (create new .toml files here)
    Path,
    /// Spawn a sub-agent from a preset with the given task
    Spawn {
        /// Preset name
        #[arg(short, long)]
        preset: String,
        /// Task description to send to the spawned agent
        task: String,
    },
}

#[derive(Subcommand)]
enum McpAction {
    /// List configured MCP servers
    List,
    /// List all MCP tools
    Tools,
}

#[derive(Subcommand)]
enum SocialAction {
    /// List connected social media accounts
    Accounts,
    /// List scheduled posts
    Posts,
    /// List campaigns
    Campaigns,
}

#[derive(Subcommand)]
enum LearnAction {
    /// List learned rules
    Rules,
    /// Show learning statistics
    Stats,
    /// Prune low-confidence rules
    Prune,
}

// ── Main ─────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize tracing with different levels based on mode.
    // Daemons (serve, telegram-bot) stay verbose; the interactive REPL and
    // one-shot chat default to "warn" for a clean screen. Set RUST_LOG
    // (e.g. RUST_LOG=pylot=info) to see agent internals when debugging.
    let log_level = match &cli.command {
        Some(Commands::TelegramBot) | Some(Commands::Serve { .. }) => "info",
        _ => "warn",
    };

    // If RUST_LOG is set, honor it verbatim; otherwise apply the mode default.
    let env_filter = match std::env::var("RUST_LOG") {
        Ok(v) if !v.is_empty() => tracing_subscriber::EnvFilter::new(v),
        _ => tracing_subscriber::EnvFilter::new(format!("pylot={}", log_level)),
    };
    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(false)
        .init();

    let config = AppConfig::load().context("Failed to load configuration")?;

    match cli.command {
        // New init wizard
        Some(Commands::Init { reset, only }) => {
            let wizard = InitWizard::new(reset);
            if let Some(service) = only {
                wizard.run_single_service(&service).await
            } else {
                wizard.run().await
            }
        }
        // Add integration (alias for init --only)
        Some(Commands::Add { service }) => {
            let wizard = InitWizard::new(false);
            wizard.run_single_service(&service).await
        }
        // Remove integration
        Some(Commands::Remove { service }) => run_remove_service(&service),
        // Legacy setup command
        Some(Commands::Setup { service }) => run_setup(&config, service.as_deref()).await,
        // One-shot chat
        Some(Commands::Chat { message }) => run_oneshot(&config, &message).await,
        // List tools
        Some(Commands::Tools) => {
            list_tools(&config);
            Ok(())
        }
        // List skills
        Some(Commands::Skills) => {
            list_skills();
            Ok(())
        }
        // Telegram bot mode
        Some(Commands::TelegramBot) => run_telegram_bot(&config).await,
        // Serve daemon with scheduler
        Some(Commands::Serve { foreground, action }) => match action {
            Some(ServeAction::Install) => scheduler::install_system_service(),
            Some(ServeAction::Uninstall) => scheduler::uninstall_system_service(),
            None => run_serve(&config, foreground).await,
        },
        // Job management
        Some(Commands::Jobs { action }) => run_jobs_command(&config, action).await,
        // Doctor diagnostics
        Some(Commands::Doctor) => init::run_doctor(),
        // Status
        Some(Commands::Status) => init::run_status(),
        // Config management
        Some(Commands::Config { action }) => run_config_command(action),
        // Logs
        Some(Commands::Logs { scheduler }) => run_logs(scheduler),
        // Memory management
        Some(Commands::Memory { action }) => run_memory_command(action, &config).await,
        // Sub-agents
        Some(Commands::Agents { action }) => run_agents_command(action, &config).await,
        // MCP servers
        Some(Commands::Mcp { action }) => run_mcp_command(action),
        // Social media
        Some(Commands::Social { action }) => run_social_command(action),
        // Learning
        Some(Commands::Learn { action }) => run_learn_command(action, &config),
        // Shell completions
        Some(Commands::Completion { shell }) => run_completion(&shell),
        // Default: interactive REPL
        None => run_interactive(&config).await,
    }
}

// ── Setup command ────────────────────────────────────────────────────

async fn run_setup(config: &AppConfig, service: Option<&str>) -> Result<()> {
    match service {
        Some("google-calendar") | Some("gcal") => {
            let client_id = config
                .google_client_id
                .as_ref()
                .context("GOOGLE_CLIENT_ID not set. Add it to your .env file.")?;
            let client_secret = config
                .google_client_secret
                .as_ref()
                .context("GOOGLE_CLIENT_SECRET not set. Add it to your .env file.")?;

            authorize_google(
                client_id,
                client_secret,
                config.google_redirect_port,
                &config.data_dir,
            )
            .await?;

            println!(
                "{}",
                "Google Calendar setup complete!".bright_green().bold()
            );
            Ok(())
        }
        Some("gmail") => {
            let client_id = config
                .google_client_id
                .as_ref()
                .context("GOOGLE_CLIENT_ID not set. Add it to your .env file.")?;
            let client_secret = config
                .google_client_secret
                .as_ref()
                .context("GOOGLE_CLIENT_SECRET not set. Add it to your .env file.")?;

            authorize_gmail(
                client_id,
                client_secret,
                config.google_redirect_port,
                &config.data_dir,
            )
            .await?;

            println!("{}", "Gmail setup complete!".bright_green().bold());
            Ok(())
        }
        Some(other) => {
            println!(
                "{} Unknown service: '{}'\n\nAvailable services:\n  • google-calendar\n  • gmail",
                "⚠".bright_yellow(),
                other
            );
            Ok(())
        }
        None => {
            println!("{}", "Pylot Setup Wizard".bright_blue().bold());
            println!("{}", "═".repeat(40));
            println!("\nAvailable setup commands:\n");
            println!(
                "  {} — Authorize Google Calendar OAuth2",
                "pylot setup google-calendar".bright_green()
            );
            println!(
                "  {} — Authorize Gmail OAuth2",
                "pylot setup gmail".bright_green()
            );
            println!("\nFor general configuration, edit your .env file.");
            println!("See .env.example for available options.\n");
            Ok(())
        }
    }
}

// ── One-shot chat ────────────────────────────────────────────────────

async fn run_oneshot(config: &AppConfig, message: &str) -> Result<()> {
    let smart_memory = init_smart_memory(config).await;
    let (llm, tools, skill_registry) = build_components(config, smart_memory.as_ref())?;
    let system_prompt = build_system_prompt(config);

    let memory_provider = smart_memory.map(|sm| sm as Arc<dyn crate::traits::MemoryProvider>);
    let mut agent = Agent::new(
        llm,
        tools,
        skill_registry,
        system_prompt,
        config.max_context_messages,
        config.max_tool_iterations,
        config.data_dir.clone(),
        memory_provider,
    )?;

    let response = agent.chat(message).await?;
    println!("{}", response);
    Ok(())
}

// ── Telegram Bot Mode ────────────────────────────────────────────────

async fn run_telegram_bot(config: &AppConfig) -> Result<()> {
    use crate::telegram_bot::TelegramBot;

    // Check if Telegram is configured
    let bot_token = config.telegram_bot_token.as_ref().context(
        "Telegram bot token not configured.\n\
             Add TELEGRAM_BOT_TOKEN to your .env file.",
    )?;

    // Clear screen for clean display
    print!("\x1B[2J\x1B[1;1H");

    println!(
        "{}",
        "╔══════════════════════════════════════════════════╗".bright_cyan()
    );
    println!(
        "{}",
        "║    🤖 OpenPylot - Telegram Bot Mode              ║".bright_cyan()
    );
    println!(
        "{}",
        "╚══════════════════════════════════════════════════╝".bright_cyan()
    );
    println!();

    // Enable agent-level logging to see tool calls and iterations
    std::env::set_var("RUST_LOG", "pylot=info");

    let smart_memory = init_smart_memory(config).await;
    let (llm, tools, skill_registry) = build_components(config, smart_memory.as_ref())?;
    let system_prompt = build_system_prompt(config);

    let memory_provider = smart_memory.map(|sm| sm as Arc<dyn crate::traits::MemoryProvider>);
    let mut agent = Agent::new(
        llm,
        tools,
        skill_registry,
        system_prompt,
        config.max_context_messages,
        config.max_tool_iterations,
        config.data_dir.clone(),
        memory_provider,
    )?;

    // Show tool calls for debugging
    agent.set_quiet_mode(false);

    let tool_count = agent.tool_names().len();
    println!(
        "{} {}",
        "✓".bright_green(),
        format!("Loaded {} tools", tool_count).bright_green()
    );
    println!(
        "{} {}",
        "✓".bright_green(),
        "Connected to LLM".bright_green()
    );
    println!("{} {}", "✓".bright_green(), "Bot is ready!".bright_green());
    println!();
    println!(
        "{}",
        "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".dimmed()
    );
    println!();

    let mut bot = TelegramBot::new(bot_token.clone());
    bot.start_polling(&mut agent).await?;

    Ok(())
}

/// Run Telegram bot polling in the background (for `serve` mode).
/// The agent is created independently so it runs on its own tokio task.
async fn run_telegram_bot_background(config: &AppConfig, bot_token: &str) -> Result<()> {
    use crate::telegram_bot::TelegramBot;

    let smart_memory = init_smart_memory(config).await;
    let (llm, tools, skill_registry) = build_components(config, smart_memory.as_ref())?;
    let system_prompt = build_system_prompt(config);

    let memory_provider = smart_memory.map(|sm| sm as Arc<dyn crate::traits::MemoryProvider>);
    let mut agent = Agent::new(
        llm,
        tools,
        skill_registry,
        system_prompt,
        config.max_context_messages,
        config.max_tool_iterations,
        config.data_dir.clone(),
        memory_provider,
    )?;
    agent.set_quiet_mode(true);

    let mut bot = TelegramBot::new(bot_token.to_string());
    bot.start_polling(&mut agent).await?;

    Ok(())
}

// ── Interactive REPL ─────────────────────────────────────────────────

async fn run_interactive(config: &AppConfig) -> Result<()> {
    let smart_memory = init_smart_memory(config).await;
    let (llm, tools, skill_registry) = build_components(config, smart_memory.as_ref())?;
    let system_prompt = build_system_prompt(config);

    let memory_provider = smart_memory.map(|sm| sm as Arc<dyn crate::traits::MemoryProvider>);
    let agent = Agent::new(
        llm,
        tools,
        skill_registry,
        system_prompt,
        config.max_context_messages,
        config.max_tool_iterations,
        config.data_dir.clone(),
        memory_provider,
    )?;

    let mut terminal = Terminal::new(agent, config);
    terminal.run(config).await
}

// ── List tools ───────────────────────────────────────────────────────

fn list_tools(config: &AppConfig) {
    let tools = build_tool_registry(config, None);
    let names = tools.names();
    if names.is_empty() {
        println!("{}", "No tools configured.".bright_yellow());
    } else {
        println!("{}", "Configured tools:".bright_blue().bold());
        for name in &names {
            println!("  • {}", name.bright_cyan());
        }
    }
}

fn list_skills() {
    let registry = skills::SkillRegistry::load_all(None);
    let all = registry.all_skills();
    if all.is_empty() {
        println!("{}", "No skills loaded.".bright_yellow());
        println!("  Place SKILL.md files in ./skills/ or ~/.pylot/skills/");
    } else {
        println!(
            "{}",
            format!("Loaded {} skill(s):", all.len())
                .bright_blue()
                .bold()
        );
        for skill in &all {
            let category = skill.meta.category.as_deref().unwrap_or("general");
            let source = match skill.source {
                skills::SkillSource::Bundled => "bundled",
                skills::SkillSource::Local => "local",
                skills::SkillSource::Workspace => "workspace",
            };
            println!(
                "  • {} {} [{}]",
                skill.meta.name.bright_cyan(),
                format!("({})", category).dimmed(),
                source.bright_green()
            );
            println!("    {}", skill.meta.description.dimmed());
        }
    }
}

// ── Build LLM provider and tool registry ─────────────────────────────

fn build_tool_registry(
    config: &AppConfig,
    smart_memory: Option<&Arc<SmartMemory>>,
) -> ToolRegistry {
    let mut tools = ToolRegistry::new();
    let data_dir = config.data_dir.clone();

    // -- Document Loader (always available) --
    tools.register(Box::new(DocumentLoaderTool::new()));
    tracing::info!("Document loader tool registered");

    // -- Web Search & Extract (always available, no API key needed) --
    tools.register(Box::new(crate::tools::web::WebSearchTool::new()));
    tools.register(Box::new(crate::tools::web::WebExtractTool::new()));
    tracing::info!("Web search and extract tools registered");

    // -- Coding Tools (always available) --
    let workspace_root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    tools.register(Box::new(crate::tools::bash::BashTool::new(
        workspace_root.clone(),
    )));
    tools.register(Box::new(crate::tools::file_ops::ReadFileTool::new(
        workspace_root.clone(),
    )));
    tools.register(Box::new(crate::tools::file_ops::WriteFileTool::new(
        workspace_root.clone(),
    )));
    tools.register(Box::new(crate::tools::file_ops::EditFileTool::new(
        workspace_root.clone(),
    )));
    tools.register(Box::new(crate::tools::search::GlobSearchTool::new(
        workspace_root.clone(),
    )));
    tools.register(Box::new(crate::tools::search::GrepSearchTool::new(
        workspace_root.clone(),
    )));
    tools.register(Box::new(crate::tools::search::ListDirectoryTool::new(
        workspace_root,
    )));
    tracing::info!("Coding tools registered (bash, file_ops, search)");

    // -- Notes (always available) --
    tools.register(Box::new(CreateNote::new(data_dir.clone())));
    tools.register(Box::new(ListNotes::new(data_dir.clone())));
    tools.register(Box::new(SearchNotes::new(data_dir.clone())));
    tools.register(Box::new(DeleteNote::new(data_dir.clone())));

    // -- Reminders (always available) --
    tools.register(Box::new(SetReminder::new(data_dir.clone())));
    tools.register(Box::new(ListReminders::new(data_dir.clone())));
    tools.register(Box::new(CompleteReminder::new(data_dir.clone())));

    // -- Google Calendar (if configured) --
    if config.google_calendar_enabled {
        if let (Some(client_id), Some(client_secret)) =
            (&config.google_client_id, &config.google_client_secret)
        {
            let cal_config = CalendarConfig {
                data_dir: data_dir.clone(),
                client_id: client_id.clone(),
                client_secret: client_secret.clone(),
            };
            tools.register(Box::new(CreateCalendarEvent::new(cal_config.clone())));
            tools.register(Box::new(ListCalendarEvents::new(cal_config.clone())));
            tools.register(Box::new(CreateMeeting::new(cal_config)));
            tracing::info!("Google Calendar tools loaded");
        }
    }

    // -- Gmail (if configured) --
    if config.gmail_enabled {
        if let (Some(client_id), Some(client_secret)) =
            (&config.google_client_id, &config.google_client_secret)
        {
            let gmail_config = GmailConfig {
                data_dir: data_dir.clone(),
                client_id: client_id.clone(),
                client_secret: client_secret.clone(),
            };
            tools.register(Box::new(GmailSearchTool::new(gmail_config.clone())));
            tools.register(Box::new(GmailGetTool::new(gmail_config.clone())));
            tools.register(Box::new(GmailSendTool::new(gmail_config.clone())));
            tools.register(Box::new(GmailReplyTool::new(gmail_config.clone())));
            tools.register(Box::new(GmailDraftCreateTool::new(gmail_config.clone())));
            tools.register(Box::new(GmailDraftSendTool::new(gmail_config.clone())));
            tools.register(Box::new(GmailDraftListTool::new(gmail_config.clone())));
            tools.register(Box::new(GmailDraftGetTool::new(gmail_config)));
            tracing::info!("Gmail tools loaded");
        }
    }

    // -- Telegram (if configured) --
    if config.telegram_enabled {
        if let Some(ref bot_token) = config.telegram_bot_token {
            tools.register(Box::new(SendTelegramMessage::new(
                bot_token.clone(),
                config.telegram_default_chat_id.clone(),
            )));
            tools.register(Box::new(GetTelegramUpdates::new(bot_token.clone())));
            tracing::info!("Telegram tools loaded");
        }
    }

    // -- WhatsApp via Twilio (if configured) --
    if config.whatsapp_enabled {
        if let (Some(ref sid), Some(ref token), Some(ref from)) = (
            &config.twilio_account_sid,
            &config.twilio_auth_token,
            &config.twilio_whatsapp_from,
        ) {
            tools.register(Box::new(SendWhatsAppMessage::new(
                sid.clone(),
                token.clone(),
                from.clone(),
            )));
            tracing::info!("WhatsApp tools loaded");
        }
    }

    // -- Social platforms (if configured) --
    {
        use crate::tools::social::{
            DiscordSendMessage, FacebookPost, LinkedInPost, SlackSendMessage, TwitterPost,
        };

        // LinkedIn — needs an access token. The `person_id` (member URN) is
        // resolved lazily on first post via /v2/userinfo or /v2/me, so we
        // register the tool as soon as a token exists.
        if config.social_linkedin_enabled {
            if let Some(token) = config.linkedin_access_token.as_ref() {
                tools.register(Box::new(LinkedInPost::new(
                    token.clone(),
                    config.linkedin_person_id.clone(),
                )));
                let pid_status = match config.linkedin_person_id.as_deref() {
                    Some(id) if !id.is_empty() => {
                        let valid = id.len() <= 60
                            && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
                        let suffix: String = id
                            .chars()
                            .rev()
                            .take(4)
                            .collect::<String>()
                            .chars()
                            .rev()
                            .collect();
                        format!("cached={} (…{}) valid={}", id.len(), suffix, valid)
                    }
                    _ => "absent (will resolve on first post)".to_string(),
                };
                let tok_len = token.len();
                tracing::info!(
                    "LinkedIn post tool loaded (token: {} chars, person_id: {})",
                    tok_len,
                    pid_status
                );
            }
        }

        // Twitter / X — OAuth 1.0a user-context (4 keys).
        if config.social_twitter_enabled {
            let k = config.twitter_api_key.as_ref();
            let s = config.twitter_api_secret.as_ref();
            let at = config.twitter_access_token.as_ref();
            let ats = config.twitter_access_token_secret.as_ref();
            if let (Some(k), Some(s), Some(at), Some(ats)) = (k, s, at, ats) {
                tools.register(Box::new(TwitterPost::new(
                    k.clone(),
                    s.clone(),
                    at.clone(),
                    ats.clone(),
                )));
                tracing::info!(
                    "Twitter post tool loaded (api_key={}c, api_secret={}c, \
                     access_token={}c, access_token_secret={}c)",
                    k.len(),
                    s.len(),
                    at.len(),
                    ats.len()
                );
            } else {
                tracing::debug!(
                    "Twitter enabled but missing credentials — \
                     api_key={}, api_secret={}, access_token={}, access_token_secret={}",
                    k.is_some(),
                    s.is_some(),
                    at.is_some(),
                    ats.is_some()
                );
            }
        } else {
            tracing::debug!(
                "Twitter post tool NOT loaded (social.twitter_enabled={}, \
                 has_api_key={})",
                config.social_twitter_enabled,
                config.twitter_api_key.is_some()
            );
        }

        // Facebook Page — page ID + long-lived page access token.
        if config.social_facebook_enabled {
            if let (Some(pid), Some(tok)) = (
                config.facebook_page_id.as_ref(),
                config.facebook_access_token.as_ref(),
            ) {
                tools.register(Box::new(FacebookPost::new(pid.clone(), tok.clone())));
                tracing::info!("Facebook post tool loaded");
            }
        }

        // Discord — bot token (+ optional default channel).
        // Read directly from vault since AppConfig doesn't expose discord fields.
        if let Ok(vault) =
            crate::secrets::SecretsVault::open(&crate::secrets::default_secrets_path(), None)
        {
            if let Some(bot_token) = vault.get("discord.bot_token") {
                let channel_id = vault.get("discord.channel_id");
                tools.register(Box::new(DiscordSendMessage::new(bot_token, channel_id)));
                tracing::info!("Discord send_message tool loaded");
            }

            if let Some(slack_token) = vault.get("slack.bot_token") {
                let default_channel = vault.get("slack.channel");
                tools.register(Box::new(SlackSendMessage::new(
                    slack_token,
                    default_channel,
                )));
                tracing::info!("Slack send_message tool loaded");
            }
        }
    }

    // -- Memory tools (if smart memory is enabled) --
    if let Some(sm) = smart_memory {
        use crate::tools::memory_tools::*;
        tools.register(Box::new(RememberFact::new(
            Arc::clone(sm),
            "default".to_string(),
        )));
        tools.register(Box::new(RecallMemories::new(
            Arc::clone(sm),
            "default".to_string(),
        )));
        tools.register(Box::new(SearchKnowledgeTool::new(Arc::clone(sm))));
        tools.register(Box::new(ForgetFact::new(Arc::clone(sm))));
        tracing::info!("Memory tools loaded (smart memory)");
    }

    tools
}

fn build_components(
    config: &AppConfig,
    smart_memory: Option<&Arc<SmartMemory>>,
) -> Result<(Arc<dyn LlmProvider>, ToolRegistry, skills::SkillRegistry)> {
    // Build LLM provider
    let llm: Arc<dyn LlmProvider> = match config.llm_provider.as_str() {
        "anthropic" => match config.anthropic_api_key.clone() {
            Some(api_key) => Arc::new(AnthropicProvider::new(
                api_key,
                config.llm_model.clone(),
                config.llm_max_tokens,
            )),
            None => llm_without_key(config)?,
        },
        "openai" | _ => match config.openai_api_key.clone() {
            Some(api_key) => Arc::new(OpenAIProvider::new(
                api_key,
                config.llm_model.clone(),
                config.llm_max_tokens,
                config.llm_temperature,
            )),
            None => llm_without_key(config)?,
        },
    };

    let tools = build_tool_registry(config, smart_memory);

    // Load skills from bundled, local, and workspace directories
    let skill_registry = skills::SkillRegistry::load_all(None);
    tracing::info!("Loaded {} skills total", skill_registry.len());

    Ok((llm, tools, skill_registry))
}

/// Called when no API key is configured for the active LLM provider.
///
/// Interactive terminal: prompt once and store the key in the encrypted
/// secrets vault. Non-interactive (start.sh, Docker, launchd): return a
/// lazy provider so the server still starts — the key can then be added
/// from the frontend setup wizard and takes effect without a restart.
fn llm_without_key(config: &AppConfig) -> Result<Arc<dyn LlmProvider>> {
    use std::io::IsTerminal;

    let provider = config.llm_provider.as_str();

    if std::io::stdin().is_terminal() && std::io::stderr().is_terminal() {
        let api_key = prompt_api_key_into_vault(provider)?;
        let built: Arc<dyn LlmProvider> = match provider {
            "anthropic" => Arc::new(AnthropicProvider::new(
                api_key,
                config.llm_model.clone(),
                config.llm_max_tokens,
            )),
            _ => Arc::new(OpenAIProvider::new(
                api_key,
                config.llm_model.clone(),
                config.llm_max_tokens,
                config.llm_temperature,
            )),
        };
        return Ok(built);
    }

    tracing::warn!(
        "No {} API key configured — starting anyway. Add the key from the web dashboard \
         setup wizard; it will be picked up without a restart.",
        provider
    );
    Ok(Arc::new(llm::lazy::LazyProvider::new(
        config.llm_provider.clone(),
        config.llm_model.clone(),
        config.llm_max_tokens,
        config.llm_temperature,
    )))
}

/// Prompt for an API key on the terminal and persist it to the encrypted vault.
fn prompt_api_key_into_vault(provider: &str) -> Result<String> {
    use dialoguer::Password;

    let (label, vault_key, prefix) = match provider {
        "anthropic" => ("Anthropic", "llm.anthropic.api_key", "sk-ant-"),
        _ => ("OpenAI", "llm.openai.api_key", "sk-"),
    };

    println!();
    println!("{} No {} API key found.", "🔑".bright_yellow(), label);
    println!("   It will be stored in the encrypted secrets vault (no .env needed).");

    let api_key: String = Password::new()
        .with_prompt(format!("Enter your {} API key", label))
        .interact()
        .context("Failed to read API key from terminal")?;
    let api_key = api_key.trim().to_string();

    if api_key.is_empty() {
        anyhow::bail!(
            "No API key provided. Run 'pylot init', or set it from the web dashboard setup wizard."
        );
    }
    if !api_key.starts_with(prefix) {
        println!(
            "   {} Key doesn't start with '{}' — it may not work.",
            "⚠".yellow(),
            prefix
        );
    }

    let vault_path = secrets::default_secrets_path();
    let mut vault =
        secrets::SecretsVault::open(&vault_path, None).context("Failed to open secrets vault")?;
    vault.set(vault_key, &api_key)?;
    vault.save()?;
    println!("   {} Key saved to encrypted vault.", "✅".bright_green());
    println!();

    Ok(api_key)
}

/// Initialize SmartMemory if enabled in config. Returns None on failure (graceful degradation).
async fn init_smart_memory(config: &AppConfig) -> Option<Arc<SmartMemory>> {
    if !config.memory_enabled {
        return None;
    }
    match SmartMemory::new(config).await {
        Ok(sm) => {
            tracing::info!(
                "Smart memory initialized (SQLite: {})",
                config.memory_db_name
            );
            Some(Arc::new(sm))
        }
        Err(e) => {
            tracing::warn!("Smart memory unavailable (falling back to legacy): {e}");
            None
        }
    }
}

// ── Serve command (daemon with scheduler) ────────────────────────────

async fn run_serve(config: &AppConfig, foreground: bool) -> Result<()> {
    println!(
        "{}",
        "╔══════════════════════════════════════════════════╗".bright_cyan()
    );
    println!(
        "{}",
        "║    🤖 OpenPylot — Serve Mode (Scheduler)        ║".bright_cyan()
    );
    println!(
        "{}",
        "╚══════════════════════════════════════════════════╝".bright_cyan()
    );
    println!();

    let data_dir = config.data_dir.clone();

    let mut sched = AgentScheduler::new(&data_dir);

    // Register default jobs based on config
    // Broadcast channel for real-time notifications to WebSocket clients
    let (notification_tx, _) = tokio::sync::broadcast::channel::<String>(64);

    // Note: reminder_check job is registered later, after the agent is created,
    // so it can trigger the agent to process reminder tasks.

    // Calendar RSVP monitor (only if Google Calendar is configured)
    if config.google_calendar_enabled {
        sched.add_job(
            "rsvp_monitor",
            "Check calendar events for RSVP changes",
            "*/10 * * * *",
            true,
            || Box::pin(async { Ok("RSVP check completed".to_string()) }),
        )?;

        sched.add_job(
            "calendar_sync",
            "Sync calendar events",
            "*/5 * * * *",
            true,
            || Box::pin(async { Ok("Calendar sync completed".to_string()) }),
        )?;
    }

    // Meeting reminders
    sched.add_job(
        "meeting_reminder",
        "Send reminders for upcoming meetings",
        "* * * * *",
        config.google_calendar_enabled,
        || Box::pin(async { Ok("Meeting reminder check completed".to_string()) }),
    )?;

    // Token refresh
    sched.add_job(
        "token_refresh",
        "Proactively refresh OAuth tokens before expiry",
        "0 * * * *",
        config.google_calendar_enabled,
        || Box::pin(async { Ok("Token refresh check completed".to_string()) }),
    )?;

    // Daily briefing (8:00 AM)
    sched.add_job(
        "daily_briefing",
        "Generate morning briefing (calendar, tasks, weather)",
        "0 8 * * *",
        true,
        || {
            Box::pin(async {
                // In a full implementation, this would:
                // 1. Fetch today's calendar events
                // 2. List pending reminders / tasks
                // 3. Compose a summary via the LLM
                // 4. Send via preferred notification channel
                Ok("Daily briefing generated".to_string())
            })
        },
    )?;

    // Email digest (9am, 1pm, 5pm)
    sched.add_job(
        "email_digest",
        "Summarize unread emails",
        "0 9,13,17 * * *",
        false, // disabled by default — requires Gmail integration
        || Box::pin(async { Ok("Email digest generated".to_string()) }),
    )?;

    let job_count = sched.list_jobs().len();
    let enabled_count = sched.list_jobs().iter().filter(|j| j.enabled).count();

    println!(
        "{} Scheduler loaded with {} jobs ({} enabled)",
        "✅".bright_green(),
        job_count.to_string().bright_cyan(),
        enabled_count.to_string().bright_cyan(),
    );

    if foreground {
        println!(
            "{} Running in foreground. Press Ctrl+C to stop.\n",
            "ℹ".bright_blue()
        );
    }

    // Start webhook server alongside the scheduler
    let webhook_port: u16 = 8443;
    let webhook_state = webhooks::server::WebhookState {
        data_dir: data_dir.clone(),
        events: Arc::new(Mutex::new(Vec::new())),
        telegram_bot_token: config.telegram_bot_token.clone(),
        telegram_chat_id: config.telegram_default_chat_id.clone(),
    };

    println!(
        "{} Webhook server listening on port {}",
        "✅".bright_green(),
        webhook_port.to_string().bright_cyan(),
    );

    // ── API + Frontend server ────────────────────────────────────────
    let api_port: u16 = 3001;

    // Build agent for the API server
    let smart_memory = init_smart_memory(config).await;
    let (llm, tools, skill_registry) = build_components(config, smart_memory.as_ref())?;
    let llm_for_api = Arc::clone(&llm);
    let system_prompt = build_system_prompt(config);
    let memory_provider = smart_memory
        .clone()
        .map(|sm| sm as Arc<dyn crate::traits::MemoryProvider>);
    let mut agent = Agent::new(
        llm,
        tools,
        skill_registry,
        system_prompt,
        config.max_context_messages,
        config.max_tool_iterations,
        config.data_dir.clone(),
        memory_provider,
    )?;
    agent.set_quiet_mode(true);

    // ── Initialize subsystems ────────────────────────────────────────

    // Memory v2
    let memory_v2_store = if config.memory_enabled {
        match crate::memory_v2::MemoryStore::open(&config.data_dir.join("memory_v2.db")) {
            Ok(store) => {
                let store = Arc::new(store);
                let embeddings = config.openai_api_key.as_ref().map(|key| {
                    Arc::new(crate::memory_v2::EmbeddingClient::new(
                        key.clone(),
                        config.memory_embedding_model.clone(),
                    ))
                });
                let retriever = Arc::new(crate::memory_v2::MemoryRetriever::new(
                    store.clone(),
                    embeddings,
                    crate::memory_v2::RetrievalMode::Auto,
                ));
                agent.set_memory_v2(store.clone(), retriever);
                tracing::info!("Memory v2 initialized");
                Some(store)
            }
            Err(e) => {
                tracing::warn!("Memory v2 unavailable: {e}");
                None
            }
        }
    } else {
        None
    };

    // MCP registry
    let mcp_registry = if config.mcp_enabled {
        let registry = Arc::new(tokio::sync::Mutex::new(crate::mcp::McpRegistry::new()));
        agent.set_mcp_registry(registry.clone());
        tracing::info!("MCP registry initialized");
        Some(registry)
    } else {
        None
    };

    // Conversation store (used by orchestrator + API)
    let conversations = Arc::new(api::ConversationStore::new(&config.data_dir));

    // Sub-agent store (SQLite persistence)
    let sub_agent_store = match crate::sub_agents::SubAgentStore::open(&config.data_dir) {
        Ok(s) => {
            tracing::info!("Sub-agent SQLite store opened");
            Some(Arc::new(s))
        }
        Err(e) => {
            tracing::warn!("Failed to open sub-agent store: {e}");
            None
        }
    };

    // Sub-agent orchestrator
    let spawn_conv_id: Arc<std::sync::Mutex<Option<String>>> =
        Arc::new(std::sync::Mutex::new(None));
    let orchestrator = {
        let mut orch = crate::sub_agents::AgentOrchestrator::new(4);
        if let Some(ref store) = sub_agent_store {
            orch.set_store(Arc::clone(store));
        }
        orch.set_conversations(Arc::clone(&conversations));
        let orch = Arc::new(orch);
        agent.set_orchestrator(orch.clone());
        // Register the spawn_sub_agent tool so the LLM can create sub-agents
        let mut spawn_tool = crate::tools::spawn_agent::SpawnSubAgentTool::new(
            orch.clone(),
            llm_for_api.clone(),
            config.data_dir.clone(),
        );
        spawn_tool.current_conversation_id = spawn_conv_id.clone();
        agent.register_tool(Box::new(spawn_tool));
        // Companion tool that lets the LLM stop recurring sub-agents on
        // user requests like "stop updates" / "cancel the news fetcher".
        let stop_tool = crate::tools::spawn_agent::StopRecurringSubAgentTool::new(orch.clone());
        agent.register_tool(Box::new(stop_tool));
        tracing::info!("Sub-agent orchestrator initialized (max 4 concurrent)");
        Some(orch)
    };

    // Social media manager
    let social_manager = {
        let mut manager =
            match crate::social::SocialManager::with_db(&config.data_dir.join("social.db")) {
                Ok(m) => m,
                Err(e) => {
                    tracing::warn!("Social DB init failed, using in-memory: {e}");
                    crate::social::SocialManager::new()
                }
            };
        let mut count = 0usize;

        if config.social_twitter_enabled {
            if let (Some(ref key), Some(ref secret), Some(ref at), Some(ref ats)) = (
                &config.twitter_api_key,
                &config.twitter_api_secret,
                &config.twitter_access_token,
                &config.twitter_access_token_secret,
            ) {
                let mut extra = std::collections::HashMap::new();
                extra.insert("access_token_secret".to_string(), ats.clone());
                manager.add_provider(Box::new(crate::social::TwitterProvider::new(
                    crate::social::PlatformConfig {
                        platform: crate::social::Platform::Twitter,
                        api_key: Some(key.clone()),
                        api_secret: Some(secret.clone()),
                        access_token: Some(at.clone()),
                        refresh_token: None,
                        extra,
                    },
                )));
                count += 1;
            }
        }
        if config.social_bluesky_enabled {
            if let (Some(ref handle), Some(ref password)) =
                (&config.bluesky_handle, &config.bluesky_app_password)
            {
                let mut extra = std::collections::HashMap::new();
                extra.insert("handle".to_string(), handle.clone());
                extra.insert("app_password".to_string(), password.clone());
                manager.add_provider(Box::new(crate::social::BlueskyProvider::new(
                    crate::social::PlatformConfig {
                        platform: crate::social::Platform::Bluesky,
                        api_key: None,
                        api_secret: None,
                        access_token: None,
                        refresh_token: None,
                        extra,
                    },
                )));
                count += 1;
            }
        }
        if config.social_linkedin_enabled {
            if let (Some(ref token), Some(ref pid)) =
                (&config.linkedin_access_token, &config.linkedin_person_id)
            {
                let mut extra = std::collections::HashMap::new();
                extra.insert("person_id".to_string(), pid.clone());
                manager.add_provider(Box::new(crate::social::LinkedInProvider::new(
                    crate::social::PlatformConfig {
                        platform: crate::social::Platform::LinkedIn,
                        api_key: None,
                        api_secret: None,
                        access_token: Some(token.clone()),
                        refresh_token: None,
                        extra,
                    },
                )));
                count += 1;
            }
        }
        if config.social_facebook_enabled {
            if let (Some(ref token), Some(ref page_id)) =
                (&config.facebook_access_token, &config.facebook_page_id)
            {
                let mut extra = std::collections::HashMap::new();
                extra.insert("page_id".to_string(), page_id.clone());
                manager.add_provider(Box::new(crate::social::FacebookProvider::new(
                    crate::social::PlatformConfig {
                        platform: crate::social::Platform::Facebook,
                        api_key: None,
                        api_secret: None,
                        access_token: Some(token.clone()),
                        refresh_token: None,
                        extra,
                    },
                )));
                count += 1;
                tracing::info!("Facebook provider registered");
            } else {
                tracing::warn!(
                    "Facebook enabled but credentials missing \
                     (access_token={}, page_id={}) — restart after connecting via UI.",
                    config.facebook_access_token.is_some(),
                    config.facebook_page_id.is_some(),
                );
            }
        }

        let sm = Arc::new(tokio::sync::Mutex::new(manager));
        agent.set_social_manager(sm.clone());
        if count > 0 {
            tracing::info!("Social media manager initialized ({count} providers)");
        }
        Some(sm)
    };

    // Learning / prompt evolution
    let prompt_evolution = if config.learning_enabled {
        let db_path = config.data_dir.join("learning.db");
        match crate::learning::PromptEvolution::new(db_path.to_str().unwrap_or("learning.db")) {
            Ok(pe) => {
                let pe = Arc::new(tokio::sync::Mutex::new(pe));
                agent.set_prompt_evolution(pe.clone());
                tracing::info!("Prompt evolution initialized");
                Some(pe)
            }
            Err(e) => {
                tracing::warn!("Prompt evolution unavailable: {e}");
                None
            }
        }
    } else {
        None
    };

    // Determine frontend build directory
    let frontend_dir = resolve_frontend_dir();

    // Wrap agent early so it can be shared with the reminder job
    let agent = Arc::new(Mutex::new(agent));

    // Register reminder_check job — needs agent + conversations to deliver reminders in chat
    {
        let dd = data_dir.clone();
        let tx = notification_tx.clone();
        let agent_for_job = agent.clone();
        let convos_for_job = conversations.clone();
        sched.add_job(
            "reminder_check",
            "Check for due reminders and send notifications",
            "* * * * *",
            true,
            move || {
                let dd = dd.clone();
                let tx = tx.clone();
                let agent = agent_for_job.clone();
                let convos = convos_for_job.clone();
                Box::pin(async move {
                    let due = jobs::reminders::check_due_reminders(&dd)?;
                    if due.is_empty() {
                        return Ok("No due reminders".to_string());
                    }

                    let count = due.len();
                    for reminder in &due {
                        // Send UI notification
                        let notif_msg = if reminder.description.is_empty() {
                            format!("🔔 Reminder: {}", reminder.title)
                        } else {
                            format!("🔔 Reminder: {}\n   {}", reminder.title, reminder.description)
                        };
                        tracing::info!("{}", notif_msg);
                        let payload = serde_json::json!({
                            "type": "reminder_due",
                            "title": notif_msg,
                            "message": notif_msg,
                        });
                        let _ = tx.send(payload.to_string());

                        // Build prompt for the agent based on the reminder
                        let prompt = if reminder.description.is_empty() {
                            format!(
                                "A reminder just fired: \"{}\". Please provide a helpful response about this topic.",
                                reminder.title
                            )
                        } else {
                            format!(
                                "A reminder just fired: \"{}\". Details: {}. Please carry out this task now.",
                                reminder.title, reminder.description
                            )
                        };

                        // Have the agent process the reminder and store in a conversation
                        let conv_id = format!("reminder-{}", uuid::Uuid::new_v4());
                        convos.add_message(
                            &conv_id,
                            api::StoredMessage {
                                id: uuid::Uuid::new_v4().to_string(),
                                role: "user".into(),
                                content: prompt.clone(),
                                timestamp: chrono::Utc::now().to_rfc3339(),
                            },
                        );

                        match agent.lock().await.chat(&prompt).await {
                            Ok(response) => {
                                convos.add_message(
                                    &conv_id,
                                    api::StoredMessage {
                                        id: uuid::Uuid::new_v4().to_string(),
                                        role: "assistant".into(),
                                        content: response.clone(),
                                        timestamp: chrono::Utc::now().to_rfc3339(),
                                    },
                                );
                                // Also send the response as a notification so it appears in UI
                                let response_payload = serde_json::json!({
                                    "type": "reminder_due",
                                    "title": format!("📋 {}", reminder.title),
                                    "message": response,
                                    "conversationId": conv_id,
                                });
                                let _ = tx.send(response_payload.to_string());
                            }
                            Err(e) => {
                                tracing::error!("Failed to process reminder '{}': {e}", reminder.title);
                            }
                        }
                    }

                    Ok(format!("{} reminders processed", count))
                })
            },
        )?;
    }

    let sched = Arc::new(Mutex::new(sched));
    let sched_clone = sched.clone();

    let api_state = api::ApiState {
        agent,
        config: Arc::new(config.clone()),
        llm: llm_for_api,
        scheduler: sched_clone,
        start_time: std::time::Instant::now(),
        conversations,
        smart_memory: smart_memory.clone(),
        mcp_registry,
        orchestrator,
        social_manager,
        prompt_evolution,
        memory_v2_store,
        sub_agent_store,
        spawn_conversation_id: spawn_conv_id,
        notification_tx,
    };

    if let Some(ref dir) = frontend_dir {
        // Dev override: serving an on-disk build.
        println!(
            "{} Frontend + API server on http://localhost:{}",
            "✅".bright_green(),
            api_port.to_string().bright_cyan(),
        );
        println!(
            "{} Frontend served from {}",
            "✅".bright_green(),
            dir.display().to_string().bright_cyan(),
        );
    } else if frontend_assets::has_embedded_frontend() {
        // Default: UI is compiled into the binary.
        println!(
            "{} Frontend + API server on http://localhost:{}",
            "✅".bright_green(),
            api_port.to_string().bright_cyan(),
        );
        println!("{} Frontend served from embedded build", "✅".bright_green());
    } else {
        println!(
            "{} API server on http://localhost:{} (no frontend build found)",
            "✅".bright_green(),
            api_port.to_string().bright_cyan(),
        );
        println!(
            "{} To build frontend: cd frontend && npm install && npm run build",
            "ℹ".bright_blue(),
        );
    }

    println!();

    // Start Telegram bot polling alongside other services (if configured)
    let telegram_handle: Option<tokio::task::JoinHandle<()>> = if config.telegram_enabled {
        if let Some(ref bot_token) = config.telegram_bot_token {
            let token = bot_token.clone();
            let cfg = config.clone();
            println!("{} Telegram bot polling started", "✅".bright_green(),);
            Some(tokio::spawn(async move {
                match run_telegram_bot_background(&cfg, &token).await {
                    Ok(_) => tracing::info!("Telegram bot stopped"),
                    Err(e) => tracing::error!("Telegram bot error: {}", e),
                }
            }))
        } else {
            tracing::warn!("Telegram enabled but no bot token configured");
            None
        }
    } else {
        None
    };

    // Run scheduler, webhook server, and API server concurrently
    let result = tokio::select! {
        result = AgentScheduler::start(sched) => result,
        result = webhooks::start_webhook_server(webhook_port, webhook_state) => result,
        result = api::start_api_server(api_port, api_state, frontend_dir) => result,
    };

    // Clean up Telegram bot task
    if let Some(handle) = telegram_handle {
        handle.abort();
    }

    result
}

// ── Resolve frontend build directory ─────────────────────────────────

fn resolve_frontend_dir() -> Option<std::path::PathBuf> {
    // 1. Check PYLOT_FRONTEND_DIR env var (with GMV_FRONTEND_DIR fallback)
    if let Ok(dir) =
        std::env::var("PYLOT_FRONTEND_DIR").or_else(|_| std::env::var("GMV_FRONTEND_DIR"))
    {
        let p = std::path::PathBuf::from(dir);
        if p.exists() {
            return Some(p);
        }
    }

    // 2. Check relative to current working directory
    let cwd = std::path::PathBuf::from("frontend/out");
    if cwd.exists() {
        return Some(cwd);
    }

    // 3. Check relative to executable location
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            let p = exe_dir.join("frontend/out");
            if p.exists() {
                return Some(p);
            }
            // Also check two levels up (target/release/ → project root)
            let p = exe_dir.join("../../frontend/out");
            if p.exists() {
                return Some(p.canonicalize().ok()?);
            }
        }
    }

    // 4. Check in Pylot home directory
    let home = crate::secrets::pylot_home_dir();
    let p = home.join("frontend/out");
    if p.exists() {
        return Some(p);
    }

    None
}

// ── Jobs command ─────────────────────────────────────────────────────

async fn run_jobs_command(config: &AppConfig, action: JobsAction) -> Result<()> {
    let mut sched = AgentScheduler::new(&config.data_dir);

    // Register jobs so we can list/manage them
    register_default_jobs(&mut sched, config)?;

    match action {
        JobsAction::List => {
            let jobs = sched.list_jobs();
            scheduler::print_jobs(&jobs);
        }
        JobsAction::Run { job_name } => {
            println!(
                "{} Running job '{}'...",
                "→".bright_blue(),
                job_name.bright_cyan()
            );
            let result = sched.run_job(&job_name).await?;
            println!("{} Result: {}", "✅".bright_green(), result);
        }
        JobsAction::Enable { job_name } => {
            if sched.set_enabled(&job_name, true) {
                println!("{} Enabled job '{}'", "✅".bright_green(), job_name);
            } else {
                println!("{} Job '{}' not found", "❌".bright_red(), job_name);
            }
        }
        JobsAction::Disable { job_name } => {
            if sched.set_enabled(&job_name, false) {
                println!("{} Disabled job '{}'", "✅".bright_green(), job_name);
            } else {
                println!("{} Job '{}' not found", "❌".bright_red(), job_name);
            }
        }
    }

    Ok(())
}

fn register_default_jobs(sched: &mut AgentScheduler, config: &AppConfig) -> Result<()> {
    let data_dir = config.data_dir.clone();

    sched.add_job(
        "reminder_check",
        "Check for due reminders",
        "* * * * *",
        true,
        move || {
            let dd = data_dir.clone();
            Box::pin(async move {
                let n = jobs::reminders::check_due_reminders(&dd)?;
                Ok(format!("{} reminders processed", n.len()))
            })
        },
    )?;

    sched.add_job(
        "rsvp_monitor",
        "Check calendar events for RSVP changes",
        "*/10 * * * *",
        config.google_calendar_enabled,
        || Box::pin(async { Ok("RSVP check completed".to_string()) }),
    )?;

    sched.add_job(
        "meeting_reminder",
        "Send reminders for upcoming meetings",
        "* * * * *",
        config.google_calendar_enabled,
        || Box::pin(async { Ok("Meeting reminder check completed".to_string()) }),
    )?;

    sched.add_job(
        "calendar_sync",
        "Sync calendar events",
        "*/5 * * * *",
        config.google_calendar_enabled,
        || Box::pin(async { Ok("Calendar sync completed".to_string()) }),
    )?;

    sched.add_job(
        "token_refresh",
        "Refresh OAuth tokens before expiry",
        "0 * * * *",
        config.google_calendar_enabled,
        || Box::pin(async { Ok("Token refresh completed".to_string()) }),
    )?;

    sched.add_job(
        "daily_briefing",
        "Generate morning briefing (calendar, tasks, weather)",
        "0 8 * * *",
        true,
        || Box::pin(async { Ok("Daily briefing generated".to_string()) }),
    )?;

    sched.add_job(
        "email_digest",
        "Summarize unread emails",
        "0 9,13,17 * * *",
        false,
        || Box::pin(async { Ok("Email digest generated".to_string()) }),
    )?;

    Ok(())
}

// ── Remove service ───────────────────────────────────────────────────

fn run_remove_service(service: &str) -> Result<()> {
    let secrets_path = secrets::default_secrets_path();
    if !secrets_path.exists() {
        println!(
            "{} No secrets vault found. Nothing to remove.",
            "ℹ".bright_blue()
        );
        return Ok(());
    }

    let mut vault = secrets::SecretsVault::open(&secrets_path, None)?;

    match service {
        "google-calendar" | "google" => {
            vault.delete("google.client_id")?;
            vault.delete("google.client_secret")?;
            vault.delete("google.access_token")?;
            vault.delete("google.refresh_token")?;
        }
        "telegram" => {
            vault.delete("telegram.bot_token")?;
            vault.delete("telegram.default_chat_id")?;
        }
        "whatsapp" => {
            vault.delete("twilio.account_sid")?;
            vault.delete("twilio.auth_token")?;
        }
        "github" => {
            vault.delete("github.access_token")?;
        }
        "slack" => {
            vault.delete("slack.bot_token")?;
        }
        "openai" => {
            vault.delete("llm.openai.api_key")?;
        }
        "anthropic" => {
            vault.delete("llm.anthropic.api_key")?;
        }
        _ => {
            println!("{} Unknown service: '{}'", "⚠".bright_yellow(), service);
            return Ok(());
        }
    }

    vault.save()?;
    println!("{} Removed '{}' credentials", "✅".bright_green(), service);

    Ok(())
}

// ── Config command ───────────────────────────────────────────────────

fn run_config_command(action: ConfigAction) -> Result<()> {
    match action {
        ConfigAction::List => {
            let config = AppConfig::load()?;
            println!("{}", "Current Configuration:".bright_blue().bold());
            println!("{}", "─".repeat(40).dimmed());
            println!("  Agent name:    {}", config.agent_name.bright_cyan());
            println!("  LLM provider:  {}", config.llm_provider.bright_cyan());
            println!("  LLM model:     {}", config.llm_model.bright_cyan());
            println!(
                "  Data dir:      {}",
                config.data_dir.display().to_string().bright_cyan()
            );
            println!();
            println!("  {}", "Integrations:".bright_blue());
            println!(
                "    Google Calendar: {}",
                if config.google_calendar_enabled {
                    "✅ enabled".bright_green().to_string()
                } else {
                    "❌ disabled".dimmed().to_string()
                }
            );
            println!(
                "    Telegram:        {}",
                if config.telegram_enabled {
                    "✅ enabled".bright_green().to_string()
                } else {
                    "❌ disabled".dimmed().to_string()
                }
            );
            println!(
                "    WhatsApp:        {}",
                if config.whatsapp_enabled {
                    "✅ enabled".bright_green().to_string()
                } else {
                    "❌ disabled".dimmed().to_string()
                }
            );
            Ok(())
        }
        ConfigAction::Set { key, value } => {
            println!(
                "{} Config set is currently managed through the init wizard.",
                "ℹ".bright_blue()
            );
            println!("  Run: {} to update settings", "pylot init".bright_green());
            println!("  Key: {}, Value: {}", key.dimmed(), value.dimmed());
            Ok(())
        }
    }
}

// ── Logs command ─────────────────────────────────────────────────────

fn run_logs(scheduler: bool) -> Result<()> {
    let home = secrets::pylot_home_dir();
    let log_file = if scheduler {
        home.join("logs").join("scheduler.log")
    } else {
        home.join("logs").join("agent.log")
    };

    if !log_file.exists() {
        println!(
            "{} Log file not found: {}",
            "ℹ".bright_blue(),
            log_file.display()
        );
        println!(
            "  Logs are created when running: {}",
            "pylot serve".bright_green()
        );
        return Ok(());
    }

    // Read and display last 50 lines
    let content = std::fs::read_to_string(&log_file)?;
    let lines: Vec<&str> = content.lines().collect();
    let start = if lines.len() > 50 {
        lines.len() - 50
    } else {
        0
    };

    println!(
        "{} Showing last {} lines of {}",
        "📋".bright_blue(),
        lines.len() - start,
        log_file.display()
    );
    println!("{}", "─".repeat(60).dimmed());

    for line in &lines[start..] {
        println!("{}", line);
    }

    Ok(())
}

// ── Memory command ───────────────────────────────────────────────────

async fn run_memory_command(action: MemoryAction, config: &AppConfig) -> Result<()> {
    let db_path = config.data_dir.join("smart_memory.db");
    match action {
        MemoryAction::Search { query } => {
            println!(
                "{} Searching memories for: {}",
                "🔍".bright_blue(),
                query.bright_white()
            );
            if let Some(ref sm) = init_smart_memory(config).await {
                match sm.search_knowledge(&query, 10).await {
                    Ok(results) => {
                        for (i, r) in results.iter().enumerate() {
                            println!(
                                "  {}. [score: {:.2}] {}",
                                i + 1,
                                r.score,
                                r.content.chars().take(100).collect::<String>()
                            );
                        }
                        if results.is_empty() {
                            println!("  No memories found.");
                        }
                    }
                    Err(e) => println!("{} Search failed: {e}", "✗".bright_red()),
                }
            } else {
                println!("{} Smart memory not configured", "✗".bright_red());
            }
        }
        MemoryAction::Stats => {
            println!("{} Memory Statistics", "📊".bright_blue());
            println!("  Database: {}", db_path.display());
            println!("  Exists: {}", db_path.exists());
        }
        MemoryAction::Consolidate => {
            println!(
                "{} Memory consolidation not yet wired to memory_v2",
                "⚠".bright_yellow()
            );
        }
    }
    Ok(())
}

// ── Agents command ───────────────────────────────────────────────────

async fn run_agents_command(action: AgentsAction, config: &AppConfig) -> Result<()> {
    use crate::sub_agents::{AgentManifestRegistry, ManifestSource};
    let workspace = std::env::current_dir().ok();
    let registry = AgentManifestRegistry::load_all(workspace.as_deref());

    match action {
        AgentsAction::List => {
            println!("{} Sub-Agent System", "🤖".bright_blue());
            println!("  No active sub-agents. Use the agent to spawn sub-agents during chat,");
            println!(
                "  or run: {} to spawn one from a preset.",
                "pylot agents spawn --preset <name> <task>".bright_green()
            );
        }
        AgentsAction::Status { id } => {
            println!("{} Agent {} not found", "✗".bright_red(), id);
        }
        AgentsAction::Presets => {
            let all = registry.all();
            if all.is_empty() {
                println!("{} No agent presets found.", "ℹ".bright_blue());
                if let Some(dir) = AgentManifestRegistry::user_agents_dir() {
                    println!(
                        "  Drop .toml files into: {}",
                        dir.display().to_string().bright_cyan()
                    );
                }
                println!("  Or bundled presets at: {}", "./agents/".bright_cyan());
            } else {
                println!(
                    "{}",
                    format!("Available agent presets ({}):", all.len())
                        .bright_blue()
                        .bold()
                );
                for m in all {
                    let src = match m.source {
                        ManifestSource::Bundled => "bundled",
                        ManifestSource::Local => "local",
                        ManifestSource::Workspace => "workspace",
                    };
                    println!(
                        "  • {} {} [{}]",
                        m.name.bright_cyan(),
                        format!("({})", m.agent_type).dimmed(),
                        src.bright_green()
                    );
                    if !m.description.is_empty() {
                        println!("    {}", m.description.dimmed());
                    }
                }
            }
        }
        AgentsAction::Show { name } => match registry.get(&name) {
            Some(m) => {
                println!(
                    "{} {}",
                    "Preset:".bright_blue().bold(),
                    m.name.bright_cyan()
                );
                println!("  type:         {}", m.agent_type);
                if let Some(ref path) = m.source_path {
                    println!("  source:       {} ({})", path.display(), m.source.as_str());
                }
                println!("  timeout:      {}s", m.timeout_secs);
                println!("  max_iters:    {}", m.max_iterations);
                if let Some(ref model) = m.model_override {
                    println!("  model:        {}", model);
                }
                if let Some(ref tools) = m.allowed_tools {
                    println!("  tools:        {}", tools.join(", "));
                }
                if !m.description.is_empty() {
                    println!("  description:  {}", m.description);
                }
                println!();
                println!("{}", "System prompt:".bright_blue());
                for line in m.system_prompt.lines() {
                    println!("  {}", line);
                }
            }
            None => {
                println!("{} Preset '{}' not found.", "✗".bright_red(), name);
                println!(
                    "  Run {} to see available presets.",
                    "pylot agents presets".bright_green()
                );
            }
        },
        AgentsAction::Path => {
            let dir = AgentManifestRegistry::user_agents_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("~/.pylot/agents"));
            if !dir.exists() {
                std::fs::create_dir_all(&dir).ok();
            }
            println!("{}", dir.display());
        }
        AgentsAction::Spawn { preset, task } => {
            let manifest = match registry.get(&preset) {
                Some(m) => m.clone(),
                None => {
                    println!("{} Preset '{}' not found.", "✗".bright_red(), preset);
                    println!(
                        "  Run {} to see available presets.",
                        "pylot agents presets".bright_green()
                    );
                    return Ok(());
                }
            };

            // Build a minimal LLM-only run: no tool registry sharing,
            // just use the parent agent's components and the preset's scope.
            let smart_memory = init_smart_memory(config).await;
            let (llm, tools, skill_registry) = build_components(config, smart_memory.as_ref())?;
            let memory_provider =
                smart_memory.map(|sm| sm as Arc<dyn crate::traits::MemoryProvider>);

            let sub_config = manifest.into_config();

            println!(
                "{} Spawning {} ({}): {}",
                "→".bright_blue(),
                sub_config.name.bright_cyan(),
                match sub_config.agent_type {
                    crate::sub_agents::SubAgentType::Task => "task",
                    crate::sub_agents::SubAgentType::Background => "background",
                    crate::sub_agents::SubAgentType::Specialist => "specialist",
                },
                task.dimmed()
            );

            // Run inline for CLI use (no orchestrator needed for one-shot).
            let mut agent = Agent::new(
                llm,
                tools,
                skill_registry,
                sub_config.system_prompt.clone(),
                config.max_context_messages,
                sub_config.max_iterations,
                config.data_dir.clone(),
                memory_provider,
            )?;
            agent.set_quiet_mode(false);
            let response = agent.chat(&task).await?;
            println!();
            println!("{}", response);
        }
    }
    Ok(())
}

// ── MCP command ──────────────────────────────────────────────────────

fn run_mcp_command(action: McpAction) -> Result<()> {
    match action {
        McpAction::List => {
            println!("{} MCP Servers", "🔌".bright_blue());
            println!("  No MCP servers configured.");
            println!("  Add servers to your config file under [mcp.servers]");
        }
        McpAction::Tools => {
            println!("{} MCP Tools", "🔧".bright_blue());
            println!("  No MCP tools available. Connect a server first.");
        }
    }
    Ok(())
}

// ── Social command ───────────────────────────────────────────────────

fn run_social_command(action: SocialAction) -> Result<()> {
    match action {
        SocialAction::Accounts => {
            println!("{} Social Media Accounts", "📱".bright_blue());
            println!("  No accounts connected.");
            println!("  Supported: Twitter, LinkedIn, Instagram, Facebook, Bluesky");
        }
        SocialAction::Posts => {
            println!("{} Scheduled Posts", "📝".bright_blue());
            println!("  No posts scheduled.");
        }
        SocialAction::Campaigns => {
            println!("{} Campaigns", "📢".bright_blue());
            println!("  No campaigns created.");
        }
    }
    Ok(())
}

// ── Learn command ────────────────────────────────────────────────────

fn run_learn_command(action: LearnAction, config: &AppConfig) -> Result<()> {
    let db_path = config.data_dir.join("learning.db");
    match action {
        LearnAction::Rules => match learning::PromptEvolution::new(&db_path.to_string_lossy()) {
            Ok(pe) => match pe.active_rules() {
                Ok(rules) => {
                    println!("{} Learned Rules ({})", "🧠".bright_blue(), rules.len());
                    for rule in &rules {
                        println!(
                            "  [{:.2}] {} (✓{} ✗{})",
                            rule.confidence, rule.rule_text, rule.success_count, rule.failure_count
                        );
                    }
                    if rules.is_empty() {
                        println!("  No rules learned yet.");
                    }
                }
                Err(e) => println!("{} Failed to load rules: {e}", "✗".bright_red()),
            },
            Err(e) => println!("{} Failed to open learning DB: {e}", "✗".bright_red()),
        },
        LearnAction::Stats => {
            println!("{} Learning Statistics", "📊".bright_blue());
            println!("  Database: {}", db_path.display());
            println!("  Exists: {}", db_path.exists());
        }
        LearnAction::Prune => match learning::PromptEvolution::new(&db_path.to_string_lossy()) {
            Ok(pe) => match pe.prune_dead_rules() {
                Ok(count) => println!("{} Pruned {} dead rules", "🧹".bright_blue(), count),
                Err(e) => println!("{} Prune failed: {e}", "✗".bright_red()),
            },
            Err(e) => println!("{} Failed to open learning DB: {e}", "✗".bright_red()),
        },
    }
    Ok(())
}

// ── Shell completions ────────────────────────────────────────────────

fn run_completion(shell: &str) -> Result<()> {
    use clap::CommandFactory;
    let mut cmd = Cli::command();
    let shell = match shell.to_lowercase().as_str() {
        "bash" => clap_complete::Shell::Bash,
        "zsh" => clap_complete::Shell::Zsh,
        "fish" => clap_complete::Shell::Fish,
        "powershell" | "ps" => clap_complete::Shell::PowerShell,
        _ => {
            println!(
                "{} Unknown shell: {}. Supported: bash, zsh, fish, powershell",
                "✗".bright_red(),
                shell
            );
            return Ok(());
        }
    };
    clap_complete::generate(shell, &mut cmd, "pylot", &mut std::io::stdout());
    Ok(())
}

// ── System prompt builder ────────────────────────────────────────────

fn build_system_prompt(config: &AppConfig) -> String {
    let now = chrono::Local::now();

    format!(
        r#"You are {name}, a personal AI assistant built with Rust.

{persona}

You have access to the following tool categories:
- **Web Search & Extract**: Search the web for current information using `web_search`, and extract full content from web pages using `web_extract`. ALWAYS use these when the user asks about recent news, updates, current events, or anything that requires up-to-date information beyond your training data.
- **Document Loader**: Load and extract text content from documents (PDF, DOCX, TXT, JSON, CSV, XML, HTML, Excel) from local file paths or URLs
- **Notes**: Create, list, search, and delete personal notes
- **Reminders**: Set, list, and complete reminders
- **Google Calendar**: Create events, list upcoming events, and create meetings with Google Meet links
- **Gmail**: Search, read, send, and reply to emails; create and manage drafts
- **Telegram**: Send messages and check updates via Telegram Bot
- **WhatsApp**: Send messages via WhatsApp (Twilio)
- **Sub-Agents**: Spawn autonomous sub-agents to handle tasks in the background. Use the `spawn_sub_agent` tool when the user asks you to spawn, create, or delegate a task to a sub-agent. You MUST call the tool — do NOT just describe the action in text.

Guidelines:
1. Use the available tools to help the user accomplish tasks.
2. **When users ask about recent news, updates, current events, or anything you are not certain about**, ALWAYS use `web_search` first to find current information, then optionally use `web_extract` to get full article content from the most relevant URLs. Never say you cannot find information without searching first.
3. **When users ask you to analyze, read, or load documents**, use the `load_document` tool with the file path and document type (pdf, docx, txt, json, csv, xml, html, xlsx, xls, xlsb). You CAN access local files on the user's system.
4. When creating calendar events, clarify the timezone if ambiguous. Use ISO 8601 format for datetimes.
5. Before sending messages (Telegram/WhatsApp), confirm the recipient and content with the user unless they're explicit.
5. For notes, use descriptive titles and appropriate tags for easy retrieval.
6. Be concise in responses but thorough in tool usage.
7. If a tool is not configured (e.g., missing API key), inform the user and suggest how to set it up.
8. When listing items (notes, events, reminders, emails), format them clearly using simple numbered lists.
9. When formatting responses, use plain text with simple formatting. Avoid mixing underscores (_) and asterisks (*) in the same response as they can cause display issues.
10. Current date and time: {datetime}
11. If the user asks you to remember something, create a note for it.
12. For meetings, always try to include a Google Meet link by using the create_meeting tool.
13. **EMAIL WORKFLOW (Gmail) — read carefully, two distinct cases:**

    **Case 1 — User asks to SEND an email** ("send an email to X", "email X about Y", "reply to this email", etc.):
    This is a STRICT, MANDATORY multi-step process. NEVER skip steps. Do NOT call `gmail_send` or `gmail_reply` on the first turn under any circumstances.

    - **Step A — Show draft in chat (no tool call):** Compose the email and display it to the user directly in the chat as plain text in this exact format:

          ✉️ Draft email — please review

          To: <recipient>
          Subject: <subject>

          <body>

          Reply with "send" to send it, or tell me what to change.

    - **Step B — Wait for the user's response.**
        - If the user requests changes (e.g. "make it shorter", "change subject", "add a sentence about…"), produce a NEW updated draft using the SAME format from Step A and ask again. Repeat as many times as needed.
        - Only when the user gives an explicit confirmation ("send", "send it", "yes send", "confirmed", "looks good send it", or similar unambiguous approval) may you proceed to Step C.
        - Vague replies like "ok", "thanks", "great" are NOT confirmation — ask explicitly: "Should I send it now?"

    - **Step C — Send.** Only after explicit confirmation, call `gmail_send` (or `gmail_reply` for replies) with the exact final draft contents the user approved. After the tool succeeds, briefly confirm to the user that the email was sent.

    **Case 2 — User asks to CREATE / SAVE a DRAFT in Gmail** ("create a draft", "save this as a draft in gmail", "draft an email and save it to my drafts folder", "make a gmail draft"):
    This means the user wants the draft saved into their actual Gmail Drafts folder — NOT just shown in chat. In this case:
        - You MAY call `gmail_draft_create` directly (no in-chat preview/confirmation loop is required, because the user explicitly asked for a draft, not a send).
        - After creation, briefly tell the user the draft was saved to Gmail Drafts and show them the To / Subject / Body so they can review it in Gmail or here.
        - If the user later says "send that draft" / "send it now", follow the Case 1 workflow before calling `gmail_draft_send` (show the draft in chat, get explicit "send" confirmation, then call `gmail_draft_send`).

    **How to tell the cases apart:** look at the verb the user used. "send / email / reply" → Case 1. "draft / save as draft / create a draft" → Case 2. If genuinely ambiguous, ask: "Do you want me to save this as a draft in Gmail, or prepare it here for you to review and send?"
14. The create_meeting tool automatically sends email invitations to all attendees — no additional notification step is needed.
15. After a tool succeeds, report the result to the user. Do NOT keep calling tools unnecessarily.
16. IMPORTANT: When you have a tool available for an action, you MUST call the tool. Never pretend you performed an action by describing it in text — always use the actual tool call."#,
        name = config.agent_name,
        persona = config.agent_persona,
        datetime = now.format("%Y-%m-%d %H:%M:%S %Z"),
    )
}
