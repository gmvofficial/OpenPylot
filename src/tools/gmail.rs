use anyhow::{Context, Result};
use async_trait::async_trait;
use base64::{engine::general_purpose::URL_SAFE, Engine};
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use crate::tools::{Tool, ToolDefinition, ToolResult};

/// Strip Markdown formatting from text for plain text emails
fn strip_markdown(text: &str) -> String {
    text
        // Remove bold/italic markers
        .replace("**", "")
        .replace("__", "")
        .replace("*", "")
        .replace("_", "")
        // Remove inline code
        .replace("`", "")
        // Remove headers
        .lines()
        .map(|line| {
            if line.starts_with("# ") {
                line.trim_start_matches("# ")
            } else if line.starts_with("## ") {
                line.trim_start_matches("## ")
            } else if line.starts_with("### ") {
                line.trim_start_matches("### ")
            } else if line.starts_with("#### ") {
                line.trim_start_matches("#### ")
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GmailTokens {
    access_token: String,
    refresh_token: String,
    expires_at: DateTime<Utc>,
}

fn gmail_tokens_path(data_dir: &PathBuf) -> PathBuf {
    data_dir.join("gmail_tokens.json")
}

fn load_gmail_tokens(data_dir: &PathBuf) -> Option<GmailTokens> {
    let path = gmail_tokens_path(data_dir);
    if !path.exists() {
        return None;
    }
    let content = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

fn save_gmail_tokens(data_dir: &PathBuf, tokens: &GmailTokens) -> Result<()> {
    let path = gmail_tokens_path(data_dir);
    let content = serde_json::to_string_pretty(tokens)?;
    std::fs::write(&path, content)?;
    Ok(())
}

pub async fn authorize_gmail(
    client_id: &str,
    client_secret: &str,
    redirect_port: u16,
    data_dir: &PathBuf,
) -> Result<()> {
    let redirect_uri = format!("http://localhost:{}", redirect_port);
    let scope = "https://www.googleapis.com/auth/gmail.modify";

    let auth_url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope={}&access_type=offline&prompt=consent",
        urlencoding::encode(client_id),
        urlencoding::encode(&redirect_uri),
        urlencoding::encode(scope),
    );

    println!("\nOpening browser for Gmail authorization...");
    println!("If the browser doesn't open, visit this URL:\n");
    println!("   {}\n", auth_url);

    let _ = open::that(&auth_url);

    let listener = TcpListener::bind(format!("127.0.0.1:{}", redirect_port))
        .await
        .with_context(|| format!("Failed to bind to port {}", redirect_port))?;

    println!(
        "Waiting for authorization callback on port {}...",
        redirect_port
    );

    let (mut socket, _) = listener.accept().await?;
    let mut buf = vec![0u8; 4096];
    let n = socket.read(&mut buf).await?;
    let request = String::from_utf8_lossy(&buf[..n]);

    let code = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|path| {
            path.split('?')
                .nth(1)?
                .split('&')
                .find(|param| param.starts_with("code="))?
                .strip_prefix("code=")
        })
        .context("Failed to extract authorization code")?;

    let response = "HTTP/1.1 200 OK\r\n\r\n<html><body><h1>Authorization successful!</h1><p>You can close this window and return to the terminal.</p></body></html>";
    socket.write_all(response.as_bytes()).await?;

    let client = Client::new();
    let token_resp = client
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("code", code),
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("redirect_uri", &redirect_uri),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .await?;

    let token_body: Value = token_resp.json().await?;

    let access_token = token_body
        .get("access_token")
        .and_then(|v| v.as_str())
        .context("Failed to get access_token")?
        .to_string();

    let refresh_token = token_body
        .get("refresh_token")
        .and_then(|v| v.as_str())
        .context("Failed to get refresh_token")?
        .to_string();

    let expires_in = token_body
        .get("expires_in")
        .and_then(|v| v.as_i64())
        .unwrap_or(3600);

    let tokens = GmailTokens {
        access_token,
        refresh_token,
        expires_at: Utc::now() + chrono::Duration::seconds(expires_in),
    };

    save_gmail_tokens(data_dir, &tokens)?;
    save_gmail_tokens_to_vault(&tokens);

    Ok(())
}

/// Load Gmail tokens from the encrypted vault as a fallback.
fn load_gmail_tokens_from_vault() -> Option<GmailTokens> {
    let vault_path = crate::secrets::default_secrets_path();
    let vault = crate::secrets::SecretsVault::open(&vault_path, None).ok()?;
    let access_token = vault.get("google.access_token")?;
    let refresh_token = vault.get("google.refresh_token")?;
    let expires_at = vault
        .get("google.token_expiry")
        .and_then(|s| s.parse::<DateTime<Utc>>().ok())
        .unwrap_or_else(|| Utc::now() - chrono::Duration::seconds(1));
    Some(GmailTokens {
        access_token,
        refresh_token,
        expires_at,
    })
}

/// Persist Gmail tokens back to the encrypted vault.
fn save_gmail_tokens_to_vault(tokens: &GmailTokens) {
    let vault_path = crate::secrets::default_secrets_path();
    if let Ok(mut vault) = crate::secrets::SecretsVault::open(&vault_path, None) {
        let _ = vault.set("google.access_token", &tokens.access_token);
        let _ = vault.set("google.refresh_token", &tokens.refresh_token);
        let _ = vault.set("google.token_expiry", &tokens.expires_at.to_rfc3339());
        let _ = vault.save();
    }
}

async fn get_gmail_access_token(
    client_id: &str,
    client_secret: &str,
    data_dir: &PathBuf,
) -> Result<String> {
    // Try loading from the JSON file first, then fall back to the vault
    let mut tokens = match load_gmail_tokens(data_dir) {
        Some(t) => t,
        None => load_gmail_tokens_from_vault().context(
            "Gmail not authorized. Connect Gmail from the Integrations page or run 'pylot setup gmail'.",
        )?,
    };

    if Utc::now() < tokens.expires_at {
        return Ok(tokens.access_token);
    }

    let client = Client::new();
    let resp = client
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("refresh_token", &tokens.refresh_token),
            ("grant_type", "refresh_token"),
        ])
        .send()
        .await?;

    let status = resp.status();
    let body: Value = resp.json().await?;

    if !status.is_success() {
        let err_desc = body
            .get("error_description")
            .and_then(|v| v.as_str())
            .or_else(|| body.get("error").and_then(|v| v.as_str()))
            .unwrap_or("unknown error");
        tracing::error!("Gmail token refresh failed ({}): {}", status, err_desc);
        anyhow::bail!("Failed to refresh Gmail token (HTTP {}): {}", status.as_u16(), err_desc);
    }

    let new_access_token = body
        .get("access_token")
        .and_then(|v| v.as_str())
        .context("Failed to refresh access token")?
        .to_string();

    let expires_in = body
        .get("expires_in")
        .and_then(|v| v.as_i64())
        .unwrap_or(3600);
    tokens.access_token = new_access_token.clone();
    tokens.expires_at = Utc::now() + chrono::Duration::seconds(expires_in);

    save_gmail_tokens(data_dir, &tokens)?;
    save_gmail_tokens_to_vault(&tokens);

    Ok(new_access_token)
}

#[derive(Debug, Clone)]
pub struct GmailConfig {
    pub data_dir: PathBuf,
    pub client_id: String,
    pub client_secret: String,
}

impl GmailConfig {
    async fn get_token(&self) -> Result<String> {
        get_gmail_access_token(&self.client_id, &self.client_secret, &self.data_dir).await
    }
}

pub struct GmailSearchTool {
    config: GmailConfig,
}

impl GmailSearchTool {
    pub fn new(config: GmailConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl Tool for GmailSearchTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "gmail_search".to_string(),
            description: "Search Gmail emails using Gmail query syntax".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Gmail search query"
                    },
                    "max_results": {
                        "type": "integer",
                        "description": "Maximum number of results (default: 10)",
                        "default": 10
                    }
                },
                "required": ["query"]
            }),
        }
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let query = args["query"].as_str().unwrap_or("newer_than:1d");
        let max_results = args["max_results"].as_u64().unwrap_or(10);

        let token = self.config.get_token().await?;
        let client = Client::new();

        let url = format!(
            "https://gmail.googleapis.com/gmail/v1/users/me/messages?q={}&maxResults={}",
            urlencoding::encode(query),
            max_results
        );

        let resp = client.get(&url).bearer_auth(&token).send().await?;
        let body: Value = resp.json().await?;

        if let Some(messages) = body.get("messages").and_then(|v| v.as_array()) {
            if messages.is_empty() {
                return Ok(ToolResult::ok("No emails found matching that query."));
            }

            let mut output = format!("Found {} email(s):\n\n", messages.len());

            for (i, msg) in messages.iter().enumerate() {
                if let Some(id) = msg.get("id").and_then(|v| v.as_str()) {
                    let msg_url = format!(
                        "https://gmail.googleapis.com/gmail/v1/users/me/messages/{}?format=metadata",
                        id
                    );

                    if let Ok(msg_resp) = client.get(&msg_url).bearer_auth(&token).send().await {
                        if let Ok(msg_body) = msg_resp.json::<Value>().await {
                            let headers = msg_body
                                .get("payload")
                                .and_then(|p| p.get("headers"))
                                .and_then(|h| h.as_array());

                            let mut from = String::new();
                            let mut subject = String::new();
                            let mut date = String::new();

                            if let Some(headers) = headers {
                                for header in headers {
                                    if let (Some(name), Some(value)) = (
                                        header.get("name").and_then(|n| n.as_str()),
                                        header.get("value").and_then(|v| v.as_str()),
                                    ) {
                                        match name {
                                            "From" => from = value.to_string(),
                                            "Subject" => subject = value.to_string(),
                                            "Date" => date = value.to_string(),
                                            _ => {}
                                        }
                                    }
                                }
                            }

                            let snippet = msg_body
                                .get("snippet")
                                .and_then(|s| s.as_str())
                                .unwrap_or("");

                            output.push_str(&format!("Email {}\n", i + 1));
                            output.push_str(&format!("ID: {}\n", id));
                            output.push_str(&format!("From: {}\n", from));
                            output.push_str(&format!("Subject: {}\n", subject));
                            output.push_str(&format!("Date: {}\n", date));
                            output.push_str(&format!("Preview: {}\n\n", snippet));
                        }
                    }
                }
            }

            Ok(ToolResult::ok(output))
        } else {
            Ok(ToolResult::ok("No emails found."))
        }
    }
}

