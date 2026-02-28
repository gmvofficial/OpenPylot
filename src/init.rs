use anyhow::{Context, Result};
use colored::Colorize;
use console::Term;
use dialoguer::{Confirm, Input, MultiSelect, Password, Select};
use indicatif::{ProgressBar, ProgressStyle};
use std::path::PathBuf;
use std::time::Duration;

use crate::secrets::{gmv_home_dir, SecretsVault};

/// Interactive setup wizard for GMV Agent.
///
/// Guides users through configuring:
/// 1. LLM provider & API key
/// 2. Agent identity (name, persona)
/// 3. Integrations (Google Calendar, Telegram, etc.)
/// 4. Notification preferences
/// 5. Background services
pub struct InitWizard {
    secrets_path: PathBuf,
    config_path: PathBuf,
    data_dir: PathBuf,
    reset: bool,
}

impl InitWizard {
    pub fn new(reset: bool) -> Self {
        let home = gmv_home_dir();
        Self {
            secrets_path: home.join("secrets.enc"),
            config_path: home.join("config.toml"),
            data_dir: home.join("data"),
            reset,
        }
    }

    /// Run the full interactive setup wizard.
    pub async fn run(&self) -> Result<()> {
        self.print_banner();

        // If reset, clear existing configuration
        if self.reset {
            self.reset_config()?;
        }

        // Ensure directories exist
        self.setup_directories()?;

        // Open or create secrets vault
        let mut vault = SecretsVault::open(&self.secrets_path, None)
            .context("Failed to open secrets vault")?;

        // Step 1: LLM Provider
        let (provider, model) = self.step_llm_provider(&mut vault)?;

        // Step 2: Agent Identity
        let (agent_name, user_name, persona) = self.step_agent_identity()?;

        // Step 3: Integrations
        let integrations = self.step_integrations(&mut vault).await?;

        // Step 4: Notifications
        let notifications = self.step_notifications(&integrations)?;

        // Step 5: Background services
        let bg_config = self.step_background_services()?;

        // Save secrets
        vault.save().context("Failed to save secrets")?;

        // Generate config file
        self.write_config(
            &provider,
            &model,
            &agent_name,
            &user_name,
            &persona,
            &integrations,
            &notifications,
            &bg_config,
        )?;

        // Step 6: Summary
        self.print_summary(
            &provider,
            &model,
            &agent_name,
            &user_name,
            &integrations,
            &notifications,
            &bg_config,
        );

        Ok(())
    }

    /// Run setup for a single service only.
    pub async fn run_single_service(&self, service: &str) -> Result<()> {
        self.setup_directories()?;
        let mut vault = SecretsVault::open(&self.secrets_path, None)
            .context("Failed to open secrets vault")?;

        match service {
            "google-calendar" | "gcal" | "google" => {
                self.setup_google_calendar(&mut vault).await?;
            }
            "telegram" => {
                self.setup_telegram(&mut vault)?;
            }
            "whatsapp" => {
                self.setup_whatsapp(&mut vault)?;
            }
            "github" => {
                self.setup_github(&mut vault)?;
            }
            "slack" => {
                self.setup_slack(&mut vault)?;
            }
            "openai" | "anthropic" | "ollama" => {
                self.setup_llm_provider(&mut vault, service)?;
            }
            _ => {
                println!(
                    "{} Unknown service: '{}'\n",
                    "⚠".bright_yellow(),
                    service
                );
                println!("Available services:");
                println!("  • google-calendar  — Google Calendar & Gmail OAuth");
                println!("  • telegram         — Telegram Bot setup");
                println!("  • whatsapp         — WhatsApp via Twilio");
                println!("  • github           — GitHub access token");
                println!("  • slack            — Slack Bot setup");
                println!("  • openai           — OpenAI API key");
                println!("  • anthropic        — Anthropic API key");
                return Ok(());
            }
        }

        vault.save().context("Failed to save secrets")?;
        println!(
            "\n{} {} setup complete!",
            "✅".bright_green(),
            service.bright_cyan()
        );

        Ok(())
    }

    // ── Banner & UI ──────────────────────────────────────────────────

