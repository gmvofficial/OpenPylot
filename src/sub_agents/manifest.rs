//! # Agent Manifest System
//!
//! Plug-and-play sub-agent definitions loaded from TOML files.
//!
//! Manifest locations (precedence: workspace > local > bundled):
//! - `./agents/*.toml` — workspace-level (per-project)
//! - `~/.pylot/agents/*.toml` — user-level
//! - `<exe_dir>/agents/*.toml` — bundled with binary
//!
//! ## Manifest format
//!
//! ```toml
//! name = "coder"
//! description = "Specialist agent for writing and refactoring code"
//! agent_type = "specialist"      # task | background | specialist
//!
//! # Optional: override the default model for this agent
//! model_override = "claude-3-5-sonnet-20241022"
//!
//! # Restrict which tools this agent may use (null = inherit all)
//! allowed_tools = ["read_file", "write_file", "run_bash", "search_web"]
//!
//! timeout_secs = 600
//! max_iterations = 20
//!
//! system_prompt = """
//! You are an expert software engineer. Write clean, tested code.
//! Prefer editing existing files over creating new ones.
//! """
//! ```
//!
//! ## Usage
//!
//! ```bash
//! pylot agents list-presets          # show all manifests
//! pylot agents show coder            # print a manifest
//! pylot agents path                  # where to drop new .toml files
//! pylot agents spawn --preset coder "refactor src/foo.rs"
//! ```

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::types::{SubAgentConfig, SubAgentType};

/// Raw manifest as parsed from a TOML file. Mirrors [`SubAgentConfig`] but with
/// user-friendly defaults and optional fields.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentManifest {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_agent_type")]
    pub agent_type: String,
    #[serde(default)]
    pub system_prompt: String,
    #[serde(default)]
    pub model_override: Option<String>,
    #[serde(default)]
    pub allowed_tools: Option<Vec<String>>,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    #[serde(default = "default_max_iterations")]
    pub max_iterations: usize,
    /// Where the manifest was loaded from (filled by loader; not user-supplied).
    #[serde(skip)]
    pub source_path: Option<PathBuf>,
    /// Source tier — bundled, local, or workspace.
    #[serde(skip)]
    pub source: ManifestSource,
}

