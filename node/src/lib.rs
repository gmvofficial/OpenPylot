use napi::bindgen_prelude::*;
use napi_derive::napi;
use std::collections::HashMap;
use std::process::Command;
use std::sync::Arc;

// ── PylotAgent: main Node.js-facing class ─────────────────────────────

/// The primary OpenPylot class exposed to Node.js / TypeScript.
///
/// ```typescript
/// import { PylotAgent } from 'pylot';
///
/// // First-time interactive setup
/// await PylotAgent.init();
///
/// // Or load from existing config
/// const agent = await PylotAgent.fromConfig('~/.pylot/secrets.enc');
/// const response = await agent.chat('What meetings do I have today?');
/// console.log(response);
/// ```
#[napi]
pub struct PylotAgent {
    config_path: String,
    settings: HashMap<String, String>,
}

#[napi]
impl PylotAgent {
    /// Launch the interactive setup wizard.
    ///
    /// Opens browser for OAuth flows, prompts for API keys,
    /// and saves credentials to the encrypted secrets vault.
    #[napi]
    pub async fn init() -> Result<()> {
        let status = Command::new("pylot")
            .arg("init")
            .status()
            .map_err(|e| {
                Error::from_reason(format!(
                    "Failed to launch pylot init: {}. \
                     Make sure the pylot binary is installed and in your PATH.",
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
                "Config file not found: {}. Run PylotAgent.init() first.",
                expanded
            )));
        }
        Ok(PylotAgent {
            config_path: expanded,
            settings: HashMap::new(),
        })
    }

    /// Create a new agent from a programmatic Config object.
    #[napi(factory)]
    pub fn from_options(config: Config) -> Result<Self> {
        let mut settings = HashMap::new();
        settings.insert("llm_provider".into(), config.llm_provider.clone());
        settings.insert("llm_model".into(), config.llm_model.clone());
        if let Some(ref k) = config.openai_api_key {
            settings.insert("openai_api_key".into(), k.clone());
        }
        if let Some(ref k) = config.anthropic_api_key {
            settings.insert("anthropic_api_key".into(), k.clone());
        }

        Ok(PylotAgent {
            config_path: String::new(),
            settings,
        })
    }

    /// Send a message and get a response.
    ///
    /// Uses the Rust agent core directly via the library crate.
    #[napi]
    pub async fn chat(&self, message: String) -> Result<String> {
        // Load config
        let config = pylot_core::config::AppConfig::load()
            .map_err(|e| Error::from_reason(format!("Failed to load config: {e}")))?;

        // Build LLM provider
        let provider: Arc<dyn pylot_core::llm::LlmProvider> = match config.llm_provider.as_str() {
            "openai" => {
                let key = config.openai_api_key.as_ref().ok_or_else(|| {
                    Error::from_reason("OpenAI API key not configured".to_string())
                })?;
                Arc::new(pylot_core::llm::openai::OpenAIProvider::new(
                    key.clone(),
                    config.llm_model.clone(),
                    4096,
                    0.7,
                ))
            }
            "anthropic" => {
                let key = config.anthropic_api_key.as_ref().ok_or_else(|| {
                    Error::from_reason("Anthropic API key not configured".to_string())
                })?;
                Arc::new(pylot_core::llm::anthropic::AnthropicProvider::new(
                    key.clone(),
                    config.llm_model.clone(),
                    4096,
                ))
            }
            other => {
                return Err(Error::from_reason(format!(
                    "Unknown LLM provider: {other}"
                )))
            }
        };

        let tool_registry = pylot_core::tools::ToolRegistry::new();
        let skill_registry = pylot_core::skills::SkillRegistry::new();
        let system_prompt = format!("You are {}, a helpful AI assistant.", config.agent_name);

        let mut agent = pylot_core::agent::Agent::new(
            provider,
            tool_registry,
            skill_registry,
            system_prompt,
            20,
            10,
            config.data_dir.clone(),
            None,
        )
        .map_err(|e| Error::from_reason(format!("Failed to build agent: {e}")))?;

        agent
            .chat(&message)
            .await
            .map_err(|e| Error::from_reason(format!("Chat error: {e}")))
    }

