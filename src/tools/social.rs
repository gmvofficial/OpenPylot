//! LLM-callable tools for posting to social platforms.
//!
//! Each tool is a thin wrapper around the corresponding HTTP API. Credentials
//! are injected at construction time (loaded from `AppConfig`/the secrets
//! vault), so the agent only needs to supply the post content at call time.

use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::{json, Value};

use crate::tools::{Tool, ToolDefinition, ToolResult};

// ════════════════════════════════════════════════════════════════════
//  LinkedInPost
// ════════════════════════════════════════════════════════════════════

/// Publish a text post to the user's personal LinkedIn profile via the UGC
/// Posts API. Requires the `w_member_social` scope on the access token.
///
/// `person_id` (the LinkedIn member URN suffix) is resolved lazily on the
/// first post if it wasn't already known at startup — we try `/v2/userinfo`
/// first (works for tokens with the `openid`/`profile` scope) and fall back
/// to `/v2/me` (older `r_liteprofile` scope). The discovered value is cached
/// back to the secrets vault so subsequent posts skip the lookup.
pub struct LinkedInPost {
    access_token: String,
    person_id: tokio::sync::Mutex<Option<String>>,
    client: Client,
}

impl LinkedInPost {
    pub fn new(access_token: String, person_id: Option<String>) -> Self {
        Self {
            access_token,
            person_id: tokio::sync::Mutex::new(person_id),
            client: Client::new(),
        }
    }

    /// Heuristic: real LinkedIn person IDs are short alphanumeric strings
    /// (the `sub` claim from /v2/userinfo, e.g. `abc123XYZ`). Vanity URL
    /// slugs like `rupak-chandra-41cg` contain hyphens and are NOT valid
    /// here — the UGC API rejects `urn:li:person:<vanity>` with HTTP 422.
    fn looks_like_valid_person_id(s: &str) -> bool {
        let s = s.trim();
        !s.is_empty() && s.len() <= 60 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    }

