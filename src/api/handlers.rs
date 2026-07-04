use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::OnceLock;
use tokio::sync::Mutex as AsyncMutex;
use tokio::task::JoinHandle;

use super::ApiState;

// ── JSON file helpers ────────────────────────────────────────────────

fn load_json_vec<T: serde::de::DeserializeOwned>(path: &std::path::Path) -> Vec<T> {
    if path.exists() {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    } else {
        vec![]
    }
}

fn save_json_vec<T: Serialize>(path: &std::path::Path, data: &[T]) -> bool {
    serde_json::to_string_pretty(data)
        .ok()
        .and_then(|s| std::fs::write(path, s).ok())
        .is_some()
}

// ── Common types ─────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct ApiResponse<T: Serialize> {
    pub success: bool,
    pub data: T,
}

#[derive(Serialize)]
pub struct ApiError {
    pub success: bool,
    pub error: String,
    /// Machine-readable error code so the frontend can branch on it.
    /// Examples: `"credentials_missing"`, `"vault_corrupted"`, `"unauthorized"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// Service that needs attention (e.g. `"google_calendar"`, `"telegram"`).
    /// When present together with `code = "credentials_missing"`, the UI shows
    /// a "Reconnect" banner pointing at this service.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service: Option<String>,
    /// Endpoint the client should POST to in order to start the reconnect flow.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reconnect_url: Option<String>,
}

fn ok<T: Serialize>(data: T) -> Json<ApiResponse<T>> {
    Json(ApiResponse {
        success: true,
        data,
    })
}

fn err(status: StatusCode, msg: impl Into<String>) -> (StatusCode, Json<ApiError>) {
    (
        status,
        Json(ApiError {
            success: false,
            error: msg.into(),
            code: None,
            service: None,
            reconnect_url: None,
        }),
    )
}

/// Build a structured 401 response telling the frontend that a specific
/// integration is missing credentials and where to start the reconnect flow.
///
/// Used by tool-execution handlers when the vault doesn't contain the keys
/// needed for `service`. The frontend recognises `code = "credentials_missing"`
/// and renders a "⚠️ <Service> disconnected — Reconnect →" card.
pub fn credentials_missing(service: &str) -> (StatusCode, Json<ApiError>) {
    let pretty = match service {
        "google_calendar" | "gmail" => "Google",
        "telegram" => "Telegram",
        "whatsapp" => "WhatsApp",
        "github" => "GitHub",
        "slack" => "Slack",
        other => other,
    };
    (
        StatusCode::UNAUTHORIZED,
        Json(ApiError {
            success: false,
            error: format!("{pretty} account disconnected. Please reconnect to continue."),
            code: Some("credentials_missing".into()),
            service: Some(service.to_string()),
            reconnect_url: Some(format!("/api/integrations/{service}/connect")),
        }),
    )
}

/// Turn a reqwest error into a user-friendly message that explains *why* a
/// connection attempt failed, and offers concrete remediation steps when the
/// failure is a local-network problem (VPN, firewall, IPv6 routing) rather
/// than something the API itself rejected.
pub fn describe_network_error(host: &str, e: &reqwest::Error) -> String {
    use std::error::Error as _;
    // Walk the source chain looking for a `std::io::Error`. Reqwest wraps
    // hyper → hyper_util → io::Error for connection-level failures.
    let mut src: Option<&(dyn std::error::Error + 'static)> = e.source();
    let mut io_err: Option<&std::io::Error> = None;
    while let Some(s) = src {
        if let Some(io) = s.downcast_ref::<std::io::Error>() {
            io_err = Some(io);
            break;
        }
        src = s.source();
    }

    if e.is_timeout() {
        return format!(
            "Timed out reaching {host}. Check your internet connection, VPN, \
             or firewall. The request didn't complete in time."
        );
    }

    if let Some(io) = io_err {
        use std::io::ErrorKind::*;
        let kind = io.kind();
        // ENETUNREACH on macOS = errno 51, EHOSTUNREACH = 65, ETIMEDOUT = 60
        let raw = io.raw_os_error();
        let is_unreachable =
            matches!(kind, NetworkUnreachable | HostUnreachable) || matches!(raw, Some(51) | Some(65));
        if is_unreachable {
            return format!(
                "Cannot reach {host} from this machine (OS reports network/host \
                 unreachable). This is almost always a *local* networking issue — \
                 not a problem with your token. Try one of these:\n\
                 • Disable any VPN/proxy and retry.\n\
                 • If on a corporate network, ask IT whether {host} is blocked.\n\
                 • Test from a terminal: `curl -I https://{host}` should return \
                 an HTTP status. If it also fails with \"Network is unreachable\", \
                 it's a routing/firewall issue at the OS level.\n\
                 • Toggle Wi-Fi off/on, or try a different network."
            );
        }
        if matches!(kind, ConnectionRefused) {
            return format!(
                "{host} refused the connection. The endpoint may be down — try again \
                 in a minute, or check status pages."
            );
        }
        return format!(
            "Network error reaching {host}: {} (raw os error: {:?}). Verify your \
             internet connection and that {host} is reachable.",
            io, raw
        );
    }

    if e.is_connect() {
        return format!(
            "Could not establish a connection to {host}. Verify your internet \
             connection, VPN settings, and that {host} is not blocked by a \
             firewall. Underlying error: {}",
            e
        );
    }

    format!("Connection failed: {}", e)
}

/// Process-wide handle to the currently-running Google OAuth callback listener.
///
/// `connect_integration` aborts whatever is in here before spawning a new task,
/// so a second Connect (or Disconnect → Connect) doesn't leave a stale listener
/// bound to the redirect port with the previous CSRF state.
fn google_oauth_task_handle() -> &'static AsyncMutex<Option<JoinHandle<()>>> {
    static CELL: OnceLock<AsyncMutex<Option<JoinHandle<()>>>> = OnceLock::new();
    CELL.get_or_init(|| AsyncMutex::new(None))
}

// ── GET /api/status ──────────────────────────────────────────────────

#[derive(Serialize)]
pub struct AgentStatusResponse {
    status: String,
    online: bool,
    uptime: String,
    model: String,
    active_integrations: u32,
    agent_name: String,
    version: String,
}

pub async fn get_status(State(state): State<ApiState>) -> Json<ApiResponse<AgentStatusResponse>> {
    let elapsed = state.start_time.elapsed();
    let hours = elapsed.as_secs() / 3600;
    let mins = (elapsed.as_secs() % 3600) / 60;
    let uptime = if hours > 0 {
        format!("{}h {}m", hours, mins)
    } else {
        format!("{}m", mins)
    };

    let config = &state.config;
    let mut integrations = 0u32;
    if config.google_calendar_enabled {
        integrations += 1;
    }
    if config.gmail_enabled {
        integrations += 1;
    }
    if config.telegram_enabled {
        integrations += 1;
    }
    if config.whatsapp_enabled {
        integrations += 1;
    }

    ok(AgentStatusResponse {
        status: "running".into(),
        online: true,
        uptime,
        model: config.llm_model.clone(),
        active_integrations: integrations,
        agent_name: config.agent_name.clone(),
        version: env!("CARGO_PKG_VERSION").into(),
    })
}

// ── POST /api/chat ───────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct ChatRequest {
    message: String,
    conversation_id: Option<String>,
}

#[derive(Serialize)]
pub struct ChatResponse {
    response: String,
    conversation_id: String,
}

pub async fn send_message(
    State(state): State<ApiState>,
    Json(body): Json<ChatRequest>,
) -> Result<Json<ApiResponse<ChatResponse>>, (StatusCode, Json<ApiError>)> {
    let conv_id = body
        .conversation_id
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    // Save user message
    state.conversations.add_message(
        &conv_id,
        super::StoredMessage {
            id: uuid::Uuid::new_v4().to_string(),
            role: "user".into(),
            content: body.message.clone(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        },
    );

    let mut agent = state.agent.lock().await;

    match agent.chat(&body.message).await {
        Ok(response) => {
            // Save assistant message
            state.conversations.add_message(
                &conv_id,
                super::StoredMessage {
                    id: uuid::Uuid::new_v4().to_string(),
                    role: "assistant".into(),
                    content: response.clone(),
                    timestamp: chrono::Utc::now().to_rfc3339(),
                },
            );

            Ok(ok(ChatResponse {
                response,
                conversation_id: conv_id,
            }))
        }
        Err(e) => Err(err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Chat failed: {}", e),
        )),
    }
}

// ── GET /api/conversations ───────────────────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationSummary {
    id: String,
    title: String,
    last_message: Option<String>,
    updated_at: String,
    message_count: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationDetail {
    id: String,
    title: String,
    messages: Vec<super::StoredMessage>,
    created_at: String,
    updated_at: String,
}

pub async fn list_conversations(
    State(state): State<ApiState>,
) -> Json<ApiResponse<Vec<ConversationSummary>>> {
    let convos = state.conversations.list();
    let summaries: Vec<ConversationSummary> = convos
        .into_iter()
        .map(|c| ConversationSummary {
            id: c.id,
            title: c.title,
            // Slice by chars, NOT bytes — `&s[..100]` panics if byte index 100
            // falls inside a multi-byte codepoint (emoji 🚀, é, 你, …).
            last_message: c.messages.last().map(|m| {
                let preview: String = m.content.chars().take(100).collect();
                if m.content.chars().count() > 100 {
                    format!("{}…", preview)
                } else {
                    preview
                }
            }),
            updated_at: c.updated_at,
            message_count: c.messages.len() as u32,
        })
        .collect();
    ok(summaries)
}

pub async fn get_conversation(
    Path(id): Path<String>,
    State(state): State<ApiState>,
) -> Json<ApiResponse<Option<ConversationDetail>>> {
    let detail = state.conversations.get(&id).map(|c| ConversationDetail {
        id: c.id,
        title: c.title,
        messages: c.messages,
        created_at: c.created_at,
        updated_at: c.updated_at,
    });
    ok(detail)
}

pub async fn delete_conversation(
    Path(id): Path<String>,
    State(state): State<ApiState>,
) -> Json<ApiResponse<bool>> {
    ok(state.conversations.delete(&id))
}

// ── GET /api/tools ───────────────────────────────────────────────────

#[derive(Serialize)]
pub struct ToolInfo {
    name: String,
    description: String,
    category: String,
}

pub async fn list_tools(State(state): State<ApiState>) -> Json<ApiResponse<Vec<ToolInfo>>> {
    let agent = state.agent.lock().await;
    let names = agent.tool_names();

    let tools: Vec<ToolInfo> = names
        .into_iter()
        .map(|name| {
            let category = if name.contains("calendar") || name.contains("meeting") {
                "calendar"
            } else if name.contains("gmail") || name.contains("email") {
                "email"
            } else if name.contains("telegram") {
                "telegram"
            } else if name.contains("whatsapp") {
                "whatsapp"
            } else if name.contains("note") {
                "notes"
            } else if name.contains("reminder") {
                "reminders"
            } else {
                "other"
            };

            ToolInfo {
                description: format!("Tool: {}", &name),
                name,
                category: category.into(),
            }
        })
        .collect();

    ok(tools)
}

// ── Integrations ─────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct IntegrationInfo {
    service: String,
    status: String,
    connected_at: Option<String>,
}

/// Read the vault fresh to get real-time integration status.
fn check_vault_integration_status() -> Vec<IntegrationInfo> {
    let vault_path = crate::secrets::default_secrets_path();
    let vault = crate::secrets::SecretsVault::open(&vault_path, None).ok();

    let get = |key: &str| -> Option<String> { vault.as_ref().and_then(|v| v.get(key)) };

    let now = chrono::Utc::now().to_rfc3339();

    let google_has_creds =
        get("google.client_id").is_some() && get("google.client_secret").is_some();
    let google_has_token =
        get("google.access_token").is_some() || get("google.refresh_token").is_some();
    let google_connected = google_has_creds && google_has_token;

    vec![
        IntegrationInfo {
            service: "google_calendar".into(),
            status: if google_connected {
                "connected"
            } else {
                "disconnected"
            }
            .into(),
            connected_at: if google_connected {
                Some(now.clone())
            } else {
                None
            },
        },
        IntegrationInfo {
            service: "gmail".into(),
            status: if google_connected {
                "connected"
            } else {
                "disconnected"
            }
            .into(),
            connected_at: if google_connected {
                Some(now.clone())
            } else {
                None
            },
        },
        IntegrationInfo {
            service: "telegram".into(),
            status: if get("telegram.bot_token").is_some() {
                "connected"
            } else {
                "disconnected"
            }
            .into(),
            connected_at: if get("telegram.bot_token").is_some() {
                Some(now.clone())
            } else {
                None
            },
        },
        IntegrationInfo {
            service: "whatsapp".into(),
            status: if get("twilio.account_sid").is_some() && get("twilio.auth_token").is_some() {
                "connected"
            } else {
                "disconnected"
            }
            .into(),
            connected_at: if get("twilio.account_sid").is_some() {
                Some(now.clone())
            } else {
                None
            },
        },
        IntegrationInfo {
            service: "github".into(),
            status: if get("github.access_token").is_some() {
                "connected"
            } else {
                "disconnected"
            }
            .into(),
            connected_at: if get("github.access_token").is_some() {
                Some(now.clone())
            } else {
                None
            },
        },
        IntegrationInfo {
            service: "slack".into(),
            status: if get("slack.bot_token").is_some() {
                "connected"
            } else {
                "disconnected"
            }
            .into(),
            connected_at: if get("slack.bot_token").is_some() {
                Some(now.clone())
            } else {
                None
            },
        },
        // ── Social Media Platforms ────────────────────────────────────
        IntegrationInfo {
            service: "twitter".into(),
            status: if get("twitter.api_key").is_some() {
                "connected"
            } else {
                "disconnected"
            }
            .into(),
            connected_at: if get("twitter.api_key").is_some() {
                Some(now.clone())
            } else {
                None
            },
        },
        IntegrationInfo {
            service: "linkedin".into(),
            status: if get("linkedin.access_token").is_some() {
                "connected"
            } else {
                "disconnected"
            }
            .into(),
            connected_at: if get("linkedin.access_token").is_some() {
                Some(now.clone())
            } else {
                None
            },
        },
        IntegrationInfo {
            service: "facebook".into(),
            status: if get("facebook.access_token").is_some() {
                "connected"
            } else {
                "disconnected"
            }
            .into(),
            connected_at: if get("facebook.access_token").is_some() {
                Some(now.clone())
            } else {
                None
            },
        },
        IntegrationInfo {
            service: "instagram".into(),
            status: if get("instagram.access_token").is_some() {
                "connected"
            } else {
                "disconnected"
            }
            .into(),
            connected_at: if get("instagram.access_token").is_some() {
                Some(now.clone())
            } else {
                None
            },
        },
        IntegrationInfo {
            service: "bluesky".into(),
            status: if get("bluesky.app_password").is_some() {
                "connected"
            } else {
                "disconnected"
            }
            .into(),
            connected_at: if get("bluesky.app_password").is_some() {
                Some(now.clone())
            } else {
                None
            },
        },
        IntegrationInfo {
            service: "tiktok".into(),
            status: if get("tiktok.access_token").is_some() {
                "connected"
            } else {
                "disconnected"
            }
            .into(),
            connected_at: if get("tiktok.access_token").is_some() {
                Some(now.clone())
            } else {
                None
            },
        },
        IntegrationInfo {
            service: "youtube".into(),
            status: if get("youtube.access_token").is_some() {
                "connected"
            } else {
                "disconnected"
            }
            .into(),
            connected_at: if get("youtube.access_token").is_some() {
                Some(now.clone())
            } else {
                None
            },
        },
        IntegrationInfo {
            service: "mastodon".into(),
            status: if get("mastodon.access_token").is_some() {
                "connected"
            } else {
                "disconnected"
            }
            .into(),
            connected_at: if get("mastodon.access_token").is_some() {
                Some(now.clone())
            } else {
                None
            },
        },
        IntegrationInfo {
            service: "discord".into(),
            status: if get("discord.bot_token").is_some() {
                "connected"
            } else {
                "disconnected"
            }
            .into(),
            connected_at: if get("discord.bot_token").is_some() {
                Some(now.clone())
            } else {
                None
            },
        },
        IntegrationInfo {
            service: "reddit".into(),
            status: if get("reddit.access_token").is_some() {
                "connected"
            } else {
                "disconnected"
            }
            .into(),
            connected_at: if get("reddit.access_token").is_some() {
                Some(now.clone())
            } else {
                None
            },
        },
        IntegrationInfo {
            service: "pinterest".into(),
            status: if get("pinterest.access_token").is_some() {
                "connected"
            } else {
                "disconnected"
            }
            .into(),
            connected_at: if get("pinterest.access_token").is_some() {
                Some(now.clone())
            } else {
                None
            },
        },
        IntegrationInfo {
            service: "threads".into(),
            status: if get("threads.access_token").is_some() {
                "connected"
            } else {
                "disconnected"
            }
            .into(),
            connected_at: if get("threads.access_token").is_some() {
                Some(now.clone())
            } else {
                None
            },
        },
        // ── Publishing Platforms ──────────────────────────────────────
        IntegrationInfo {
            service: "medium".into(),
            status: if get("medium.api_key").is_some() {
                "connected"
            } else {
                "disconnected"
            }
            .into(),
            connected_at: if get("medium.api_key").is_some() {
                Some(now.clone())
            } else {
                None
            },
        },
        IntegrationInfo {
            service: "devto".into(),
            status: if get("devto.api_key").is_some() {
                "connected"
            } else {
                "disconnected"
            }
            .into(),
            connected_at: if get("devto.api_key").is_some() {
                Some(now.clone())
            } else {
                None
            },
        },
        IntegrationInfo {
            service: "hashnode".into(),
            status: if get("hashnode.api_key").is_some() {
                "connected"
            } else {
                "disconnected"
            }
            .into(),
            connected_at: if get("hashnode.api_key").is_some() {
                Some(now.clone())
            } else {
                None
            },
        },
        IntegrationInfo {
            service: "wordpress".into(),
            status: if get("wordpress.username").is_some() {
                "connected"
            } else {
                "disconnected"
            }
            .into(),
            connected_at: if get("wordpress.username").is_some() {
                Some(now)
            } else {
                None
            },
        },
    ]
}