    fn print_banner(&self) {
        let term = Term::stdout();
        let _ = term.clear_screen();

        println!(
            "{}",
            "┌──────────────────────────────────────────────────────────────┐"
                .bright_cyan()
        );
        println!(
            "{}",
            "│                                                              │"
                .bright_cyan()
        );
        println!(
            "{}",
            "│   🤖 GMV Agent — Interactive Setup                          │"
                .bright_cyan()
        );
        println!(
            "{}",
            "│                                                              │"
                .bright_cyan()
        );
        println!(
            "{}",
            "│   This wizard will walk you through configuring your         │"
                .bright_cyan()
        );
        println!(
            "{}",
            "│   personal AI assistant. You can re-run this anytime         │"
                .bright_cyan()
        );
        println!(
            "{}",
            "│   to add or update integrations.                             │"
                .bright_cyan()
        );
        println!(
            "{}",
            "│                                                              │"
                .bright_cyan()
        );
        println!(
            "{}",
            "└──────────────────────────────────────────────────────────────┘"
                .bright_cyan()
        );
        println!();
    }

    // ── Step 1: LLM Provider ────────────────────────────────────────

    fn step_llm_provider(&self, vault: &mut SecretsVault) -> Result<(String, String)> {
        println!(
            "\n{}\n{}",
            "Step 1 of 5: LLM Provider".bright_blue().bold(),
            "─".repeat(40).dimmed()
        );

        let providers = vec![
            "OpenAI (GPT-4o, GPT-4.1)",
            "Anthropic (Claude Sonnet 4, Claude Opus 4)",
            "Ollama (Local — free, private)",
            "Skip for now",
        ];

        let selection = Select::new()
            .with_prompt("Which LLM provider would you like to use?")
            .items(&providers)
            .default(0)
            .interact()
            .context("Failed to get provider selection")?;

        let (provider, model) = match selection {
            0 => {
                self.setup_llm_provider(vault, "openai")?;
                let models = vec![
                    "gpt-4o (recommended)",
                    "gpt-4o-mini (faster, cheaper)",
                    "gpt-4.1 (latest)",
                ];
                let model_idx = Select::new()
                    .with_prompt("Select default model")
                    .items(&models)
                    .default(0)
                    .interact()?;
                let model = match model_idx {
                    0 => "gpt-4o",
                    1 => "gpt-4o-mini",
                    2 => "gpt-4.1",
                    _ => "gpt-4o",
                };
                ("openai".to_string(), model.to_string())
            }
            1 => {
                self.setup_llm_provider(vault, "anthropic")?;
                let models = vec![
                    "claude-sonnet-4-20250514 (recommended)",
                    "claude-opus-4-20250514 (most powerful)",
                ];
                let model_idx = Select::new()
                    .with_prompt("Select default model")
                    .items(&models)
                    .default(0)
                    .interact()?;
                let model = match model_idx {
                    0 => "claude-sonnet-4-20250514",
                    1 => "claude-opus-4-20250514",
                    _ => "claude-sonnet-4-20250514",
                };
                ("anthropic".to_string(), model.to_string())
            }
            2 => {
                println!(
                    "  {} Ollama uses local models — no API key needed.",
                    "ℹ".bright_blue()
                );
                println!(
                    "  {} Make sure Ollama is running: {}",
                    "→".dimmed(),
                    "ollama serve".bright_green()
                );
                ("ollama".to_string(), "llama3.1".to_string())
            }
            _ => {
                println!(
                    "  {} Skipped. You can configure later with: {}",
                    "⏭".dimmed(),
                    "gmv-agent init --only openai".bright_green()
                );
                ("openai".to_string(), "gpt-4o".to_string())
            }
        };

        Ok((provider, model))
    }

