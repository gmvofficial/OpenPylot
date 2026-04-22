//! # Skill Configuration
//!
//! Per-skill config entries (enable/disable, API keys, env overrides)
//! and global limits. Mirrors OpenClaw's `SkillsConfig` / `SkillConfig`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Top-level `[skills]` config in `default.toml`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SkillsConfig {
    /// Global on/off switch for the skill system.
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Matching strategy: "keyword", "embedding", or "auto".
    #[serde(default = "default_retrieval_mode")]
    pub retrieval_mode: String,

    /// Max skills to inject per message.
    #[serde(default = "default_top_k")]
    pub top_k: usize,

    /// Max total chars for matched skill content in the prompt.
    #[serde(default = "default_max_prompt_chars")]
    pub max_prompt_chars: usize,

    /// Hot-reload on file changes.
    #[serde(default)]
    pub watch: bool,

    /// Extra directories to scan for skills.
    #[serde(default)]
    pub extra_dirs: Vec<String>,

    /// Resource limits to prevent abuse.
    #[serde(default)]
    pub limits: SkillsLimits,

    /// Per-skill configuration.
    #[serde(default)]
    pub entries: HashMap<String, SkillEntryConfig>,
}

/// Limits on skill loading (prevents abuse from huge skill directories).
/// Mirrors OpenClaw's `SkillsLimitsConfig`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillsLimits {
    /// Max immediate child dirs to scan per skill root.
    #[serde(default = "default_max_candidates")]
    pub max_candidates_per_root: usize,

    /// Max skills to load from any single source (bundled/local/workspace).
    #[serde(default = "default_max_per_source")]
    pub max_skills_per_source: usize,

    /// Max skills to include in the LLM-facing prompt.
    #[serde(default = "default_max_in_prompt")]
    pub max_skills_in_prompt: usize,

    /// Max SKILL.md file size in bytes.
    #[serde(default = "default_max_file_bytes")]
    pub max_skill_file_bytes: u64,
}

impl Default for SkillsLimits {
    fn default() -> Self {
        Self {
            max_candidates_per_root: 300,
            max_skills_per_source: 200,
            max_skills_in_prompt: 50,
            max_skill_file_bytes: 256_000,
        }
    }
}

/// Per-skill config entry. Stored in config under `[skills.entries.<skill-key>]`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SkillEntryConfig {
    /// Explicitly enable/disable this skill.
    pub enabled: Option<bool>,

    /// API key for skills that need one (stored in config, injected as env var).
    pub api_key: Option<String>,

    /// Extra environment variables to set when this skill is active.
    #[serde(default)]
    pub env: HashMap<String, String>,
}

impl SkillEntryConfig {
    /// Whether this skill is explicitly disabled.
    pub fn is_disabled(&self) -> bool {
        self.enabled == Some(false)
    }

    /// Whether this skill is explicitly enabled (not just default).
    pub fn is_explicitly_enabled(&self) -> bool {
        self.enabled == Some(true)
    }
}

// ── Defaults ─────────────────────────────────────────────────────────

fn default_true() -> bool {
    true
}
fn default_retrieval_mode() -> String {
    "keyword".to_string()
}
fn default_top_k() -> usize {
    3
}
fn default_max_prompt_chars() -> usize {
    30_000
}
fn default_max_candidates() -> usize {
    300
}
fn default_max_per_source() -> usize {
    200
}
fn default_max_in_prompt() -> usize {
    50
}
fn default_max_file_bytes() -> u64 {
    256_000
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_defaults() {
        let config = SkillsConfig::default();
        assert!(!config.enabled); // Default derive gives false, use default_true via serde
        assert_eq!(config.top_k, 0); // Default derive
    }

    #[test]
    fn test_entry_disabled() {
        let entry = SkillEntryConfig {
            enabled: Some(false),
            ..Default::default()
        };
        assert!(entry.is_disabled());
        assert!(!entry.is_explicitly_enabled());
    }

    #[test]
    fn test_entry_with_api_key() {
        let entry = SkillEntryConfig {
            api_key: Some("sk-test123".to_string()),
            ..Default::default()
        };
        assert!(entry.api_key.is_some());
        assert!(!entry.is_disabled());
    }
}