pub struct GmailGetTool {
    config: GmailConfig,
}

impl GmailGetTool {
    pub fn new(config: GmailConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl Tool for GmailGetTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "gmail_get".to_string(),
            description: "Get full email content by message ID".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "message_id": {
                        "type": "string",
                        "description": "Gmail message ID"
                    }
                },
                "required": ["message_id"]
            }),
        }
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let message_id = match args["message_id"].as_str() {
            Some(id) => id,
            None => return Ok(ToolResult::err("Missing message_id parameter")),
        };

        let token = self.config.get_token().await?;
        let client = Client::new();

        let url = format!(
            "https://gmail.googleapis.com/gmail/v1/users/me/messages/{}?format=full",
            message_id
        );

        let resp = client.get(&url).bearer_auth(&token).send().await?;
        let body: Value = resp.json().await?;

        let headers = body
            .get("payload")
            .and_then(|p| p.get("headers"))
            .and_then(|h| h.as_array());

        let mut from = String::new();
        let mut to = String::new();
        let mut subject = String::new();
        let mut date = String::new();

        if let Some(headers) = headers {
            for header in headers {
                if let (Some(name), Some(value)) = (
                    header.get("name").and_then(|n| n.as_str()),
                    header.get("value").and_then(|v| v.as_str()),
                ) {
                    match name {
                        "From" => from = value.to_string(),
                        "To" => to = value.to_string(),
                        "Subject" => subject = value.to_string(),
                        "Date" => date = value.to_string(),
                        _ => {}
                    }
                }
            }
        }

        let snippet = body.get("snippet").and_then(|s| s.as_str()).unwrap_or("");

        let output = format!(
            "Email Details\n\nFrom: {}\nTo: {}\nSubject: {}\nDate: {}\n\n{}\n",
            from, to, subject, date, snippet
        );

        Ok(ToolResult::ok(output))
    }
}