pub async fn list_integrations(
    State(_state): State<ApiState>,
) -> Json<ApiResponse<Vec<IntegrationInfo>>> {
    ok(check_vault_integration_status())
}

#[derive(Serialize)]
pub struct ConnectResult {
    auth_url: Option<String>,
    message: String,
    requires_credentials: bool,
    credential_fields: Vec<CredentialField>,
}

#[derive(Serialize)]
pub struct CredentialField {
    name: String,
    label: String,
    field_type: String,
    required: bool,
    placeholder: String,
}

#[derive(Deserialize)]
pub struct ConnectRequest {
    credentials: Option<serde_json::Value>,
}

/// Generic connect handler for social/publishing platforms.
/// `fields` is &[(vault_key_suffix, label, field_type, required, placeholder)]
fn connect_social_credentials(
    service: &str,
    body: Option<&ConnectRequest>,
    fields: &[(&str, &str, &str, bool, &str)],
) -> Result<Json<ApiResponse<ConnectResult>>, (StatusCode, Json<ApiError>)> {
    let creds = body.and_then(|b| b.credentials.as_ref());

    // Check if all required fields are present
    let has_required = fields.iter().filter(|f| f.3).all(|f| {
        creds
            .and_then(|c| c.get(f.0))
            .and_then(|v| v.as_str())
            .map(|s| !s.is_empty())
            .unwrap_or(false)
    });

    if has_required {
        let vault_path = crate::secrets::default_secrets_path();
        let mut vault = crate::secrets::SecretsVault::open(&vault_path, None).map_err(|e| {
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Vault error: {e}"),
            )
        })?;

        for &(key, _, _, _, _) in fields {
            if let Some(val) = creds.and_then(|c| c.get(key)).and_then(|v| v.as_str()) {
                if !val.is_empty() {
                    let vault_key = format!("{}.{}", service, key);
                    vault.set(&vault_key, val).map_err(|e| {
                        err(StatusCode::INTERNAL_SERVER_ERROR, format!("Set error: {e}"))
                    })?;
                }
            }
        }

        vault.save().map_err(|e| {
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Save error: {e}"),
            )
        })?;

        // Mark the platform as enabled in the TOML config so the agent
        // registers the matching LLM tool (and the SocialManager wires up
        // the provider) on the next start.
        let enable_key = format!("social.{}_enabled", service);
        update_toml_config(&[(enable_key.as_str(), "true")]);

        tracing::info!("{} integration connected via API", service);

        Ok(ok(ConnectResult {
            auth_url: None,
            message: format!("{} connected successfully", service),
            requires_credentials: false,
            credential_fields: vec![],
        }))
    } else {
        Ok(ok(ConnectResult {
            auth_url: None,
            message: format!("{} requires credentials", service),
            requires_credentials: true,
            credential_fields: fields
                .iter()
                .map(
                    |&(name, label, ft, required, placeholder)| CredentialField {
                        name: name.into(),
                        label: label.into(),
                        field_type: ft.into(),
                        required,
                        placeholder: placeholder.into(),
                    },
                )
                .collect(),
        }))
    }
}