    fn setup_llm_provider(&self, vault: &mut SecretsVault, provider: &str) -> Result<()> {
        match provider {
            "openai" => {
                let api_key: String = Password::new()
                    .with_prompt("Enter your OpenAI API key")
                    .interact()
                    .context("Failed to read API key")?;

                if api_key.is_empty() {
                    println!("  {} No key provided, skipping.", "⏭".dimmed());
                    return Ok(());
                }

                let spinner = self.spinner("Validating API key...");
                // Validate key format
                if api_key.starts_with("sk-") || api_key.starts_with("sk-proj-") {
                    spinner.finish_with_message("✅ API key format looks valid");
                } else {
                    spinner.finish_with_message("⚠ Key doesn't start with 'sk-' — may not work");
                }

                vault.set("llm.openai.api_key", &api_key)?;
            }
            "anthropic" => {
                let api_key: String = Password::new()
                    .with_prompt("Enter your Anthropic API key")
                    .interact()
                    .context("Failed to read API key")?;

                if api_key.is_empty() {
                    println!("  {} No key provided, skipping.", "⏭".dimmed());
                    return Ok(());
                }

                let spinner = self.spinner("Validating API key...");
                if api_key.starts_with("sk-ant-") {
                    spinner.finish_with_message("✅ API key format looks valid");
                } else {
                    spinner.finish_with_message(
                        "⚠ Key doesn't start with 'sk-ant-' — may not work",
                    );
                }

                vault.set("llm.anthropic.api_key", &api_key)?;
            }
            _ => {}
        }
        Ok(())
    }

    // ── Step 2: Agent Identity ──────────────────────────────────────

    fn step_agent_identity(&self) -> Result<(String, String, String)> {
        println!(
            "\n{}\n{}",
            "Step 2 of 5: Agent Identity".bright_blue().bold(),
            "─".repeat(40).dimmed()
        );

        let user_name: String = Input::new()
            .with_prompt("What should the agent call you?")
            .default("User".to_string())
            .interact_text()
            .context("Failed to read user name")?;

        let agent_name: String = Input::new()
            .with_prompt("Name your agent")
            .default("Jarvis".to_string())
            .interact_text()
            .context("Failed to read agent name")?;

        let persona_options = vec![
            "Professional & concise",
            "Friendly & conversational",
            "Technical & detailed",
            "Custom (enter description)",
        ];

        let persona_idx = Select::new()
            .with_prompt("Persona style")
            .items(&persona_options)
            .default(0)
            .interact()?;

        let persona = match persona_idx {
            0 => "You are a helpful, concise, and professional personal AI assistant.".to_string(),
            1 => "You are a friendly, warm, and conversational personal AI assistant that enjoys helping people.".to_string(),
            2 => "You are a technical, detailed, and thorough personal AI assistant focused on precision and clarity.".to_string(),
            3 => {
                let custom: String = Input::new()
                    .with_prompt("Enter custom persona description")
                    .interact_text()?;
                custom
            },
            _ => "You are a helpful, concise, and professional personal AI assistant.".to_string(),
        };

        println!(
            "  {} Agent '{}' for user '{}'",
            "✅".bright_green(),
            agent_name.bright_cyan(),
            user_name.bright_cyan()
        );

        Ok((agent_name, user_name, persona))
    }

    // ── Step 3: Integrations ────────────────────────────────────────

    async fn step_integrations(
        &self,
        vault: &mut SecretsVault,
    ) -> Result<Vec<String>> {
        println!(
            "\n{}\n{}",
            "Step 3 of 5: Integrations".bright_blue().bold(),
            "─".repeat(40).dimmed()
        );

        let integration_options = vec![
            "Google Calendar & Gmail",
            "Telegram",
            "WhatsApp (Twilio)",
            "GitHub",
            "Slack",
        ];

        let selections = MultiSelect::new()
            .with_prompt("Select integrations to set up (space to toggle)")
            .items(&integration_options)
            .interact()
            .context("Failed to get integration selections")?;

        let mut configured = Vec::new();

        for &idx in &selections {
            match idx {
                0 => {
                    println!(
                        "\n  {} Setting up Google Calendar & Gmail...",
                        "→".bright_blue()
                    );
                    match self.setup_google_calendar(vault).await {
                        Ok(_) => configured.push("google-calendar".to_string()),
                        Err(e) => println!(
                            "  {} Google setup failed: {}",
                            "⚠".bright_yellow(),
                            e
                        ),
                    }
                }
                1 => {
                    println!("\n  {} Setting up Telegram...", "→".bright_blue());
                    match self.setup_telegram(vault) {
                        Ok(_) => configured.push("telegram".to_string()),
                        Err(e) => println!(
                            "  {} Telegram setup failed: {}",
                            "⚠".bright_yellow(),
                            e
                        ),
                    }
                }
                2 => {
                    println!(
                        "\n  {} Setting up WhatsApp (Twilio)...",
                        "→".bright_blue()
                    );
                    match self.setup_whatsapp(vault) {
                        Ok(_) => configured.push("whatsapp".to_string()),
                        Err(e) => println!(
                            "  {} WhatsApp setup failed: {}",
                            "⚠".bright_yellow(),
                            e
                        ),
                    }
                }
                3 => {
                    println!("\n  {} Setting up GitHub...", "→".bright_blue());
                    match self.setup_github(vault) {
                        Ok(_) => configured.push("github".to_string()),
                        Err(e) => println!(
                            "  {} GitHub setup failed: {}",
                            "⚠".bright_yellow(),
                            e
                        ),
                    }
                }
                4 => {
                    println!("\n  {} Setting up Slack...", "→".bright_blue());
                    match self.setup_slack(vault) {
                        Ok(_) => configured.push("slack".to_string()),
                        Err(e) => println!(
                            "  {} Slack setup failed: {}",
                            "⚠".bright_yellow(),
                            e
                        ),
                    }
                }
                _ => {}
            }
        }

        if configured.is_empty() && selections.is_empty() {
            println!(
                "  {} Skipped. Configure later with: {}",
                "⏭".dimmed(),
                "gmv-agent init --only <service>".bright_green()
            );
        }

        Ok(configured)
    }