pub struct GmailSendTool {
    config: GmailConfig,
}

impl GmailSendTool {
    pub fn new(config: GmailConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl Tool for GmailSendTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "gmail_send".to_string(),
            description:
                "Send an email. IMPORTANT: Always confirm with the user before using this tool."
                    .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "to": {
                        "type": "string",
                        "description": "Recipient email address"
                    },
                    "subject": {
                        "type": "string",
                        "description": "Email subject"
                    },
                    "body": {
                        "type": "string",
                        "description": "Email body content"
                    }
                },
                "required": ["to", "subject", "body"]
            }),
        }
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let to = match args["to"].as_str() {
            Some(t) => t,
            None => return Ok(ToolResult::err("Missing 'to' parameter")),
        };
        let subject = match args["subject"].as_str() {
            Some(s) => s,
            None => return Ok(ToolResult::err("Missing 'subject' parameter")),
        };
        let body = match args["body"].as_str() {
            Some(b) => b,
            None => return Ok(ToolResult::err("Missing 'body' parameter")),
        };

        // Strip Markdown formatting for plain text email
        let clean_body = strip_markdown(body);

        let email_content = format!(
            "To: {}\r\nSubject: {}\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n{}",
            to, subject, clean_body
        );

        let encoded = URL_SAFE.encode(email_content.as_bytes());

        let token = self.config.get_token().await?;
        let client = Client::new();

        let resp = client
            .post("https://gmail.googleapis.com/gmail/v1/users/me/messages/send")
            .bearer_auth(&token)
            .json(&json!({ "raw": encoded }))
            .send()
            .await?;

        let status = resp.status();
        let result: Value = resp.json().await?;

        if !status.is_success() {
            let api_error = result
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown Gmail API error");
            tracing::error!("Gmail send failed ({}): {}", status, api_error);
            return Ok(ToolResult::err(format!(
                "Failed to send email (HTTP {}): {}",
                status.as_u16(),
                api_error
            )));
        }

        if let Some(id) = result.get("id").and_then(|v| v.as_str()) {
            Ok(ToolResult::ok(format!(
                "Email sent successfully to {}. Message ID: {}",
                to, id
            )))
        } else {
            Ok(ToolResult::err(format!("Failed to send email: unexpected response: {}", result)))
        }
    }
}