pub async fn connect_integration(
    Path(service): Path<String>,
    State(state): State<ApiState>,
    body: Option<Json<ConnectRequest>>,
) -> Result<Json<ApiResponse<ConnectResult>>, (StatusCode, Json<ApiError>)> {
    let body = body.map(|b| b.0);

    match service.as_str() {
        "telegram" => {
            // Telegram requires bot_token (and optional chat_id) via credentials
            let creds = body.as_ref().and_then(|b| b.credentials.as_ref());
            let bot_token = creds
                .and_then(|c| c.get("bot_token"))
                .and_then(|v| v.as_str());
            let chat_id = creds
                .and_then(|c| c.get("chat_id"))
                .and_then(|v| v.as_str());

            if let Some(token) = bot_token {
                let vault_path = crate::secrets::default_secrets_path();
                let mut vault =
                    crate::secrets::SecretsVault::open(&vault_path, None).map_err(|e| {
                        err(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            format!("Vault error: {e}"),
                        )
                    })?;
                vault.set("telegram.bot_token", token).map_err(|e| {
                    err(StatusCode::INTERNAL_SERVER_ERROR, format!("Set error: {e}"))
                })?;
                if let Some(cid) = chat_id {
                    if !cid.is_empty() {
                        vault.set("telegram.default_chat_id", cid).map_err(|e| {
                            err(StatusCode::INTERNAL_SERVER_ERROR, format!("Set error: {e}"))
                        })?;
                    }
                }
                vault.save().map_err(|e| {
                    err(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Save error: {e}"),
                    )
                })?;
                update_toml_config(&[("telegram.enabled", "true")]);
                tracing::info!("Telegram integration connected via API");

                Ok(ok(ConnectResult {
                    auth_url: None,
                    message: "Telegram connected successfully".into(),
                    requires_credentials: false,
                    credential_fields: vec![],
                }))
            } else {
                // Return required fields for Telegram
                Ok(ok(ConnectResult {
                    auth_url: None,
                    message: "Telegram requires bot token and optional chat ID".into(),
                    requires_credentials: true,
                    credential_fields: vec![
                        CredentialField {
                            name: "bot_token".into(),
                            label: "Bot Token".into(),
                            field_type: "password".into(),
                            required: true,
                            placeholder: "123456:ABCdefGHIjklMNO".into(),
                        },
                        CredentialField {
                            name: "chat_id".into(),
                            label: "Default Chat ID".into(),
                            field_type: "text".into(),
                            required: false,
                            placeholder: "-1001234567890".into(),
                        },
                    ],
                }))
            }
        }

        "whatsapp" => {
            let creds = body.as_ref().and_then(|b| b.credentials.as_ref());
            let account_sid = creds
                .and_then(|c| c.get("account_sid"))
                .and_then(|v| v.as_str());
            let auth_token = creds
                .and_then(|c| c.get("auth_token"))
                .and_then(|v| v.as_str());
            let whatsapp_from = creds
                .and_then(|c| c.get("whatsapp_from"))
                .and_then(|v| v.as_str());

            if let (Some(sid), Some(token)) = (account_sid, auth_token) {
                let vault_path = crate::secrets::default_secrets_path();
                let mut vault =
                    crate::secrets::SecretsVault::open(&vault_path, None).map_err(|e| {
                        err(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            format!("Vault error: {e}"),
                        )
                    })?;
                vault.set("twilio.account_sid", sid).map_err(|e| {
                    err(StatusCode::INTERNAL_SERVER_ERROR, format!("Set error: {e}"))
                })?;
                vault.set("twilio.auth_token", token).map_err(|e| {
                    err(StatusCode::INTERNAL_SERVER_ERROR, format!("Set error: {e}"))
                })?;
                if let Some(from) = whatsapp_from {
                    if !from.is_empty() {
                        vault.set("twilio.whatsapp_from", from).map_err(|e| {
                            err(StatusCode::INTERNAL_SERVER_ERROR, format!("Set error: {e}"))
                        })?;
                    }
                }
                vault.save().map_err(|e| {
                    err(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Save error: {e}"),
                    )
                })?;
                update_toml_config(&[("whatsapp.enabled", "true")]);
                tracing::info!("WhatsApp integration connected via API");

                Ok(ok(ConnectResult {
                    auth_url: None,
                    message: "WhatsApp connected successfully".into(),
                    requires_credentials: false,
                    credential_fields: vec![],
                }))
            } else {
                Ok(ok(ConnectResult {
                    auth_url: None,
                    message: "WhatsApp requires Twilio credentials".into(),
                    requires_credentials: true,
                    credential_fields: vec![
                        CredentialField {
                            name: "account_sid".into(),
                            label: "Twilio Account SID".into(),
                            field_type: "text".into(),
                            required: true,
                            placeholder: "ACxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx".into(),
                        },
                        CredentialField {
                            name: "auth_token".into(),
                            label: "Twilio Auth Token".into(),
                            field_type: "password".into(),
                            required: true,
                            placeholder: "your auth token".into(),
                        },
                        CredentialField {
                            name: "whatsapp_from".into(),
                            label: "WhatsApp From Number".into(),
                            field_type: "text".into(),
                            required: false,
                            placeholder: "whatsapp:+14155238886".into(),
                        },
                    ],
                }))
            }
        }

        "google_calendar" | "gmail" => {
            // Check for Google credentials first
            let creds = body.as_ref().and_then(|b| b.credentials.as_ref());

            // Allow passing client credentials in the request body
            let vault_path = crate::secrets::default_secrets_path();
            let mut vault = crate::secrets::SecretsVault::open(&vault_path, None).map_err(|e| {
                err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Vault error: {e}"),
                )
            })?;

            // If credentials are provided in body, store them first
            if let Some(creds) = creds {
                if let Some(cid) = creds.get("client_id").and_then(|v| v.as_str()) {
                    vault.set("google.client_id", cid).map_err(|e| {
                        err(StatusCode::INTERNAL_SERVER_ERROR, format!("Set error: {e}"))
                    })?;
                }
                if let Some(cs) = creds.get("client_secret").and_then(|v| v.as_str()) {
                    vault.set("google.client_secret", cs).map_err(|e| {
                        err(StatusCode::INTERNAL_SERVER_ERROR, format!("Set error: {e}"))
                    })?;
                }
                vault.save().map_err(|e| {
                    err(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Save error: {e}"),
                    )
                })?;
            }

            let client_id = vault.get("google.client_id");
            let client_secret = vault.get("google.client_secret");

            if let (Some(cid), Some(cs)) = (client_id, client_secret) {
                // Start OAuth flow
                let scopes = crate::oauth::default_google_scopes();
                let redirect_port = state.config.google_redirect_port;
                let oauth_config =
                    crate::oauth::google_oauth_config(&cid, &cs, scopes, redirect_port);

                let redirect_uri = format!("http://localhost:{}/callback", redirect_port);
                let state_param = crate::oauth::generate_state();

                let auth_url = format!(
                    "{}?client_id={}&redirect_uri={}&response_type=code&scope={}&state={}&access_type=offline&prompt=consent",
                    oauth_config.auth_url,
                    urlencoding::encode(&oauth_config.client_id),
                    urlencoding::encode(&redirect_uri),
                    urlencoding::encode(&oauth_config.scopes.join(" ")),
                    urlencoding::encode(&state_param),
                );

                // ── Cancel any in-flight Google OAuth listener before starting a new one ──
                //
                // Without this, clicking "Connect" twice (or Disconnect → Connect) leaves
                // the previous background task bound to redirect_port with the *old* state
                // string. The new task can't bind the port, dies silently, and the user's
                // browser hits the stale listener — which rejects the callback with
                // "❌ Invalid State / CSRF state mismatch".
                //
                // We hold a single global handle; a new connect attempt aborts the old one,
                // waits a beat for the OS to release the port, then spawns the new task.
                {
                    let cell = google_oauth_task_handle();
                    let mut guard = cell.lock().await;
                    if let Some(prev) = guard.take() {
                        tracing::info!(
                            "Aborting previous Google OAuth listener before starting a new one"
                        );
                        prev.abort();
                        // Give the OS a moment to release the TCP port.
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    }

                    let state_clone = state_param.clone();
                    let oauth_clone = oauth_config.clone();
                    let vault_path_clone = vault_path.clone();
                    let service_clone = service.clone();
                    let handle = tokio::spawn(async move {
                        match handle_google_oauth_callback(
                            &oauth_clone,
                            &state_clone,
                            &vault_path_clone,
                            &service_clone,
                        )
                        .await
                        {
                            Ok(_) => {
                                tracing::info!("{} OAuth completed successfully", service_clone)
                            }
                            Err(e) => {
                                tracing::error!("{} OAuth failed: {}", service_clone, e)
                            }
                        }
                        // Once finished (success or failure), clear the handle so the
                        // next connect attempt doesn't try to abort an already-finished task.
                        let cell = google_oauth_task_handle();
                        let mut guard = cell.lock().await;
                        *guard = None;
                    });
                    *guard = Some(handle);
                }

                Ok(ok(ConnectResult {
                    auth_url: Some(auth_url),
                    message: format!("Opening Google authorization for {}...", service),
                    requires_credentials: false,
                    credential_fields: vec![],
                }))
            } else {
                // Need Google credentials first
                Ok(ok(ConnectResult {
                    auth_url: None,
                    message: "Google OAuth requires client credentials first".into(),
                    requires_credentials: true,
                    credential_fields: vec![
                        CredentialField {
                            name: "client_id".into(),
                            label: "Google Client ID".into(),
                            field_type: "text".into(),
                            required: true,
                            placeholder: "xxxx.apps.googleusercontent.com".into(),
                        },
                        CredentialField {
                            name: "client_secret".into(),
                            label: "Google Client Secret".into(),
                            field_type: "password".into(),
                            required: true,
                            placeholder: "GOCSPX-xxxxxx".into(),
                        },
                    ],
                }))
            }
        }

        "github" => {
            let creds = body.as_ref().and_then(|b| b.credentials.as_ref());
            let token = creds
                .and_then(|c| c.get("access_token"))
                .and_then(|v| v.as_str());

            if let Some(token) = token {
                let vault_path = crate::secrets::default_secrets_path();
                let mut vault =
                    crate::secrets::SecretsVault::open(&vault_path, None).map_err(|e| {
                        err(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            format!("Vault error: {e}"),
                        )
                    })?;
                vault.set("github.access_token", token).map_err(|e| {
                    err(StatusCode::INTERNAL_SERVER_ERROR, format!("Set error: {e}"))
                })?;
                vault.save().map_err(|e| {
                    err(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Save error: {e}"),
                    )
                })?;
                tracing::info!("GitHub integration connected via API");

                Ok(ok(ConnectResult {
                    auth_url: None,
                    message: "GitHub connected successfully".into(),
                    requires_credentials: false,
                    credential_fields: vec![],
                }))
            } else {
                Ok(ok(ConnectResult {
                    auth_url: None,
                    message: "GitHub requires a personal access token".into(),
                    requires_credentials: true,
                    credential_fields: vec![CredentialField {
                        name: "access_token".into(),
                        label: "Personal Access Token".into(),
                        field_type: "password".into(),
                        required: true,
                        placeholder: "ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx".into(),
                    }],
                }))
            }
        }

        "slack" => {
            let creds = body.as_ref().and_then(|b| b.credentials.as_ref());
            let token = creds
                .and_then(|c| c.get("bot_token"))
                .and_then(|v| v.as_str());

            if let Some(token) = token {
                let vault_path = crate::secrets::default_secrets_path();
                let mut vault =
                    crate::secrets::SecretsVault::open(&vault_path, None).map_err(|e| {
                        err(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            format!("Vault error: {e}"),
                        )
                    })?;
                vault.set("slack.bot_token", token).map_err(|e| {
                    err(StatusCode::INTERNAL_SERVER_ERROR, format!("Set error: {e}"))
                })?;
                vault.save().map_err(|e| {
                    err(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Save error: {e}"),
                    )
                })?;
                update_toml_config(&[("social.slack_enabled", "true")]);
                tracing::info!("Slack integration connected via API");

                Ok(ok(ConnectResult {
                    auth_url: None,
                    message: "Slack connected successfully".into(),
                    requires_credentials: false,
                    credential_fields: vec![],
                }))
            } else {
                Ok(ok(ConnectResult {
                    auth_url: None,
                    message: "Slack requires a bot token".into(),
                    requires_credentials: true,
                    credential_fields: vec![CredentialField {
                        name: "bot_token".into(),
                        label: "Slack Bot Token".into(),
                        field_type: "password".into(),
                        required: true,
                        placeholder: "xoxb-xxxxxxxxxxxx-xxxxxxxxxxxx".into(),
                    }],
                }))
            }
        }

        // ── Social Media Platforms ────────────────────────────────────
        "twitter" => connect_social_credentials(
            &service,
            body.as_ref(),
            &[
                ("api_key", "API Key", "password", true, "your-api-key"),
                (
                    "api_secret",
                    "API Secret",
                    "password",
                    true,
                    "your-api-secret",
                ),
                (
                    "access_token",
                    "Access Token",
                    "password",
                    true,
                    "your-access-token",
                ),
                (
                    "access_token_secret",
                    "Access Token Secret",
                    "password",
                    true,
                    "your-access-token-secret",
                ),
            ],
        ),

        "linkedin" => {
            // Accept access_token (required) and an optional person_id. If
            // person_id isn't provided, try to discover it from /v2/userinfo
            // (works for tokens with the `openid`/`profile` scope).
            let result = connect_social_credentials(
                &service,
                body.as_ref(),
                &[
                    (
                        "access_token",
                        "Access Token",
                        "password",
                        true,
                        "your-linkedin-access-token",
                    ),
                    (
                        "person_id",
                        "Person ID (optional, auto-detected if blank)",
                        "text",
                        false,
                        "abc123XYZ",
                    ),
                ],
            )?;

            // If we just stored credentials, validate any user-supplied
            // person_id and (if missing or invalid) auto-detect it via
            // /v2/userinfo so the LinkedIn post tool has everything it needs.
            if !result.0.data.requires_credentials {
                let vault_path = crate::secrets::default_secrets_path();
                if let Ok(mut vault) = crate::secrets::SecretsVault::open(&vault_path, None) {
                    // Reject manually-pasted vanity slugs (e.g. "rupak-chandra-41cg")
                    // — LinkedIn's UGC API will reject `urn:li:person:<slug>`.
                    let stored = vault.get("linkedin.person_id");
                    let valid = stored
                        .as_ref()
                        .map(|s| {
                            let t = s.trim();
                            !t.is_empty()
                                && t.len() <= 60
                                && t.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                        })
                        .unwrap_or(false);

                    if !valid {
                        if stored.is_some() {
                            tracing::warn!(
                                "User-supplied linkedin.person_id is not the \
                                 alphanumeric LinkedIn member ID (looks like a \
                                 vanity URL slug). Discarding and auto-detecting."
                            );
                            let _ = vault.delete("linkedin.person_id");
                            let _ = vault.save();
                        }

                        if let Some(token) = vault.get("linkedin.access_token") {
                            let token = token.trim().to_string();
                            if let Ok(client) = reqwest::Client::builder()
                                .timeout(std::time::Duration::from_secs(10))
                                .user_agent("pylot/0.1 (+https://pylot.dev)")
                                .build()
                            {
                                if let Ok(resp) = client
                                    .get("https://api.linkedin.com/v2/userinfo")
                                    .bearer_auth(&token)
                                    .send()
                                    .await
                                {
                                    if resp.status().is_success() {
                                        if let Ok(json) = resp.json::<serde_json::Value>().await {
                                            if let Some(sub) = json.get("sub").and_then(|v| v.as_str()) {
                                                let _ = vault.set("linkedin.person_id", sub);
                                                let _ = vault.save();
                                                tracing::info!(
                                                    "LinkedIn person_id auto-detected via /v2/userinfo"
                                                );
                                            }
                                        }
                                    } else {
                                        // Try /v2/me as fallback (older r_liteprofile tokens)
                                        if let Ok(resp2) = client
                                            .get("https://api.linkedin.com/v2/me")
                                            .bearer_auth(&token)
                                            .header("X-Restli-Protocol-Version", "2.0.0")
                                            .send()
                                            .await
                                        {
                                            if resp2.status().is_success() {
                                                if let Ok(json) = resp2.json::<serde_json::Value>().await {
                                                    if let Some(id) = json.get("id").and_then(|v| v.as_str()) {
                                                        let _ = vault.set("linkedin.person_id", id);
                                                        let _ = vault.save();
                                                        tracing::info!(
                                                            "LinkedIn person_id auto-detected via /v2/me"
                                                        );
                                                    }
                                                }
                                            } else {
                                                tracing::warn!(
                                                    "LinkedIn person_id auto-detection failed (token may lack profile scope). \
                                                     User will need to supply person_id manually for posting."
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            Ok(result)
        }
        "instagram" | "tiktok" | "youtube" | "reddit" | "pinterest"
        | "threads" => connect_social_credentials(
            &service,
            body.as_ref(),
            &[(
                "access_token",
                "Access Token",
                "password",
                true,
                "your-access-token",
            )],
        ),

        "facebook" => connect_social_credentials(
            &service,
            body.as_ref(),
            &[
                (
                    "page_id",
                    "Page ID",
                    "text",
                    true,
                    "1234567890",
                ),
                (
                    "access_token",
                    "Page Access Token",
                    "password",
                    true,
                    "EAAG...",
                ),
            ],
        ),

        "bluesky" => connect_social_credentials(
            &service,
            body.as_ref(),
            &[
                ("handle", "Handle", "text", true, "you.bsky.social"),
                (
                    "app_password",
                    "App Password",
                    "password",
                    true,
                    "xxxx-xxxx-xxxx-xxxx",
                ),
            ],
        ),

        "mastodon" => connect_social_credentials(
            &service,
            body.as_ref(),
            &[
                (
                    "instance_url",
                    "Instance URL",
                    "text",
                    true,
                    "https://mastodon.social",
                ),
                (
                    "access_token",
                    "Access Token",
                    "password",
                    true,
                    "your-access-token",
                ),
            ],
        ),

        "discord" => connect_social_credentials(
            &service,
            body.as_ref(),
            &[
                ("bot_token", "Bot Token", "password", true, "your-bot-token"),
                (
                    "channel_id",
                    "Default Channel ID",
                    "text",
                    false,
                    "1234567890",
                ),
            ],
        ),

        // ── Publishing Platforms ──────────────────────────────────────
        "medium" | "devto" => connect_social_credentials(
            &service,
            body.as_ref(),
            &[("api_key", "API Key", "password", true, "your-api-key")],
        ),

        "hashnode" => connect_social_credentials(
            &service,
            body.as_ref(),
            &[
                ("api_key", "API Key", "password", true, "your-api-key"),
                (
                    "publication_id",
                    "Publication ID",
                    "text",
                    false,
                    "your-publication-id",
                ),
            ],
        ),

        "wordpress" => connect_social_credentials(
            &service,
            body.as_ref(),
            &[
                (
                    "site_url",
                    "Site URL",
                    "text",
                    true,
                    "https://your-site.wordpress.com",
                ),
                ("username", "Username", "text", true, "admin"),
                (
                    "app_password",
                    "Application Password",
                    "password",
                    true,
                    "xxxx xxxx xxxx xxxx",
                ),
            ],
        ),

        _ => Err(err(
            StatusCode::BAD_REQUEST,
            format!("Unknown service: {}", service),
        )),
    }
}

/// Background handler for Google OAuth callback
async fn handle_google_oauth_callback(
    oauth_config: &crate::oauth::OAuthConfig,
    expected_state: &str,
    vault_path: &std::path::Path,
    service: &str,
) -> anyhow::Result<()> {
    let redirect_uri = format!("http://localhost:{}/callback", oauth_config.redirect_port);

    // Wait for the OAuth callback (starts a temp server on redirect_port)
    let code = crate::oauth::wait_for_callback(oauth_config.redirect_port, expected_state).await?;

    // Exchange code for tokens
    let tokens = crate::oauth::exchange_code(oauth_config, &code, &redirect_uri, None).await?;

    let expires_in = tokens.expires_in.unwrap_or(3600) as i64;
    let expiry = chrono::Utc::now() + chrono::Duration::seconds(expires_in);

    // Store tokens in vault
    let mut vault = crate::secrets::SecretsVault::open(vault_path, None)?;
    vault.set("google.access_token", &tokens.access_token)?;
    if let Some(ref rt) = tokens.refresh_token {
        vault.set("google.refresh_token", rt)?;
    }
    vault.set("google.token_expiry", &expiry.to_rfc3339())?;
    vault.save()?;

    // Also write the JSON token files that the calendar/gmail tools read at runtime.
    // This ensures all code paths (vault-based and file-based) stay in sync.
    let data_dir = crate::secrets::pylot_home_dir().join("data");
    std::fs::create_dir_all(&data_dir).ok();

    let token_json = serde_json::json!({
        "access_token": tokens.access_token,
        "refresh_token": tokens.refresh_token.as_deref().unwrap_or(""),
        "expires_at": expiry.to_rfc3339(),
    });
    let json_str = serde_json::to_string_pretty(&token_json).unwrap_or_default();

    // Write for both calendar and gmail tools
    let _ = std::fs::write(data_dir.join("google_tokens.json"), &json_str);
    let _ = std::fs::write(data_dir.join("gmail_tokens.json"), &json_str);

    // Enable in TOML
    match service {
        "google_calendar" => update_toml_config(&[("google_calendar.enabled", "true")]),
        "gmail" => update_toml_config(&[("gmail.enabled", "true")]),
        _ => {
            update_toml_config(&[
                ("google_calendar.enabled", "true"),
                ("gmail.enabled", "true"),
            ]);
        }
    }

    tracing::info!("{} Google OAuth tokens stored successfully", service);
    Ok(())
}

/// Optional flags accepted by `DELETE /api/integrations/{service}`.
#[derive(Deserialize, Default)]
pub struct DisconnectQuery {
    /// For Google services only: when `true`, the OAuth `client_id` /
    /// `client_secret` are kept in the vault so the next Connect skips the
    /// credentials prompt. Default is `false` — disconnect fully clears
    /// everything so the user can switch to a different OAuth client.
    #[serde(default)]
    keep_credentials: bool,
}

pub async fn disconnect_integration(
    Path(service): Path<String>,
    Query(opts): Query<DisconnectQuery>,
    State(_state): State<ApiState>,
) -> Result<Json<ApiResponse<bool>>, (StatusCode, Json<ApiError>)> {
    let vault_path = crate::secrets::default_secrets_path();

    // If the user is disconnecting Google while a previous Connect attempt is
    // still waiting for a callback, kill that listener so the redirect port is
    // free for the next Connect (and so the stale CSRF state is gone).
    if matches!(service.as_str(), "google_calendar" | "gmail") {
        let cell = google_oauth_task_handle();
        let mut guard = cell.lock().await;
        if let Some(prev) = guard.take() {
            tracing::info!("Disconnect: aborting in-flight Google OAuth listener");
            prev.abort();
        }
    }

    // If the vault file is missing or was just quarantined as corrupted,
    // there's nothing to delete — disconnect is already effectively done.
    // `open_with_recovery` guarantees we get a usable (possibly fresh) vault.
    let (mut vault, _recovery) =
        crate::secrets::SecretsVault::open_with_recovery(&vault_path, None).map_err(|e| {
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Vault error: {e}"),
            )
        })?;

    match service.as_str() {
        "google_calendar" | "gmail" => {
            let _ = vault.delete("google.access_token");
            let _ = vault.delete("google.refresh_token");
            let _ = vault.delete("google.token_expiry");

            // By default also wipe the OAuth client credentials so the next
            // Connect prompts for them again — this is what users want when
            // switching to a different Google Cloud project / OAuth client.
            // Pass `?keep_credentials=true` to preserve them (e.g. when only
            // revoking the user grant but keeping the same app credentials).
            if !opts.keep_credentials {
                let _ = vault.delete("google.client_id");
                let _ = vault.delete("google.client_secret");
            }

            vault.save().map_err(|e| {
                err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Save error: {e}"),
                )
            })?;

            // Remove the on-disk Google token files so calendar/gmail tools
            // also see the disconnect (they read these directly).
            let data_dir = crate::secrets::pylot_home_dir().join("data");
            let _ = std::fs::remove_file(data_dir.join("google_tokens.json"));
            let _ = std::fs::remove_file(data_dir.join("gmail_tokens.json"));

            update_toml_config(&[
                ("google_calendar.enabled", "false"),
                ("gmail.enabled", "false"),
            ]);
        }
        "telegram" => {
            let _ = vault.delete("telegram.bot_token");
            let _ = vault.delete("telegram.default_chat_id");
            vault.save().map_err(|e| {
                err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Save error: {e}"),
                )
            })?;
            update_toml_config(&[("telegram.enabled", "false")]);
        }
        "whatsapp" => {
            let _ = vault.delete("twilio.account_sid");
            let _ = vault.delete("twilio.auth_token");
            vault.save().map_err(|e| {
                err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Save error: {e}"),
                )
            })?;
            update_toml_config(&[("whatsapp.enabled", "false")]);
        }
        "github" => {
            let _ = vault.delete("github.access_token");
            vault.save().map_err(|e| {
                err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Save error: {e}"),
                )
            })?;
        }
        "slack" => {
            let _ = vault.delete("slack.bot_token");
            let _ = vault.delete("slack.channel");
            vault.save().map_err(|e| {
                err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Save error: {e}"),
                )
            })?;
            update_toml_config(&[("social.slack_enabled", "false")]);
        }
        // Generic disconnect for social/publishing platforms: clear every
        // vault key that begins with `<service>.` and flip the matching
        // `social.<service>_enabled` flag in the TOML config back to false.
        "twitter" | "linkedin" | "facebook" | "instagram" | "discord" | "bluesky"
        | "mastodon" | "tiktok" | "youtube" | "reddit" | "pinterest" | "threads"
        | "medium" | "devto" | "hashnode" | "wordpress" => {
            let prefix = format!("{}.", service);
            let keys: Vec<String> = vault
                .flatten_for_test()
                .into_keys()
                .filter(|k| k.starts_with(&prefix))
                .collect();
            for k in &keys {
                let _ = vault.delete(k);
            }
            vault.save().map_err(|e| {
                err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Save error: {e}"),
                )
            })?;
            let enable_key = format!("social.{}_enabled", service);
            update_toml_config(&[(enable_key.as_str(), "false")]);
        }
        _ => {
            return Err(err(
                StatusCode::BAD_REQUEST,
                format!("Unknown service: {}", service),
            ))
        }
    }

    tracing::info!("Integration disconnected: {}", service);
    Ok(ok(true))
}

#[derive(Serialize)]
pub struct TestResult {
    healthy: bool,
    details: String,
}

pub async fn test_integration(
    Path(service): Path<String>,
    State(_state): State<ApiState>,
) -> Result<Json<ApiResponse<TestResult>>, (StatusCode, Json<ApiError>)> {
    let vault_path = crate::secrets::default_secrets_path();
    let vault = crate::secrets::SecretsVault::open(&vault_path, None).map_err(|e| {
        err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Vault error: {e}"),
        )
    })?;

    match service.as_str() {
        "telegram" => {
            if let Some(token) = vault.get("telegram.bot_token") {
                let url = format!("https://api.telegram.org/bot{}/getMe", token);
                match reqwest::get(&url).await {
                    Ok(resp) if resp.status().is_success() => {
                        let body: serde_json::Value = resp.json().await.unwrap_or_default();
                        let bot_name = body
                            .get("result")
                            .and_then(|r| r.get("username"))
                            .and_then(|u| u.as_str())
                            .unwrap_or("unknown");
                        Ok(ok(TestResult {
                            healthy: true,
                            details: format!("Connected as @{}", bot_name),
                        }))
                    }
                    Ok(resp) => Ok(ok(TestResult {
                        healthy: false,
                        details: format!("Telegram API returned HTTP {}", resp.status()),
                    })),
                    Err(e) => Ok(ok(TestResult {
                        healthy: false,
                        details: format!("Connection failed: {}", e),
                    })),
                }
            } else {
                Ok(ok(TestResult {
                    healthy: false,
                    details: "No bot token configured".into(),
                }))
            }
        }

        "whatsapp" => {
            if let (Some(sid), Some(token)) = (
                vault.get("twilio.account_sid"),
                vault.get("twilio.auth_token"),
            ) {
                let url = format!("https://api.twilio.com/2010-04-01/Accounts/{}.json", sid);
                let client = reqwest::Client::new();
                match client.get(&url).basic_auth(&sid, Some(&token)).send().await {
                    Ok(resp) if resp.status().is_success() => Ok(ok(TestResult {
                        healthy: true,
                        details: format!("Twilio account {} is active", sid),
                    })),
                    Ok(resp) => Ok(ok(TestResult {
                        healthy: false,
                        details: format!("Twilio API returned HTTP {}", resp.status()),
                    })),
                    Err(e) => Ok(ok(TestResult {
                        healthy: false,
                        details: format!("Connection failed: {}", e),
                    })),
                }
            } else {
                Ok(ok(TestResult {
                    healthy: false,
                    details: "Twilio credentials not configured".into(),
                }))
            }
        }

        "google_calendar" | "gmail" => {
            if let Some(token) = vault.get("google.access_token") {
                let url = "https://www.googleapis.com/oauth2/v1/tokeninfo";
                let client = reqwest::Client::new();
                match client
                    .get(url)
                    .query(&[("access_token", &token)])
                    .send()
                    .await
                {
                    Ok(resp) if resp.status().is_success() => Ok(ok(TestResult {
                        healthy: true,
                        details: "Google OAuth token is valid".into(),
                    })),
                    _ => {
                        // Try refresh if we have a refresh token
                        if vault.get("google.refresh_token").is_some() {
                            Ok(ok(TestResult {
                                healthy: true,
                                details: "Token expired but refresh token available".into(),
                            }))
                        } else {
                            Ok(ok(TestResult {
                                healthy: false,
                                details: "Google token expired and no refresh token".into(),
                            }))
                        }
                    }
                }
            } else {
                Ok(ok(TestResult {
                    healthy: false,
                    details: "Not connected — no access token".into(),
                }))
            }
        }

        "github" => {
            if let Some(token) = vault.get("github.access_token") {
                let client = reqwest::Client::new();
                match client
                    .get("https://api.github.com/user")
                    .header("Authorization", format!("Bearer {}", token))
                    .header("User-Agent", "pylot")
                    .send()
                    .await
                {
                    Ok(resp) if resp.status().is_success() => {
                        let body: serde_json::Value = resp.json().await.unwrap_or_default();
                        let login = body
                            .get("login")
                            .and_then(|l| l.as_str())
                            .unwrap_or("unknown");
                        Ok(ok(TestResult {
                            healthy: true,
                            details: format!("Authenticated as {}", login),
                        }))
                    }
                    Ok(resp) => Ok(ok(TestResult {
                        healthy: false,
                        details: format!("GitHub API returned HTTP {}", resp.status()),
                    })),
                    Err(e) => Ok(ok(TestResult {
                        healthy: false,
                        details: format!("Connection failed: {}", e),
                    })),
                }
            } else {
                Ok(ok(TestResult {
                    healthy: false,
                    details: "No token configured".into(),
                }))
            }
        }

        "slack" => {
            if let Some(token) = vault.get("slack.bot_token") {
                let client = reqwest::Client::new();
                match client
                    .post("https://slack.com/api/auth.test")
                    .header("Authorization", format!("Bearer {}", token))
                    .send()
                    .await
                {
                    Ok(resp) if resp.status().is_success() => {
                        let body: serde_json::Value = resp.json().await.unwrap_or_default();
                        let ok_val = body.get("ok").and_then(|o| o.as_bool()).unwrap_or(false);
                        if ok_val {
                            let team = body
                                .get("team")
                                .and_then(|t| t.as_str())
                                .unwrap_or("unknown");
                            Ok(ok(TestResult {
                                healthy: true,
                                details: format!("Connected to {}", team),
                            }))
                        } else {
                            let error = body
                                .get("error")
                                .and_then(|e| e.as_str())
                                .unwrap_or("unknown");
                            Ok(ok(TestResult {
                                healthy: false,
                                details: format!("Slack error: {}", error),
                            }))
                        }
                    }
                    Ok(resp) => Ok(ok(TestResult {
                        healthy: false,
                        details: format!("Slack API returned HTTP {}", resp.status()),
                    })),
                    Err(e) => Ok(ok(TestResult {
                        healthy: false,
                        details: format!("Connection failed: {}", e),
                    })),
                }
            } else {
                Ok(ok(TestResult {
                    healthy: false,
                    details: "No bot token configured".into(),
                }))
            }
        }

        "linkedin" => {
            if let Some(raw) = vault.get("linkedin.access_token") {
                // Tokens pasted from the LinkedIn UI sometimes pick up surrounding
                // whitespace or quotes, which silently breaks the Authorization header.
                let token = raw.trim().trim_matches('"').to_string();
                if token.is_empty() {
                    return Ok(ok(TestResult {
                        healthy: false,
                        details: "Stored access token is empty after trimming".into(),
                    }));
                }

                let client = match reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(15))
                    .user_agent("pylot/0.1 (+https://pylot.dev)")
                    .build()
                {
                    Ok(c) => c,
                    Err(e) => {
                        return Ok(ok(TestResult {
                            healthy: false,
                            details: format!("HTTP client init failed: {}", e),
                        }));
                    }
                };

                // Try the OpenID Connect userinfo endpoint first (works for tokens
                // minted with the `openid`/`profile` scopes).
                let userinfo = client
                    .get("https://api.linkedin.com/v2/userinfo")
                    .header("Authorization", format!("Bearer {}", token))
                    .send()
                    .await;

                match userinfo {
                    Ok(resp) if resp.status().is_success() => {
                        let body: serde_json::Value = resp.json().await.unwrap_or_default();
                        let name = body
                            .get("name")
                            .and_then(|v| v.as_str())
                            .or_else(|| body.get("sub").and_then(|v| v.as_str()))
                            .unwrap_or("unknown");
                        return Ok(ok(TestResult {
                            healthy: true,
                            details: format!("Authenticated as {}", name),
                        }));
                    }
                    Ok(resp) => {
                        let status = resp.status();
                        // 401 means the token itself is invalid/expired.
                        // 403 means the token is valid but doesn't have profile-read
                        // scope (`openid` / `r_liteprofile`). Many tokens are minted
                        // with only `w_member_social`, which is enough for posting
                        // but cannot read /userinfo or /me. Try /me as a secondary
                        // probe and, if that also 403s, report the token as healthy
                        // with a warning rather than failing the integration.
                        if status == reqwest::StatusCode::UNAUTHORIZED {
                            let text = resp.text().await.unwrap_or_default();
                            return Ok(ok(TestResult {
                                healthy: false,
                                details: format!(
                                    "LinkedIn rejected the token as invalid/expired \
                                     (HTTP 401). Generate a new access token. {}",
                                    text
                                ),
                            }));
                        }
                        if status == reqwest::StatusCode::FORBIDDEN {
                            let me = client
                                .get("https://api.linkedin.com/v2/me")
                                .header("Authorization", format!("Bearer {}", token))
                                .header("X-Restli-Protocol-Version", "2.0.0")
                                .send()
                                .await;
                            return Ok(ok(match me {
                                Ok(r) if r.status().is_success() => TestResult {
                                    healthy: true,
                                    details: "Token valid (r_liteprofile scope). \
                                              Add `w_member_social` if posting fails."
                                        .into(),
                                },
                                Ok(r) if r.status() == reqwest::StatusCode::FORBIDDEN => {
                                    // Both /userinfo and /me forbidden — token most
                                    // likely has only `w_member_social` (posting).
                                    // We cannot cheaply verify post scope without
                                    // actually creating a draft, so report as
                                    // healthy-with-warning.
                                    TestResult {
                                        healthy: true,
                                        details: "Token stored. LinkedIn does not \
                                                  allow scope introspection — if \
                                                  posting fails, regenerate the \
                                                  token with `w_member_social` and \
                                                  `openid`/`profile` scopes."
                                            .into(),
                                    }
                                }
                                Ok(r) => {
                                    let s = r.status();
                                    let t = r.text().await.unwrap_or_default();
                                    TestResult {
                                        healthy: false,
                                        details: format!(
                                            "LinkedIn rejected token (HTTP {}): {}",
                                            s, t
                                        ),
                                    }
                                }
                                Err(e) => TestResult {
                                    healthy: false,
                                    details: describe_network_error("api.linkedin.com", &e),
                                },
                            }));
                        }
                        let text = resp.text().await.unwrap_or_default();
                        Ok(ok(TestResult {
                            healthy: false,
                            details: format!("LinkedIn API returned HTTP {}: {}", status, text),
                        }))
                    }
                    Err(e) => Ok(ok(TestResult {
                        healthy: false,
                        details: describe_network_error("api.linkedin.com", &e),
                    })),
                }
            } else {
                Ok(ok(TestResult {
                    healthy: false,
                    details: "No access token configured".into(),
                }))
            }
        }

        "twitter" => {
            // OAuth 1.0a signing is non-trivial; do a lightweight credential
            // presence check instead of a live API call.
            let has_all = ["api_key", "api_secret", "access_token", "access_token_secret"]
                .iter()
                .all(|k| {
                    vault
                        .get(&format!("twitter.{}", k))
                        .map(|v| !v.is_empty())
                        .unwrap_or(false)
                });
            if has_all {
                Ok(ok(TestResult {
                    healthy: true,
                    details: "All 4 OAuth 1.0a keys are stored. Live API check happens on first post.".into(),
                }))
            } else {
                Ok(ok(TestResult {
                    healthy: false,
                    details: "Missing one or more of: api_key, api_secret, access_token, access_token_secret".into(),
                }))
            }
        }

        "facebook" => {
            let token = vault.get("facebook.access_token");
            let page_id = vault.get("facebook.page_id");
            match (token, page_id) {
                (Some(token), Some(page_id)) => {
                    let url = format!("https://graph.facebook.com/v22.0/{}", page_id);
                    let client = reqwest::Client::new();
                    match client
                        .get(&url)
                        .query(&[("fields", "name"), ("access_token", token.as_str())])
                        .send()
                        .await
                    {
                        Ok(resp) if resp.status().is_success() => {
                            let body: serde_json::Value = resp.json().await.unwrap_or_default();
                            let name = body
                                .get("name")
                                .and_then(|v| v.as_str())
                                .unwrap_or("unknown");
                            Ok(ok(TestResult {
                                healthy: true,
                                details: format!("Connected to Page \"{}\"", name),
                            }))
                        }
                        Ok(resp) => {
                            let status = resp.status();
                            let text = resp.text().await.unwrap_or_default();
                            Ok(ok(TestResult {
                                healthy: false,
                                details: format!("Facebook Graph API HTTP {}: {}", status, text),
                            }))
                        }
                        Err(e) => Ok(ok(TestResult {
                            healthy: false,
                            details: format!("Connection failed: {}", e),
                        })),
                    }
                }
                _ => Ok(ok(TestResult {
                    healthy: false,
                    details: "Missing page_id or access_token".into(),
                })),
            }
        }

        "discord" => {
            if let Some(token) = vault.get("discord.bot_token") {
                let client = reqwest::Client::new();
                match client
                    .get("https://discord.com/api/v10/users/@me")
                    .header("Authorization", format!("Bot {}", token))
                    .send()
                    .await
                {
                    Ok(resp) if resp.status().is_success() => {
                        let body: serde_json::Value = resp.json().await.unwrap_or_default();
                        let name = body
                            .get("username")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown");
                        Ok(ok(TestResult {
                            healthy: true,
                            details: format!("Connected as {}#{}",
                                name,
                                body.get("discriminator").and_then(|v| v.as_str()).unwrap_or("0000")),
                        }))
                    }
                    Ok(resp) => {
                        let status = resp.status();
                        let text = resp.text().await.unwrap_or_default();
                        Ok(ok(TestResult {
                            healthy: false,
                            details: format!("Discord API HTTP {}: {}", status, text),
                        }))
                    }
                    Err(e) => Ok(ok(TestResult {
                        healthy: false,
                        details: format!("Connection failed: {}", e),
                    })),
                }
            } else {
                Ok(ok(TestResult {
                    healthy: false,
                    details: "No bot token configured".into(),
                }))
            }
        }

        // Generic fallback for every other service that uses
        // `connect_social_credentials`. We don't ship live API checks for these
        // yet (each provider needs bespoke auth), but we can at least confirm
        // that credentials were stored so the UI doesn't blow up with
        // "Unknown service".
        other => {
            let prefix = format!("{}.", other);
            let stored: Vec<String> = {
                let flat = vault.flatten_for_test();
                flat.iter()
                    .filter_map(|(k, v)| {
                        if k.starts_with(&prefix) && !v.is_empty() {
                            Some(k.clone())
                        } else {
                            None
                        }
                    })
                    .collect()
            };
            if stored.is_empty() {
                Ok(ok(TestResult {
                    healthy: false,
                    details: format!("No credentials stored for {}", other),
                }))
            } else {
                Ok(ok(TestResult {
                    healthy: true,
                    details: format!(
                        "Stored {} credential(s) for {}. Live API check happens on first use.",
                        stored.len(),
                        other
                    ),
                }))
            }
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct AgentSettings {
    agent_name: String,
    user_name: Option<String>,
    model: String,
    temperature: f64,
    persona: String,
    max_context_messages: usize,
    max_tool_iterations: usize,
}

pub async fn get_settings(State(state): State<ApiState>) -> Json<ApiResponse<AgentSettings>> {
    let config = &state.config;
    ok(AgentSettings {
        agent_name: config.agent_name.clone(),
        user_name: None,
        model: config.llm_model.clone(),
        temperature: config.llm_temperature,
        persona: config.agent_persona.clone(),
        max_context_messages: config.max_context_messages,
        max_tool_iterations: config.max_tool_iterations,
    })
}

pub async fn update_settings(
    State(_state): State<ApiState>,
    Json(body): Json<serde_json::Value>,
) -> Json<ApiResponse<bool>> {
    let mut updates: Vec<(&str, String)> = vec![];

    if let Some(name) = body.get("agent_name").and_then(|v| v.as_str()) {
        updates.push(("agent.name", name.to_string()));
    }
    if let Some(persona) = body.get("persona").and_then(|v| v.as_str()) {
        updates.push(("agent.persona", persona.to_string()));
    }
    if let Some(model) = body.get("model").and_then(|v| v.as_str()) {
        updates.push(("llm.model", model.to_string()));
    }
    if let Some(temp) = body.get("temperature").and_then(|v| v.as_f64()) {
        updates.push(("llm.temperature", format!("{:.1}", temp)));
    }

    if !updates.is_empty() {
        let refs: Vec<(&str, &str)> = updates.iter().map(|(k, v)| (*k, v.as_str())).collect();
        update_toml_config(&refs);
    }

    tracing::info!("Settings updated via API");
    ok(true)
}

// ── Memory ───────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct MemoryFactResponse {
    id: String,
    content: String,
    category: String,
    created_at: Option<String>,
}

pub async fn get_memory(
    State(state): State<ApiState>,
) -> Json<ApiResponse<Vec<MemoryFactResponse>>> {
    // Use SmartMemory if available
    if let Some(ref smart_mem) = state.smart_memory {
        let user_id = &state.config.agent_name;
        match smart_mem.get_all_memories(user_id).await {
            Ok(memories) => {
                let facts: Vec<MemoryFactResponse> = memories
                    .into_iter()
                    .map(|m| MemoryFactResponse {
                        id: m.id,
                        content: m.content,
                        category: m.category.unwrap_or_else(|| "general".into()),
                        created_at: Some(m.created_at),
                    })
                    .collect();
                return ok(facts);
            }
            Err(e) => {
                tracing::warn!("SmartMemory get_all failed, falling back to legacy: {e}");
            }
        }
    }

    // Fallback to legacy memory store
    let data_dir = &state.config.data_dir;
    let memory = crate::memory::MemoryStore::load(data_dir).unwrap_or_default();

    let facts: Vec<MemoryFactResponse> = memory
        .all_facts()
        .iter()
        .enumerate()
        .map(|(i, fact)| MemoryFactResponse {
            id: format!("fact_{}", i),
            content: format!("{}: {}", fact.key, fact.value),
            category: "general".into(),
            created_at: Some(fact.learned_at.to_rfc3339()),
        })
        .collect();

    ok(facts)
}

pub async fn update_memory_fact(
    Path(id): Path<String>,
    State(state): State<ApiState>,
    Json(body): Json<serde_json::Value>,
) -> Json<ApiResponse<bool>> {
    let content = body.get("content").and_then(|v| v.as_str()).unwrap_or("");

    // Use SmartMemory if available and id is not legacy format
    if let Some(ref smart_mem) = state.smart_memory {
        if !id.starts_with("fact_") {
            match smart_mem.update_memory(&id, content).await {
                Ok(_) => return ok(true),
                Err(e) => {
                    tracing::warn!("SmartMemory update failed: {e}");
                    return ok(false);
                }
            }
        }
    }

    // Fallback to legacy memory store
    let data_dir = &state.config.data_dir;
    let mut memory = crate::memory::MemoryStore::load(data_dir).unwrap_or_default();

    if let Some(idx_str) = id.strip_prefix("fact_") {
        if let Ok(idx) = idx_str.parse::<usize>() {
            if memory.update_fact_at(idx, content) {
                let _ = memory.save(data_dir);
                return ok(true);
            }
        }
    }

    ok(false)
}

pub async fn delete_memory_fact(
    Path(id): Path<String>,
    State(state): State<ApiState>,
) -> Json<ApiResponse<bool>> {
    // Use SmartMemory if available and id is not legacy format
    if let Some(ref smart_mem) = state.smart_memory {
        if !id.starts_with("fact_") {
            match smart_mem.forget(&id).await {
                Ok(_) => return ok(true),
                Err(e) => {
                    tracing::warn!("SmartMemory delete failed: {e}");
                    return ok(false);
                }
            }
        }
    }

    // Fallback to legacy memory store
    let data_dir = &state.config.data_dir;
    let mut memory = crate::memory::MemoryStore::load(data_dir).unwrap_or_default();

    if let Some(idx_str) = id.strip_prefix("fact_") {
        if let Ok(idx) = idx_str.parse::<usize>() {
            if memory.remove_fact_at(idx) {
                let _ = memory.save(data_dir);
                return ok(true);
            }
        }
    }

    ok(false)
}

// ── Jobs ─────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct JobResponse {
    id: String,
    name: String,
    description: String,
    schedule: String,
    enabled: bool,
    last_run: Option<String>,
    next_run: Option<String>,
}

pub async fn list_jobs(State(state): State<ApiState>) -> Json<ApiResponse<Vec<JobResponse>>> {
    let sched = state.scheduler.lock().await;
    let jobs: Vec<JobResponse> = sched
        .list_jobs()
        .into_iter()
        .map(|j| JobResponse {
            id: j.name.clone(),
            name: j.name,
            description: j.description,
            schedule: j.cron_expr,
            enabled: j.enabled,
            last_run: j.last_run.map(|t| t.to_rfc3339()),
            next_run: j.next_run.map(|t| t.to_rfc3339()),
        })
        .collect();
    ok(jobs)
}

#[derive(Deserialize)]
pub struct UpdateJobRequest {
    enabled: Option<bool>,
}

pub async fn update_job(
    Path(id): Path<String>,
    State(state): State<ApiState>,
    Json(body): Json<UpdateJobRequest>,
) -> Json<ApiResponse<bool>> {
    let mut sched = state.scheduler.lock().await;
    if let Some(enabled) = body.enabled {
        let result = sched.set_enabled(&id, enabled);
        return ok(result);
    }
    ok(false)
}

pub async fn run_job(
    Path(id): Path<String>,
    State(state): State<ApiState>,
) -> Result<Json<ApiResponse<String>>, (StatusCode, Json<ApiError>)> {
    let mut sched = state.scheduler.lock().await;
    match sched.run_job(&id).await {
        Ok(result) => Ok(ok(result)),
        Err(e) => Err(err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to run job: {}", e),
        )),
    }
}