    /// Run diagnostic checks on the current configuration.
    #[napi]
    pub async fn doctor() -> Result<()> {
        let status = Command::new("pylot")
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
        let status = Command::new("pylot")
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
/// import { PylotAgent, Config } from 'pylot';
///
/// const config = new Config({
///   llmProvider: 'anthropic',
///   llmModel: 'claude-sonnet-4-20250514',
///   anthropicApiKey: process.env.ANTHROPIC_API_KEY,
/// });
/// const agent = new PylotAgent(config);
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

// ── PylotMemory: Node.js wrapper for memory system ──────────────────

#[napi(object)]
pub struct MemorySearchResult {
    pub id: String,
    pub content: String,
    pub score: f64,
}

#[napi(object)]
pub struct SkillInfo {
    pub name: String,
    pub has_skill_file: bool,
}

#[napi(object)]
pub struct LearnedRuleInfo {
    pub id: String,
    pub rule_text: String,
    pub confidence: f64,
}

/// Access the memory system from Node.js.
#[napi]
pub struct PylotMemory {
    db_path: String,
}

#[napi]
impl PylotMemory {
    #[napi(constructor)]
    pub fn new(db_path: Option<String>) -> Self {
        let path = db_path.unwrap_or_else(|| {
            let home = std::env::var("HOME").unwrap_or_default();
            format!("{}/.pylot/data/smart_memory.db", home)
        });
        Self { db_path: path }
    }

    /// Store a memory unit.
    #[napi]
    pub fn remember(&self, content: String, memory_type: Option<String>) -> Result<String> {
        let store = pylot_core::memory_v2::store::MemoryStore::open(std::path::Path::new(&self.db_path))
            .map_err(|e| Error::from_reason(format!("Failed to open memory store: {e}")))?;

        let mem_type = match memory_type.as_deref().unwrap_or("semantic") {
            "episodic" => pylot_core::memory_v2::types::MemoryType::Episodic,
            "preference" => pylot_core::memory_v2::types::MemoryType::Preference,
            "project" => pylot_core::memory_v2::types::MemoryType::ProjectState,
            "procedural" => pylot_core::memory_v2::types::MemoryType::ProceduralObservation,
            _ => pylot_core::memory_v2::types::MemoryType::Semantic,
        };

        let unit = pylot_core::memory_v2::types::MemoryUnit::new(mem_type, content, "default".to_string());
        store.insert(&unit)
            .map_err(|e| Error::from_reason(format!("Failed to store memory: {e}")))?;
        Ok(unit.id.clone())
    }

    /// Search memories by keyword.
    #[napi]
    pub fn search(&self, query: String, limit: Option<u32>) -> Result<Vec<MemorySearchResult>> {
        let store = pylot_core::memory_v2::store::MemoryStore::open(std::path::Path::new(&self.db_path))
            .map_err(|e| Error::from_reason(format!("Failed to open memory store: {e}")))?;

        let results = store.search_keyword(&query, "default", limit.unwrap_or(10) as usize)
            .map_err(|e| Error::from_reason(format!("Search failed: {e}")))?;

        Ok(results.into_iter().map(|(unit, score)| MemorySearchResult {
            id: unit.id,
            content: unit.content,
            score,
        }).collect())
    }

    /// Get total memory count.
    #[napi]
    pub fn count(&self) -> Result<u32> {
        let store = pylot_core::memory_v2::store::MemoryStore::open(std::path::Path::new(&self.db_path))
            .map_err(|e| Error::from_reason(format!("Failed to open memory store: {e}")))?;
        store.count("default")
            .map(|c| c as u32)
            .map_err(|e| Error::from_reason(format!("Count failed: {e}")))
    }
}

// ── PylotSkills: Node.js wrapper for skills system ──────────────────

/// Access the skills system from Node.js.
#[napi]
pub struct PylotSkills {
    skills_dir: String,
}

#[napi]
impl PylotSkills {
    #[napi(constructor)]
    pub fn new(skills_dir: Option<String>) -> Self {
        let dir = skills_dir.unwrap_or_else(|| {
            let home = std::env::var("HOME").unwrap_or_default();
            format!("{}/.pylot/skills", home)
        });
        Self { skills_dir: dir }
    }

    /// List all available skills.
    #[napi]
    pub fn list(&self) -> Result<Vec<SkillInfo>> {
        let mut skills = Vec::new();
        let dir = std::path::Path::new(&self.skills_dir);
        if dir.exists() {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                        let skill_file = path.join("SKILL.md");
                        skills.push(SkillInfo {
                            name,
                            has_skill_file: skill_file.exists(),
                        });
                    }
                }
            }
        }
        Ok(skills)
    }
}

// ── PylotLearning: Node.js wrapper for learning system ──────────────

/// Access the learning system from Node.js.
#[napi]
pub struct PylotLearning {
    db_path: String,
}

#[napi]
impl PylotLearning {
    #[napi(constructor)]
    pub fn new(db_path: Option<String>) -> Self {
        let path = db_path.unwrap_or_else(|| {
            let home = std::env::var("HOME").unwrap_or_default();
            format!("{}/.pylot/data/learning.db", home)
        });
        Self { db_path: path }
    }

    /// List active learned rules.
    #[napi]
    pub fn rules(&self) -> Result<Vec<LearnedRuleInfo>> {
        let pe = pylot_core::learning::PromptEvolution::new(&self.db_path)
            .map_err(|e| Error::from_reason(format!("Failed to open learning DB: {e}")))?;

        let rules = pe.active_rules()
            .map_err(|e| Error::from_reason(format!("Failed to get rules: {e}")))?;

        Ok(rules.into_iter().map(|r| LearnedRuleInfo {
            id: r.id,
            rule_text: r.rule_text,
            confidence: r.confidence,
        }).collect())
    }

    /// Submit feedback.
    #[napi]
    pub fn feedback(&self, session_id: String, turn_id: String, rating: i32, comment: Option<String>) -> Result<()> {
        let pe = pylot_core::learning::PromptEvolution::new(&self.db_path)
            .map_err(|e| Error::from_reason(format!("Failed to open learning DB: {e}")))?;

        let fb = pylot_core::learning::FeedbackProcessor::create_feedback(
            &session_id, &turn_id, rating as i8, comment,
        );
        pylot_core::learning::FeedbackProcessor::process(&pe, &fb)
            .map_err(|e| Error::from_reason(format!("Feedback failed: {e}")))
    }
}