    // ── Step 4: Notifications ───────────────────────────────────────

    fn step_notifications(&self, integrations: &[String]) -> Result<Vec<String>> {
        println!(
            "\n{}\n{}",
            "Step 4 of 5: Notifications".bright_blue().bold(),
            "─".repeat(40).dimmed()
        );

        let notification_options = vec![
            "Calendar RSVP updates (accepted/declined)",
            "Meeting reminders (15 min before)",
            "Daily briefing (morning summary)",
            "Reminder alerts",
        ];

        let selections = MultiSelect::new()
            .with_prompt("Enable proactive notifications?")
            .items(&notification_options)
            .defaults(&[true, true, false, true])
            .interact()
            .context("Failed to get notification selections")?;

        let mut enabled = Vec::new();
        let names = [
            "rsvp_updates",
            "meeting_reminders",
            "daily_briefing",
            "reminder_alerts",
        ];

        for &idx in &selections {
            if idx < names.len() {
                enabled.push(names[idx].to_string());
            }
        }

        if !integrations.is_empty() {
            let has_telegram = integrations.iter().any(|i| i == "telegram");
            if has_telegram {
                println!(
                    "  {} Notifications will be sent via Telegram",
                    "✅".bright_green()
                );
            } else {
                println!(
                    "  {} Notifications will appear in the terminal",
                    "ℹ".bright_blue()
                );
            }
        }

        Ok(enabled)
    }

    // ── Step 5: Background Services ─────────────────────────────────

    fn step_background_services(&self) -> Result<BackgroundConfig> {
        println!(
            "\n{}\n{}",
            "Step 5 of 5: Background Services".bright_blue().bold(),
            "─".repeat(40).dimmed()
        );

        let scheduler_options = vec![
            "Yes — run as system service (launchd/systemd)",
            "Yes — run as background process",
            "No — manual only",
        ];

        let selection = Select::new()
            .with_prompt("Enable scheduler for background tasks?")
            .items(&scheduler_options)
            .default(2)
            .interact()?;

        let bg_config = match selection {
            0 => {
                println!(
                    "  {} System service will be configured on first 'gmv-agent serve'",
                    "ℹ".bright_blue()
                );
                BackgroundConfig {
                    scheduler_enabled: true,
                    system_service: true,
                    calendar_sync_cron: "*/5 * * * *".to_string(),
                    rsvp_check_cron: "*/10 * * * *".to_string(),
                    daily_briefing_cron: "0 8 * * *".to_string(),
                    reminder_check_cron: "* * * * *".to_string(),
                }
            }
            1 => BackgroundConfig {
                scheduler_enabled: true,
                system_service: false,
                calendar_sync_cron: "*/5 * * * *".to_string(),
                rsvp_check_cron: "*/10 * * * *".to_string(),
                daily_briefing_cron: "0 8 * * *".to_string(),
                reminder_check_cron: "* * * * *".to_string(),
            },
            _ => BackgroundConfig {
                scheduler_enabled: false,
                system_service: false,
                calendar_sync_cron: String::new(),
                rsvp_check_cron: String::new(),
                daily_briefing_cron: String::new(),
                reminder_check_cron: String::new(),
            },
        };

        Ok(bg_config)
    }

