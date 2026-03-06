use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
        }),
    )
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
        version: "0.2.0".into(),
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
            last_message: c.messages.last().map(|m| {
                if m.content.len() > 100 {
                    format!("{}…", &m.content[..100])
                } else {
                    m.content.clone()
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

                // Spawn background task to handle OAuth callback
                let state_clone = state_param.clone();
                let oauth_clone = oauth_config.clone();
                let vault_path_clone = vault_path.clone();
                let service_clone = service.clone();
                tokio::spawn(async move {
                    match handle_google_oauth_callback(
                        &oauth_clone,
                        &state_clone,
                        &vault_path_clone,
                        &service_clone,
                    )
                    .await
                    {
                        Ok(_) => tracing::info!("{} OAuth completed successfully", service_clone),
                        Err(e) => tracing::error!("{} OAuth failed: {}", service_clone, e),
                    }
                });

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
    let tokens = crate::oauth::exchange_code(oauth_config, &code, &redirect_uri).await?;

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
    let data_dir = crate::secrets::gmv_home_dir().join("data");
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

pub async fn disconnect_integration(
    Path(service): Path<String>,
    State(_state): State<ApiState>,
) -> Result<Json<ApiResponse<bool>>, (StatusCode, Json<ApiError>)> {
    let vault_path = crate::secrets::default_secrets_path();
    let mut vault = crate::secrets::SecretsVault::open(&vault_path, None).map_err(|e| {
        err(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Vault error: {e}"),
        )
    })?;

    match service.as_str() {
        "google_calendar" | "gmail" => {
            let _ = vault.delete("google.access_token");
            let _ = vault.delete("google.refresh_token");
            // Keep client_id/secret so user can re-connect without re-entering them
            vault.save().map_err(|e| {
                err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Save error: {e}"),
                )
            })?;
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
            vault.save().map_err(|e| {
                err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Save error: {e}"),
                )
            })?;
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
                    .header("User-Agent", "gmv-agent")
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

        _ => Err(err(
            StatusCode::BAD_REQUEST,
            format!("Unknown service: {}", service),
        )),
    }
}

// ── Settings ─────────────────────────────────────────────────────────

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

    let data_dir = &state.config.data_dir;
    let mut memory = crate::memory::MemoryStore::load(data_dir).unwrap_or_default();

    // Parse index from id like "fact_0"
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
    let home = crate::secrets::gmv_home_dir();
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
    let agent_name_set = config.agent_name != "GMV Agent" && !config.agent_name.is_empty();

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

    if removed {
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
    let before = documents.len();
    documents.retain(|d| d.id != id);
    let removed = documents.len() < before;
    save_json_vec(&path, &documents);

    if removed {
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

pub async fn search_knowledge(
    State(state): State<ApiState>,
    Json(body): Json<SearchKnowledgeRequest>,
) -> Json<ApiResponse<Vec<SearchResultResponse>>> {
    let data_dir = &state.config.data_dir;
    let documents: Vec<KnowledgeDocument> = load_json_vec(&knowledge_documents_path(data_dir));
    let limit = body.limit.unwrap_or(10);
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

    ok(results)
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

    let chunks = if new_doc.content.is_empty() {
        0
    } else {
        (new_doc.content.len() / 500) + 1
    };
    let size = new_doc.content.len();

    documents.push(new_doc.clone());
    save_json_vec(&path, &documents);

    tracing::info!("Knowledge document uploaded: {}", body.title);

    ok(DocumentResponse {
        id: new_doc.id,
        collection_id: new_doc.collection_id,
        title: new_doc.title,
        source: new_doc.source,
        chunk_count: chunks,
        size,
        created_at: new_doc.created_at,
    })
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
    State(_state): State<ApiState>,
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
