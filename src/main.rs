mod agent;
mod config;
mod context;
mod llm;
mod memory;
mod terminal;
mod tools;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use colored::Colorize;

use crate::agent::Agent;
use crate::config::AppConfig;
use crate::llm::anthropic::AnthropicProvider;
use crate::llm::openai::OpenAIProvider;
use crate::llm::LlmProvider;
use crate::terminal::Terminal;
use crate::tools::calendar::{
    authorize_google, CalendarConfig, CreateCalendarEvent, CreateMeeting, ListCalendarEvents,
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
    version = "0.1.0"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Interactive setup wizard
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
}

// ── Main ─────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("gmv_agent=info".parse().unwrap()),
        )
        .with_target(false)
        .init();

    let cli = Cli::parse();
    let config = AppConfig::load().context("Failed to load configuration")?;

    match cli.command {
        Some(Commands::Setup { service }) => run_setup(&config, service.as_deref()).await,
        Some(Commands::Chat { message }) => run_oneshot(&config, &message).await,
        Some(Commands::Tools) => {
            list_tools(&config);
            Ok(())
        }
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

            println!("{}", "Google Calendar setup complete!".bright_green().bold());
            Ok(())
        }
        Some(other) => {
            println!(
                "{} Unknown service: '{}'\n\nAvailable services:\n  • google-calendar",
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
                .context(
                    "ANTHROPIC_API_KEY not set. Add it to your .env file to use Anthropic.",
                )?
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
                .context(
                    "OPENAI_API_KEY not set. Add it to your .env file to use OpenAI.",
                )?
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
- **Telegram**: Send messages and check updates via Telegram Bot
- **WhatsApp**: Send messages via WhatsApp (Twilio)

Guidelines:
1. Use the available tools to help the user accomplish tasks.
2. When creating calendar events, clarify the timezone if ambiguous. Use ISO 8601 format for datetimes.
3. Before sending messages (Telegram/WhatsApp), confirm the recipient and content with the user unless they're explicit.
4. For notes, use descriptive titles and appropriate tags for easy retrieval.
5. Be concise in responses but thorough in tool usage.
6. If a tool is not configured (e.g., missing API key), inform the user and suggest how to set it up.
7. When listing items (notes, events, reminders), format them clearly.
8. Current date and time: {datetime}
9. If the user asks you to remember something, create a note for it.
10. For meetings, always try to include a Google Meet link by using the create_meeting tool."#,
        name = config.agent_name,
        persona = config.agent_persona,
        datetime = now.format("%Y-%m-%d %H:%M:%S %Z"),
    )
}
