use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::PathBuf;

use crate::secrets::{self, SecretsVault};

/// Top-level application configuration assembled from TOML + secrets vault + env vars.
/// Priority: env vars > secrets vault > TOML defaults
#[derive(Debug, Clone)]
pub struct AppConfig {
    // Agent
    pub agent_name: String,
    pub agent_persona: String,
    pub max_context_messages: usize,
    pub max_tool_iterations: usize,

    // LLM
    pub llm_provider: String,
    pub llm_model: String,
    pub llm_max_tokens: u32,
    pub llm_temperature: f64,

    // API keys
    pub openai_api_key: Option<String>,
    pub anthropic_api_key: Option<String>,

    // Storage
    pub data_dir: PathBuf,

    // Google Calendar
    pub google_calendar_enabled: bool,
    pub google_client_id: Option<String>,
    pub google_client_secret: Option<String>,
    pub google_redirect_port: u16,

    // Gmail
    pub gmail_enabled: bool,

    // Telegram
    pub telegram_enabled: bool,
    pub telegram_bot_token: Option<String>,
    pub telegram_default_chat_id: Option<String>,

    // WhatsApp (Twilio)
    pub whatsapp_enabled: bool,
    pub twilio_account_sid: Option<String>,
    pub twilio_auth_token: Option<String>,
    pub twilio_whatsapp_from: Option<String>,

    // Scheduler
    pub scheduler_enabled: bool,
}

/// Raw TOML structure for the config file.
#[derive(Debug, Deserialize)]
struct TomlConfig {
    agent: Option<AgentToml>,
    llm: Option<LlmToml>,
    storage: Option<StorageToml>,
    google_calendar: Option<GoogleCalendarToml>,
    gmail: Option<GmailToml>,
    telegram: Option<TelegramToml>,
    whatsapp: Option<WhatsAppToml>,
    scheduler: Option<SchedulerToml>,
}