// ── Logs ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct LogsQuery {
    limit: Option<usize>,
    level: Option<String>,
}

#[derive(Serialize)]
pub struct LogEntryResponse {
    id: String,
    level: String,
    message: String,
    timestamp: String,
}

pub async fn get_logs(
    State(state): State<ApiState>,
    Query(params): Query<LogsQuery>,
) -> Json<ApiResponse<Vec<LogEntryResponse>>> {
    let limit = params.limit.unwrap_or(50);
    let home = crate::secrets::pylot_home_dir();
    let log_file = home.join("logs").join("agent.log");

    if !log_file.exists() {
        return ok(vec![]);
    }

    let content = match std::fs::read_to_string(&log_file) {
        Ok(c) => c,
        Err(_) => return ok(vec![]),
    };

    let lines: Vec<&str> = content.lines().collect();
    let start = if lines.len() > limit {
        lines.len() - limit
    } else {
        0
    };

    let entries: Vec<LogEntryResponse> = lines[start..]
        .iter()
        .enumerate()
        .map(|(i, line)| {
            let level = if line.contains("ERROR") {
                "error"
            } else if line.contains("WARN") {
                "warn"
            } else if line.contains("INFO") {
                "info"
            } else {
                "debug"
            };

            LogEntryResponse {
                id: format!("log_{}", start + i),
                level: level.into(),
                message: line.to_string(),
                timestamp: chrono::Utc::now().to_rfc3339(),
            }
        })
        .collect();

    // Filter by level if specified
    let entries = if let Some(ref level_filter) = params.level {
        entries
            .into_iter()
            .filter(|e| e.level == *level_filter)
            .collect()
    } else {
        entries
    };

    ok(entries)
}

// ── Setup ─────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct SetupStatusResponse {
    pub llm_configured: bool,
    pub telegram_configured: bool,
    pub google_configured: bool,
    pub whatsapp_configured: bool,
    pub agent_name_set: bool,
}

