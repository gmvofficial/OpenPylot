use napi::bindgen_prelude::*;
use napi_derive::napi;
use std::collections::HashMap;
use std::process::Command;

// ── GMVAgent: main Node.js-facing class ─────────────────────────────

/// The primary GMV Agent class exposed to Node.js / TypeScript.
///
/// ```typescript
/// import { GMVAgent } from 'gmv-agent';
///
/// // First-time interactive setup
/// await GMVAgent.init();
///
/// // Or load from existing config
/// const agent = await GMVAgent.fromConfig('~/.gmv-agent/secrets.enc');
/// const response = await agent.chat('What meetings do I have today?');
/// console.log(response);
/// ```
#[napi]
pub struct GMVAgent {
    config_path: String,
    settings: HashMap<String, String>,
}

#[napi]
impl GMVAgent {
    /// Launch the interactive setup wizard.
    ///
    /// Opens browser for OAuth flows, prompts for API keys,
    /// and saves credentials to the encrypted secrets vault.
    #[napi(factory)]
    pub async fn init() -> Result<()> {
        let status = Command::new("gmv-agent")
            .arg("init")
            .status()
            .map_err(|e| {
                Error::from_reason(format!(
                    "Failed to launch gmv-agent init: {}. \
                     Make sure the gmv-agent binary is installed and in your PATH.",
                    e
                ))
            })?;

        if !status.success() {
            return Err(Error::from_reason("Init wizard exited with an error"));
        }
        Ok(())
    }

    /// Initialize an agent instance from an existing configuration file.
    #[napi(factory)]
    pub async fn from_config(config_path: String) -> Result<Self> {
        let expanded = shellexpand(&config_path);
        if !std::path::Path::new(&expanded).exists() {
            return Err(Error::from_reason(format!(
                "Config file not found: {}. Run GMVAgent.init() first.",
                expanded
            )));
        }
        Ok(GMVAgent {
            config_path: expanded,
            settings: HashMap::new(),
        })
    }

    /// Create a new agent from a programmatic Config object.
    #[napi(constructor)]
    pub fn new(config: &Config) -> Result<Self> {
        let mut settings = HashMap::new();
        settings.insert("llm_provider".into(), config.llm_provider.clone());
        settings.insert("llm_model".into(), config.llm_model.clone());
        if let Some(ref k) = config.openai_api_key {
            settings.insert("openai_api_key".into(), k.clone());
        }
        if let Some(ref k) = config.anthropic_api_key {
            settings.insert("anthropic_api_key".into(), k.clone());
        }

        Ok(GMVAgent {
            config_path: String::new(),
            settings,
        })
    }

    /// Send a message and get a response.
    #[napi]
    pub async fn chat(&self, message: String) -> Result<String> {
        let output = Command::new("gmv-agent")
            .arg("chat")
            .arg(&message)
            .output()
            .map_err(|e| Error::from_reason(format!("Failed to run gmv-agent chat: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::from_reason(format!("Chat failed: {}", stderr)));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Run diagnostic checks on the current configuration.
    #[napi]
    pub async fn doctor() -> Result<()> {
        let status = Command::new("gmv-agent")
            .arg("doctor")
            .status()
            .map_err(|e| Error::from_reason(format!("Failed to run doctor: {}", e)))?;

        if !status.success() {
            return Err(Error::from_reason("Doctor exited with an error"));
        }
        Ok(())
    }

    /// Show agent status and connected services.
    #[napi]
    pub async fn status() -> Result<()> {
        let status = Command::new("gmv-agent")
            .arg("status")
            .status()
            .map_err(|e| Error::from_reason(format!("Failed to run status: {}", e)))?;

        if !status.success() {
            return Err(Error::from_reason("Status check failed"));
        }
        Ok(())
    }
}

// ── Config: programmatic configuration ──────────────────────────────

/// Configuration object for headless / CI use.
///
/// ```typescript
/// import { GMVAgent, Config } from 'gmv-agent';
///
/// const config = new Config({
///   llmProvider: 'anthropic',
///   llmModel: 'claude-sonnet-4-20250514',
///   anthropicApiKey: process.env.ANTHROPIC_API_KEY,
/// });
/// const agent = new GMVAgent(config);
/// ```
#[napi(object)]
pub struct Config {
    pub llm_provider: String,
    pub llm_model: String,
    pub openai_api_key: Option<String>,
    pub anthropic_api_key: Option<String>,
    pub google_credentials_file: Option<String>,
    pub telegram_bot_token: Option<String>,
    pub telegram_chat_id: Option<String>,
}

// ── Helpers ─────────────────────────────────────────────────────────

fn shellexpand(path: &str) -> String {
    if path.starts_with("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return format!("{}/{}", home, &path[2..]);
        }
    }
    path.to_string()
}