#[derive(Debug, Deserialize)]
struct SchedulerToml {
    enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct AgentToml {
    name: Option<String>,
    persona: Option<String>,
    max_context_messages: Option<usize>,
    max_tool_iterations: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct LlmToml {
    provider: Option<String>,
    model: Option<String>,
    max_tokens: Option<u32>,
    temperature: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct StorageToml {
    data_dir: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GoogleCalendarToml {
    enabled: Option<bool>,
    redirect_port: Option<u16>,
    #[allow(dead_code)]
    scopes: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct GmailToml {
    enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct TelegramToml {
    enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct WhatsAppToml {
    enabled: Option<bool>,
}

fn env_opt(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

/// Map environment variable names to secrets vault key paths.
fn env_to_vault_key(env_key: &str) -> Option<&'static str> {
    match env_key {
        "OPENAI_API_KEY" => Some("llm.openai.api_key"),
        "ANTHROPIC_API_KEY" => Some("llm.anthropic.api_key"),
        "GOOGLE_CLIENT_ID" => Some("google.client_id"),
        "GOOGLE_CLIENT_SECRET" => Some("google.client_secret"),
        "GOOGLE_REDIRECT_PORT" => None, // not stored in vault
        "TELEGRAM_BOT_TOKEN" => Some("telegram.bot_token"),
        "TELEGRAM_DEFAULT_CHAT_ID" => Some("telegram.default_chat_id"),
        "TWILIO_ACCOUNT_SID" => Some("twilio.account_sid"),
        "TWILIO_AUTH_TOKEN" => Some("twilio.auth_token"),
        "TWILIO_WHATSAPP_FROM" => Some("twilio.whatsapp_from"),
        _ => None,
    }
}

/// Helper: returns env var if set and non-empty, then tries secrets vault, else None.
fn secret_opt(key: &str, vault: &Option<SecretsVault>) -> Option<String> {
    // Environment variables always win
    if let Some(v) = env_opt(key) {
        return Some(v);
    }
    // Then try secrets vault using mapped key path
    if let Some(ref v) = vault {
        if let Some(vault_key) = env_to_vault_key(key) {
            if let Some(val) = v.get(vault_key) {
                return Some(val);
            }
        }
    }
    None
}

impl AppConfig {
    /// Load configuration from config/default.toml + secrets vault + environment variables.
    /// Priority: env vars > secrets vault > TOML defaults.
    pub fn load() -> Result<Self> {
        // Try to load .env file (ignore if missing — vault is preferred)
        let _ = dotenvy::dotenv();

        // Try to open secrets vault (non-fatal if missing — falls back to .env)
        let vault = Self::try_open_vault();

        // Load TOML config
        let toml_cfg = Self::load_toml()?;

        // Determine data directory
        let data_dir = env_opt("GMV_DATA_DIR")
            .or_else(|| {
                toml_cfg
                    .storage
                    .as_ref()
                    .and_then(|s| s.data_dir.clone())
                    .filter(|s| !s.is_empty())
            })
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                dirs::home_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join(".gmv-agent")
                    .join("data")
            });

        // Create data directory if needed
        std::fs::create_dir_all(&data_dir)
            .with_context(|| format!("Failed to create data directory: {}", data_dir.display()))?;

        let agent = toml_cfg.agent.unwrap_or(AgentToml {
            name: None,
            persona: None,
            max_context_messages: None,
            max_tool_iterations: None,
        });
        let llm = toml_cfg.llm.unwrap_or(LlmToml {
            provider: None,
            model: None,
            max_tokens: None,
            temperature: None,
        });
        let gcal = toml_cfg.google_calendar.unwrap_or(GoogleCalendarToml {
            enabled: None,
            redirect_port: None,
            scopes: None,
        });
        let tg = toml_cfg.telegram.unwrap_or(TelegramToml { enabled: None });
        let wa = toml_cfg.whatsapp.unwrap_or(WhatsAppToml { enabled: None });
        let sched = toml_cfg.scheduler.unwrap_or(SchedulerToml { enabled: None });

        let provider = secret_opt("LLM_PROVIDER", &vault)
            .or(llm.provider)
            .unwrap_or_else(|| "openai".into());

        let model = secret_opt("LLM_MODEL", &vault)
            .or(llm.model)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                if provider == "anthropic" {
                    "claude-sonnet-4-20250514".into()
                } else {
                    "gpt-4o".into()
                }
            });

        let google_calendar_enabled = gcal.enabled.unwrap_or(false)
            || (secret_opt("GOOGLE_CLIENT_ID", &vault).is_some()
                && secret_opt("GOOGLE_CLIENT_SECRET", &vault).is_some());

        let gmail_enabled = toml_cfg
            .gmail
            .as_ref()
            .and_then(|g| g.enabled)
            .unwrap_or(false)
            || (secret_opt("GOOGLE_CLIENT_ID", &vault).is_some()
                && secret_opt("GOOGLE_CLIENT_SECRET", &vault).is_some());

        let telegram_enabled =
            tg.enabled.unwrap_or(false) || secret_opt("TELEGRAM_BOT_TOKEN", &vault).is_some();

        let whatsapp_enabled =
            wa.enabled.unwrap_or(false) || secret_opt("TWILIO_ACCOUNT_SID", &vault).is_some();

        let scheduler_enabled = sched.enabled.unwrap_or(false)
            || env_opt("GMV_SCHEDULER_ENABLED")
                .map(|v| v == "true" || v == "1")
                .unwrap_or(false);

        Ok(AppConfig {
            agent_name: secret_opt("AGENT_NAME", &vault)
                .or(agent.name)
                .unwrap_or_else(|| "GMV Agent".into()),
            agent_persona: secret_opt("AGENT_PERSONA", &vault)
                .or(agent.persona)
                .unwrap_or_else(|| {
                    "You are a helpful, concise, and professional personal AI assistant.".into()
                }),
            max_context_messages: agent.max_context_messages.unwrap_or(50),
            max_tool_iterations: agent.max_tool_iterations.unwrap_or(10),

            llm_provider: provider,
            llm_model: model,
            llm_max_tokens: llm.max_tokens.unwrap_or(4096),
            llm_temperature: llm.temperature.unwrap_or(0.7),

            openai_api_key: secret_opt("OPENAI_API_KEY", &vault),
            anthropic_api_key: secret_opt("ANTHROPIC_API_KEY", &vault),

            data_dir,

            google_calendar_enabled,
            google_client_id: secret_opt("GOOGLE_CLIENT_ID", &vault),
            google_client_secret: secret_opt("GOOGLE_CLIENT_SECRET", &vault),
            google_redirect_port: secret_opt("GOOGLE_REDIRECT_PORT", &vault)
                .and_then(|s| s.parse().ok())
                .or(gcal.redirect_port)
                .unwrap_or(8085),

            gmail_enabled,

            telegram_enabled,
            telegram_bot_token: secret_opt("TELEGRAM_BOT_TOKEN", &vault),
            telegram_default_chat_id: secret_opt("TELEGRAM_DEFAULT_CHAT_ID", &vault),

            whatsapp_enabled,
            twilio_account_sid: secret_opt("TWILIO_ACCOUNT_SID", &vault),
            twilio_auth_token: secret_opt("TWILIO_AUTH_TOKEN", &vault),
            twilio_whatsapp_from: secret_opt("TWILIO_WHATSAPP_FROM", &vault),

            scheduler_enabled,
        })
    }

    /// Attempt to open the secrets vault. Returns None if vault doesn't exist or can't be opened.
    fn try_open_vault() -> Option<SecretsVault> {
        let path = secrets::default_secrets_path();
        if !path.exists() {
            return None;
        }
        match SecretsVault::open(&path, None) {
            Ok(v) => Some(v),
            Err(e) => {
                tracing::debug!("Could not open secrets vault (falling back to .env): {e}");
                None
            }
        }
    }

    fn load_toml() -> Result<TomlConfig> {
        // Look for config in several locations
        let candidates = vec![
            PathBuf::from("config/default.toml"),
            PathBuf::from("default.toml"),
            dirs::home_dir()
                .unwrap_or_default()
                .join(".gmv-agent")
                .join("config.toml"),
        ];

        for path in &candidates {
            if path.exists() {
                let content = std::fs::read_to_string(path)
                    .with_context(|| format!("Failed to read config: {}", path.display()))?;
                let cfg: TomlConfig = toml::from_str(&content)
                    .with_context(|| format!("Failed to parse config: {}", path.display()))?;
                return Ok(cfg);
            }
        }

        // Return defaults if no config file found
        Ok(TomlConfig {
            agent: None,
            llm: None,
            storage: None,
            google_calendar: None,
            gmail: None,
            telegram: None,
            whatsapp: None,
            scheduler: None,
        })
    }
}