    /// Return the LinkedIn member URN suffix, fetching+caching it if needed.
    async fn resolve_person_id(&self) -> Result<String> {
        {
            let mut guard = self.person_id.lock().await;
            if let Some(ref id) = *guard {
                if Self::looks_like_valid_person_id(id) {
                    return Ok(id.clone());
                }
                // Cached value is clearly wrong (e.g. vanity slug). Wipe the
                // bad value from BOTH the in-memory cache and the on-disk
                // vault so we never return it again.
                tracing::warn!(
                    "Cached linkedin.person_id ({:?}) looks invalid — \
                     wiping and re-resolving via LinkedIn API",
                    id
                );
                *guard = None;
                if let Ok(mut vault) = crate::secrets::SecretsVault::open(
                    &crate::secrets::default_secrets_path(),
                    None,
                ) {
                    let _ = vault.delete("linkedin.person_id");
                    let _ = vault.save();
                }
            }
        }

        // Track each attempt so we can surface a helpful error if both fail.
        let mut diagnostics: Vec<String> = Vec::new();

        // Try /v2/userinfo (OpenID Connect — needs `openid`/`profile`).
        let mut discovered: Option<String> = None;
        match self
            .client
            .get("https://api.linkedin.com/v2/userinfo")
            .bearer_auth(&self.access_token)
            .send()
            .await
        {
            Ok(resp) => {
                let status = resp.status();
                if status.is_success() {
                    match resp.json::<Value>().await {
                        Ok(json) => {
                            if let Some(sub) = json.get("sub").and_then(|v| v.as_str()) {
                                discovered = Some(sub.to_string());
                            } else {
                                diagnostics.push(format!(
                                    "/v2/userinfo returned 200 but no `sub` field: {}",
                                    json
                                ));
                            }
                        }
                        Err(e) => {
                            diagnostics.push(format!("/v2/userinfo response parse error: {e}"))
                        }
                    }
                } else {
                    let body = resp.text().await.unwrap_or_default();
                    diagnostics.push(format!(
                        "/v2/userinfo HTTP {} — {}",
                        status,
                        body.chars().take(200).collect::<String>()
                    ));
                }
            }
            Err(e) => diagnostics.push(format!(
                "/v2/userinfo network error: {}",
                crate::api::handlers::describe_network_error("api.linkedin.com", &e)
            )),
        }

        // Fall back to /v2/me (legacy `r_liteprofile`).
        if discovered.is_none() {
            match self
                .client
                .get("https://api.linkedin.com/v2/me")
                .bearer_auth(&self.access_token)
                .header("X-Restli-Protocol-Version", "2.0.0")
                .send()
                .await
            {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        match resp.json::<Value>().await {
                            Ok(json) => {
                                if let Some(id) = json.get("id").and_then(|v| v.as_str()) {
                                    discovered = Some(id.to_string());
                                } else {
                                    diagnostics.push(format!(
                                        "/v2/me returned 200 but no `id` field: {}",
                                        json
                                    ));
                                }
                            }
                            Err(e) => diagnostics.push(format!("/v2/me response parse error: {e}")),
                        }
                    } else {
                        let body = resp.text().await.unwrap_or_default();
                        diagnostics.push(format!(
                            "/v2/me HTTP {} — {}",
                            status,
                            body.chars().take(200).collect::<String>()
                        ));
                    }
                }
                Err(e) => diagnostics.push(format!(
                    "/v2/me network error: {}",
                    crate::api::handlers::describe_network_error("api.linkedin.com", &e)
                )),
            }
        }

        let id = match discovered {
            Some(v) => v,
            None => {
                return Err(anyhow::anyhow!(
                    "Could not auto-detect your LinkedIn person_id. The token \
                     likely lacks the `openid`/`profile` scope (or `r_liteprofile`). \
                     \n\nTo fix: open https://www.linkedin.com/developers/apps → your \
                     app → Auth → OAuth 2.0 tools → Generate access token, and tick \
                     `openid`, `profile`, AND `w_member_social`. Then in pylot's \
                     setup page, click Disconnect on LinkedIn and Connect again \
                     (leave the Person ID field blank — it will auto-detect).\n\n\
                     Diagnostics:\n  • {}",
                    diagnostics.join("\n  • ")
                ));
            }
        };

        if !Self::looks_like_valid_person_id(&id) {
            return Err(anyhow::anyhow!(
                "LinkedIn returned an unexpected person_id format ({:?}). \
                 Expected an alphanumeric string. Try regenerating the access token.",
                id
            ));
        }

        // Cache in memory.
        {
            let mut guard = self.person_id.lock().await;
            *guard = Some(id.clone());
        }
        // Best-effort persist to vault so future starts skip the lookup.
        if let Ok(mut vault) =
            crate::secrets::SecretsVault::open(&crate::secrets::default_secrets_path(), None)
        {
            let _ = vault.set("linkedin.person_id", &id);
            let _ = vault.save();
        }

        Ok(id)
    }
}

#[async_trait]
impl Tool for LinkedInPost {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "linkedin_post".into(),
            description: "Publish a text post to the user's personal LinkedIn profile. \
                 The post is visible to the user's network. Use this whenever \
                 the user asks to share, post, publish, or announce something \
                 on LinkedIn."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "content": {
                        "type": "string",
                        "description": "The full text of the LinkedIn post (max 3000 chars)."
                    },
                    "visibility": {
                        "type": "string",
                        "enum": ["PUBLIC", "CONNECTIONS"],
                        "description": "Post audience (default: PUBLIC)."
                    }
                },
                "required": ["content"]
            }),
        }
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let content = params["content"]
            .as_str()
            .context("Missing 'content' parameter")?;
        if content.trim().is_empty() {
            return Ok(ToolResult::err("LinkedIn post content cannot be empty."));
        }

        // LinkedIn shows raw text — strip any markdown the LLM may have
        // included (`**bold**`, `*italic*`, `## heading`, bullet points,
        // backtick code) so it doesn't appear literally on the timeline.
        let clean = crate::social::strip_markdown(content);
        let content = clean.as_str();

        let visibility = params["visibility"].as_str().unwrap_or("PUBLIC");

        let person_id = match self.resolve_person_id().await {
            Ok(id) => id,
            Err(e) => return Ok(ToolResult::err(format!("{}", e))),
        };

        let body = json!({
            "author": format!("urn:li:person:{}", person_id),
            "lifecycleState": "PUBLISHED",
            "specificContent": {
                "com.linkedin.ugc.ShareContent": {
                    "shareCommentary": { "text": content },
                    "shareMediaCategory": "NONE"
                }
            },
            "visibility": {
                "com.linkedin.ugc.MemberNetworkVisibility": visibility
            }
        });

        let resp = match self
            .client
            .post("https://api.linkedin.com/v2/ugcPosts")
            .bearer_auth(&self.access_token)
            .header("X-Restli-Protocol-Version", "2.0.0")
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                return Ok(ToolResult::err(format!(
                    "LinkedIn post failed at the network layer.\n{}",
                    crate::api::handlers::describe_network_error("api.linkedin.com", &e)
                )));
            }
        };

        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();

        if !status.is_success() {
            return Ok(ToolResult::err(format!(
                "LinkedIn API returned HTTP {}: {}",
                status, text
            )));
        }

        let json: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        let post_id = json
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("(unknown)");

        // Build a clickable browser URL from the URN so the LLM (and the
        // user reading the chat reply) gets a real https:// link instead
        // of a `urn:li:share:...` string that the markdown renderer would
        // treat as a relative path → internal app route.
        let public_url = crate::social::linkedin_post_url(post_id);

        let success_msg = match public_url {
            Some(url) => format!(
                "LinkedIn post published successfully.\n\
                 View it here: [{url}]({url})\n\
                 Post URN: {post_id}"
            ),
            None => format!("LinkedIn post published successfully.\nPost URN: {post_id}"),
        };

        Ok(ToolResult::ok(success_msg))
    }
}

