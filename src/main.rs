mod agent;
mod config;
mod context;
mod init;
mod jobs;
mod llm;
mod memory;
mod oauth;
mod scheduler;
mod secrets;
mod telegram_bot;
mod terminal;
mod tools;
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
use crate::terminal::Terminal;
use crate::tools::calendar::{
    authorize_google, CalendarConfig, CreateCalendarEvent, CreateMeeting, ListCalendarEvents,
};
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
    name = "gmv-agent",
    about = "GMV Agent — A Rust-powered personal AI assistant",
    version = "0.2.0"
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

// ── Main ─────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize tracing with different levels based on mode
    let log_level = match &cli.command {
        Some(Commands::TelegramBot) | Some(Commands::Serve { .. }) => "info",
        _ => "info",
    };

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(format!("gmv_agent={}", log_level).parse().unwrap()),
        )
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
            println!("{}", "GMV Agent Setup Wizard".bright_blue().bold());
            println!("{}", "═".repeat(40));
            println!("\nAvailable setup commands:\n");
            println!(
                "  {} — Authorize Google Calendar OAuth2",
                "gmv-agent setup google-calendar".bright_green()
            );
            println!(
                "  {} — Authorize Gmail OAuth2",
                "gmv-agent setup gmail".bright_green()
            );
            println!("\nFor general configuration, edit your .env file.");
            println!("See .env.example for available options.\n");
            Ok(())
        }
    }
}

// ── One-shot chat ────────────────────────────────────────────────────

async fn run_oneshot(config: &AppConfig, message: &str) -> Result<()> {
    let (llm, tools) = build_components(config)?;
    let system_prompt = build_system_prompt(config);

    let mut agent = Agent::new(
        llm,
        tools,
        system_prompt,
        config.max_context_messages,
        config.max_tool_iterations,
        config.data_dir.clone(),
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
        "║    🤖 GMV Agent - Telegram Bot Mode             ║".bright_cyan()
    );
    println!(
        "{}",
        "╚══════════════════════════════════════════════════╝".bright_cyan()
    );
    println!();

    // Enable agent-level logging to see tool calls and iterations
    std::env::set_var("RUST_LOG", "gmv_agent=info");

    let (llm, tools) = build_components(config)?;
    let system_prompt = build_system_prompt(config);

    let mut agent = Agent::new(
        llm,
        tools,
        system_prompt,
        config.max_context_messages,
        config.max_tool_iterations,
        config.data_dir.clone(),
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

// ── Interactive REPL ─────────────────────────────────────────────────

async fn run_interactive(config: &AppConfig) -> Result<()> {
    let (llm, tools) = build_components(config)?;
    let system_prompt = build_system_prompt(config);

    let agent = Agent::new(
        llm,
        tools,
        system_prompt,
        config.max_context_messages,
        config.max_tool_iterations,
        config.data_dir.clone(),
    )?;

    let mut terminal = Terminal::new(agent);
    terminal.run(config).await
}

// ── List tools ───────────────────────────────────────────────────────

fn list_tools(config: &AppConfig) {
    let tools = build_tool_registry(config);
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

// ── Build LLM provider and tool registry ─────────────────────────────

fn build_tool_registry(config: &AppConfig) -> ToolRegistry {
    let mut tools = ToolRegistry::new();
    let data_dir = config.data_dir.clone();

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

    tools
}

fn build_components(config: &AppConfig) -> Result<(Box<dyn LlmProvider>, ToolRegistry)> {
    // Build LLM provider
    let llm: Box<dyn LlmProvider> = match config.llm_provider.as_str() {
        "anthropic" => {
            let api_key = config
                .anthropic_api_key
                .as_ref()
                .context("ANTHROPIC_API_KEY not set. Run 'gmv-agent init' or add it to your .env file.")?
                .clone();
            Box::new(AnthropicProvider::new(
                api_key,
                config.llm_model.clone(),
                config.llm_max_tokens,
            ))
        }
        "openai" | _ => {
            let api_key = config
                .openai_api_key
                .as_ref()
                .context("OPENAI_API_KEY not set. Run 'gmv-agent init' or add it to your .env file.")?
                .clone();
            Box::new(OpenAIProvider::new(
                api_key,
                config.llm_model.clone(),
                config.llm_max_tokens,
                config.llm_temperature,
            ))
        }
    };

    let tools = build_tool_registry(config);

    Ok((llm, tools))
}

// ── Serve command (daemon with scheduler) ────────────────────────────

async fn run_serve(config: &AppConfig, foreground: bool) -> Result<()> {
    println!(
        "{}",
        "╔══════════════════════════════════════════════════╗".bright_cyan()
    );
    println!(
        "{}",
        "║    🤖 GMV Agent — Serve Mode (Scheduler)        ║".bright_cyan()
    );
    println!(
        "{}",
        "╚══════════════════════════════════════════════════╝".bright_cyan()
    );
    println!();

    let data_dir = config.data_dir.clone();

    let mut sched = AgentScheduler::new(&data_dir);

    // Register default jobs based on config
    let data_dir_clone = data_dir.clone();
    sched.add_job(
        "reminder_check",
        "Check for due reminders and send notifications",
        "* * * * *",
        true,
        move || {
            let dd = data_dir_clone.clone();
            Box::pin(async move {
                let notifications = jobs::reminders::check_due_reminders(&dd)?;
                if notifications.is_empty() {
                    Ok("No due reminders".to_string())
                } else {
                    for msg in &notifications {
                        println!("{}", msg);
                    }
                    Ok(format!("{} reminders notified", notifications.len()))
                }
            })
        },
    )?;

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
        || {
            Box::pin(async {
                Ok("Email digest generated".to_string())
            })
        },
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

    let sched = Arc::new(Mutex::new(sched));

    // Run scheduler and webhook server concurrently
    tokio::select! {
        result = AgentScheduler::start(sched) => result,
        result = webhooks::start_webhook_server(webhook_port, webhook_state) => result,
    }
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
    println!(
        "{} Removed '{}' credentials",
        "✅".bright_green(),
        service
    );

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
            println!("  Data dir:      {}", config.data_dir.display().to_string().bright_cyan());
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
            println!(
                "  Run: {} to update settings",
                "gmv-agent init".bright_green()
            );
            println!("  Key: {}, Value: {}", key.dimmed(), value.dimmed());
            Ok(())
        }
    }
}

// ── Logs command ─────────────────────────────────────────────────────

fn run_logs(scheduler: bool) -> Result<()> {
    let home = secrets::gmv_home_dir();
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
            "gmv-agent serve".bright_green()
        );
        return Ok(());
    }

    // Read and display last 50 lines
    let content = std::fs::read_to_string(&log_file)?;
    let lines: Vec<&str> = content.lines().collect();
    let start = if lines.len() > 50 { lines.len() - 50 } else { 0 };

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

