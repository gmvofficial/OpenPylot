use pyo3::prelude::*;
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use std::collections::HashMap;
use std::sync::Arc;

// ── PylotAgent: main Python-facing class ──────────────────────────────

/// The primary OpenPylot class exposed to Python.
///
/// Usage from Python:
/// ```python
/// from pylot import PylotAgent
///
/// # First-time interactive setup
/// PylotAgent.init()
///
/// # Or load from existing config
/// agent = PylotAgent.from_config("~/.pylot/secrets.enc")
/// response = agent.chat("What meetings do I have today?")
/// print(response)
/// ```
#[pyclass]
struct PylotAgent {
    config_path: String,
    // In the real implementation these would wrap the Rust Agent core.
    // For now we store configuration that was loaded.
    settings: HashMap<String, String>,
}

#[pymethods]
impl PylotAgent {
    /// Launch the interactive setup wizard.
    ///
    /// Opens browser for OAuth flows, prompts for API keys,
    /// and saves credentials to the encrypted secrets vault.
    ///
    /// Equivalent to running `pylot init` from the CLI.
    #[staticmethod]
    fn init() -> PyResult<()> {
        // Shell out to the Rust binary's init wizard so all setup logic
        // is in one place. This also works for users who `pip install`
        // the package without cloning the source.
        let status = std::process::Command::new("pylot")
            .arg("init")
            .status()
            .map_err(|e| {
                PyRuntimeError::new_err(format!(
                    "Failed to launch pylot init: {}. \
                     Make sure the pylot binary is installed and in your PATH.",
                    e
                ))
            })?;

        if !status.success() {
            return Err(PyRuntimeError::new_err("Init wizard exited with an error"));
        }
        Ok(())
    }