pub async fn get_setup_status(
    State(state): State<ApiState>,
) -> Json<ApiResponse<SetupStatusResponse>> {
    let config = &state.config;

    // Also check vault for the most current state
    let vault_path = crate::secrets::default_secrets_path();
    let vault = crate::secrets::SecretsVault::open(&vault_path, None).ok();
    let vault_has = |key: &str| -> bool { vault.as_ref().and_then(|v| v.get(key)).is_some() };

    let llm_configured = config.openai_api_key.is_some()
        || config.anthropic_api_key.is_some()
        || vault_has("llm.openai.api_key")
        || vault_has("llm.anthropic.api_key");
    let telegram_configured =
        config.telegram_bot_token.is_some() || vault_has("telegram.bot_token");
    let google_configured = (config.google_client_id.is_some()
        && config.google_client_secret.is_some())
        || (vault_has("google.client_id") && vault_has("google.client_secret"));
    let whatsapp_configured = (config.twilio_account_sid.is_some()
        && config.twilio_auth_token.is_some())
        || (vault_has("twilio.account_sid") && vault_has("twilio.auth_token"));
    let agent_name_set = config.agent_name != "Pylot" && !config.agent_name.is_empty();

    ok(SetupStatusResponse {
        llm_configured,
        telegram_configured,
        google_configured,
        whatsapp_configured,
        agent_name_set,
    })
}

#[derive(Deserialize)]
pub struct SetupLlmRequest {
    provider: String,
    model: String,
    api_key: Option<String>,
}

pub async fn setup_llm(
    State(_state): State<ApiState>,
    Json(body): Json<SetupLlmRequest>,
) -> Result<Json<ApiResponse<bool>>, (StatusCode, Json<ApiError>)> {
    let vault_path = crate::secrets::default_secrets_path();
    let mut vault = crate::secrets::SecretsVault::open(&vault_path, None).map_err(|e| {
        err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Vault error: {}", e),
        )
    })?;

    // Store API key in secrets vault
    if let Some(ref api_key) = body.api_key {
        let key_path = match body.provider.as_str() {
            "anthropic" => "llm.anthropic.api_key",
            "openai" => "llm.openai.api_key",
            _ => "llm.openai.api_key",
        };
        vault
            .set(key_path, api_key)
            .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, format!("Set key: {}", e)))?;
    }

    vault.save().map_err(|e| {
        err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Save vault: {}", e),
        )
    })?;

    // Also update TOML config for provider/model
    update_toml_config(&[("llm.provider", &body.provider), ("llm.model", &body.model)]);

    tracing::info!(
        "Setup: LLM configured — provider={}, model={}",
        body.provider,
        body.model
    );
    Ok(ok(true))
}

#[derive(Deserialize)]
pub struct SetupIdentityRequest {
    agent_name: String,
    user_name: Option<String>,
    persona: Option<String>,
}

pub async fn setup_identity(
    State(_state): State<ApiState>,
    Json(body): Json<SetupIdentityRequest>,
) -> Json<ApiResponse<bool>> {
    let mut updates: Vec<(&str, &str)> = vec![("agent.name", &body.agent_name)];
    let persona_ref;
    let user_ref;
    if let Some(ref p) = body.persona {
        persona_ref = p.clone();
        updates.push(("agent.persona", &persona_ref));
    }
    if let Some(ref u) = body.user_name {
        user_ref = u.clone();
        updates.push(("agent.user_name", &user_ref));
    }
    update_toml_config(&updates);

    tracing::info!(
        "Setup: Agent identity configured — name={}",
        body.agent_name
    );
    ok(true)
}

#[derive(Deserialize)]
pub struct SetupTelegramRequest {
    bot_token: String,
    chat_id: Option<String>,
}

pub async fn setup_telegram(
    State(_state): State<ApiState>,
    Json(body): Json<SetupTelegramRequest>,
) -> Result<Json<ApiResponse<bool>>, (StatusCode, Json<ApiError>)> {
    let vault_path = crate::secrets::default_secrets_path();
    let mut vault = crate::secrets::SecretsVault::open(&vault_path, None).map_err(|e| {
        err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Vault error: {}", e),
        )
    })?;

    vault
        .set("telegram.bot_token", &body.bot_token)
        .map_err(|e| {
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Set token: {}", e),
            )
        })?;

    if let Some(ref chat_id) = body.chat_id {
        if !chat_id.is_empty() {
            vault
                .set("telegram.default_chat_id", chat_id)
                .map_err(|e| {
                    err(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Set chat_id: {}", e),
                    )
                })?;
        }
    }

    vault.save().map_err(|e| {
        err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Save vault: {}", e),
        )
    })?;

    // Enable telegram in TOML
    update_toml_config(&[("telegram.enabled", "true")]);

    tracing::info!("Setup: Telegram configured");
    Ok(ok(true))
}

#[derive(Deserialize)]
pub struct ValidateKeyRequest {
    provider: String,
    api_key: String,
}

#[derive(Serialize)]
pub struct ValidateKeyResponse {
    valid: bool,
    error: Option<String>,
}

pub async fn validate_api_key(
    Json(body): Json<ValidateKeyRequest>,
) -> Json<ApiResponse<ValidateKeyResponse>> {
    // Quick validation: check format and attempt a lightweight API call
    let result = match body.provider.as_str() {
        "anthropic" => {
            if body.api_key.starts_with("sk-ant-") {
                ValidateKeyResponse {
                    valid: true,
                    error: None,
                }
            } else {
                ValidateKeyResponse {
                    valid: false,
                    error: Some("Anthropic keys usually start with sk-ant-".into()),
                }
            }
        }
        "openai" => {
            if body.api_key.starts_with("sk-") {
                ValidateKeyResponse {
                    valid: true,
                    error: None,
                }
            } else {
                ValidateKeyResponse {
                    valid: false,
                    error: Some("OpenAI keys usually start with sk-".into()),
                }
            }
        }
        _ => ValidateKeyResponse {
            valid: true,
            error: None,
        },
    };

    ok(result)
}

/// Helper to update the TOML config file with key-value pairs.
/// Keys use dot notation like "llm.provider", "agent.name".
fn update_toml_config(updates: &[(&str, &str)]) {
    let config_path = std::path::PathBuf::from("config/default.toml");
    if !config_path.exists() {
        return;
    }

    let content = match std::fs::read_to_string(&config_path) {
        Ok(c) => c,
        Err(_) => return,
    };

    let mut doc = match content.parse::<toml_edit::DocumentMut>() {
        Ok(d) => d,
        Err(_) => return,
    };

    for (key, value) in updates {
        let parts: Vec<&str> = key.split('.').collect();
        if parts.len() == 2 {
            let section = parts[0];
            let field = parts[1];

            // Ensure section exists
            if doc.get(section).is_none() {
                doc[section] = toml_edit::Item::Table(toml_edit::Table::new());
            }

            // Set value (try bool first, then string)
            if *value == "true" || *value == "false" {
                doc[section][field] = toml_edit::value(*value == "true");
            } else {
                doc[section][field] = toml_edit::value(*value);
            }
        }
    }

    let _ = std::fs::write(&config_path, doc.to_string());
}

// ── Knowledge Base ────────────────────────────────────────────────────
// Persistent knowledge store using JSON files in data_dir.

