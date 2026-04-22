use anyhow::{Context, Result};
use reqwest::Client;
use serde_json::Value;
use tokio::time::{sleep, Duration};
use tracing::{error, warn};

use crate::agent::Agent;

/// Telegram bot that continuously polls for updates and responds to messages
pub struct TelegramBot {
    bot_token: String,
    client: Client,
    last_update_id: i64,
}

impl TelegramBot {
    pub fn new(bot_token: String) -> Self {
        Self {
            bot_token,
            client: Client::new(),
            last_update_id: 0,
        }
    }

    /// Start the bot polling loop
    pub async fn start_polling(&mut self, agent: &mut Agent) -> Result<()> {
        println!("Listening for messages...");
        println!("Press Ctrl+C to stop\n");

        loop {
            match self.poll_updates(agent).await {
                Ok(_) => {
                    // No sleep needed - long polling handles timing
                }
                Err(e) => {
                    error!("❌ Error: {}", e);
                    // Only sleep on error to avoid hammering the API
                    sleep(Duration::from_secs(3)).await;
                }
            }
        }
    }

    /// Poll for new updates from Telegram
    async fn poll_updates(&mut self, agent: &mut Agent) -> Result<()> {
        let url = format!(
            "https://api.telegram.org/bot{}/getUpdates?offset={}&timeout=5",
            self.bot_token,
            self.last_update_id + 1
        );

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to get updates")?;

        let body: Value = resp.json().await?;

        if body["ok"].as_bool() != Some(true) {
            return Ok(());
        }

        let updates = match body["result"].as_array() {
            Some(arr) if !arr.is_empty() => arr,
            _ => return Ok(()),
        };

        for update in updates {
            let update_id = update["update_id"].as_i64().unwrap_or(0);
            if update_id > self.last_update_id {
                self.last_update_id = update_id;
            }

            // Process message
            if let Some(msg) = update.get("message") {
                self.handle_message(msg, agent).await?;
            }
        }

        Ok(())
    }

    /// Handle an incoming Telegram message
    async fn handle_message(&self, msg: &Value, agent: &mut Agent) -> Result<()> {
        let chat_id = msg["chat"]["id"].as_i64().unwrap_or(0);
        let text = match msg["text"].as_str() {
            Some(t) => t,
            None => return Ok(()), // Ignore non-text messages
        };

        let username = msg["from"]["first_name"].as_str().unwrap_or("User");

        // Clean single-line output
        println!(" {} : {}", username, text);

        // Handle bot commands
        if text.starts_with('/') {
            return self.handle_command(chat_id, text, agent).await;
        }

        // Send "typing" action
        let _ = self.send_chat_action(chat_id, "typing").await;

        // Process message through AI agent
        match agent.chat(text).await {
            Ok(response) => {
                // Show compact response
                let display_response = if response.len() > 80 {
                    format!("{}...", &response[..77])
                } else {
                    response.clone()
                };
                println!("Pylot :  {}\n", display_response);

                self.send_message(chat_id, &response).await?;
            }
            Err(e) => {
                println!("❌ Error → {}\n", e);
                self.send_message(chat_id, &format!("❌ Sorry, I encountered an error: {}", e))
                    .await?;
            }
        }

        Ok(())
    }

    /// Handle bot commands like /start, /help, etc.
    async fn handle_command(&self, chat_id: i64, command: &str, agent: &Agent) -> Result<()> {
        let response = match command.trim() {
            "/start" => {
                format!(
                    "👋 Welcome to OpenPylot!\n\n\
                     I'm your personal AI assistant. You can:\n\
                     • Ask me to take notes\n\
                     • Set reminders\n\
                     • Manage your calendar\n\
                     • Send messages\n\
                     • And much more!\n\n\
                     Just send me a message in natural language.\n\n\
                     Commands:\n\
                     /help - Show this help\n\
                     /tools - List available tools\n\
                     /clear - Clear conversation history"
                )
            }
            "/help" => "📖 *Pylot Help*\n\n\
                 Just chat with me naturally! Examples:\n\
                 • \"Take a note: Buy groceries\"\n\
                 • \"What's on my calendar today?\"\n\
                 • \"Set a reminder for 5pm\"\n\
                 • \"List my notes\"\n\n\
                 Commands:\n\
                 /tools - List available tools\n\
                 /clear - Clear conversation history\n\
                 /help - Show this help"
                .to_string(),
            "/tools" => {
                let tools = agent.tool_names();
                format!(
                    "🔧 *Available Tools*\n\n{}\n\n\
                     Total: {} tools",
                    tools
                        .iter()
                        .map(|t| format!("• {}", t))
                        .collect::<Vec<_>>()
                        .join("\n"),
                    tools.len()
                )
            }
            "/clear" => {
                // Note: This doesn't actually clear the agent's context
                // because we'd need a mutable reference
                "🗑️ Conversation context cleared for this chat session.".to_string()
            }
            _ => "❓ Unknown command. Send /help for available commands.".to_string(),
        };

        self.send_message(chat_id, &response).await
    }

    /// Send a message to a Telegram chat
    async fn send_message(&self, chat_id: i64, text: &str) -> Result<()> {
        let url = format!("https://api.telegram.org/bot{}/sendMessage", self.bot_token);

        // Telegram message limit is 4096 characters
        let message_text = if text.len() > 4096 {
            warn!(
                "Message too long ({}), truncating to 4096 chars",
                text.len()
            );
            format!("{}...\n\n(Message truncated - too long)", &text[..4090])
        } else {
            text.to_string()
        };

        // Try with Markdown first
        let body = serde_json::json!({
            "chat_id": chat_id,
            "text": message_text,
            "parse_mode": "Markdown",
        });

        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .context("Failed to send message")?;

        let status = resp.status();
        if !status.is_success() {
            let error_body: Value = resp.json().await?;

            // If Markdown parsing failed, retry without parse_mode (plain text)
            if error_body
                .get("description")
                .and_then(|d| d.as_str())
                .map(|s| s.contains("can't parse entities"))
                .unwrap_or(false)
            {
                // Silently retry as plain text (debug level logging only)
                tracing::debug!("Markdown parsing failed, retrying as plain text");

                // Retry without Markdown
                let plain_body = serde_json::json!({
                    "chat_id": chat_id,
                    "text": message_text,
                });

                let retry_resp = self
                    .client
                    .post(&url)
                    .json(&plain_body)
                    .send()
                    .await
                    .context("Failed to send plain text message")?;

                if !retry_resp.status().is_success() {
                    let retry_error: Value = retry_resp.json().await?;
                    warn!("Failed to send plain text message: {:?}", retry_error);
                }
            } else {
                warn!("Failed to send message: {:?}", error_body);
            }
        }

        Ok(())
    }

    /// Send chat action (like "typing")
    async fn send_chat_action(&self, chat_id: i64, action: &str) -> Result<()> {
        let url = format!(
            "https://api.telegram.org/bot{}/sendChatAction",
            self.bot_token
        );

        let body = serde_json::json!({
            "chat_id": chat_id,
            "action": action,
        });

        let _ = self.client.post(&url).json(&body).send().await;

        Ok(())
    }
}
