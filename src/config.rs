use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::PathBuf;

/// Top-level application configuration assembled from TOML + env vars.
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

impl AppConfig {
    /// Load configuration from config/default.toml + environment variables.
    /// Environment variables take precedence over TOML values.
    pub fn load() -> Result<Self> {
        // Try to load .env file (ignore if missing)
        let _ = dotenvy::dotenv();

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

        let provider = env_opt("LLM_PROVIDER")
            .or(llm.provider)
            .unwrap_or_else(|| "openai".into());

        let model = env_opt("LLM_MODEL")
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
            || (env_opt("GOOGLE_CLIENT_ID").is_some() && env_opt("GOOGLE_CLIENT_SECRET").is_some());

        let gmail_enabled = toml_cfg
            .gmail
            .as_ref()
            .and_then(|g| g.enabled)
            .unwrap_or(false)
            || (env_opt("GOOGLE_CLIENT_ID").is_some() && env_opt("GOOGLE_CLIENT_SECRET").is_some());

        let telegram_enabled =
            tg.enabled.unwrap_or(false) || env_opt("TELEGRAM_BOT_TOKEN").is_some();

        let whatsapp_enabled =
            wa.enabled.unwrap_or(false) || env_opt("TWILIO_ACCOUNT_SID").is_some();

        Ok(AppConfig {
            agent_name: env_opt("AGENT_NAME")
                .or(agent.name)
                .unwrap_or_else(|| "GMV Agent".into()),
            agent_persona: env_opt("AGENT_PERSONA")
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

            openai_api_key: env_opt("OPENAI_API_KEY"),
            anthropic_api_key: env_opt("ANTHROPIC_API_KEY"),

            data_dir,

            google_calendar_enabled,
            google_client_id: env_opt("GOOGLE_CLIENT_ID"),
            google_client_secret: env_opt("GOOGLE_CLIENT_SECRET"),
            google_redirect_port: env_opt("GOOGLE_REDIRECT_PORT")
                .and_then(|s| s.parse().ok())
                .or(gcal.redirect_port)
                .unwrap_or(8085),

            gmail_enabled,

            telegram_enabled,
            telegram_bot_token: env_opt("TELEGRAM_BOT_TOKEN"),
            telegram_default_chat_id: env_opt("TELEGRAM_DEFAULT_CHAT_ID"),

            whatsapp_enabled,
            twilio_account_sid: env_opt("TWILIO_ACCOUNT_SID"),
            twilio_auth_token: env_opt("TWILIO_AUTH_TOKEN"),
            twilio_whatsapp_from: env_opt("TWILIO_WHATSAPP_FROM"),
        })
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
        })
    }
}
