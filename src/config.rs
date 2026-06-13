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

    // Social Media
    pub social_twitter_enabled: bool,
    pub twitter_api_key: Option<String>,
    pub twitter_api_secret: Option<String>,
    pub twitter_access_token: Option<String>,
    pub twitter_access_token_secret: Option<String>,

    pub social_linkedin_enabled: bool,
    pub linkedin_access_token: Option<String>,
    pub linkedin_person_id: Option<String>,

    pub social_bluesky_enabled: bool,
    pub bluesky_handle: Option<String>,
    pub bluesky_app_password: Option<String>,

    pub social_facebook_enabled: bool,
    pub facebook_access_token: Option<String>,
    pub facebook_page_id: Option<String>,

    pub social_instagram_enabled: bool,
    pub instagram_access_token: Option<String>,
    pub instagram_user_id: Option<String>,

    pub social_tiktok_enabled: bool,
    pub tiktok_access_token: Option<String>,

    pub social_youtube_enabled: bool,
    pub youtube_access_token: Option<String>,

    pub social_pinterest_enabled: bool,
    pub pinterest_access_token: Option<String>,
    pub pinterest_board_id: Option<String>,

    pub social_reddit_enabled: bool,
    pub reddit_access_token: Option<String>,
    pub reddit_subreddit: Option<String>,

    pub social_threads_enabled: bool,
    pub threads_access_token: Option<String>,
    pub threads_user_id: Option<String>,

    pub social_mastodon_enabled: bool,
    pub mastodon_access_token: Option<String>,
    pub mastodon_instance: Option<String>,

    pub social_discord_enabled: bool,
    pub discord_bot_token: Option<String>,
    pub discord_channel_id: Option<String>,
    pub discord_webhook_url: Option<String>,

    pub social_slack_enabled: bool,
    pub slack_bot_token: Option<String>,
    pub slack_channel: Option<String>,

    pub social_medium_enabled: bool,
    pub medium_token: Option<String>,

    pub social_devto_enabled: bool,
    pub devto_api_key: Option<String>,

    pub social_hashnode_enabled: bool,
    pub hashnode_api_key: Option<String>,
    pub hashnode_publication_id: Option<String>,

    pub social_wordpress_enabled: bool,
    pub wordpress_site_url: Option<String>,
    pub wordpress_username: Option<String>,
    pub wordpress_app_password: Option<String>,

    // MCP (Model Context Protocol)
    pub mcp_enabled: bool,
    pub mcp_config_path: Option<String>,

    // Learning / Auto-scoring
    pub learning_enabled: bool,
    pub learning_auto_score: bool,
    pub learning_judge_votes: usize,
    pub learning_skill_evolution: bool,

    // Marketing Agent
    pub marketing_enabled: bool,

    // Smart Memory (embedded SQLite + OpenAI embeddings)
    pub memory_enabled: bool,
    pub memory_db_name: String,
    pub memory_embedding_model: String,
    pub memory_auto_extract: bool,
    pub memory_extraction_interval: usize,
    pub memory_similarity_threshold: f32,
    pub memory_max_memory_context: usize,
    pub memory_max_knowledge_context: usize,
    pub memory_chunk_size: usize,
    pub memory_chunk_overlap: usize,
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
    memory: Option<MemoryToml>,
    social: Option<SocialToml>,
    mcp: Option<McpToml>,
    learning: Option<LearningToml>,
    marketing: Option<MarketingToml>,
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