// ════════════════════════════════════════════════════════════════════
//  DiscordSendMessage
// ════════════════════════════════════════════════════════════════════

/// Post a message to a Discord channel using a bot token.
pub struct DiscordSendMessage {
    bot_token: String,
    default_channel_id: Option<String>,
    client: Client,
}

impl DiscordSendMessage {
    pub fn new(bot_token: String, default_channel_id: Option<String>) -> Self {
        Self {
            bot_token,
            default_channel_id,
            client: Client::new(),
        }
    }
}

#[async_trait]
impl Tool for DiscordSendMessage {
    fn definition(&self) -> ToolDefinition {
        let mut desc = "Send a message to a Discord channel via the Discord Bot API.".to_string();
        if self.default_channel_id.is_some() {
            desc.push_str(" A default channel is configured, so channel_id is optional.");
        }

        ToolDefinition {
            name: "discord_send_message".into(),
            description: desc,
            parameters: json!({
                "type": "object",
                "properties": {
                    "channel_id": {
                        "type": "string",
                        "description": "Numeric Discord channel ID to post in. Optional if a default channel is configured."
                    },
                    "content": {
                        "type": "string",
                        "description": "Message text (max 2000 chars). Supports Discord markdown."
                    }
                },
                "required": ["content"]
            }),
        }
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let content = params["content"]
            .as_str()
            .context("Missing 'content' parameter")?;

        let channel_id = params["channel_id"]
            .as_str()
            .map(String::from)
            .or_else(|| self.default_channel_id.clone())
            .context(
                "No channel_id provided and no default channel configured. \
                 Provide a channel_id parameter or set DISCORD_CHANNEL_ID.",
            )?;

        let url = format!(
            "https://discord.com/api/v10/channels/{}/messages",
            channel_id
        );

        let resp = match self
            .client
            .post(&url)
            .header("Authorization", format!("Bot {}", self.bot_token))
            .json(&json!({ "content": content }))
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                return Ok(ToolResult::err(
                    crate::api::handlers::describe_network_error("discord.com", &e),
                ));
            }
        };

        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();

        if !status.is_success() {
            return Ok(ToolResult::err(format!(
                "Discord API returned HTTP {}: {}",
                status, text
            )));
        }

        let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        let msg_id = v.get("id").and_then(|x| x.as_str()).unwrap_or("(unknown)");

        Ok(ToolResult::ok(format!(
            "Discord message sent.\nChannel: {}\nMessage ID: {}",
            channel_id, msg_id
        )))
    }
}

// ════════════════════════════════════════════════════════════════════
//  SlackSendMessage
// ════════════════════════════════════════════════════════════════════

/// Post a message to a Slack channel using a Bot User OAuth Token.
pub struct SlackSendMessage {
    bot_token: String,
    default_channel: Option<String>,
    client: Client,
}