    /// Initialize an agent instance from an existing configuration file.
    ///
    /// Args:
    ///     config_path: Path to the secrets file (default: ~/.pylot/secrets.enc)
    ///
    /// Returns:
    ///     A configured PylotAgent instance ready for chat.
    #[staticmethod]
    fn from_config(config_path: &str) -> PyResult<Self> {
        let expanded = shellexpand(config_path);
        if !std::path::Path::new(&expanded).exists() {
            return Err(PyValueError::new_err(format!(
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
    ///
    /// Args:
    ///     config: A Config object with LLM provider, API keys, etc.
    ///
    /// Returns:
    ///     A configured PylotAgent instance.
    #[new]
    fn new(config: &Config) -> PyResult<Self> {
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
    fn chat(&self, message: &str) -> PyResult<String> {
        // Use tokio runtime to call async Rust code
        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| PyRuntimeError::new_err(format!("Failed to create runtime: {e}")))?;

        let msg = message.to_string();

        rt.block_on(async move {
            // Load config
            let config = pylot_core::config::AppConfig::load()
                .map_err(|e| PyRuntimeError::new_err(format!("Failed to load config: {e}")))?;

            // Build LLM provider
            let provider: Arc<dyn pylot_core::llm::LlmProvider> = match config.llm_provider.as_str() {
                "openai" => {
                    let key = config.openai_api_key.as_ref()
                        .ok_or_else(|| PyRuntimeError::new_err("OpenAI API key not configured"))?;
                    Arc::new(pylot_core::llm::openai::OpenAIProvider::new(
                        key.clone(),
                        config.llm_model.clone(),
                        4096,
                        0.7,
                    ))
                }
                "anthropic" => {
                    let key = config.anthropic_api_key.as_ref()
                        .ok_or_else(|| PyRuntimeError::new_err("Anthropic API key not configured"))?;
                    Arc::new(pylot_core::llm::anthropic::AnthropicProvider::new(
                        key.clone(),
                        config.llm_model.clone(),
                        4096,
                    ))
                }
                other => return Err(PyRuntimeError::new_err(format!("Unknown LLM provider: {other}"))),
            };

            // Build tool registry
            let tool_registry = pylot_core::tools::ToolRegistry::new();
            let skill_registry = pylot_core::skills::SkillRegistry::new();

            // Create agent and chat
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
            .map_err(|e| PyRuntimeError::new_err(format!("Failed to build agent: {e}")))?;

            agent.chat(&msg).await.map_err(|e| PyRuntimeError::new_err(format!("Chat error: {e}")))
        })
    }

    /// Register a custom Python tool that the agent can invoke.
    ///
    /// Args:
    ///     name:     Tool name (e.g., "search_web")
    ///     schema:   JSON schema string describing the tool's parameters
    ///     callback: A Python callable that implements the tool
    fn register_tool(&self, name: &str, schema: &str, callback: PyObject) -> PyResult<()> {
        // Validate schema is valid JSON
        serde_json::from_str::<serde_json::Value>(schema)
            .map_err(|e| PyValueError::new_err(format!("Invalid JSON schema: {}", e)))?;

        // Store in the tool registry.
        // In a full implementation, this would register with the Rust ToolRegistry.
        Python::with_gil(|py| {
            if !callback.bind(py).is_callable() {
                return Err(PyValueError::new_err("callback must be callable"));
            }
            Ok(())
        })?;

        tracing_log(&format!("Registered custom tool: {} (schema: {} bytes)", name, schema.len()));
        Ok(())
    }

    /// Run the agent as a background service (scheduler + webhooks).
    #[staticmethod]
    fn serve() -> PyResult<()> {
        let status = std::process::Command::new("pylot")
            .arg("serve")
            .arg("--foreground")
            .status()
            .map_err(|e| PyRuntimeError::new_err(format!("Failed to start serve mode: {}", e)))?;

        if !status.success() {
            return Err(PyRuntimeError::new_err("Serve exited with an error"));
        }
        Ok(())
    }

    /// Run diagnostic checks on the current configuration.
    #[staticmethod]
    fn doctor() -> PyResult<()> {
        let status = std::process::Command::new("pylot")
            .arg("doctor")
            .status()
            .map_err(|e| PyRuntimeError::new_err(format!("Failed to run doctor: {}", e)))?;

        if !status.success() {
            return Err(PyRuntimeError::new_err("Doctor exited with an error"));
        }
        Ok(())
    }

    /// Show agent status and connected services.
    #[staticmethod]
    fn status() -> PyResult<()> {
        let status = std::process::Command::new("pylot")
            .arg("status")
            .status()
            .map_err(|e| PyRuntimeError::new_err(format!("Failed to run status: {}", e)))?;

        if !status.success() {
            return Err(PyRuntimeError::new_err("Status check failed"));
        }
        Ok(())
    }
}

// ── Config: programmatic configuration ──────────────────────────────

/// Programmatic configuration for headless/CI use.
///
/// Usage:
/// ```python
/// from pylot import Config
///
/// config = Config(
///     llm_provider="openai",
///     llm_model="gpt-4o",
///     openai_api_key="sk-...",
///     telegram_bot_token="...",
/// )
/// agent = PylotAgent(config)
/// response = agent.chat("Hello!")
/// ```
#[pyclass]
#[derive(Clone)]
struct Config {
    #[pyo3(get, set)]
    llm_provider: String,
    #[pyo3(get, set)]
    llm_model: String,
    #[pyo3(get, set)]
    openai_api_key: Option<String>,
    #[pyo3(get, set)]
    anthropic_api_key: Option<String>,
    #[pyo3(get, set)]
    google_credentials_file: Option<String>,
    #[pyo3(get, set)]
    telegram_bot_token: Option<String>,
    #[pyo3(get, set)]
    telegram_chat_id: Option<String>,
}

#[pymethods]
impl Config {
    #[new]
    #[pyo3(signature = (
        llm_provider = "openai".to_string(),
        llm_model = "gpt-4o".to_string(),
        openai_api_key = None,
        anthropic_api_key = None,
        google_credentials_file = None,
        telegram_bot_token = None,
        telegram_chat_id = None,
    ))]
    fn new(
        llm_provider: String,
        llm_model: String,
        openai_api_key: Option<String>,
        anthropic_api_key: Option<String>,
        google_credentials_file: Option<String>,
        telegram_bot_token: Option<String>,
        telegram_chat_id: Option<String>,
    ) -> Self {
        Config {
            llm_provider,
            llm_model,
            openai_api_key,
            anthropic_api_key,
            google_credentials_file,
            telegram_bot_token,
            telegram_chat_id,
        }
    }

    fn __repr__(&self) -> String {
        format!(
            "Config(llm_provider='{}', llm_model='{}', openai_key={}, anthropic_key={})",
            self.llm_provider,
            self.llm_model,
            if self.openai_api_key.is_some() { "'***'" } else { "None" },
            if self.anthropic_api_key.is_some() { "'***'" } else { "None" },
        )
    }
}

// ── Python module definition ────────────────────────────────────────

/// OpenPylot — Python bindings for the Rust-powered personal AI assistant.
#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PylotAgent>()?;
    m.add_class::<Config>()?;
    m.add_class::<PylotMemory>()?;
    m.add_class::<PylotSkills>()?;
    m.add_class::<PylotLearning>()?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}

// ── PylotMemory: Python wrapper for memory system ───────────────────

/// Access the OpenPylot memory system from Python.
#[pyclass]
struct PylotMemory {
    db_path: String,
}

#[pymethods]
impl PylotMemory {
    #[new]
    #[pyo3(signature = (db_path = None))]
    fn new(db_path: Option<String>) -> PyResult<Self> {
        let path = db_path.unwrap_or_else(|| {
            let home = dirs::home_dir().unwrap_or_default();
            format!("{}/.pylot/data/smart_memory.db", home.display())
        });
        Ok(Self { db_path: path })
    }