/// Strip filesystem-unsafe characters from a user-supplied name.
/// Prevents path traversal (`../`), absolute paths, NULs, and weird unicode.
/// Returns at most 80 characters; falls back to "untitled" if empty.
fn sanitize_filename(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect();
    // Disallow leading dots (hidden files / `..`) and collapse repeats.
    let trimmed = cleaned.trim_matches('.').trim_matches('_');
    let truncated: String = trimmed.chars().take(80).collect();
    if truncated.is_empty() {
        "untitled".to_string()
    } else {
        truncated
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct KnowledgeCollection {
    id: String,
    name: String,
    description: String,
    created_at: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct KnowledgeDocument {
    id: String,
    collection_id: String,
    title: String,
    content: String,
    source: String,
    created_at: String,
}

#[derive(Serialize)]
pub struct CollectionResponse {
    id: String,
    name: String,
    description: String,
    document_count: usize,
    created_at: String,
}

#[derive(Serialize)]
pub struct DocumentResponse {
    id: String,
    collection_id: String,
    title: String,
    source: String,
    chunk_count: usize,
    size: usize,
    created_at: String,
}

fn knowledge_collections_path(data_dir: &std::path::Path) -> PathBuf {
    data_dir.join("knowledge_collections.json")
}

fn knowledge_documents_path(data_dir: &std::path::Path) -> PathBuf {
    data_dir.join("knowledge_documents.json")
}

pub async fn list_collections(
    State(state): State<ApiState>,
) -> Json<ApiResponse<Vec<CollectionResponse>>> {
    let data_dir = &state.config.data_dir;
    let collections: Vec<KnowledgeCollection> =
        load_json_vec(&knowledge_collections_path(data_dir));
    let documents: Vec<KnowledgeDocument> = load_json_vec(&knowledge_documents_path(data_dir));

    let result: Vec<CollectionResponse> = collections
        .iter()
        .map(|c| {
            let doc_count = documents.iter().filter(|d| d.collection_id == c.id).count();
            CollectionResponse {
                id: c.id.clone(),
                name: c.name.clone(),
                description: c.description.clone(),
                document_count: doc_count,
                created_at: c.created_at.clone(),
            }
        })
        .collect();

    ok(result)
}

#[derive(Deserialize)]
pub struct CreateCollectionRequest {
    name: String,
    description: Option<String>,
}

pub async fn create_collection(
    State(state): State<ApiState>,
    Json(body): Json<CreateCollectionRequest>,
) -> Json<ApiResponse<CollectionResponse>> {
    let data_dir = &state.config.data_dir;
    let path = knowledge_collections_path(data_dir);
    let mut collections: Vec<KnowledgeCollection> = load_json_vec(&path);

    let new_collection = KnowledgeCollection {
        id: uuid::Uuid::new_v4().to_string(),
        name: body.name.clone(),
        description: body.description.unwrap_or_default(),
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    collections.push(new_collection.clone());
    save_json_vec(&path, &collections);

    tracing::info!("Knowledge collection created: {}", body.name);

    ok(CollectionResponse {
        id: new_collection.id,
        name: new_collection.name,
        description: new_collection.description,
        document_count: 0,
        created_at: new_collection.created_at,
    })
}

pub async fn delete_collection(
    Path(id): Path<String>,
    State(state): State<ApiState>,
) -> Json<ApiResponse<bool>> {
    let data_dir = &state.config.data_dir;

    // Remove collection
    let col_path = knowledge_collections_path(data_dir);
    let mut collections: Vec<KnowledgeCollection> = load_json_vec(&col_path);
    let before = collections.len();
    collections.retain(|c| c.id != id);
    let removed = collections.len() < before;
    save_json_vec(&col_path, &collections);

    // Remove all documents in this collection
    let doc_path = knowledge_documents_path(data_dir);
    let mut documents: Vec<KnowledgeDocument> = load_json_vec(&doc_path);
    documents.retain(|d| d.collection_id != id);
    save_json_vec(&doc_path, &documents);

    // Clean up embedded chunks from smart memory
    if removed {
        if let Some(ref smart_mem) = state.smart_memory {
            if let Err(e) = smart_mem.delete_knowledge_by_collection(&id).await {
                tracing::warn!("Failed to clean up chunks for collection '{}': {e}", id);
            }
        }
        tracing::info!("Knowledge collection deleted: {}", id);
    }
    ok(removed)
}

pub async fn list_documents(
    Path(collection_id): Path<String>,
    State(state): State<ApiState>,
) -> Json<ApiResponse<Vec<DocumentResponse>>> {
    let data_dir = &state.config.data_dir;
    let documents: Vec<KnowledgeDocument> = load_json_vec(&knowledge_documents_path(data_dir));

    let result: Vec<DocumentResponse> = documents
        .iter()
        .filter(|d| d.collection_id == collection_id)
        .map(|d| {
            let chunks = if d.content.is_empty() {
                0
            } else {
                (d.content.len() / 500) + 1
            };
            DocumentResponse {
                id: d.id.clone(),
                collection_id: d.collection_id.clone(),
                title: d.title.clone(),
                source: d.source.clone(),
                chunk_count: chunks,
                size: d.content.len(),
                created_at: d.created_at.clone(),
            }
        })
        .collect();

    ok(result)
}

/// List documents across all collections or filter by query param
pub async fn list_all_documents(
    State(state): State<ApiState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Json<ApiResponse<Vec<DocumentResponse>>> {
    let data_dir = &state.config.data_dir;
    let documents: Vec<KnowledgeDocument> = load_json_vec(&knowledge_documents_path(data_dir));
    let collection_id = params.get("collection_id");

    let result: Vec<DocumentResponse> = documents
        .iter()
        .filter(|d| collection_id.map_or(true, |cid| &d.collection_id == cid))
        .map(|d| {
            let chunks = if d.content.is_empty() {
                0
            } else {
                (d.content.len() / 500) + 1
            };
            DocumentResponse {
                id: d.id.clone(),
                collection_id: d.collection_id.clone(),
                title: d.title.clone(),
                source: d.source.clone(),
                chunk_count: chunks,
                size: d.content.len(),
                created_at: d.created_at.clone(),
            }
        })
        .collect();

    ok(result)
}

pub async fn delete_document(
    Path(id): Path<String>,
    State(state): State<ApiState>,
) -> Json<ApiResponse<bool>> {
    let data_dir = &state.config.data_dir;
    let path = knowledge_documents_path(data_dir);
    let mut documents: Vec<KnowledgeDocument> = load_json_vec(&path);

    // Find the document before removing so we can clean up chunks
    let doc_info = documents
        .iter()
        .find(|d| d.id == id)
        .map(|d| (d.title.clone(), d.source.clone()));

    let before = documents.len();
    documents.retain(|d| d.id != id);
    let removed = documents.len() < before;
    save_json_vec(&path, &documents);

    // Clean up embedded chunks from smart memory
    if removed {
        if let (Some((title, source)), Some(ref smart_mem)) = (doc_info, &state.smart_memory) {
            if let Err(e) = smart_mem
                .delete_knowledge_by_document(&title, &source)
                .await
            {
                tracing::warn!("Failed to clean up chunks for '{}': {e}", title);
            }
        }
        tracing::info!("Knowledge document deleted: {}", id);
    }
    ok(removed)
}

#[derive(Deserialize)]
pub struct SearchKnowledgeRequest {
    query: String,
    collection_id: Option<String>,
    limit: Option<usize>,
}

#[derive(Serialize)]
pub struct SearchResultResponse {
    chunk: String,
    content: String,
    document_title: String,
    document_id: String,
    score: f64,
}

#[derive(Serialize)]
pub struct SearchKnowledgeResponse {
    /// LLM-synthesized markdown answer built from the retrieved chunks.
    /// `None` when no chunks were found or the LLM call failed (UI falls back to raw sources).
    answer: Option<String>,
    /// Raw chunks shown as expandable "Sources" so the user can verify citations.
    results: Vec<SearchResultResponse>,
}

/// Build a well-structured markdown answer from the retrieved chunks using the chat LLM.
/// Returns `None` on any failure — caller should fall back to showing raw sources.
async fn synthesize_search_answer(
    llm: &std::sync::Arc<dyn crate::llm::LlmProvider>,
    query: &str,
    results: &[SearchResultResponse],
) -> Option<String> {
    use crate::llm::{LlmResponse, Message};

    if results.is_empty() {
        return None;
    }

    // Build a context block with citation tags the model is instructed to reuse.
    let mut context_block = String::with_capacity(2048);
    for (i, r) in results.iter().enumerate() {
        context_block.push_str(&format!(
            "[doc:{}#{}] (score {:.2})\n{}\n\n",
            r.document_title,
            i + 1,
            r.score,
            r.content.trim()
        ));
    }

    let system = Message::system(
        "You are a knowledge-base assistant. Answer the user's question using ONLY the document \
         excerpts provided. Format your answer in clean, well-structured Markdown: use a short \
         opening summary, then headings or bullet points where appropriate, and code blocks for \
         code or commands. Cite supporting facts inline using the `[doc:Title#N]` markers shown \
         next to each excerpt. If the excerpts do not contain enough information to answer, say \
         so plainly in one sentence — do not invent facts.",
    );
    let user = Message::user(format!(
        "Question: {query}\n\n--- Document excerpts ---\n{context_block}\n--- End of excerpts ---\n\nWrite the answer now."
    ));

    match llm.chat(&[system, user], &[]).await {
        Ok(LlmResponse::Text(t)) => Some(t),
        Ok(LlmResponse::TextWithThinking { text, .. }) => Some(text),
        Ok(LlmResponse::ToolCalls(_)) => None,
        Err(e) => {
            tracing::warn!("KB search synthesis LLM call failed: {e}");
            None
        }
    }
}

pub async fn search_knowledge(
    State(state): State<ApiState>,
    Json(body): Json<SearchKnowledgeRequest>,
) -> Json<ApiResponse<SearchKnowledgeResponse>> {
    let limit = body.limit.unwrap_or(10);

    // Use semantic search via SmartMemory if available
    if let Some(ref smart_mem) = state.smart_memory {
        match smart_mem.search_knowledge(&body.query, limit).await {
            Ok(results) => {
                let items: Vec<SearchResultResponse> = results
                    .into_iter()
                    .map(|entry| {
                        let title = entry.title.as_deref().unwrap_or("Unknown").to_string();
                        let doc_id = entry
                            .metadata
                            .as_ref()
                            .and_then(|m| m.get("collection_id"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        SearchResultResponse {
                            chunk: entry.content.clone(),
                            content: entry.content,
                            document_title: title,
                            document_id: doc_id,
                            score: entry.score as f64,
                        }
                    })
                    .collect();
                let answer = synthesize_search_answer(&state.llm, &body.query, &items).await;
                return ok(SearchKnowledgeResponse {
                    answer,
                    results: items,
                });
            }
            Err(e) => {
                tracing::warn!("Semantic search failed, falling back to keyword: {e}");
                // Fall through to keyword search
            }
        }
    }

    // Fallback: keyword-based scoring
    let data_dir = &state.config.data_dir;
    let documents: Vec<KnowledgeDocument> = load_json_vec(&knowledge_documents_path(data_dir));
    let query_lower = body.query.to_lowercase();
    let query_words: Vec<&str> = query_lower.split_whitespace().collect();

    let mut results: Vec<SearchResultResponse> = documents
        .iter()
        .filter(|d| {
            body.collection_id
                .as_ref()
                .map_or(true, |cid| &d.collection_id == cid)
        })
        .filter_map(|d| {
            let content_lower = d.content.to_lowercase();
            let title_lower = d.title.to_lowercase();

            // Simple keyword scoring
            let mut score = 0.0f64;
            for word in &query_words {
                if title_lower.contains(word) {
                    score += 0.4;
                }
                let matches = content_lower.matches(word).count();
                score += (matches as f64) * 0.1;
            }

            if score > 0.0 {
                // Find best snippet around first match
                let snippet_start = content_lower
                    .find(&query_words[0])
                    .unwrap_or(0)
                    .saturating_sub(100);
                let snippet_end = (snippet_start + 300).min(d.content.len());
                let snippet = &d.content[snippet_start..snippet_end];

                Some(SearchResultResponse {
                    chunk: snippet.to_string(),
                    content: snippet.to_string(),
                    document_title: d.title.clone(),
                    document_id: d.id.clone(),
                    score: (score / query_words.len() as f64).min(1.0),
                })
            } else {
                None
            }
        })
        .collect();

    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    results.truncate(limit);

    let answer = synthesize_search_answer(&state.llm, &body.query, &results).await;
    ok(SearchKnowledgeResponse { answer, results })
}

/// Upload a text document to a collection
#[derive(Deserialize)]
pub struct UploadDocumentRequest {
    collection_id: String,
    title: String,
    content: String,
    source: Option<String>,
}

pub async fn upload_document(
    State(state): State<ApiState>,
    Json(body): Json<UploadDocumentRequest>,
) -> Json<ApiResponse<DocumentResponse>> {
    let data_dir = &state.config.data_dir;
    let path = knowledge_documents_path(data_dir);
    let mut documents: Vec<KnowledgeDocument> = load_json_vec(&path);

    let new_doc = KnowledgeDocument {
        id: uuid::Uuid::new_v4().to_string(),
        collection_id: body.collection_id.clone(),
        title: body.title.clone(),
        content: body.content.clone(),
        source: body.source.unwrap_or_else(|| "upload".into()),
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    let size = new_doc.content.len();

    documents.push(new_doc.clone());
    save_json_vec(&path, &documents);

    // Save extracted content to a .txt file (sanitized name to prevent traversal)
    let knowledge_texts_dir = data_dir.join("knowledge_texts");
    if let Err(e) = std::fs::create_dir_all(&knowledge_texts_dir) {
        tracing::warn!("Failed to create knowledge_texts dir: {e}");
    } else {
        let safe_title = sanitize_filename(&body.title);
        let txt_filename = format!("{}-{}.txt", &new_doc.id, safe_title);
        let txt_path = knowledge_texts_dir.join(&txt_filename);
        match std::fs::write(&txt_path, &body.content) {
            Ok(_) => tracing::info!("Saved document text to {}", txt_path.display()),
            Err(e) => tracing::warn!("Failed to save text file '{}': {e}", txt_path.display()),
        }
    }

    // ── Real RAG indexing: chunk + embed + insert into knowledge_chunks ──
    let mut chunk_count = 0usize;
    if !new_doc.content.trim().is_empty() {
        if let Some(ref smart_mem) = state.smart_memory {
            let chunks = crate::document_chunker::chunk_document(
                &new_doc.title,
                &new_doc.content,
                &new_doc.source,
                &new_doc.collection_id,
                state.config.memory_chunk_size,
                state.config.memory_chunk_overlap,
            );
            chunk_count = chunks.len();
            match smart_mem
                .index_document_chunks_batched(&chunks, 64, |_, _| {})
                .await
            {
                Ok(n) => tracing::info!("Indexed {} chunks for '{}'", n, new_doc.title),
                Err(e) => tracing::error!("Failed to index '{}': {e}", new_doc.title),
            }
        } else {
            tracing::warn!(
                "SmartMemory not available — '{}' saved but NOT indexed",
                new_doc.title
            );
        }
    }

    tracing::info!("Knowledge document uploaded: {}", body.title);

    ok(DocumentResponse {
        id: new_doc.id,
        collection_id: new_doc.collection_id,
        title: new_doc.title,
        source: new_doc.source,
        chunk_count,
        size,
        created_at: new_doc.created_at,
    })
}

/// Upload a document with SSE progress reporting during chunk indexing.
pub async fn upload_document_stream(
    State(state): State<ApiState>,
    Json(body): Json<UploadDocumentRequest>,
) -> axum::response::sse::Sse<
    impl tokio_stream::Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>>,
> {
    use axum::response::sse::{Event, KeepAlive};
    use tokio::sync::mpsc;
    use tokio_stream::wrappers::ReceiverStream;

    let (tx, rx) = mpsc::channel::<Result<Event, std::convert::Infallible>>(64);

    // Spawn the indexing work in a background task
    let state_clone = state.clone();
    tokio::spawn(async move {
        let data_dir = &state_clone.config.data_dir;
        let path = knowledge_documents_path(data_dir);
        let mut documents: Vec<KnowledgeDocument> = load_json_vec(&path);

        let new_doc = KnowledgeDocument {
            id: uuid::Uuid::new_v4().to_string(),
            collection_id: body.collection_id.clone(),
            title: body.title.clone(),
            content: body.content.clone(),
            source: body.source.clone().unwrap_or_else(|| "upload".into()),
            created_at: chrono::Utc::now().to_rfc3339(),
        };

        let size = new_doc.content.len();
        documents.push(new_doc.clone());
        save_json_vec(&path, &documents);

        // Save extracted content to a .txt file
        let _ = tx
            .send(Ok(Event::default().data(
                serde_json::json!({"phase": "saving", "message": "Saving document text..."})
                    .to_string(),
            )))
            .await;

        let knowledge_texts_dir = data_dir.join("knowledge_texts");
        if let Err(e) = std::fs::create_dir_all(&knowledge_texts_dir) {
            tracing::warn!("Failed to create knowledge_texts dir: {e}");
        } else {
            let safe_title = sanitize_filename(&body.title);
            let txt_filename = format!("{}-{}.txt", &new_doc.id, safe_title);
            let txt_path = knowledge_texts_dir.join(&txt_filename);
            match std::fs::write(&txt_path, &body.content) {
                Ok(_) => tracing::info!("Saved document text to {}", txt_path.display()),
                Err(e) => tracing::warn!("Failed to save text file '{}': {e}", txt_path.display()),
            }
        }

        // ── Real RAG indexing: chunk + embed + insert into knowledge_chunks ──
        let mut chunk_count = 0usize;

        if body.content.trim().is_empty() {
            tracing::warn!("Skipping indexing of empty document '{}'", body.title);
        } else if let Some(ref smart_mem) = state_clone.smart_memory {
            let _ = tx
                .send(Ok(Event::default().data(
                    serde_json::json!({"phase": "chunking", "message": "Chunking document..."})
                        .to_string(),
                )))
                .await;

            let chunk_size = state_clone.config.memory_chunk_size;
            let chunk_overlap = state_clone.config.memory_chunk_overlap;
            let chunks = crate::document_chunker::chunk_document(
                &body.title,
                &body.content,
                &new_doc.source,
                &body.collection_id,
                chunk_size,
                chunk_overlap,
            );
            chunk_count = chunks.len();

            tracing::info!(
                "Chunked '{}' into {} chunks ({}w each, {}w overlap)",
                body.title,
                chunk_count,
                chunk_size,
                chunk_overlap,
            );

            let _ = tx
                .send(Ok(Event::default().data(
                    serde_json::json!({
                        "phase": "indexing",
                        "message": format!("Embedding {} chunks...", chunk_count),
                        "total": chunk_count,
                        "completed": 0
                    })
                    .to_string(),
                )))
                .await;

            // Stream-friendly progress callback uses a shared sender via try_send.
            // We can't await inside the FnMut closure, so use a small unbounded channel
            // pumped from the closure into the SSE channel.
            let (ptx, mut prx) = tokio::sync::mpsc::unbounded_channel::<(usize, usize)>();
            let tx_progress = tx.clone();
            let progress_pump = tokio::spawn(async move {
                while let Some((completed, total)) = prx.recv().await {
                    let _ = tx_progress
                        .send(Ok(Event::default().data(
                            serde_json::json!({
                                "phase": "indexing",
                                "message": format!("Embedded {}/{} chunks", completed, total),
                                "total": total,
                                "completed": completed
                            })
                            .to_string(),
                        )))
                        .await;
                }
            });

            // Embedding batch size of 64 keeps OpenAI requests well under token limits.
            match smart_mem
                .index_document_chunks_batched(&chunks, 64, move |completed, total| {
                    let _ = ptx.send((completed, total));
                })
                .await
            {
                Ok(n) => {
                    tracing::info!("Indexed {} chunks for '{}'", n, body.title);
                }
                Err(e) => {
                    tracing::error!("Failed to index '{}': {e}", body.title);
                    let _ = tx
                        .send(Ok(Event::default().data(
                            serde_json::json!({
                                "phase": "error",
                                "message": format!("Indexing failed: {}", e)
                            })
                            .to_string(),
                        )))
                        .await;
                }
            }
            // progress_pump will end when ptx is dropped (after closure finishes)
            let _ = progress_pump.await;
        } else {
            tracing::warn!(
                "SmartMemory not available — document '{}' saved but NOT indexed",
                body.title
            );
        }

        // Send completion
        let _ = tx
            .send(Ok(Event::default().data(
                serde_json::json!({
                    "phase": "done",
                    "message": "Upload complete!",
                    "document": {
                        "id": new_doc.id,
                        "collection_id": new_doc.collection_id,
                        "title": new_doc.title,
                        "source": new_doc.source,
                        "chunk_count": chunk_count,
                        "size": size,
                        "created_at": new_doc.created_at,
                    }
                })
                .to_string(),
            )))
            .await;
    });

    // Convert mpsc receiver into an SSE stream
    let sse_stream = ReceiverStream::new(rx);

    axum::response::sse::Sse::new(sse_stream).keep_alive(KeepAlive::default())
}

/// Setup WhatsApp (Twilio) credentials
#[derive(Deserialize)]
pub struct SetupWhatsAppRequest {
    account_sid: String,
    auth_token: String,
    whatsapp_from: Option<String>,
}

pub async fn setup_whatsapp(
    State(_state): State<ApiState>,
    Json(body): Json<SetupWhatsAppRequest>,
) -> Result<Json<ApiResponse<bool>>, (StatusCode, Json<ApiError>)> {
    let vault_path = crate::secrets::default_secrets_path();
    let mut vault = crate::secrets::SecretsVault::open(&vault_path, None).map_err(|e| {
        err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Vault error: {}", e),
        )
    })?;

    vault
        .set("twilio.account_sid", &body.account_sid)
        .map_err(|e| {
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Set error: {}", e),
            )
        })?;
    vault
        .set("twilio.auth_token", &body.auth_token)
        .map_err(|e| {
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Set error: {}", e),
            )
        })?;

    if let Some(ref from) = body.whatsapp_from {
        if !from.is_empty() {
            vault.set("twilio.whatsapp_from", from).map_err(|e| {
                err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Set error: {}", e),
                )
            })?;
        }
    }

    vault.save().map_err(|e| {
        err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Save error: {}", e),
        )
    })?;

    update_toml_config(&[("whatsapp.enabled", "true")]);

    tracing::info!("Setup: WhatsApp configured");
    Ok(ok(true))
}

/// Setup Google OAuth client credentials (not the access token — just client ID/secret)
#[derive(Deserialize)]
pub struct SetupGoogleRequest {
    client_id: String,
    client_secret: String,
}

pub async fn setup_google(
    State(_state): State<ApiState>,
    Json(body): Json<SetupGoogleRequest>,
) -> Result<Json<ApiResponse<bool>>, (StatusCode, Json<ApiError>)> {
    let vault_path = crate::secrets::default_secrets_path();
    let mut vault = crate::secrets::SecretsVault::open(&vault_path, None).map_err(|e| {
        err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Vault error: {}", e),
        )
    })?;

    vault
        .set("google.client_id", &body.client_id)
        .map_err(|e| {
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Set error: {}", e),
            )
        })?;
    vault
        .set("google.client_secret", &body.client_secret)
        .map_err(|e| {
            err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Set error: {}", e),
            )
        })?;

    vault.save().map_err(|e| {
        err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Save error: {}", e),
        )
    })?;

    tracing::info!("Setup: Google client credentials configured");
    Ok(ok(true))
}

// ── Document Extraction ──────────────────────────────────────────────

use crate::tools::document_loader::{detect_document_type, DocumentContent, DocumentLoader};
use axum::extract::Multipart;

/// Better multipart file upload handler using axum's Multipart extractor
pub async fn extract_document_multipart(
    State(state): State<ApiState>,
    mut multipart: Multipart,
) -> Result<Json<ApiResponse<DocumentContent>>, (StatusCode, Json<ApiError>)> {
    tracing::info!("Received document extraction request");
    let loader = DocumentLoader::new();

    let mut file_bytes: Option<Vec<u8>> = None;
    let mut filename: Option<String> = None;
    let mut document_type: Option<String> = None;

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        tracing::error!("Failed to read multipart field: {}", e);
        err(
            StatusCode::BAD_REQUEST,
            format!("Failed to read multipart field: {}", e),
        )
    })? {
        let field_name = field.name().map(|s| s.to_string());
        tracing::debug!("Processing field: {:?}", field_name);

        match field_name.as_deref() {
            Some("file") => {
                filename = field.file_name().map(|s| s.to_string());
                tracing::info!("Received file: {:?}", filename);
                let data = field.bytes().await.map_err(|e| {
                    tracing::error!("Failed to read file data: {}", e);
                    err(
                        StatusCode::BAD_REQUEST,
                        format!("Failed to read file data: {}", e),
                    )
                })?;
                tracing::info!("File data size: {} bytes", data.len());
                file_bytes = Some(data.to_vec());
            }
            Some("document_type") => {
                let data = field.text().await.map_err(|e| {
                    tracing::error!("Failed to read document_type: {}", e);
                    err(
                        StatusCode::BAD_REQUEST,
                        format!("Failed to read document_type: {}", e),
                    )
                })?;
                tracing::info!("Document type specified: {}", data);
                document_type = Some(data);
            }
            _ => {
                tracing::warn!("Skipping unknown field: {:?}", field_name);
            }
        }
    }

    let file_bytes = file_bytes.ok_or_else(|| {
        tracing::error!("No file uploaded in request");
        err(StatusCode::BAD_REQUEST, "No file uploaded".to_string())
    })?;

    let filename = filename.unwrap_or_else(|| "unknown".to_string());
    tracing::info!("Processing file: {} ({} bytes)", filename, file_bytes.len());

    let doc_type = document_type
        .or_else(|| detect_document_type(&filename))
        .ok_or_else(|| {
            tracing::error!("Could not determine document type for: {}", filename);
            err(
                StatusCode::BAD_REQUEST,
                format!("Could not determine document type for '{}'. Please specify document_type or use a file with a recognized extension.", filename),
            )
        })?;

    tracing::info!("Detected document type: {}", doc_type);

    tracing::info!("Starting document extraction...");
    match loader
        .extract_from_bytes(&file_bytes, &filename, &doc_type)
        .await
    {
        Ok(content) => {
            tracing::info!(
                "Document extracted successfully: {} ({} bytes of content)",
                filename,
                content.content.len()
            );

            // Save extracted content to .txt file immediately so it's not lost
            let knowledge_texts_dir = state.config.data_dir.join("knowledge_texts");
            if let Err(e) = std::fs::create_dir_all(&knowledge_texts_dir) {
                tracing::warn!("Failed to create knowledge_texts dir: {e}");
            } else {
                let safe_name = sanitize_filename(&filename);
                let txt_filename = format!("extract-{}.txt", safe_name);
                let txt_path = knowledge_texts_dir.join(&txt_filename);
                match std::fs::write(&txt_path, &content.content) {
                    Ok(_) => tracing::info!("Saved extracted text to {}", txt_path.display()),
                    Err(e) => {
                        tracing::warn!("Failed to save text file '{}': {e}", txt_path.display())
                    }
                }
            }

            Ok(ok(content))
        }
        Err(e) => {
            tracing::error!("Failed to extract document {}: {}", filename, e);
            Err(err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to extract document: {}", e),
            ))
        }
    }
}

// ── Skills API ───────────────────────────────────────────────────────