impl SlackSendMessage {
    pub fn new(bot_token: String, default_channel: Option<String>) -> Self {
        Self {
            bot_token,
            default_channel,
            client: Client::new(),
        }
    }
}

#[async_trait]
impl Tool for SlackSendMessage {
    fn definition(&self) -> ToolDefinition {
        let mut desc = "Send a message to a Slack channel via the Slack Web API.".to_string();
        if self.default_channel.is_some() {
            desc.push_str(" A default channel is configured, so channel is optional.");
        }

        ToolDefinition {
            name: "slack_send_message".into(),
            description: desc,
            parameters: json!({
                "type": "object",
                "properties": {
                    "channel": {
                        "type": "string",
                        "description": "Channel name (e.g. #general) or channel ID. Optional if default is configured."
                    },
                    "text": {
                        "type": "string",
                        "description": "Message text (Slack mrkdwn supported)."
                    }
                },
                "required": ["text"]
            }),
        }
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let text = params["text"]
            .as_str()
            .context("Missing 'text' parameter")?;

        let channel = params["channel"]
            .as_str()
            .map(String::from)
            .or_else(|| self.default_channel.clone())
            .context(
                "No channel provided and no default channel configured. \
                 Provide a channel parameter or set SLACK_CHANNEL.",
            )?;

        let resp = match self
            .client
            .post("https://slack.com/api/chat.postMessage")
            .bearer_auth(&self.bot_token)
            .json(&json!({ "channel": channel, "text": text }))
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                return Ok(ToolResult::err(
                    crate::api::handlers::describe_network_error("slack.com", &e),
                ));
            }
        };

        let body: Value = resp.json().await.unwrap_or(Value::Null);
        if body.get("ok").and_then(|v| v.as_bool()) == Some(true) {
            let ts = body.get("ts").and_then(|v| v.as_str()).unwrap_or("");
            Ok(ToolResult::ok(format!(
                "Slack message sent.\nChannel: {}\nTimestamp: {}",
                channel, ts
            )))
        } else {
            let err_str = body
                .get("error")
                .and_then(|e| e.as_str())
                .unwrap_or("unknown");
            Ok(ToolResult::err(format!("Slack error: {}", err_str)))
        }
    }
}

// ════════════════════════════════════════════════════════════════════
//  FacebookPost
// ════════════════════════════════════════════════════════════════════

/// Publish a text post to a Facebook Page using a long-lived Page Access Token.
pub struct FacebookPost {
    page_id: String,
    access_token: String,
    client: Client,
}

impl FacebookPost {
    pub fn new(page_id: String, access_token: String) -> Self {
        Self {
            page_id,
            access_token,
            client: Client::new(),
        }
    }
}

#[async_trait]
impl Tool for FacebookPost {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "facebook_post".into(),
            description: "Publish a text post to the configured Facebook Page using the \
                 Graph API. Use this whenever the user asks to share, post, or \
                 announce something on Facebook."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "message": {
                        "type": "string",
                        "description": "The text content of the Facebook post."
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

        let url = format!("https://graph.facebook.com/v22.0/{}/feed", self.page_id);

        let resp = match self
            .client
            .post(&url)
            .form(&[("message", message), ("access_token", &self.access_token)])
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                return Ok(ToolResult::err(
                    crate::api::handlers::describe_network_error("graph.facebook.com", &e),
                ));
            }
        };

        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();

        if !status.is_success() {
            return Ok(ToolResult::err(format!(
                "Facebook Graph API returned HTTP {}: {}",
                status, text
            )));
        }

        let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        let post_id = v.get("id").and_then(|x| x.as_str()).unwrap_or("(unknown)");

        Ok(ToolResult::ok(format!(
            "Facebook post published.\nPost ID: {}",
            post_id
        )))
    }
}

// ════════════════════════════════════════════════════════════════════
//  TwitterPost (OAuth 1.0a)
// ════════════════════════════════════════════════════════════════════

/// Publish a tweet via the Twitter v2 `POST /2/tweets` endpoint using
/// OAuth 1.0a user-context credentials (4 keys/secrets).
pub struct TwitterPost {
    api_key: String,
    api_secret: String,
    access_token: String,
    access_token_secret: String,
    client: Client,
}

