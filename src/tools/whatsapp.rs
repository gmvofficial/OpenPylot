use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::{json, Value};

use crate::tools::{Tool, ToolDefinition, ToolResult};

// ════════════════════════════════════════════════════════════════════
//  SendWhatsAppMessage (via Twilio API)
// ════════════════════════════════════════════════════════════════════

pub struct SendWhatsAppMessage {
    account_sid: String,
    auth_token: String,
    from_number: String,
    client: Client,
}

impl SendWhatsAppMessage {
    pub fn new(account_sid: String, auth_token: String, from_number: String) -> Self {
        Self {
            account_sid,
            auth_token,
            from_number,
            client: Client::new(),
        }
    }
}

#[async_trait]
impl Tool for SendWhatsAppMessage {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "send_whatsapp_message".into(),
            description: "Send a WhatsApp message via Twilio API.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "to": {
                        "type": "string",
                        "description": "Recipient phone number in E.164 format (e.g., +1234567890). Will be prefixed with 'whatsapp:' automatically."
                    },
                    "message": {
                        "type": "string",
                        "description": "Message text to send"
                    }
                },
                "required": ["to", "message"]
            }),
        }
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let to_raw = params["to"]
            .as_str()
            .context("Missing 'to' parameter")?;
        let message = params["message"]
            .as_str()
            .context("Missing 'message' parameter")?;

        // Ensure whatsapp: prefix
        let to = if to_raw.starts_with("whatsapp:") {
            to_raw.to_string()
        } else {
            format!("whatsapp:{}", to_raw)
        };

        let from = if self.from_number.starts_with("whatsapp:") {
            self.from_number.clone()
        } else {
            format!("whatsapp:{}", self.from_number)
        };

        let url = format!(
            "https://api.twilio.com/2010-04-01/Accounts/{}/Messages.json",
            self.account_sid
        );

        let resp = self
            .client
            .post(&url)
            .basic_auth(&self.account_sid, Some(&self.auth_token))
            .form(&[
                ("From", from.as_str()),
                ("To", to.as_str()),
                ("Body", message),
            ])
            .send()
            .await
            .context("Failed to send WhatsApp message via Twilio")?;

        let status = resp.status();
        let body: Value = resp.json().await?;

        if !status.is_success() {
            let error_msg = body["message"]
                .as_str()
                .unwrap_or("Unknown error");
            return Ok(ToolResult::err(format!(
                "Failed to send WhatsApp message: {} (Status: {})",
                error_msg, status
            )));
        }

        let sid = body["sid"].as_str().unwrap_or("unknown");
        let msg_status = body["status"].as_str().unwrap_or("unknown");

        Ok(ToolResult::ok(format!(
            "WhatsApp message sent successfully!\nTo: {}\nStatus: {}\nSID: {}",
            to, msg_status, sid
        )))
    }
}