pub struct GmailReplyTool {
    config: GmailConfig,
}

impl GmailReplyTool {
    pub fn new(config: GmailConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl Tool for GmailReplyTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "gmail_reply".to_string(),
            description:
                "Reply to an email. IMPORTANT: Always confirm with the user before using this tool."
                    .to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "message_id": {
                        "type": "string",
                        "description": "ID of the email to reply to"
                    },
                    "body": {
                        "type": "string",
                        "description": "Reply message body"
                    }
                },
                "required": ["message_id", "body"]
            }),
        }
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let message_id = match args["message_id"].as_str() {
            Some(id) => id,
            None => return Ok(ToolResult::err("Missing message_id parameter")),
        };
        let reply_body = match args["body"].as_str() {
            Some(b) => b,
            None => return Ok(ToolResult::err("Missing 'body' parameter")),
        };

        let token = self.config.get_token().await?;
        let client = Client::new();

        let url = format!(
            "https://gmail.googleapis.com/gmail/v1/users/me/messages/{}?format=metadata",
            message_id
        );

        let resp = client.get(&url).bearer_auth(&token).send().await?;
        let body: Value = resp.json().await?;

        let headers = body
            .get("payload")
            .and_then(|p| p.get("headers"))
            .and_then(|h| h.as_array());

        let mut from = String::new();
        let mut subject = String::new();

        if let Some(headers) = headers {
            for header in headers {
                if let (Some(name), Some(value)) = (
                    header.get("name").and_then(|n| n.as_str()),
                    header.get("value").and_then(|v| v.as_str()),
                ) {
                    match name {
                        "From" => from = value.to_string(),
                        "Subject" => subject = value.to_string(),
                        _ => {}
                    }
                }
            }
        }

        let reply_subject = if subject.starts_with("Re:") {
            subject
        } else {
            format!("Re: {}", subject)
        };

        // Strip Markdown formatting for plain text email
        let clean_reply_body = strip_markdown(reply_body);

        let email_content = format!(
            "To: {}\r\nSubject: {}\r\nIn-Reply-To: {}\r\nReferences: {}\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n{}",
            from, reply_subject, message_id, message_id, clean_reply_body
        );

        let encoded = URL_SAFE.encode(email_content.as_bytes());
        let thread_id = body.get("threadId").and_then(|t| t.as_str());

        let mut payload = json!({ "raw": encoded });
        if let Some(thread_id) = thread_id {
            payload["threadId"] = json!(thread_id);
        }

        let resp = client
            .post("https://gmail.googleapis.com/gmail/v1/users/me/messages/send")
            .bearer_auth(&token)
            .json(&payload)
            .send()
            .await?;

        let status = resp.status();
        let result: Value = resp.json().await?;

        if !status.is_success() {
            let api_error = result
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown Gmail API error");
            tracing::error!("Gmail reply failed ({}): {}", status, api_error);
            return Ok(ToolResult::err(format!(
                "Failed to send reply (HTTP {}): {}",
                status.as_u16(),
                api_error
            )));
        }

        if let Some(id) = result.get("id").and_then(|v| v.as_str()) {
            Ok(ToolResult::ok(format!(
                "Reply sent successfully. Message ID: {}",
                id
            )))
        } else {
            Ok(ToolResult::err(format!("Failed to send reply: unexpected response: {}", result)))
        }
    }
}

