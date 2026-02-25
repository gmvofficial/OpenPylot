use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::{json, Value};

use crate::tools::{Tool, ToolDefinition, ToolResult};

// ════════════════════════════════════════════════════════════════════
//  SendTelegramMessage
// ════════════════════════════════════════════════════════════════════

pub struct SendTelegramMessage {
    bot_token: String,
    default_chat_id: Option<String>,
    client: Client,
}

impl SendTelegramMessage {
    pub fn new(bot_token: String, default_chat_id: Option<String>) -> Self {
        Self {
            bot_token,
            default_chat_id,
            client: Client::new(),
        }
    }
}

#[async_trait]
impl Tool for SendTelegramMessage {
    fn definition(&self) -> ToolDefinition {
        let mut desc =
            "Send a message via Telegram Bot API.".to_string();
        if self.default_chat_id.is_some() {
            desc.push_str(" A default chat ID is configured, so chat_id is optional.");
        }

        ToolDefinition {
            name: "send_telegram_message".into(),
            description: desc,
            parameters: json!({
                "type": "object",
                "properties": {
                    "chat_id": {
                        "type": "string",
                        "description": "Telegram chat ID or @username to send the message to"
                    },
                    "message": {
                        "type": "string",
                        "description": "Message text to send (supports Markdown)"
                    },
                    "parse_mode": {
                        "type": "string",
                        "enum": ["Markdown", "HTML", "MarkdownV2"],
                        "description": "Message formatting mode (optional, default: Markdown)"
                    }
                },
                "required": ["message"]
            }),
        }
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let message = params["message"]
            .as_str()
            .context("Missing 'message' parameter")?;

        let chat_id = params["chat_id"]
            .as_str()
            .map(String::from)
            .or_else(|| self.default_chat_id.clone())
            .context(
                "No chat_id provided and no default chat ID configured. \
                 Provide a chat_id parameter or set TELEGRAM_DEFAULT_CHAT_ID.",
            )?;

        let parse_mode = params["parse_mode"]
            .as_str()
            .unwrap_or("Markdown");

        let url = format!(
            "https://api.telegram.org/bot{}/sendMessage",
            self.bot_token
        );

        let body = json!({
            "chat_id": chat_id,
            "text": message,
            "parse_mode": parse_mode,
        });

        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .context("Failed to send Telegram message")?;

        let status = resp.status();
        let resp_body: Value = resp.json().await?;

        if !status.is_success() || resp_body["ok"].as_bool() != Some(true) {
            let error_desc = resp_body["description"]
                .as_str()
                .unwrap_or("Unknown error");
            return Ok(ToolResult::err(format!(
                "Failed to send Telegram message: {}",
                error_desc
            )));
        }

        let message_id = resp_body["result"]["message_id"]
            .as_i64()
            .unwrap_or_default();

        Ok(ToolResult::ok(format!(
            "Telegram message sent successfully!\nChat: {}\nMessage ID: {}",
            chat_id, message_id
        )))
    }
}

// ════════════════════════════════════════════════════════════════════
//  GetTelegramUpdates — helper to find chat IDs
// ════════════════════════════════════════════════════════════════════

pub struct GetTelegramUpdates {
    bot_token: String,
    client: Client,
}

impl GetTelegramUpdates {
    pub fn new(bot_token: String) -> Self {
        Self {
            bot_token,
            client: Client::new(),
        }
    }
}

#[async_trait]
impl Tool for GetTelegramUpdates {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "get_telegram_updates".into(),
            description:
                "Get recent messages/updates from the Telegram bot to find chat IDs and see incoming messages."
                    .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "limit": {
                        "type": "integer",
                        "description": "Number of updates to fetch (default: 10)"
                    }
                }
            }),
        }
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let limit = params["limit"].as_u64().unwrap_or(10);

        let url = format!(
            "https://api.telegram.org/bot{}/getUpdates?limit={}",
            self.bot_token, limit
        );

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to get Telegram updates")?;

        let body: Value = resp.json().await?;

        if body["ok"].as_bool() != Some(true) {
            return Ok(ToolResult::err("Failed to get Telegram updates."));
        }

        let updates = match body["result"].as_array() {
            Some(arr) if !arr.is_empty() => arr,
            _ => return Ok(ToolResult::ok("No recent updates/messages.")),
        };

        let mut output = format!("Recent Telegram updates ({}):\n\n", updates.len());
        for update in updates {
            if let Some(msg) = update.get("message") {
                let chat_id = msg["chat"]["id"].as_i64().unwrap_or_default();
                let chat_name = msg["chat"]["first_name"]
                    .as_str()
                    .or_else(|| msg["chat"]["title"].as_str())
                    .unwrap_or("Unknown");
                let text = msg["text"].as_str().unwrap_or("(non-text message)");
                let date = msg["date"].as_i64().unwrap_or_default();

                output.push_str(&format!(
                    "- Chat: {} (ID: {})\n  Message: {}\n  Date: {}\n\n",
                    chat_name, chat_id, text, date
                ));
            }
        }

        Ok(ToolResult::ok(output))
    }
}