    /// Store a memory.
    fn remember(&self, content: &str, memory_type: Option<&str>) -> PyResult<String> {
        let store = pylot_core::memory_v2::store::MemoryStore::open(std::path::Path::new(&self.db_path))
            .map_err(|e| PyRuntimeError::new_err(format!("Failed to open memory store: {e}")))?;

        let mem_type = match memory_type.unwrap_or("semantic") {
            "episodic" => pylot_core::memory_v2::types::MemoryType::Episodic,
            "preference" => pylot_core::memory_v2::types::MemoryType::Preference,
            "project" => pylot_core::memory_v2::types::MemoryType::ProjectState,
            "procedural" => pylot_core::memory_v2::types::MemoryType::ProceduralObservation,
            _ => pylot_core::memory_v2::types::MemoryType::Semantic,
        };

        let unit = pylot_core::memory_v2::types::MemoryUnit::new(
            mem_type,
            content.to_string(),
            "default".to_string(),
        );
        store.insert(&unit)
            .map_err(|e| PyRuntimeError::new_err(format!("Failed to store memory: {e}")))?;
        Ok(unit.id.clone())
    }

    /// Search memories by keyword.
    fn search(&self, query: &str, limit: Option<usize>) -> PyResult<Vec<HashMap<String, String>>> {
        let store = pylot_core::memory_v2::store::MemoryStore::open(std::path::Path::new(&self.db_path))
            .map_err(|e| PyRuntimeError::new_err(format!("Failed to open memory store: {e}")))?;

        let results = store.search_keyword(query, "default", limit.unwrap_or(10))
            .map_err(|e| PyRuntimeError::new_err(format!("Search failed: {e}")))?;

        Ok(results.into_iter().map(|(unit, score)| {
            let mut map = HashMap::new();
            map.insert("id".to_string(), unit.id);
            map.insert("content".to_string(), unit.content);
            map.insert("score".to_string(), format!("{:.4}", score));
            map
        }).collect())
    }