pub struct GmailDraftCreateTool {
    config: GmailConfig,
}

impl GmailDraftCreateTool {
    pub fn new(config: GmailConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl Tool for GmailDraftCreateTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "gmail_draft_create".to_string(),
            description: "Create an email draft".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "to": {
                        "type": "string",
                        "description": "Recipient email address"
                    },
                    "subject": {
                        "type": "string",
                        "description": "Email subject"
                    },
                    "body": {
                        "type": "string",
                        "description": "Email body content"
                    }
                },
                "required": ["to", "subject", "body"]
            }),
        }
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let to = match args["to"].as_str() {
            Some(t) => t,
            None => return Ok(ToolResult::err("Missing 'to' parameter")),
        };
        let subject = match args["subject"].as_str() {
            Some(s) => s,
            None => return Ok(ToolResult::err("Missing 'subject' parameter")),
        };
        let body = match args["body"].as_str() {
            Some(b) => b,
            None => return Ok(ToolResult::err("Missing 'body' parameter")),
        };

        // Strip Markdown formatting for plain text email
        let clean_body = strip_markdown(body);

        let email_content = format!(
            "To: {}\r\nSubject: {}\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n{}",
            to, subject, clean_body
        );

        let encoded = URL_SAFE.encode(email_content.as_bytes());

        let token = self.config.get_token().await?;
        let client = Client::new();

        let resp = client
            .post("https://gmail.googleapis.com/gmail/v1/users/me/drafts")
            .bearer_auth(&token)
            .json(&json!({ "message": { "raw": encoded } }))
            .send()
            .await?;

        let status = resp.status();
        let result: Value = resp.json().await?;

        if !status.is_success() {
            let api_error = result
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown Gmail API error");
            tracing::error!("Gmail draft create failed ({}): {}", status, api_error);
            return Ok(ToolResult::err(format!(
                "Failed to create draft (HTTP {}): {}",
                status.as_u16(),
                api_error
            )));
        }

        if let Some(id) = result.get("id").and_then(|v| v.as_str()) {
            Ok(ToolResult::ok(format!(
                "Draft created successfully. Draft ID: {}",
                id
            )))
        } else {
            Ok(ToolResult::err(format!("Failed to create draft: unexpected response: {}", result)))
        }
    }
}

pub struct GmailDraftSendTool {
    config: GmailConfig,
}

impl GmailDraftSendTool {
    pub fn new(config: GmailConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl Tool for GmailDraftSendTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "gmail_draft_send".to_string(),
            description: "Send a previously created draft. IMPORTANT: Always confirm with the user before using this tool.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "draft_id": {
                        "type": "string",
                        "description": "ID of the draft to send"
                    }
                },
                "required": ["draft_id"]
            }),
        }
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let draft_id = match args["draft_id"].as_str() {
            Some(id) => id,
            None => return Ok(ToolResult::err("Missing draft_id parameter")),
        };

        let token = self.config.get_token().await?;
        let client = Client::new();

        let resp = client
            .post("https://gmail.googleapis.com/gmail/v1/users/me/drafts/send")
            .bearer_auth(&token)
            .json(&json!({ "id": draft_id }))
            .send()
            .await?;

        let status = resp.status();
        let result: Value = resp.json().await?;

        if !status.is_success() {
            let api_error = result
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown Gmail API error");
            tracing::error!("Gmail draft send failed ({}): {}", status, api_error);
            return Ok(ToolResult::err(format!(
                "Failed to send draft (HTTP {}): {}",
                status.as_u16(),
                api_error
            )));
        }

        if let Some(id) = result.get("id").and_then(|v| v.as_str()) {
            Ok(ToolResult::ok(format!(
                "Draft sent successfully. Message ID: {}",
                id
            )))
        } else {
            Ok(ToolResult::err(format!("Failed to send draft: unexpected response: {}", result)))
        }
    }
}

pub struct GmailDraftListTool {
    config: GmailConfig,
}

