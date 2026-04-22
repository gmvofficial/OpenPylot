//! # Skills Module
//!
//! The skill system loads SKILL.md files (YAML frontmatter + markdown instructions)
//! from multiple sources with precedence: Extra < Bundled < Local < Workspace.
//!
//! Skills are matched to user messages via keyword scoring and injected into the
//! LLM system prompt to provide specialized behavior.
//!
//! ## Architecture (inspired by OpenClaw)
//!
//! - **Loading**: `loader.rs` — parses SKILL.md (YAML frontmatter + markdown body)
//! - **Registry**: `registry.rs` — multi-source loading with precedence merging
//! - **Matching**: `matcher.rs` — keyword-based skill selection (no LLM call)
//! - **Config**: `config.rs` — per-skill enable/disable, API keys, limits
//! - **Security**: `scanner.rs` — scans scripts for dangerous patterns
//! - **Status**: `status.rs` — detailed report for frontend dashboard
//! - **Prompt**: `prompt.rs` — XML-formatted progressive disclosure for LLM
//! - **Watcher**: `watcher.rs` — hot-reload on file changes

pub mod config;
pub mod loader;
pub mod matcher;
pub mod prompt;
pub mod registry;
pub mod scanner;
pub mod status;
pub mod types;
pub mod watcher;

pub use config::{SkillEntryConfig, SkillsConfig, SkillsLimits};
pub use loader::SkillLoader;
pub use prompt::{format_matched_skills, format_skills_xml};
pub use registry::SkillRegistry;
pub use scanner::{scan_skill_directory, verify_contained_path, ScanSummary};
pub use status::{build_status_report, SkillStatusEntry, SkillStatusReport};
pub use types::{Skill, SkillMeta, SkillSource};