#[derive(Debug, Deserialize)]
struct MemoryToml {
    enabled: Option<bool>,
    db_name: Option<String>,
    embedding_model: Option<String>,
    auto_extract: Option<bool>,
    extraction_interval: Option<usize>,
    similarity_threshold: Option<f32>,
    max_memory_context: Option<usize>,
    max_knowledge_context: Option<usize>,
    chunk_size: Option<usize>,
    chunk_overlap: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct SocialToml {
    twitter_enabled: Option<bool>,
    linkedin_enabled: Option<bool>,
    bluesky_enabled: Option<bool>,
    facebook_enabled: Option<bool>,
    instagram_enabled: Option<bool>,
    tiktok_enabled: Option<bool>,
    youtube_enabled: Option<bool>,
    pinterest_enabled: Option<bool>,
    reddit_enabled: Option<bool>,
    threads_enabled: Option<bool>,
    mastodon_enabled: Option<bool>,
    discord_enabled: Option<bool>,
    slack_enabled: Option<bool>,
    medium_enabled: Option<bool>,
    devto_enabled: Option<bool>,
    hashnode_enabled: Option<bool>,
    wordpress_enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct McpToml {
    enabled: Option<bool>,
    config_path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LearningToml {
    enabled: Option<bool>,
    auto_score: Option<bool>,
    judge_votes: Option<usize>,
    skill_evolution: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct MarketingToml {
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
        "TWITTER_API_KEY" => Some("twitter.api_key"),
        "TWITTER_API_SECRET" => Some("twitter.api_secret"),
        "TWITTER_ACCESS_TOKEN" => Some("twitter.access_token"),
        "TWITTER_ACCESS_TOKEN_SECRET" => Some("twitter.access_token_secret"),
        "LINKEDIN_ACCESS_TOKEN" => Some("linkedin.access_token"),
        "LINKEDIN_PERSON_ID" => Some("linkedin.person_id"),
        "BLUESKY_HANDLE" => Some("bluesky.handle"),
        "BLUESKY_APP_PASSWORD" => Some("bluesky.app_password"),
        "FACEBOOK_ACCESS_TOKEN" => Some("facebook.access_token"),
        "FACEBOOK_PAGE_ID" => Some("facebook.page_id"),
        "INSTAGRAM_ACCESS_TOKEN" => Some("instagram.access_token"),
        "INSTAGRAM_USER_ID" => Some("instagram.user_id"),
        "TIKTOK_ACCESS_TOKEN" => Some("tiktok.access_token"),
        "YOUTUBE_ACCESS_TOKEN" => Some("youtube.access_token"),
        "PINTEREST_ACCESS_TOKEN" => Some("pinterest.access_token"),
        "PINTEREST_BOARD_ID" => Some("pinterest.board_id"),
        "REDDIT_ACCESS_TOKEN" => Some("reddit.access_token"),
        "REDDIT_SUBREDDIT" => Some("reddit.subreddit"),
        "THREADS_ACCESS_TOKEN" => Some("threads.access_token"),
        "THREADS_USER_ID" => Some("threads.user_id"),
        "MASTODON_ACCESS_TOKEN" => Some("mastodon.access_token"),
        "MASTODON_INSTANCE" => Some("mastodon.instance"),
        "DISCORD_BOT_TOKEN" => Some("discord.bot_token"),
        "DISCORD_CHANNEL_ID" => Some("discord.channel_id"),
        "DISCORD_WEBHOOK_URL" => Some("discord.webhook_url"),
        "SLACK_BOT_TOKEN" => Some("slack.bot_token"),
        "SLACK_CHANNEL" => Some("slack.channel"),
        "MEDIUM_TOKEN" => Some("medium.token"),
        "DEVTO_API_KEY" => Some("devto.api_key"),
        "HASHNODE_API_KEY" => Some("hashnode.api_key"),
        "HASHNODE_PUBLICATION_ID" => Some("hashnode.publication_id"),
        "WORDPRESS_SITE_URL" => Some("wordpress.site_url"),
        "WORDPRESS_USERNAME" => Some("wordpress.username"),
        "WORDPRESS_APP_PASSWORD" => Some("wordpress.app_password"),
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
        let data_dir = env_opt("PYLOT_DATA_DIR")
            .or_else(|| env_opt("GMV_DATA_DIR")) // backward compat
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
                    .join(".pylot")
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
        let sched = toml_cfg
            .scheduler
            .unwrap_or(SchedulerToml { enabled: None });
        let mem = toml_cfg.memory.unwrap_or(MemoryToml {
            enabled: None,
            db_name: None,
            embedding_model: None,
            auto_extract: None,
            extraction_interval: None,
            similarity_threshold: None,
            max_memory_context: None,
            max_knowledge_context: None,
            chunk_size: None,
            chunk_overlap: None,
        });

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
            || env_opt("PYLOT_SCHEDULER_ENABLED")
                .or_else(|| env_opt("GMV_SCHEDULER_ENABLED")) // backward compat
                .map(|v| v == "true" || v == "1")
                .unwrap_or(false);

        let social = toml_cfg.social.unwrap_or(SocialToml {
            twitter_enabled: None,
            linkedin_enabled: None,
            bluesky_enabled: None,
            facebook_enabled: None,
            instagram_enabled: None,
            tiktok_enabled: None,
            youtube_enabled: None,
            pinterest_enabled: None,
            reddit_enabled: None,
            threads_enabled: None,
            mastodon_enabled: None,
            discord_enabled: None,
            slack_enabled: None,
            medium_enabled: None,
            devto_enabled: None,
            hashnode_enabled: None,
            wordpress_enabled: None,
        });
        let mcp_cfg = toml_cfg.mcp.unwrap_or(McpToml {
            enabled: None,
            config_path: None,
        });
        let learn = toml_cfg.learning.unwrap_or(LearningToml {
            enabled: None,
            auto_score: None,
            judge_votes: None,
            skill_evolution: None,
        });
        let mkt = toml_cfg
            .marketing
            .unwrap_or(MarketingToml { enabled: None });

        let social_twitter_enabled = social.twitter_enabled.unwrap_or(false)
            || secret_opt("TWITTER_API_KEY", &vault).is_some();
        let social_linkedin_enabled = social.linkedin_enabled.unwrap_or(false)
            || secret_opt("LINKEDIN_ACCESS_TOKEN", &vault).is_some();
        let social_bluesky_enabled = social.bluesky_enabled.unwrap_or(false)
            || secret_opt("BLUESKY_HANDLE", &vault).is_some();
        let social_facebook_enabled = social.facebook_enabled.unwrap_or(false)
            || secret_opt("FACEBOOK_ACCESS_TOKEN", &vault).is_some();
        let social_instagram_enabled = social.instagram_enabled.unwrap_or(false)
            || secret_opt("INSTAGRAM_ACCESS_TOKEN", &vault).is_some();
        let social_tiktok_enabled = social.tiktok_enabled.unwrap_or(false)
            || secret_opt("TIKTOK_ACCESS_TOKEN", &vault).is_some();
        let social_youtube_enabled = social.youtube_enabled.unwrap_or(false)
            || secret_opt("YOUTUBE_ACCESS_TOKEN", &vault).is_some();
        let social_pinterest_enabled = social.pinterest_enabled.unwrap_or(false)
            || secret_opt("PINTEREST_ACCESS_TOKEN", &vault).is_some();
        let social_reddit_enabled = social.reddit_enabled.unwrap_or(false)
            || secret_opt("REDDIT_ACCESS_TOKEN", &vault).is_some();
        let social_threads_enabled = social.threads_enabled.unwrap_or(false)
            || secret_opt("THREADS_ACCESS_TOKEN", &vault).is_some();
        let social_mastodon_enabled = social.mastodon_enabled.unwrap_or(false)
            || secret_opt("MASTODON_ACCESS_TOKEN", &vault).is_some();
        let social_discord_enabled = social.discord_enabled.unwrap_or(false)
            || secret_opt("DISCORD_BOT_TOKEN", &vault).is_some();
        let social_slack_enabled = social.slack_enabled.unwrap_or(false)
            || secret_opt("SLACK_BOT_TOKEN", &vault).is_some();
        let social_medium_enabled =
            social.medium_enabled.unwrap_or(false) || secret_opt("MEDIUM_TOKEN", &vault).is_some();
        let social_devto_enabled =
            social.devto_enabled.unwrap_or(false) || secret_opt("DEVTO_API_KEY", &vault).is_some();
        let social_hashnode_enabled = social.hashnode_enabled.unwrap_or(false)
            || secret_opt("HASHNODE_API_KEY", &vault).is_some();
        let social_wordpress_enabled = social.wordpress_enabled.unwrap_or(false)
            || secret_opt("WORDPRESS_SITE_URL", &vault).is_some();

        Ok(AppConfig {
            agent_name: secret_opt("AGENT_NAME", &vault)
                .or(agent.name)
                .unwrap_or_else(|| "Pylot".into()),
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

            // Social Media
            social_twitter_enabled,
            twitter_api_key: secret_opt("TWITTER_API_KEY", &vault),
            twitter_api_secret: secret_opt("TWITTER_API_SECRET", &vault),
            twitter_access_token: secret_opt("TWITTER_ACCESS_TOKEN", &vault),
            twitter_access_token_secret: secret_opt("TWITTER_ACCESS_TOKEN_SECRET", &vault),

            social_linkedin_enabled,
            linkedin_access_token: secret_opt("LINKEDIN_ACCESS_TOKEN", &vault),
            linkedin_person_id: secret_opt("LINKEDIN_PERSON_ID", &vault),

            social_bluesky_enabled,
            bluesky_handle: secret_opt("BLUESKY_HANDLE", &vault),
            bluesky_app_password: secret_opt("BLUESKY_APP_PASSWORD", &vault),

            social_facebook_enabled,
            facebook_access_token: secret_opt("FACEBOOK_ACCESS_TOKEN", &vault),
            facebook_page_id: secret_opt("FACEBOOK_PAGE_ID", &vault),

            social_instagram_enabled,
            instagram_access_token: secret_opt("INSTAGRAM_ACCESS_TOKEN", &vault),
            instagram_user_id: secret_opt("INSTAGRAM_USER_ID", &vault),

            social_tiktok_enabled,
            tiktok_access_token: secret_opt("TIKTOK_ACCESS_TOKEN", &vault),

            social_youtube_enabled,
            youtube_access_token: secret_opt("YOUTUBE_ACCESS_TOKEN", &vault),

            social_pinterest_enabled,
            pinterest_access_token: secret_opt("PINTEREST_ACCESS_TOKEN", &vault),
            pinterest_board_id: secret_opt("PINTEREST_BOARD_ID", &vault),

            social_reddit_enabled,
            reddit_access_token: secret_opt("REDDIT_ACCESS_TOKEN", &vault),
            reddit_subreddit: secret_opt("REDDIT_SUBREDDIT", &vault),

            social_threads_enabled,
            threads_access_token: secret_opt("THREADS_ACCESS_TOKEN", &vault),
            threads_user_id: secret_opt("THREADS_USER_ID", &vault),

            social_mastodon_enabled,
            mastodon_access_token: secret_opt("MASTODON_ACCESS_TOKEN", &vault),
            mastodon_instance: secret_opt("MASTODON_INSTANCE", &vault),

            social_discord_enabled,
            discord_bot_token: secret_opt("DISCORD_BOT_TOKEN", &vault),
            discord_channel_id: secret_opt("DISCORD_CHANNEL_ID", &vault),
            discord_webhook_url: secret_opt("DISCORD_WEBHOOK_URL", &vault),

            social_slack_enabled,
            slack_bot_token: secret_opt("SLACK_BOT_TOKEN", &vault),
            slack_channel: secret_opt("SLACK_CHANNEL", &vault),

            social_medium_enabled,
            medium_token: secret_opt("MEDIUM_TOKEN", &vault),

            social_devto_enabled,
            devto_api_key: secret_opt("DEVTO_API_KEY", &vault),

            social_hashnode_enabled,
            hashnode_api_key: secret_opt("HASHNODE_API_KEY", &vault),
            hashnode_publication_id: secret_opt("HASHNODE_PUBLICATION_ID", &vault),

            social_wordpress_enabled,
            wordpress_site_url: secret_opt("WORDPRESS_SITE_URL", &vault),
            wordpress_username: secret_opt("WORDPRESS_USERNAME", &vault),
            wordpress_app_password: secret_opt("WORDPRESS_APP_PASSWORD", &vault),

            // MCP
            mcp_enabled: mcp_cfg.enabled.unwrap_or(false),
            mcp_config_path: mcp_cfg.config_path,

            // Learning
            learning_enabled: learn.enabled.unwrap_or(true),
            learning_auto_score: learn.auto_score.unwrap_or(false),
            learning_judge_votes: learn.judge_votes.unwrap_or(3),
            learning_skill_evolution: learn.skill_evolution.unwrap_or(false),

            // Marketing
            marketing_enabled: mkt.enabled.unwrap_or(false),

            memory_enabled: mem.enabled.unwrap_or(false),
            memory_db_name: mem.db_name.unwrap_or_else(|| "smart_memory.db".into()),
            memory_embedding_model: mem
                .embedding_model
                .unwrap_or_else(|| "text-embedding-3-small".into()),
            memory_auto_extract: mem.auto_extract.unwrap_or(true),
            memory_extraction_interval: mem.extraction_interval.unwrap_or(5),
            memory_similarity_threshold: mem.similarity_threshold.unwrap_or(0.35),
            memory_max_memory_context: mem.max_memory_context.unwrap_or(10),
            memory_max_knowledge_context: mem.max_knowledge_context.unwrap_or(10),
            memory_chunk_size: mem.chunk_size.unwrap_or(500),
            memory_chunk_overlap: mem.chunk_overlap.unwrap_or(50),
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
                .join(".pylot")
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
            memory: None,
            social: None,
            mcp: None,
            learning: None,
            marketing: None,
        })
    }
}