impl GmailDraftListTool {
    pub fn new(config: GmailConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl Tool for GmailDraftListTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "gmail_draft_list".to_string(),
            description: "List email drafts".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "max_results": {
                        "type": "integer",
                        "description": "Maximum number of drafts to return (default: 10)",
                        "default": 10
                    }
                }
            }),
        }
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let max_results = args["max_results"].as_u64().unwrap_or(10);

        let token = self.config.get_token().await?;
        let client = Client::new();

        let url = format!(
            "https://gmail.googleapis.com/gmail/v1/users/me/drafts?maxResults={}",
            max_results
        );

        let resp = client.get(&url).bearer_auth(&token).send().await?;
        let body: Value = resp.json().await?;

        if let Some(drafts) = body.get("drafts").and_then(|v| v.as_array()) {
            if drafts.is_empty() {
                return Ok(ToolResult::ok("No drafts found."));
            }

            let mut output = format!("Found {} draft(s):\n\n", drafts.len());

            for (i, draft) in drafts.iter().enumerate() {
                if let Some(draft_id) = draft.get("id").and_then(|v| v.as_str()) {
                    // Fetch full draft details
                    let draft_url = format!(
                        "https://gmail.googleapis.com/gmail/v1/users/me/drafts/{}?format=metadata",
                        draft_id
                    );

                    if let Ok(draft_resp) = client.get(&draft_url).bearer_auth(&token).send().await
                    {
                        if let Ok(draft_data) = draft_resp.json::<Value>().await {
                            let message = draft_data.get("message");

                            let mut subject = String::from("(No subject)");
                            let mut to = String::from("(No recipient)");

                            if let Some(headers) = message
                                .and_then(|m| m.get("payload"))
                                .and_then(|p| p.get("headers"))
                                .and_then(|h| h.as_array())
                            {
                                for header in headers {
                                    if let (Some(name), Some(value)) = (
                                        header.get("name").and_then(|n| n.as_str()),
                                        header.get("value").and_then(|v| v.as_str()),
                                    ) {
                                        match name {
                                            "Subject" => subject = value.to_string(),
                                            "To" => to = value.to_string(),
                                            _ => {}
                                        }
                                    }
                                }
                            }

                            output.push_str(&format!("{}. To: {}\n", i + 1, to));
                            output.push_str(&format!("   Subject: {}\n", subject));
                            output.push_str(&format!("   Draft ID: {}\n\n", draft_id));
                            continue;
                        }
                    }

                    // Fallback if detailed fetch fails
                    output.push_str(&format!("{}. Draft ID: {}\n\n", i + 1, draft_id));
                }
            }

            Ok(ToolResult::ok(output))
        } else {
            Ok(ToolResult::ok("No drafts found."))
        }
    }
}

pub struct GmailDraftGetTool {
    config: GmailConfig,
}

impl GmailDraftGetTool {
    pub fn new(config: GmailConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl Tool for GmailDraftGetTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "gmail_draft_get".to_string(),
            description: "Get full details of a specific draft including subject, recipient, and body content".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "draft_id": {
                        "type": "string",
                        "description": "The ID of the draft to retrieve"
                    }
                },
                "required": ["draft_id"]
            }),
        }
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let draft_id = match args["draft_id"].as_str() {
            Some(id) => id,
            None => return Ok(ToolResult::err("Missing draft_id parameter")),
        };

        let token = self.config.get_token().await?;
        let client = Client::new();

        let url = format!(
            "https://gmail.googleapis.com/gmail/v1/users/me/drafts/{}",
            draft_id
        );

        let resp = client.get(&url).bearer_auth(&token).send().await?;
        let draft_data: Value = resp.json().await?;

        let message = draft_data.get("message");

        let mut subject = String::from("(No subject)");
        let mut to = String::from("(No recipient)");

        if let Some(headers) = message
            .and_then(|m| m.get("payload"))
            .and_then(|p| p.get("headers"))
            .and_then(|h| h.as_array())
        {
            for header in headers {
                if let (Some(name), Some(value)) = (
                    header.get("name").and_then(|n| n.as_str()),
                    header.get("value").and_then(|v| v.as_str()),
                ) {
                    match name {
                        "Subject" => subject = value.to_string(),
                        "To" => to = value.to_string(),
                        _ => {}
                    }
                }
            }
        }

        // Get snippet (preview of body)
        let snippet = message
            .and_then(|m| m.get("snippet"))
            .and_then(|s| s.as_str())
            .unwrap_or("(Empty draft)");

        let mut output = format!("Draft Details:\n\n");
        output.push_str(&format!("To: {}\n", to));
        output.push_str(&format!("Subject: {}\n", subject));
        output.push_str(&format!("Draft ID: {}\n\n", draft_id));
        output.push_str("Body Preview:\n");
        output.push_str(&format!("{}\n", snippet));

        Ok(ToolResult::ok(output))
    }
}