fn default_agent_type() -> String {
    "task".to_string()
}
fn default_timeout() -> u64 {
    300
}
fn default_max_iterations() -> usize {
    10
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ManifestSource {
    #[default]
    Bundled,
    Local,
    Workspace,
}

impl ManifestSource {
    fn precedence(&self) -> u8 {
        match self {
            ManifestSource::Bundled => 0,
            ManifestSource::Local => 1,
            ManifestSource::Workspace => 2,
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            ManifestSource::Bundled => "bundled",
            ManifestSource::Local => "local",
            ManifestSource::Workspace => "workspace",
        }
    }
}

impl AgentManifest {
    /// Convert to a runtime [`SubAgentConfig`] (assigns a fresh id).
    pub fn into_config(self) -> SubAgentConfig {
        let agent_type = match self.agent_type.to_ascii_lowercase().as_str() {
            "background" => SubAgentType::Background,
            "specialist" => SubAgentType::Specialist,
            _ => SubAgentType::Task,
        };
        SubAgentConfig {
            id: uuid::Uuid::new_v4().to_string(),
            name: self.name,
            agent_type,
            system_prompt: self.system_prompt,
            model_override: self.model_override,
            allowed_tools: self.allowed_tools,
            timeout_secs: self.timeout_secs,
            max_iterations: self.max_iterations,
            parent_id: None,
            interval_secs: None,
        }
    }
}

/// Registry of loaded agent manifests, keyed by name.
#[derive(Debug, Default, Clone)]
pub struct AgentManifestRegistry {
    manifests: HashMap<String, AgentManifest>,
}

impl AgentManifestRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Load from all standard locations (bundled, local, workspace).
    /// Higher-precedence sources override lower ones by manifest `name`.
    pub fn load_all(workspace: Option<&Path>) -> Self {
        let mut reg = Self::new();
        for (dir, source) in Self::search_paths(workspace) {
            if !dir.exists() {
                continue;
            }
            let loaded = Self::scan_dir(&dir, source.clone());
            for m in loaded {
                reg.insert(m);
            }
        }
        reg
    }

    fn search_paths(workspace: Option<&Path>) -> Vec<(PathBuf, ManifestSource)> {
        let mut paths = Vec::new();

        // Bundled (next to exe, or repo root during dev)
        if let Ok(exe) = std::env::current_exe() {
            if let Some(parent) = exe.parent() {
                paths.push((parent.join("agents"), ManifestSource::Bundled));
            }
        }
        paths.push((PathBuf::from("agents"), ManifestSource::Bundled));

        // Local (~/.pylot/agents/)
        if let Some(home) = dirs::home_dir() {
            paths.push((home.join(".pylot").join("agents"), ManifestSource::Local));
        }

        // Workspace (./agents relative to a workspace root)
        if let Some(ws) = workspace {
            paths.push((ws.join("agents"), ManifestSource::Workspace));
        }

        paths
    }

    fn scan_dir(dir: &Path, source: ManifestSource) -> Vec<AgentManifest> {
        let mut out = Vec::new();
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return out,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                continue;
            }
            match Self::parse_file(&path, source.clone()) {
                Ok(m) => out.push(m),
                Err(e) => {
                    tracing::warn!("Invalid agent manifest {}: {e:#}", path.display());
                }
            }
        }
        out
    }

    fn parse_file(path: &Path, source: ManifestSource) -> Result<AgentManifest> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        let mut m: AgentManifest = toml::from_str(&text)
            .with_context(|| format!("Invalid TOML in {}", path.display()))?;
        m.source_path = Some(path.to_path_buf());
        m.source = source;
        Ok(m)
    }

    /// Insert, respecting precedence (higher source wins).
    fn insert(&mut self, m: AgentManifest) {
        let replace = match self.manifests.get(&m.name) {
            Some(existing) => m.source.precedence() >= existing.source.precedence(),
            None => true,
        };
        if replace {
            self.manifests.insert(m.name.clone(), m);
        }
    }

    pub fn get(&self, name: &str) -> Option<&AgentManifest> {
        self.manifests.get(name)
    }

    pub fn all(&self) -> Vec<&AgentManifest> {
        let mut v: Vec<&AgentManifest> = self.manifests.values().collect();
        v.sort_by(|a, b| a.name.cmp(&b.name));
        v
    }

    pub fn len(&self) -> usize {
        self.manifests.len()
    }

    pub fn is_empty(&self) -> bool {
        self.manifests.is_empty()
    }

    /// Path that users should drop new manifest TOML files into.
    pub fn user_agents_dir() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(".pylot").join("agents"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn parse_minimal_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("coder.toml");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(
            f,
            "name = \"coder\"\nagent_type = \"specialist\"\nsystem_prompt = \"be helpful\"\n"
        )
        .unwrap();

        let m = AgentManifestRegistry::parse_file(&path, ManifestSource::Local).unwrap();
        assert_eq!(m.name, "coder");
        assert_eq!(m.agent_type, "specialist");
        assert_eq!(m.source, ManifestSource::Local);

        let cfg = m.into_config();
        assert_eq!(cfg.agent_type, SubAgentType::Specialist);
        assert_eq!(cfg.timeout_secs, 300);
        assert_eq!(cfg.max_iterations, 10);
    }

    #[test]
    fn precedence_workspace_beats_local() {
        let mut reg = AgentManifestRegistry::new();
        let bundled = AgentManifest {
            name: "dup".into(),
            description: "bundled".into(),
            agent_type: "task".into(),
            system_prompt: "b".into(),
            model_override: None,
            allowed_tools: None,
            timeout_secs: 300,
            max_iterations: 10,
            source_path: None,
            source: ManifestSource::Bundled,
        };
        let workspace = AgentManifest {
            source: ManifestSource::Workspace,
            description: "workspace".into(),
            ..bundled.clone()
        };
        reg.insert(bundled);
        reg.insert(workspace);
        assert_eq!(reg.get("dup").unwrap().description, "workspace");
    }
}