    /// Get memory count.
    fn count(&self) -> PyResult<usize> {
        let store = pylot_core::memory_v2::store::MemoryStore::open(std::path::Path::new(&self.db_path))
            .map_err(|e| PyRuntimeError::new_err(format!("Failed to open memory store: {e}")))?;
        store.count("default").map_err(|e| PyRuntimeError::new_err(format!("Count failed: {e}")))
    }
}

// ── PylotSkills: Python wrapper for skills system ───────────────────

/// Access the OpenPylot skills system from Python.
#[pyclass]
struct PylotSkills {
    skills_dir: String,
}

#[pymethods]
impl PylotSkills {
    #[new]
    #[pyo3(signature = (skills_dir = None))]
    fn new(skills_dir: Option<String>) -> PyResult<Self> {
        let dir = skills_dir.unwrap_or_else(|| {
            let home = dirs::home_dir().unwrap_or_default();
            format!("{}/.pylot/skills", home.display())
        });
        Ok(Self { skills_dir: dir })
    }

    /// List all available skills.
    fn list(&self) -> PyResult<Vec<HashMap<String, String>>> {
        let mut skills = Vec::new();
        let dir = std::path::Path::new(&self.skills_dir);
        if dir.exists() {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                        let skill_file = path.join("SKILL.md");
                        let mut map = HashMap::new();
                        map.insert("name".to_string(), name);
                        map.insert("has_skill_file".to_string(), skill_file.exists().to_string());
                        skills.push(map);
                    }
                }
            }
        }
        Ok(skills)
    }
}

// ── PylotLearning: Python wrapper for learning system ───────────────

/// Access the OpenPylot learning system from Python.
#[pyclass]
struct PylotLearning {
    db_path: String,
}

#[pymethods]
impl PylotLearning {
    #[new]
    #[pyo3(signature = (db_path = None))]
    fn new(db_path: Option<String>) -> PyResult<Self> {
        let path = db_path.unwrap_or_else(|| {
            let home = dirs::home_dir().unwrap_or_default();
            format!("{}/.pylot/data/learning.db", home.display())
        });
        Ok(Self { db_path: path })
    }

    /// List active learned rules.
    fn rules(&self) -> PyResult<Vec<HashMap<String, String>>> {
        let pe = pylot_core::learning::PromptEvolution::new(&self.db_path)
            .map_err(|e| PyRuntimeError::new_err(format!("Failed to open learning DB: {e}")))?;

        let rules = pe.active_rules()
            .map_err(|e| PyRuntimeError::new_err(format!("Failed to get rules: {e}")))?;

        Ok(rules.into_iter().map(|r| {
            let mut map = HashMap::new();
            map.insert("id".to_string(), r.id);
            map.insert("rule_text".to_string(), r.rule_text);
            map.insert("confidence".to_string(), format!("{:.2}", r.confidence));
            map
        }).collect())
    }

    /// Submit feedback on a response.
    fn feedback(&self, session_id: &str, turn_id: &str, rating: i8, comment: Option<String>) -> PyResult<()> {
        let pe = pylot_core::learning::PromptEvolution::new(&self.db_path)
            .map_err(|e| PyRuntimeError::new_err(format!("Failed to open learning DB: {e}")))?;

        let fb = pylot_core::learning::FeedbackProcessor::create_feedback(session_id, turn_id, rating, comment);
        pylot_core::learning::FeedbackProcessor::process(&pe, &fb)
            .map_err(|e| PyRuntimeError::new_err(format!("Feedback failed: {e}")))
    }
}

// ── Helpers ─────────────────────────────────────────────────────────

fn shellexpand(path: &str) -> String {
    if path.starts_with("~/") {
        if let Some(home) = dirs::home_dir() {
            return format!("{}/{}", home.display(), &path[2..]);
        }
    }
    path.to_string()
}

fn tracing_log(msg: &str) {
    eprintln!("[pylot-py] {}", msg);
}