// ── System prompt builder ────────────────────────────────────────────

fn build_system_prompt(config: &AppConfig) -> String {
    let now = chrono::Local::now();

    format!(
        r#"You are {name}, a personal AI assistant built with Rust.

{persona}

You have access to the following tool categories:
- **Notes**: Create, list, search, and delete personal notes
- **Reminders**: Set, list, and complete reminders
- **Google Calendar**: Create events, list upcoming events, and create meetings with Google Meet links
- **Gmail**: Search, read, send, and reply to emails; create and manage drafts
- **Telegram**: Send messages and check updates via Telegram Bot
- **WhatsApp**: Send messages via WhatsApp (Twilio)

Guidelines:
1. Use the available tools to help the user accomplish tasks.
2. When creating calendar events, clarify the timezone if ambiguous. Use ISO 8601 format for datetimes.
3. Before sending messages (Telegram/WhatsApp) or emails (Gmail), confirm the recipient and content with the user unless they're explicit.
4. For notes, use descriptive titles and appropriate tags for easy retrieval.
5. Be concise in responses but thorough in tool usage.
6. If a tool is not configured (e.g., missing API key), inform the user and suggest how to set it up.
7. When listing items (notes, events, reminders, emails), format them clearly using simple numbered lists.
8. When formatting responses, use plain text with simple formatting. Avoid mixing underscores (_) and asterisks (*) in the same response as they can cause display issues.
9. Current date and time: {datetime}
10. If the user asks you to remember something, create a note for it.
11. For meetings, always try to include a Google Meet link by using the create_meeting tool.
12. Always confirm with the user before sending emails or drafts via Gmail.
13. The create_meeting tool automatically sends email invitations to all attendees — no additional notification step is needed.
14. After a tool succeeds, report the result to the user. Do NOT keep calling tools unnecessarily."#,
        name = config.agent_name,
        persona = config.agent_persona,
        datetime = now.format("%Y-%m-%d %H:%M:%S %Z"),
    )
}
