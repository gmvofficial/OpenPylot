use pyo3::prelude::*;
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use std::collections::HashMap;

// ── GMVAgent: main Python-facing class ──────────────────────────────

/// The primary GMV Agent class exposed to Python.
///
/// Usage from Python:
/// ```python
/// from gmv_agent import GMVAgent
///
/// # First-time interactive setup
/// GMVAgent.init()
///
/// # Or load from existing config
/// agent = GMVAgent.from_config("~/.gmv-agent/secrets.enc")
/// response = agent.chat("What meetings do I have today?")
/// print(response)
/// ```
#[pyclass]
struct GMVAgent {
    config_path: String,
    // In the real implementation these would wrap the Rust Agent core.
    // For now we store configuration that was loaded.
    settings: HashMap<String, String>,
}

#[pymethods]
impl GMVAgent {
    /// Launch the interactive setup wizard.
    ///
    /// Opens browser for OAuth flows, prompts for API keys,
    /// and saves credentials to the encrypted secrets vault.
    ///
    /// Equivalent to running `gmv-agent init` from the CLI.
    #[staticmethod]
    fn init() -> PyResult<()> {
        // Shell out to the Rust binary's init wizard so all setup logic
        // is in one place. This also works for users who `pip install`
        // the package without cloning the source.
        let status = std::process::Command::new("gmv-agent")
            .arg("init")
            .status()
            .map_err(|e| {
                PyRuntimeError::new_err(format!(
                    "Failed to launch gmv-agent init: {}. \
                     Make sure the gmv-agent binary is installed and in your PATH.",
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
    ///     config_path: Path to the secrets file (default: ~/.gmv-agent/secrets.enc)
    ///
    /// Returns:
    ///     A configured GMVAgent instance ready for chat.
    #[staticmethod]
    fn from_config(config_path: &str) -> PyResult<Self> {
        let expanded = shellexpand(config_path);
        if !std::path::Path::new(&expanded).exists() {
            return Err(PyValueError::new_err(format!(
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
    ///
    /// Args:
    ///     config: A Config object with LLM provider, API keys, etc.
    ///
    /// Returns:
    ///     A configured GMVAgent instance.
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

        Ok(GMVAgent {
            config_path: String::new(),
            settings,
        })
    }

    /// Send a message and get a response.
    ///
    /// Args:
    ///     message: The user message to send.
    ///
    /// Returns:
    ///     The agent's response as a string.
    fn chat(&self, message: &str) -> PyResult<String> {
        // Delegate to the gmv-agent binary for now.
        // A future version will call the Rust agent core directly
        // via the library crate for lower latency.
        let output = std::process::Command::new("gmv-agent")
            .arg("chat")
            .arg(message)
            .output()
            .map_err(|e| PyRuntimeError::new_err(format!("Failed to run gmv-agent chat: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(PyRuntimeError::new_err(format!("Chat failed: {}", stderr)));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
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
        let status = std::process::Command::new("gmv-agent")
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
        let status = std::process::Command::new("gmv-agent")
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
        let status = std::process::Command::new("gmv-agent")
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
/// from gmv_agent import Config
///
/// config = Config(
///     llm_provider="openai",
///     llm_model="gpt-4o",
///     openai_api_key="sk-...",
///     telegram_bot_token="...",
/// )
/// agent = GMVAgent(config)
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

/// GMV Agent — Python bindings for the Rust-powered personal AI assistant.
#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<GMVAgent>()?;
    m.add_class::<Config>()?;
    m.add("__version__", "0.2.0")?;
    Ok(())
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
    eprintln!("[gmv-agent-py] {}", msg);
}