    // ── Integration setup helpers ───────────────────────────────────

    async fn setup_google_calendar(&self, vault: &mut SecretsVault) -> Result<()> {
        println!(
            "  {} Google Calendar requires OAuth2 credentials.",
            "ℹ".bright_blue()
        );
        println!(
            "  {} Create credentials at: {}",
            "→".dimmed(),
            "https://console.cloud.google.com/apis/credentials"
                .bright_blue()
                .underline()
        );

        let has_credentials = Confirm::new()
            .with_prompt("Do you have Google OAuth2 credentials?")
            .default(false)
            .interact()?;

        if !has_credentials {
            println!(
                "  {} Skip for now. Set up later with: {}",
                "⏭".dimmed(),
                "gmv-agent init --only google-calendar".bright_green()
            );
            return Ok(());
        }

        let client_id: String = Input::new()
            .with_prompt("  Google Client ID")
            .interact_text()?;

        let client_secret: String = Password::new()
            .with_prompt("  Google Client Secret")
            .interact()?;

        vault.set("google.client_id", &client_id)?;
        vault.set("google.client_secret", &client_secret)?;

        // Attempt OAuth flow
        let do_oauth = Confirm::new()
            .with_prompt("Open browser to authorize Google Calendar now?")
            .default(true)
            .interact()?;

        if do_oauth {
            let spinner = self.spinner("Opening browser for Google authorization...");
            match crate::tools::calendar::authorize_google(
                &client_id,
                &client_secret,
                8085,
                &self.data_dir,
            )
            .await
            {
                Ok(_) => {
                    spinner.finish_with_message("✅ Google Calendar connected");
                }
                Err(e) => {
                    spinner.finish_with_message(format!(
                        "⚠ OAuth flow failed: {}. You can retry later.",
                        e
                    ));
                }
            }
        }

        Ok(())
    }

    fn setup_telegram(&self, vault: &mut SecretsVault) -> Result<()> {
        println!(
            "  {} Telegram bots require a token from @BotFather.",
            "ℹ".bright_blue()
        );

        let has_bot = Confirm::new()
            .with_prompt("Do you already have a Telegram bot?")
            .default(false)
            .interact()?;

        if !has_bot {
            println!("  {} Steps to create a Telegram bot:", "→".bright_blue());
            println!("     1. Open Telegram and search for @BotFather");
            println!("     2. Send /newbot and follow the prompts");
            println!("     3. Copy the token BotFather gives you");
            println!();

            let open_telegram = Confirm::new()
                .with_prompt("Open Telegram BotFather?")
                .default(true)
                .interact()?;

            if open_telegram {
                let _ = open::that("https://t.me/BotFather");
            }
        }

        let token: String = Password::new()
            .with_prompt("Paste your bot token")
            .interact()?;

        if token.is_empty() {
            println!("  {} No token provided, skipping.", "⏭".dimmed());
            return Ok(());
        }

        vault.set("telegram.bot_token", &token)?;

        // Try to get chat ID
        let chat_id: String = Input::new()
            .with_prompt("Default chat ID (leave empty to skip)")
            .default(String::new())
            .interact_text()?;

        if !chat_id.is_empty() {
            vault.set("telegram.default_chat_id", &chat_id)?;
        }

        println!("  {} Telegram bot configured", "✅".bright_green());
        Ok(())
    }

    fn setup_whatsapp(&self, vault: &mut SecretsVault) -> Result<()> {
        println!(
            "  {} WhatsApp uses Twilio's API.",
            "ℹ".bright_blue()
        );
        println!(
            "  {} Get credentials at: {}",
            "→".dimmed(),
            "https://www.twilio.com/console"
                .bright_blue()
                .underline()
        );

        let account_sid: String = Input::new()
            .with_prompt("Twilio Account SID")
            .interact_text()?;

        if account_sid.is_empty() {
            println!("  {} Skipping WhatsApp setup.", "⏭".dimmed());
            return Ok(());
        }

        let auth_token: String = Password::new()
            .with_prompt("Twilio Auth Token")
            .interact()?;

        let from_number: String = Input::new()
            .with_prompt("WhatsApp From number (e.g., whatsapp:+14155238886)")
            .default("whatsapp:+14155238886".to_string())
            .interact_text()?;

        vault.set("twilio.account_sid", &account_sid)?;
        vault.set("twilio.auth_token", &auth_token)?;
        vault.set("twilio.whatsapp_from", &from_number)?;

        println!("  {} WhatsApp configured", "✅".bright_green());
        Ok(())
    }