pub async fn list_skills_api(
    State(state): State<ApiState>,
) -> Json<ApiResponse<Vec<serde_json::Value>>> {
    let registry = crate::skills::SkillRegistry::load_all(None);
    let all = registry.all_skills();

    // Load per-skill config (enabled/disabled state)
    let config_path = state.config.data_dir.join("skills-config.json");
    let configs: std::collections::HashMap<String, crate::skills::SkillEntryConfig> =
        if config_path.exists() {
            std::fs::read_to_string(&config_path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            std::collections::HashMap::new()
        };

    let mut skills: Vec<serde_json::Value> = all
        .iter()
        .map(|s| {
            let enabled = configs
                .get(&s.meta.name)
                .and_then(|c| c.enabled)
                .unwrap_or(true);
            serde_json::json!({
                "name": s.meta.name,
                "description": s.meta.description,
                "category": s.meta.category.as_deref().unwrap_or("other"),
                "triggers": s.meta.tags,
                "examples": s.meta.examples,
                "enabled": enabled,
            })
        })
        .collect();

    // Sort by category then name for consistent ordering
    skills.sort_by(|a, b| {
        let cat_a = a["category"].as_str().unwrap_or("");
        let cat_b = b["category"].as_str().unwrap_or("");
        cat_a.cmp(cat_b).then_with(|| {
            let name_a = a["name"].as_str().unwrap_or("");
            let name_b = b["name"].as_str().unwrap_or("");
            name_a.cmp(name_b)
        })
    });

    ok(skills)
}

/// GET /skills/status — Full status report (for frontend dashboard).
/// Returns eligibility, missing deps, security scan, install options.
pub async fn skills_status_api(
    State(_state): State<ApiState>,
) -> Json<ApiResponse<serde_json::Value>> {
    let registry = crate::skills::SkillRegistry::load_all(None);
    let all_skills = registry.all_skills();
    let entry_configs = std::collections::HashMap::new(); // TODO: load from config file
    let report = crate::skills::build_status_report(&all_skills, &entry_configs);
    ok(serde_json::to_value(report).unwrap_or_default())
}

/// POST /skills/update — Enable/disable a skill or set its API key.
#[derive(Deserialize)]
pub struct SkillUpdateRequest {
    pub skill_key: String,
    pub enabled: Option<bool>,
    pub api_key: Option<String>,
}

pub async fn skills_update_api(
    State(state): State<ApiState>,
    Json(body): Json<SkillUpdateRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    // Write to config file at data_dir/skills-config.json
    let config_path = state.config.data_dir.join("skills-config.json");
    let mut configs: std::collections::HashMap<String, crate::skills::SkillEntryConfig> =
        if config_path.exists() {
            std::fs::read_to_string(&config_path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            std::collections::HashMap::new()
        };

    let entry = configs.entry(body.skill_key.clone()).or_default();
    if let Some(enabled) = body.enabled {
        entry.enabled = Some(enabled);
    }
    if let Some(api_key) = body.api_key {
        entry.api_key = if api_key.is_empty() {
            None
        } else {
            Some(api_key)
        };
    }

    match serde_json::to_string_pretty(&configs) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&config_path, json) {
                return ok(serde_json::json!({ "error": format!("Failed to save: {}", e) }));
            }
            ok(serde_json::json!({ "status": "ok", "skill_key": body.skill_key }))
        }
        Err(e) => ok(serde_json::json!({ "error": format!("Serialization error: {}", e) })),
    }
}

/// DELETE /skills/delete/{name} — Delete a skill from disk.
pub async fn skill_delete_api(
    State(_state): State<ApiState>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Json<ApiResponse<serde_json::Value>> {
    // Find the skill directory across all sources
    let mut removed = false;
    let categories: &[&str] = &[
        "agentic",
        "coding",
        "communication",
        "productivity",
        "research",
    ];

    // Try local skills dir first (~/.pylot/skills/)
    let local_dir = crate::skills::SkillLoader::local_skills_dir();
    if let Some(dir) = local_dir {
        for category in categories {
            let skill_path = dir.join(category).join(&name);
            if skill_path.exists() {
                if let Err(e) = std::fs::remove_dir_all(&skill_path) {
                    return ok(serde_json::json!({ "error": format!("Failed to delete: {}", e) }));
                }
                removed = true;
                break;
            }
        }
    }

    // Try bundled skills dir
    if !removed {
        if let Some(dir) = crate::skills::SkillLoader::bundled_skills_dir() {
            for category in categories {
                let skill_path = dir.join(category).join(&name);
                if skill_path.exists() {
                    if let Err(e) = std::fs::remove_dir_all(&skill_path) {
                        return ok(
                            serde_json::json!({ "error": format!("Failed to delete: {}", e) }),
                        );
                    }
                    removed = true;
                    break;
                }
            }
        }
    }

    if removed {
        ok(serde_json::json!({ "status": "ok", "deleted": name }))
    } else {
        ok(serde_json::json!({ "error": format!("Skill '{}' not found", name) }))
    }
}

/// GET /skills/:name — Get a single skill's full details.
pub async fn skill_detail_api(
    State(_state): State<ApiState>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Json<ApiResponse<serde_json::Value>> {
    let registry = crate::skills::SkillRegistry::load_all(None);
    match registry.get(&name) {
        Some(skill) => ok(serde_json::json!({
            "name": skill.meta.name,
            "description": skill.meta.description,
            "version": skill.meta.version,
            "category": skill.meta.category,
            "tags": skill.meta.tags,
            "author": skill.meta.author,
            "os": skill.meta.os,
            "source": format!("{:?}", skill.source),
            "source_path": skill.source_path.display().to_string(),
            "content": skill.content,
            "requires": skill.meta.requires.as_ref().map(|r| serde_json::json!({
                "bins": r.bins,
                "env": r.env,
                "tools": r.tools,
            })),
        })),
        None => ok(serde_json::json!({ "error": "Skill not found" })),
    }
}

/// POST /skills/scan — Security scan a skill directory.
#[derive(Deserialize)]
pub struct SkillScanRequest {
    pub skill_name: String,
}

pub async fn skill_scan_api(
    State(_state): State<ApiState>,
    Json(body): Json<SkillScanRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    let registry = crate::skills::SkillRegistry::load_all(None);
    match registry.get(&body.skill_name) {
        Some(skill) => {
            if let Some(skill_dir) = skill.source_path.parent() {
                let summary = crate::skills::scan_skill_directory(skill_dir);
                ok(serde_json::to_value(summary).unwrap_or_default())
            } else {
                ok(serde_json::json!({ "error": "Cannot resolve skill directory" }))
            }
        }
        None => ok(serde_json::json!({ "error": "Skill not found" })),
    }
}

// ── Learning API ─────────────────────────────────────────────────────

pub async fn list_learned_rules(
    State(state): State<ApiState>,
) -> Json<ApiResponse<serde_json::Value>> {
    let db_path = state.config.data_dir.join("learning.db");
    match crate::learning::PromptEvolution::new(&db_path.to_string_lossy()) {
        Ok(pe) => match pe.active_rules() {
            Ok(rules) => {
                let rules_json: Vec<serde_json::Value> = rules
                    .iter()
                    .map(|r| {
                        serde_json::json!({
                            "id": r.id,
                            "rule_text": r.rule_text,
                            "confidence": r.confidence,
                            "success_count": r.success_count,
                            "failure_count": r.failure_count,
                        })
                    })
                    .collect();
                ok(serde_json::json!({ "rules": rules_json }))
            }
            Err(e) => ok(serde_json::json!({ "error": e })),
        },
        Err(e) => ok(serde_json::json!({ "error": e })),
    }
}

#[derive(Deserialize)]
pub struct FeedbackRequest {
    pub session_id: String,
    pub turn_id: String,
    pub rating: i8,
    pub comment: Option<String>,
}

pub async fn submit_feedback(
    State(state): State<ApiState>,
    Json(body): Json<FeedbackRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    let db_path = state.config.data_dir.join("learning.db");
    match crate::learning::PromptEvolution::new(&db_path.to_string_lossy()) {
        Ok(pe) => {
            let fb = crate::learning::FeedbackProcessor::create_feedback(
                &body.session_id,
                &body.turn_id,
                body.rating,
                body.comment,
            );
            match crate::learning::FeedbackProcessor::process(&pe, &fb) {
                Ok(()) => ok(serde_json::json!({ "status": "ok" })),
                Err(e) => ok(serde_json::json!({ "error": e })),
            }
        }
        Err(e) => ok(serde_json::json!({ "error": e })),
    }
}

// ── MCP API ──────────────────────────────────────────────────────────

pub async fn list_mcp_servers(
    State(state): State<ApiState>,
) -> Json<ApiResponse<serde_json::Value>> {
    if let Some(ref registry) = state.mcp_registry {
        let reg = registry.lock().await;
        let servers = reg.server_names();
        let count = servers.len();
        ok(serde_json::json!({ "servers": servers, "count": count }))
    } else {
        ok(serde_json::json!({ "servers": [], "count": 0, "message": "MCP not enabled" }))
    }
}

pub async fn list_mcp_tools(State(state): State<ApiState>) -> Json<ApiResponse<serde_json::Value>> {
    if let Some(ref registry) = state.mcp_registry {
        let reg = registry.lock().await;
        let tools = reg.list_tools();
        let count = tools.len();
        let items: Vec<serde_json::Value> = tools
            .iter()
            .map(|(prefixed, def)| {
                serde_json::json!({
                    "name": prefixed,
                    "description": def.description,
                    "server": def.server_name,
                })
            })
            .collect();
        ok(serde_json::json!({ "tools": items, "count": count }))
    } else {
        ok(serde_json::json!({ "tools": [], "count": 0, "message": "MCP not enabled" }))
    }
}

// ── Social Media API ─────────────────────────────────────────────────

pub async fn list_social_posts(
    State(state): State<ApiState>,
) -> Json<ApiResponse<serde_json::Value>> {
    if let Some(ref sm) = state.social_manager {
        let manager = sm.lock().await;
        let posts = manager.list_posts();
        let count = posts.len();
        ok(serde_json::json!({ "posts": posts, "count": count }))
    } else {
        ok(serde_json::json!({ "posts": [], "count": 0, "message": "Social not enabled" }))
    }
}

#[derive(Deserialize)]
pub struct CreatePostRequest {
    pub platform: String,
    pub content: String,
    pub hashtags: Option<Vec<String>>,
    pub campaign_id: Option<String>,
    pub content_type: Option<String>,
    pub media_urls: Option<Vec<String>>,
    pub title: Option<String>,
}

pub async fn create_social_post(
    State(state): State<ApiState>,
    Json(body): Json<CreatePostRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    if let Some(ref sm) = state.social_manager {
        let mut manager = sm.lock().await;
        let platform = match crate::social::Platform::from_str(&body.platform) {
            Some(p) => p,
            None => {
                return ok(
                    serde_json::json!({"error": format!("Unknown platform: {}", body.platform)}),
                )
            }
        };
        // Strip markdown at create time so the stored draft, the post-card
        // preview, and what eventually ships to the platform are all the same
        // plain-text string. Otherwise users see `**bold**` in the UI even
        // after a successful publish (the publish path strips on the way out
        // but never updates the stored copy).
        let cleaned = crate::social::strip_markdown(&body.content);

        let mut content_type = body
            .content_type
            .as_deref()
            .map(crate::social::ContentType::from_str)
            .unwrap_or(crate::social::ContentType::Text);
        let media_urls = body.media_urls.clone().unwrap_or_default();

        if matches!(content_type, crate::social::ContentType::Text) && !media_urls.is_empty() {
            let looks_like_pdf = media_urls
                .iter()
                .any(|u| u.to_lowercase().split('?').next().unwrap_or("").ends_with(".pdf"));
            content_type = if looks_like_pdf {
                crate::social::ContentType::Document
            } else {
                crate::social::ContentType::Image
            };
        }

        tracing::info!(
            content_type = %content_type.as_str(),
            media_urls_count = media_urls.len(),
            "create_social_post: dispatching"
        );

        let post_id = manager.create_post_with_media(
            platform,
            &cleaned,
            body.hashtags.unwrap_or_default(),
            body.campaign_id,
            content_type,
            body.title.clone(),
            media_urls,
        );
        ok(serde_json::json!({
            "id": post_id,
            "platform": body.platform,
            "content": cleaned,
            "status": "draft"
        }))
    } else {
        ok(serde_json::json!({"error": "Social media manager not enabled"}))
    }
}


/// Upload a media file (image or PDF) for use in a social post.
///
/// Returns a URL the publish flow can fetch from. The file is stored under
/// `<data_dir>/uploads/` and served from `/uploads/<filename>`. This works
/// because LinkedIn's publish path runs inside the same backend process —
/// our `upload_media_from_url` fetches the URL, then PUTs the bytes to
/// LinkedIn's signed upload endpoint, so localhost URLs work fine.
pub async fn upload_social_media(
    State(state): State<ApiState>,
    mut multipart: Multipart,
) -> Result<Json<ApiResponse<serde_json::Value>>, (StatusCode, Json<ApiError>)> {
    use std::io::Write;

    let uploads_dir = state.config.data_dir.join("uploads");
    if let Err(e) = std::fs::create_dir_all(&uploads_dir) {
        tracing::error!("Failed to create uploads dir: {e}");
        return Err(err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to create uploads dir: {e}"),
        ));
    }

    let mut file_bytes: Option<Vec<u8>> = None;
    let mut original_name: Option<String> = None;
    let mut content_type: Option<String> = None;

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        err(
            StatusCode::BAD_REQUEST,
            format!("Failed to read multipart field: {e}"),
        )
    })? {
        if field.name() == Some("file") {
            original_name = field.file_name().map(|s| s.to_string());
            content_type = field.content_type().map(|s| s.to_string());
            let bytes = field.bytes().await.map_err(|e| {
                err(
                    StatusCode::BAD_REQUEST,
                    format!("Failed to read file data: {e}"),
                )
            })?;
            file_bytes = Some(bytes.to_vec());
        }
    }

    let bytes = file_bytes.ok_or_else(|| {
        err(StatusCode::BAD_REQUEST, "No `file` field in upload".to_string())
    })?;
    let original = original_name.unwrap_or_else(|| "upload.bin".to_string());

    let extension = std::path::Path::new(&original)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();
    let allowed = matches!(
        extension.as_str(),
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "pdf"
    );
    if !allowed {
        return Err(err(
            StatusCode::BAD_REQUEST,
            format!("Unsupported file type '.{extension}'. Allowed: jpg, jpeg, png, gif, webp, pdf"),
        ));
    }

    if bytes.len() > 25 * 1024 * 1024 {
        return Err(err(
            StatusCode::BAD_REQUEST,
            format!("File too large ({} bytes). Max 25 MB.", bytes.len()),
        ));
    }

    let id = uuid::Uuid::new_v4().to_string();
    let filename = format!("{id}.{extension}");
    let dest = uploads_dir.join(&filename);

    let mut file = match std::fs::File::create(&dest) {
        Ok(f) => f,
        Err(e) => {
            tracing::error!(path = %dest.display(), error = %e, "failed to create upload file");
            return Err(err(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to create upload file: {e}"),
            ));
        }
    };
    if let Err(e) = file.write_all(&bytes) {
        tracing::error!(path = %dest.display(), error = %e, "failed to write upload file");
        return Err(err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to write upload file: {e}"),
        ));
    }

    let host = std::env::var("PYLOT_PUBLIC_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:3001".to_string());
    let url = format!("{host}/uploads/{filename}");

    Ok(ok(serde_json::json!({
        "url": url,
        "filename": filename,
        "original_name": original,
        "content_type": content_type,
        "size_bytes": bytes.len(),
    })))
}


