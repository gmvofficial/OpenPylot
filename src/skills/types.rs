use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ── Skill metadata (YAML frontmatter) ────────────────────────────────

/// Parsed from the YAML frontmatter of a SKILL.md file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMeta {
    pub name: String,
    pub description: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    /// OS filter: empty = all platforms. Values: "macos", "linux", "windows"
    #[serde(default)]
    pub os: Vec<String>,
    /// Optional requirements that must be met for the skill to be available.
    #[serde(default)]
    pub requires: Option<SkillRequirements>,
    /// Optional installers for missing requirements.
    #[serde(default)]
    pub install: Vec<SkillInstaller>,
    /// Example prompts that trigger this skill.
    #[serde(default)]
    pub examples: Vec<String>,
}

fn default_version() -> String {
    "1.0.0".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillRequirements {
    /// Required binaries that must be on PATH.
    #[serde(default)]
    pub bins: Vec<String>,
    /// Required environment variables.
    #[serde(default)]
    pub env: Vec<String>,
    /// Required tools that must be registered in the ToolRegistry.
    #[serde(default)]
    pub tools: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillInstaller {
    pub id: String,
    /// Kind: brew, npm, pip, cargo, download
    pub kind: String,
    #[serde(default)]
    pub formula: Option<String>,
    #[serde(default)]
    pub package: Option<String>,
    #[serde(default)]
    pub bins: Vec<String>,
    #[serde(default)]
    pub label: Option<String>,
}

// ── Loaded skill ─────────────────────────────────────────────────────

/// A fully loaded skill: metadata + markdown body + origin.
#[derive(Debug, Clone)]
pub struct Skill {
    pub meta: SkillMeta,
    /// The markdown body (instructions) after the YAML frontmatter.
    pub content: String,
    /// Filesystem path the SKILL.md was loaded from.
    pub source_path: PathBuf,
    /// Where this skill was loaded from (determines precedence).
    pub source: SkillSource,
}

/// The origin of a loaded skill. Higher-precedence sources override lower ones
/// when two skills share the same `meta.name`.
///
/// Precedence (lowest → highest): Bundled < Local < Workspace
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SkillSource {
    /// Ships with the binary (in `skills/` next to the executable).
    Bundled,
    /// User-installed at `~/.pylot/skills/`.
    Local,
    /// Project-specific at `./skills/` in the current workspace.
    Workspace,
}