    fn setup_github(&self, vault: &mut SecretsVault) -> Result<()> {
        println!(
            "  {} GitHub requires a personal access token.",
            "ℹ".bright_blue()
        );
        println!(
            "  {} Create one at: {}",
            "→".dimmed(),
            "https://github.com/settings/tokens"
                .bright_blue()
                .underline()
        );

        let token: String = Password::new()
            .with_prompt("GitHub access token")
            .interact()?;

        if token.is_empty() {
            println!("  {} Skipping GitHub setup.", "⏭".dimmed());
            return Ok(());
        }

        vault.set("github.access_token", &token)?;
        println!("  {} GitHub configured", "✅".bright_green());
        Ok(())
    }

    fn setup_slack(&self, vault: &mut SecretsVault) -> Result<()> {
        println!(
            "  {} Slack requires a Bot Token from your Slack app.",
            "ℹ".bright_blue()
        );
        println!(
            "  {} Create an app at: {}",
            "→".dimmed(),
            "https://api.slack.com/apps"
                .bright_blue()
                .underline()
        );

        let bot_token: String = Password::new()
            .with_prompt("Slack Bot Token (xoxb-...)")
            .interact()?;

        if bot_token.is_empty() {
            println!("  {} Skipping Slack setup.", "⏭".dimmed());
            return Ok(());
        }

        vault.set("slack.bot_token", &bot_token)?;

        let app_token: String = Password::new()
            .with_prompt("Slack App Token (xapp-..., optional)")
            .interact()?;

        if !app_token.is_empty() {
            vault.set("slack.app_token", &app_token)?;
        }

        println!("  {} Slack configured", "✅".bright_green());
        Ok(())
    }

    // ── Config file generation ──────────────────────────────────────

    fn write_config(
        &self,
        provider: &str,
        model: &str,
        agent_name: &str,
        _user_name: &str,
        persona: &str,
        integrations: &[String],
        _notifications: &[String],
        bg_config: &BackgroundConfig,
    ) -> Result<()> {
        let google_enabled = integrations.iter().any(|i| i == "google-calendar");
        let telegram_enabled = integrations.iter().any(|i| i == "telegram");
        let whatsapp_enabled = integrations.iter().any(|i| i == "whatsapp");

        let config = format!(
            r#"# GMV Agent Configuration
# Generated by: gmv-agent init
# Date: {date}

[agent]
name = "{agent_name}"
persona = "{persona}"
max_context_messages = 50
max_tool_iterations = 15

[llm]
provider = "{provider}"
model = "{model}"
max_tokens = 4096
temperature = 0.7

[storage]
data_dir = "{data_dir}"

[google_calendar]
enabled = {google_enabled}
redirect_port = 8085
scopes = ["https://www.googleapis.com/auth/calendar"]

[telegram]
enabled = {telegram_enabled}

[whatsapp]
enabled = {whatsapp_enabled}

[scheduler]
enabled = {scheduler_enabled}
calendar_sync_cron = "{calendar_sync}"
rsvp_check_cron = "{rsvp_check}"
daily_briefing_cron = "{daily_briefing}"
reminder_check_cron = "{reminder_check}"
"#,
            date = chrono::Utc::now().format("%Y-%m-%d"),
            agent_name = agent_name,
            persona = persona.replace('"', "\\\""),
            provider = provider,
            model = model,
            data_dir = self.data_dir.display(),
            google_enabled = google_enabled,
            telegram_enabled = telegram_enabled,
            whatsapp_enabled = whatsapp_enabled,
            scheduler_enabled = bg_config.scheduler_enabled,
            calendar_sync = bg_config.calendar_sync_cron,
            rsvp_check = bg_config.rsvp_check_cron,
            daily_briefing = bg_config.daily_briefing_cron,
            reminder_check = bg_config.reminder_check_cron,
        );

        if let Some(parent) = self.config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.config_path, config)
            .with_context(|| format!("Failed to write config to {}", self.config_path.display()))?;