pub async fn delete_social_post(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Json<ApiResponse<serde_json::Value>> {
    if let Some(ref sm) = state.social_manager {
        let mut manager = sm.lock().await;
        if manager.delete_post(&id) {
            ok(serde_json::json!({"id": id, "deleted": true}))
        } else {
            ok(serde_json::json!({"error": "Post not found"}))
        }
    } else {
        ok(serde_json::json!({"error": "Social media manager not enabled"}))
    }
}

pub async fn publish_social_post(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Json<ApiResponse<serde_json::Value>> {
    if let Some(ref sm) = state.social_manager {
        let mut manager = sm.lock().await;
        match manager.publish_post(&id).await {
            Ok(platform_id) => ok(serde_json::json!({
                "id": id,
                "platform_post_id": platform_id,
                "status": "published"
            })),
            Err(e) => ok(serde_json::json!({"error": e})),
        }
    } else {
        ok(serde_json::json!({"error": "Social media manager not enabled"}))
    }
}

// ── POST /api/social/improve-post ────────────────────────────────────

#[derive(Deserialize)]
pub struct ImprovePostRequest {
    pub content: String,
    /// Target platform e.g. "linkedin", "twitter". Defaults to "linkedin".
    pub platform: Option<String>,
}

pub async fn improve_social_post(
    State(state): State<ApiState>,
    Json(body): Json<ImprovePostRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    let draft = body.content.trim();
    if draft.is_empty() {
        return ok(serde_json::json!({ "error": "Cannot improve empty content." }));
    }

    let platform = body.platform.as_deref().unwrap_or("linkedin");
    let (audience, char_limit) = match platform {
        "twitter"  => ("X (Twitter)", 280usize),
        "threads"  => ("Threads", 500),
        "bluesky"  => ("Bluesky", 300),
        "facebook" => ("Facebook", 5000),
        "reddit"   => ("Reddit", 10000),
        _          => ("LinkedIn", 3000),
    };

    let system_prompt = format!(
        "You are a {audience} post editor. The user will give you a draft post.          Your job:
         1. Fix grammar, spelling, and punctuation.
         2. Improve structure and readability for the {audience} audience.
         3. Suggest what to add or remove to make the post more engaging.
         4. Keep it under {char_limit} characters.

         IMPORTANT OUTPUT RULES:
         - Return ONLY the improved post text. Nothing else.
         - No markdown formatting (no **bold**, *italic*, ## headings,            backticks, or bullet asterisks). {audience} renders plain text.
         - No commentary, no explanation, no preamble. Just the post itself.
         - Preserve hashtags (#example) and mentions (@example)."
    );

    use crate::llm::{Message, LlmResponse};
    let messages = vec![
        Message::system(system_prompt),
        Message::user(format!("Draft to improve:\n\n{draft}")),
    ];

    match state.llm.chat(&messages, &[]).await {
        Ok(LlmResponse::Text(text)) | Ok(LlmResponse::TextWithThinking { text, .. }) => {
            let cleaned = crate::social::strip_markdown(&text);
            ok(serde_json::json!({
                "improved": cleaned,
                "original_length": draft.chars().count(),
                "improved_length": cleaned.chars().count(),
            }))
        }
        Ok(LlmResponse::ToolCalls(_)) => ok(serde_json::json!({
            "error": "LLM tried to call a tool — improver should be tool-free."
        })),
        Err(e) => ok(serde_json::json!({ "error": format!("LLM error: {e}") })),
    }
}

pub async fn list_campaigns(State(state): State<ApiState>) -> Json<ApiResponse<serde_json::Value>> {
    if let Some(ref sm) = state.social_manager {
        let manager = sm.lock().await;
        let campaigns = manager.list_campaigns();
        let count = campaigns.len();
        ok(serde_json::json!({ "campaigns": campaigns, "count": count }))
    } else {
        ok(serde_json::json!({ "campaigns": [], "count": 0 }))
    }
}

#[derive(Deserialize)]
pub struct CreateCampaignRequest {
    pub name: String,
    pub description: Option<String>,
    pub platforms: Vec<String>,
}

pub async fn create_campaign(
    State(state): State<ApiState>,
    Json(body): Json<CreateCampaignRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    if let Some(ref sm) = state.social_manager {
        let mut manager = sm.lock().await;
        let platforms: Vec<crate::social::Platform> = body
            .platforms
            .iter()
            .filter_map(|p| crate::social::Platform::from_str(p))
            .collect();
        let desc = body.description.as_deref().unwrap_or("");
        let id = manager.create_campaign(&body.name, desc, platforms);
        ok(serde_json::json!({
            "id": id,
            "name": body.name,
            "status": "planning"
        }))
    } else {
        ok(serde_json::json!({"error": "Social media manager not enabled"}))
    }
}

pub async fn list_social_platforms(
    State(state): State<ApiState>,
) -> Json<ApiResponse<serde_json::Value>> {
    if let Some(ref sm) = state.social_manager {
        let manager = sm.lock().await;
        let connected: Vec<String> = manager
            .connected_platforms()
            .iter()
            .map(|p| format!("{:?}", p))
            .collect();
        ok(serde_json::json!({
            "connected": connected,
            "count": connected.len()
        }))
    } else {
        ok(serde_json::json!({ "connected": [], "count": 0 }))
    }
}

pub async fn connect_social_platform(
    State(_state): State<ApiState>,
    Path(platform): Path<String>,
) -> Json<ApiResponse<serde_json::Value>> {
    // For MVP, return instructions for setting up credentials
    ok(serde_json::json!({
        "platform": platform,
        "status": "pending",
        "message": format!("Set up {} credentials via 'pylot init --only social-{}' or environment variables. See docs/SOCIAL-PLATFORMS.md", platform, platform)
    }))
}

pub async fn disconnect_social_platform(
    State(_state): State<ApiState>,
    Path(platform): Path<String>,
) -> Json<ApiResponse<serde_json::Value>> {
    ok(serde_json::json!({
        "platform": platform,
        "status": "disconnected"
    }))
}
// ── Sub-Agent API ────────────────────────────────────────────────────

pub async fn list_sub_agents(
    State(state): State<ApiState>,
) -> Json<ApiResponse<serde_json::Value>> {
    if let Some(ref orch) = state.orchestrator {
        let agents = orch.list().await;
        let count = agents.len();
        let items: Vec<serde_json::Value> = agents
            .iter()
            .map(|a| {
                serde_json::json!({
                    "id": a.config.id,
                    "name": a.config.name,
                    "agent_type": format!("{:?}", a.config.agent_type),
                    "status": format!("{:?}", a.status),
                    "result": a.result,
                    "error": a.error,
                    "started_at": a.started_at,
                    "completed_at": a.completed_at,
                })
            })
            .collect();
        ok(serde_json::json!({ "agents": items, "count": count }))
    } else {
        ok(serde_json::json!({ "agents": [], "count": 0, "message": "Sub-agents not enabled" }))
    }
}

#[derive(Deserialize)]
pub struct SpawnAgentRequest {
    pub name: String,
    pub task: String,
    pub model: Option<String>,
    /// If set, the agent runs recurrently every `interval_secs` seconds.
    /// If absent or 0, the agent runs exactly once.
    pub interval_secs: Option<u64>,
}

pub async fn spawn_sub_agent(
    State(state): State<ApiState>,
    Json(body): Json<SpawnAgentRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    tracing::info!(
        "HTTP spawn_sub_agent: name={:?}, task_len={}, interval_secs={:?}",
        body.name,
        body.task.len(),
        body.interval_secs
    );
    if let Some(ref orch) = state.orchestrator {
        let config = crate::sub_agents::types::SubAgentConfig {
            name: body.name.clone(),
            model_override: body.model.clone(),
            interval_secs: body.interval_secs.filter(|&v| v > 0),
            ..Default::default()
        };
        let llm = Arc::clone(&state.llm);
        let data_dir = state.config.data_dir.clone();

        // ── Recurring agent ──────────────────────────────────────────
        if let Some(interval) = body.interval_secs.filter(|&v| v > 0) {
            tracing::info!(
                "HTTP spawn_sub_agent: routing to spawn_recurring with interval={}s",
                interval
            );
            let data_dir2 = data_dir.clone();
            // When spawned from the Sub-Agents page there is no originating
            // chat conversation. Fall back to the *most recent* conversation
            // so each iteration still posts a visible "Run #N completed"
            // message into the chat.
            let fallback_conv_id = state
                .conversations
                .list()
                .into_iter()
                .next()
                .map(|c| c.id);
            let result = orch
                .spawn_recurring(
                    config,
                    body.task.clone(),
                    llm,
                    move || {
                        let tools = crate::tools::build_sub_agent_tools(data_dir2.clone());
                        let skills = crate::skills::SkillRegistry::load_all(None);
                        (tools, skills)
                    },
                    data_dir,
                    fallback_conv_id,
                    interval,
                )
                .await;
            return match result {
                Ok(id) => ok(serde_json::json!({
                    "id": id,
                    "name": body.name,
                    "status": "Running",
                    "interval_secs": interval,
                })),
                Err(e) => ok(serde_json::json!({"error": e.to_string()})),
            };
        }

        // ── One-shot agent ───────────────────────────────────────────
        let tools = crate::tools::build_sub_agent_tools(data_dir.clone());
        let skills = crate::skills::SkillRegistry::load_all(None);

        match orch
            .spawn(
                config,
                body.task.clone(),
                llm,
                tools,
                skills,
                data_dir,
                None,
            )
            .await
        {
            Ok(id) => ok(serde_json::json!({
                "id": id,
                "name": body.name,
                "status": "Running",
            })),
            Err(e) => ok(serde_json::json!({"error": e.to_string()})),
        }
    } else {
        ok(serde_json::json!({"error": "Sub-agent system not enabled"}))
    }
}

pub async fn get_sub_agent(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Json<ApiResponse<serde_json::Value>> {
    if let Some(ref orch) = state.orchestrator {
        match orch.get_state(&id).await {
            Some(s) => ok(serde_json::json!({
                "id": id,
                "name": s.config.name,
                "agent_type": format!("{:?}", s.config.agent_type),
                "status": format!("{:?}", s.status),
                "result": s.result,
                "error": s.error,
                "started_at": s.started_at,
                "completed_at": s.completed_at,
            })),
            None => ok(serde_json::json!({"error": format!("Sub-agent not found: {id}")})),
        }
    } else {
        ok(serde_json::json!({"error": "Sub-agent system not enabled"}))
    }
}

pub async fn cancel_sub_agent(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Json<ApiResponse<serde_json::Value>> {
    if let Some(ref orch) = state.orchestrator {
        match orch.cancel(&id).await {
            Ok(_) => ok(serde_json::json!({ "id": id, "status": "cancelled" })),
            Err(e) => ok(serde_json::json!({"error": e.to_string()})),
        }
    } else {
        ok(serde_json::json!({"error": "Sub-agent system not enabled"}))
    }
}

/// GET /api/agents/{id}/runs — full run history for a sub-agent, newest first.
pub async fn list_sub_agent_runs(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Json<ApiResponse<serde_json::Value>> {
    if let Some(ref store) = state.sub_agent_store {
        match store.list_runs(&id) {
            Ok(runs) => ok(serde_json::json!({ "id": id, "runs": runs, "count": runs.len() })),
            Err(e) => ok(serde_json::json!({"error": e.to_string()})),
        }
    } else {
        ok(serde_json::json!({ "id": id, "runs": [], "count": 0 }))
    }
}

/// DELETE /api/agents/{id}/runs — clear run history but keep the sub-agent active.
pub async fn clear_sub_agent_runs(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Json<ApiResponse<serde_json::Value>> {
    if let Some(ref store) = state.sub_agent_store {
        match store.clear_runs(&id) {
            Ok(n) => ok(serde_json::json!({ "id": id, "cleared": n })),
            Err(e) => ok(serde_json::json!({"error": e.to_string()})),
        }
    } else {
        ok(serde_json::json!({"error": "Sub-agent store not enabled"}))
    }
}

/// DELETE /api/agents/{id}/permanent — cancel (if running), then remove the
/// sub-agent record AND its run history. Use this for the panel's [Delete] button.
pub async fn delete_sub_agent_permanent(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Json<ApiResponse<serde_json::Value>> {
    // Best-effort: cancel any running task first so we don't orphan a tokio handle.
    if let Some(ref orch) = state.orchestrator {
        let _ = orch.cancel(&id).await;
    }
    if let Some(ref store) = state.sub_agent_store {
        match store.delete_agent(&id) {
            Ok(_) => ok(serde_json::json!({ "id": id, "deleted": true })),
            Err(e) => ok(serde_json::json!({"error": e.to_string()})),
        }
    } else {
        ok(serde_json::json!({"error": "Sub-agent store not enabled"}))
    }
}

// ── Agent Presets (plug-and-play manifests) ──────────────────────────

/// List agent presets loaded from bundled/, ~/.pylot/agents/, and workspace ./agents/.
pub async fn list_agent_presets(
    State(_state): State<ApiState>,
) -> Json<ApiResponse<serde_json::Value>> {
    let workspace = std::env::current_dir().ok();
    let registry = crate::sub_agents::AgentManifestRegistry::load_all(workspace.as_deref());
    let items: Vec<serde_json::Value> = registry
        .all()
        .iter()
        .map(|m| {
            serde_json::json!({
                "name": m.name,
                "description": m.description,
                "agent_type": m.agent_type,
                "model_override": m.model_override,
                "allowed_tools": m.allowed_tools,
                "timeout_secs": m.timeout_secs,
                "max_iterations": m.max_iterations,
                "source": m.source.as_str(),
                "source_path": m.source_path.as_ref().map(|p| p.display().to_string()),
            })
        })
        .collect();
    let dir = crate::sub_agents::AgentManifestRegistry::user_agents_dir()
        .map(|p| p.display().to_string());
    ok(serde_json::json!({
        "presets": items,
        "count": registry.len(),
        "user_dir": dir,
    }))
}

/// Get the full details (incl. system_prompt) of a single preset.
pub async fn get_agent_preset(
    State(_state): State<ApiState>,
    Path(name): Path<String>,
) -> Json<ApiResponse<serde_json::Value>> {
    let workspace = std::env::current_dir().ok();
    let registry = crate::sub_agents::AgentManifestRegistry::load_all(workspace.as_deref());
    match registry.get(&name) {
        Some(m) => ok(serde_json::json!({
            "name": m.name,
            "description": m.description,
            "agent_type": m.agent_type,
            "system_prompt": m.system_prompt,
            "model_override": m.model_override,
            "allowed_tools": m.allowed_tools,
            "timeout_secs": m.timeout_secs,
            "max_iterations": m.max_iterations,
            "source": m.source.as_str(),
            "source_path": m.source_path.as_ref().map(|p| p.display().to_string()),
        })),
        None => ok(serde_json::json!({ "error": format!("Preset '{name}' not found") })),
    }
}

// ── Memory v2 API ────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct MemoryV2SearchRequest {
    pub query: String,
    pub limit: Option<usize>,
}

pub async fn memory_v2_search(
    State(state): State<ApiState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Json<ApiResponse<serde_json::Value>> {
    let user_id = params
        .get("user_id")
        .map(|s| s.as_str())
        .unwrap_or("default");
    if let Some(ref store) = state.memory_v2_store {
        let units = store.list(user_id, None, 50).unwrap_or_default();
        let items: Vec<serde_json::Value> = units
            .iter()
            .map(|u| {
                serde_json::json!({
                    "id": u.id,
                    "type": u.memory_type.as_str(),
                    "content": u.content,
                    "importance": u.importance,
                    "created_at": u.created_at,
                })
            })
            .collect();
        ok(serde_json::json!({ "units": items, "count": items.len() }))
    } else {
        ok(serde_json::json!({ "units": [], "count": 0, "message": "Memory v2 not enabled" }))
    }
}

pub async fn memory_v2_list(
    State(state): State<ApiState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Json<ApiResponse<serde_json::Value>> {
    let user_id = params
        .get("user_id")
        .map(|s| s.as_str())
        .unwrap_or("default");
    if let Some(ref store) = state.memory_v2_store {
        let units = store.list(user_id, None, 100).unwrap_or_default();
        let items: Vec<serde_json::Value> = units
            .iter()
            .map(|u| {
                serde_json::json!({
                    "id": u.id,
                    "type": u.memory_type.as_str(),
                    "content": u.content,
                    "summary": u.summary,
                    "importance": u.importance,
                    "confidence": u.confidence,
                    "access_count": u.access_count,
                    "entities": u.entities,
                    "topics": u.topics,
                    "tags": u.tags,
                    "created_at": u.created_at,
                    "updated_at": u.updated_at,
                })
            })
            .collect();
        ok(serde_json::json!({ "units": items, "count": items.len() }))
    } else {
        ok(serde_json::json!({ "units": [], "count": 0, "message": "Memory v2 not enabled" }))
    }
}

// ── SSE Streaming Chat ──────────────────────────────────────────────

pub async fn chat_stream(
    State(state): State<ApiState>,
    Json(body): Json<ChatRequest>,
) -> axum::response::Sse<
    impl futures_core::Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>>,
> {
    use axum::response::sse::{Event, KeepAlive, Sse};
    use std::time::Duration;
    use tokio_stream::wrappers::UnboundedReceiverStream;

    let conv_id = body
        .conversation_id
        .clone()
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let msg_id = uuid::Uuid::new_v4().to_string();

    // Persist user message immediately so it's not lost if the connection drops.
    state.conversations.add_message(
        &conv_id,
        super::StoredMessage {
            id: uuid::Uuid::new_v4().to_string(),
            role: "user".into(),
            content: body.message.clone(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        },
    );

    // SSE bridge channel — unbounded so the LLM is never blocked by a slow reader.
    let (sse_tx, sse_rx) =
        tokio::sync::mpsc::unbounded_channel::<Result<Event, std::convert::Infallible>>();

    let agent = state.agent.clone();
    let conversations = state.conversations.clone();
    let user_msg = body.message.clone();

    tokio::spawn(async move {
        // Signal that the agent is working.
        let _ = sse_tx.send(Ok(Event::default().event("thinking").data("true")));

        // Open a stream channel for the LLM provider.
        let (stream_tx, mut stream_rx) = crate::streaming::stream_channel();

        // Clone sse_tx so we can forward stream events while still owning sse_tx below.
        let sse_forward = sse_tx.clone();

        // Spawn a task that forwards StreamEvents → SSE events.
        let forward_handle = tokio::spawn(async move {
            while let Some(event) = stream_rx.recv().await {
                let json = match serde_json::to_string(&event) {
                    Ok(j) => j,
                    Err(_) => continue,
                };
                let sse_event = match &event {
                    crate::streaming::StreamEvent::TextDelta { .. } => {
                        Event::default().event("text_delta").data(&json)
                    }
                    crate::streaming::StreamEvent::ToolUseStart { .. } => {
                        Event::default().event("tool_use_start").data(&json)
                    }
                    crate::streaming::StreamEvent::ToolInputDelta { .. } => {
                        Event::default().event("tool_input_delta").data(&json)
                    }
                    crate::streaming::StreamEvent::ToolResult { .. } => {
                        Event::default().event("tool_result").data(&json)
                    }
                    crate::streaming::StreamEvent::Thinking { .. } => {
                        Event::default().event("thinking").data(&json)
                    }
                    crate::streaming::StreamEvent::Usage { .. } => {
                        Event::default().event("usage").data(&json)
                    }
                    crate::streaming::StreamEvent::MessageStop => {
                        Event::default().event("message_stop").data(&json)
                    }
                    crate::streaming::StreamEvent::Error { .. } => {
                        Event::default().event("error").data(&json)
                    }
                };
                if sse_forward.send(Ok(sse_event)).is_err() {
                    // Client disconnected.
                    break;
                }
            }
        });

        // Enable streaming on the agent and attach our sender.
        let full_response = {
            let mut agent_guard = agent.lock().await;
            agent_guard.set_streaming(true);
            agent_guard.set_stream_sender(stream_tx);
            let result = agent_guard.chat(&user_msg).await;
            // Clear the stream sender so the channel closes and the
            // forward task can terminate (recv() returns None).
            agent_guard.clear_stream_sender();
            result
        };

        // Wait for the forwarding task to drain the channel.
        let _ = forward_handle.await;

        match full_response {
            Ok(response) => {
                // Persist the completed assistant message.
                conversations.add_message(
                    &conv_id,
                    super::StoredMessage {
                        id: msg_id.clone(),
                        role: "assistant".into(),
                        content: response.clone(),
                        timestamp: chrono::Utc::now().to_rfc3339(),
                    },
                );

                // Final "done" event carries the conversation id and message id so the
                // frontend can update its state without a separate API call.
                let done_payload = serde_json::json!({
                    "conversationId": conv_id,
                    "messageId": msg_id,
                    "content": response,
                });
                let _ = sse_tx.send(Ok(Event::default()
                    .event("done")
                    .data(&done_payload.to_string())));
            }
            Err(e) => {
                let err_payload = serde_json::json!({ "message": format!("{e}") }).to_string();
                let _ = sse_tx.send(Ok(Event::default().event("error").data(&err_payload)));
            }
        }
    });

    let stream = UnboundedReceiverStream::new(sse_rx);
    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("ping"),
    )
}