impl TwitterPost {
    pub fn new(
        api_key: String,
        api_secret: String,
        access_token: String,
        access_token_secret: String,
    ) -> Self {
        Self {
            api_key,
            api_secret,
            access_token,
            access_token_secret,
            client: Client::new(),
        }
    }

    /// Build the OAuth 1.0a `Authorization` header for a request.
    fn oauth_header(&self, method: &str, url: &str, body_json: &str) -> Result<String> {
        use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
        use hmac::{Hmac, Mac};
        use sha1::Sha1;

        let nonce: String = (0..32)
            .map(|_| {
                let n: u8 = rand::random();
                format!("{:02x}", n)
            })
            .collect();
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs().to_string())
            .unwrap_or_else(|_| "0".to_string());

        // OAuth params (signature added below)
        let mut params: Vec<(String, String)> = vec![
            ("oauth_consumer_key".into(), self.api_key.clone()),
            ("oauth_nonce".into(), nonce.clone()),
            ("oauth_signature_method".into(), "HMAC-SHA1".into()),
            ("oauth_timestamp".into(), timestamp.clone()),
            ("oauth_token".into(), self.access_token.clone()),
            ("oauth_version".into(), "1.0".into()),
        ];

        // Note: For JSON-bodied v2 endpoints, the body is NOT signed — only
        // the OAuth params and any URL query params are part of the signature
        // base string. body_json is intentionally unused here.
        let _ = body_json;

        // Build signature base string
        let mut sorted = params.clone();
        sorted.sort_by(|a, b| a.0.cmp(&b.0));
        let param_string = sorted
            .iter()
            .map(|(k, v)| format!("{}={}", percent_encode(k), percent_encode(v)))
            .collect::<Vec<_>>()
            .join("&");
        let base = format!(
            "{}&{}&{}",
            method.to_uppercase(),
            percent_encode(url),
            percent_encode(&param_string)
        );
        let signing_key = format!(
            "{}&{}",
            percent_encode(&self.api_secret),
            percent_encode(&self.access_token_secret)
        );

        type HmacSha1 = Hmac<Sha1>;
        let mut mac = HmacSha1::new_from_slice(signing_key.as_bytes())
            .map_err(|e| anyhow::anyhow!("HMAC init failed: {e}"))?;
        mac.update(base.as_bytes());
        let signature = B64.encode(mac.finalize().into_bytes());
        params.push(("oauth_signature".into(), signature));

        let header = params
            .iter()
            .map(|(k, v)| format!("{}=\"{}\"", percent_encode(k), percent_encode(v)))
            .collect::<Vec<_>>()
            .join(", ");
        Ok(format!("OAuth {}", header))
    }
}

fn percent_encode(s: &str) -> String {
    // RFC 3986 unreserved characters: A-Z a-z 0-9 - _ . ~
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

#[async_trait]
impl Tool for TwitterPost {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "twitter_post".into(),
            description: "Publish a tweet (X post) to the user's Twitter/X account. \
                 Use this whenever the user asks to tweet, post on X, or \
                 share something on Twitter."
                .into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "text": {
                        "type": "string",
                        "description": "The tweet text (max 280 chars for standard accounts)."
                    }
                },
                "required": ["text"]
            }),
        }
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let text = params["text"]
            .as_str()
            .context("Missing 'text' parameter")?;

        let url = "https://api.twitter.com/2/tweets";
        let body = json!({ "text": text });
        let body_str = body.to_string();

        let auth = self.oauth_header("POST", url, &body_str)?;

        let resp = match self
            .client
            .post(url)
            .header("Authorization", auth)
            .header("Content-Type", "application/json")
            .body(body_str)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                return Ok(ToolResult::err(
                    crate::api::handlers::describe_network_error("api.twitter.com", &e),
                ));
            }
        };

        let status = resp.status();
        let text_resp = resp.text().await.unwrap_or_default();

        if !status.is_success() {
            return Ok(ToolResult::err(format!(
                "Twitter API returned HTTP {}: {}",
                status, text_resp
            )));
        }

        let v: Value = serde_json::from_str(&text_resp).unwrap_or(Value::Null);
        let id = v
            .get("data")
            .and_then(|d| d.get("id"))
            .and_then(|x| x.as_str())
            .unwrap_or("(unknown)");

        Ok(ToolResult::ok(format!("Tweet posted.\nTweet ID: {}", id)))
    }
}