        println!(
            "  {} Config written to {}",
            "✅".bright_green(),
            self.config_path.display()
        );

        Ok(())
    }

    // ── Summary ─────────────────────────────────────────────────────

    fn print_summary(
        &self,
        provider: &str,
        model: &str,
        agent_name: &str,
        user_name: &str,
        integrations: &[String],
        notifications: &[String],
        bg_config: &BackgroundConfig,
    ) {
        println!();
        println!(
            "{}",
            "┌─────────────────────────────────────────┐".bright_green()
        );
        println!(
            "{}",
            "│ Configuration Summary                   │".bright_green()
        );
        println!(
            "{}",
            "├─────────────────────────────────────────┤".bright_green()
        );
        println!(
            "│ LLM: {} ({}){}│",
            provider.bright_cyan(),
            model.bright_cyan(),
            " ".repeat(38usize.saturating_sub(provider.len() + model.len() + 8))
        );
        println!(
            "│ Agent: {}{}│",
            agent_name.bright_cyan(),
            " ".repeat(33usize.saturating_sub(agent_name.len()))
        );
        println!(
            "│ User: {}{}│",
            user_name.bright_cyan(),
            " ".repeat(34usize.saturating_sub(user_name.len()))
        );
        println!("│                                         │");
        println!("│ Integrations:                           │");
        for int in integrations {
            println!(
                "│   ✅ {}{}│",
                int.bright_cyan(),
                " ".repeat(36usize.saturating_sub(int.len() + 2))
            );
        }
        if integrations.is_empty() {
            println!("│   (none configured)                     │");
        }

        if bg_config.scheduler_enabled {
            println!("│                                         │");
            println!("│ Background:                             │");
            println!("│   ✅ Scheduler enabled                  │");
            for notif in notifications {
                println!(
                    "│   ✅ {}{}│",
                    notif.replace('_', " ").bright_cyan(),
                    " ".repeat(36usize.saturating_sub(notif.len() + 2))
                );
            }
        }

        println!("│                                         │");
        println!(
            "│ Secrets: {}│",
            format!("{}", self.secrets_path.display())
                + &" ".repeat(30usize.saturating_sub(self.secrets_path.display().to_string().len()))
        );
        println!(
            "│ Config:  {}│",
            format!("{}", self.config_path.display())
                + &" ".repeat(30usize.saturating_sub(self.config_path.display().to_string().len()))
        );
        println!(
            "{}",
            "├─────────────────────────────────────────┤".bright_green()
        );
        println!(
            "{}",
            "│ ✅ Setup complete!                      │".bright_green()
        );
        println!("│                                         │");
        println!("│ Start the agent:                        │");
        println!(
            "│   $ {}{}│",
            "gmv-agent".bright_green(),
            " ".repeat(23)
        );
        println!(
            "│   $ {}{}│",
            "gmv-agent telegram-bot".bright_green(),
            " ".repeat(10)
        );
        println!(
            "│   $ {}{}│",
            "gmv-agent serve".bright_green(),
            " ".repeat(17)
        );
        println!(
            "{}",
            "└─────────────────────────────────────────┘".bright_green()
        );
    }

    // ── Helpers ─────────────────────────────────────────────────────

    fn setup_directories(&self) -> Result<()> {
        let home = gmv_home_dir();
        std::fs::create_dir_all(&home)?;
        std::fs::create_dir_all(home.join("data"))?;
        std::fs::create_dir_all(home.join("logs"))?;
        std::fs::create_dir_all(home.join("plugins"))?;
        Ok(())
    }

    fn reset_config(&self) -> Result<()> {
        let confirm = Confirm::new()
            .with_prompt("This will reset ALL configuration. Are you sure?")
            .default(false)
            .interact()?;

        if confirm {
            if self.secrets_path.exists() {
                std::fs::remove_file(&self.secrets_path)?;
            }
            if self.config_path.exists() {
                std::fs::remove_file(&self.config_path)?;
            }
            println!(
                "  {} Configuration reset",
                "✅".bright_green()
            );
        }
        Ok(())
    }

    fn spinner(&self, msg: &str) -> ProgressBar {
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("  {spinner:.cyan} {msg}")
                .unwrap(),
        );
        pb.set_message(msg.to_string());
        pb.enable_steady_tick(Duration::from_millis(100));
        pb
    }
}

/// Background scheduler configuration.
#[derive(Debug, Clone)]
pub struct BackgroundConfig {
    pub scheduler_enabled: bool,
    pub system_service: bool,
    pub calendar_sync_cron: String,
    pub rsvp_check_cron: String,
    pub daily_briefing_cron: String,
    pub reminder_check_cron: String,
}

/// Run the `gmv-agent doctor` diagnostic command.
pub fn run_doctor() -> Result<()> {
    println!(
        "{}",
        "🩺 GMV Agent Doctor — Checking configuration...".bright_blue().bold()
    );
    println!("{}", "─".repeat(50).dimmed());

    let home = gmv_home_dir();

    // Check directories
    check_item(
        "Data directory",
        home.join("data").exists(),
        &format!("{}", home.join("data").display()),
    );
    check_item(
        "Logs directory",
        home.join("logs").exists(),
        &format!("{}", home.join("logs").display()),
    );

    // Check secrets vault
    let secrets_path = home.join("secrets.enc");
    let vault_ok = if secrets_path.exists() {
        match SecretsVault::open(&secrets_path, None) {
            Ok(_) => true,
            Err(_) => false,
        }
    } else {
        false
    };
    check_item("Secrets vault", vault_ok, &format!("{}", secrets_path.display()));

    // Check config
    let config_path = home.join("config.toml");
    check_item(
        "Config file",
        config_path.exists(),
        &format!("{}", config_path.display()),
    );

    // Check LLM configuration
    if let Ok(vault) = SecretsVault::open(&secrets_path, None) {
        check_item(
            "LLM provider",
            vault.has_llm_configured(),
            "API key configured",
        );
        check_item(
            "Telegram",
            vault.get("telegram.bot_token").is_some(),
            "Bot token configured",
        );
        check_item(
            "Google Calendar",
            vault.get("google.client_id").is_some(),
            "OAuth credentials configured",
        );
    }

    // Check for .env fallback
    let env_path = std::path::PathBuf::from(".env");
    if env_path.exists() {
        println!(
            "\n  {} Legacy .env file detected. Consider migrating with: {}",
            "⚠".bright_yellow(),
            "gmv-agent init".bright_green()
        );
    }

    println!("\n{}", "─".repeat(50).dimmed());
    println!(
        "{}",
        "Done! Fix any ❌ issues above by running: gmv-agent init".dimmed()
    );

    Ok(())
}

fn check_item(name: &str, ok: bool, detail: &str) {
    let symbol = if ok {
        "✅".to_string()
    } else {
        "❌".to_string()
    };
    let status = if ok { "OK" } else { "Missing" };
    println!(
        "  {} {}: {} ({})",
        symbol,
        name,
        status.to_string().bright_white(),
        detail.dimmed()
    );
}

/// Show agent status and connected services.
pub fn run_status() -> Result<()> {
    println!(
        "{}",
        "📊 GMV Agent Status".bright_blue().bold()
    );
    println!("{}", "─".repeat(40).dimmed());

    let home = gmv_home_dir();
    let secrets_path = home.join("secrets.enc");

    if let Ok(vault) = SecretsVault::open(&secrets_path, None) {
        let data = vault.data();

        println!(" LLM Providers:");
        if data.llm.openai.is_some() {
            println!("   ✅ OpenAI — configured");
        }
        if data.llm.anthropic.is_some() {
            println!("   ✅ Anthropic — configured");
        }
        if data.llm.openai.is_none() && data.llm.anthropic.is_none() {
            println!("   ❌ No LLM provider configured");
        }

        println!(" Integrations:");
        if data.google.client_id.is_some() {
            println!("   ✅ Google Calendar & Gmail");
        }
        if data.telegram.bot_token.is_some() {
            println!("   ✅ Telegram");
        }
        if data.twilio.account_sid.is_some() {
            println!("   ✅ WhatsApp (Twilio)");
        }
        if data.github.access_token.is_some() {
            println!("   ✅ GitHub");
        }
        if data.slack.bot_token.is_some() {
            println!("   ✅ Slack");
        }
    } else {
        println!(
            "  {} No secrets vault found. Run: {}",
            "⚠".bright_yellow(),
            "gmv-agent init".bright_green()
        );
    }

    Ok(())
}
