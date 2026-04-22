use std::collections::HashMap;
use std::path::Path;

use tracing;

use super::config::SkillEntryConfig;
use super::loader::SkillLoader;
use super::matcher::{is_skill_eligible, SkillMatcher};
use super::types::{Skill, SkillSource};

/// Central registry of all loaded skills.
///
/// Implements 3-source precedence: Bundled < Local < Workspace.
/// Skills with the same `meta.name` are overwritten by higher-precedence sources.
pub struct SkillRegistry {
    /// Skills keyed by name. Higher-precedence sources overwrite lower ones.
    skills: HashMap<String, Skill>,
    /// Per-skill config (enabled/disabled state) loaded from skills-config.json.
    entry_configs: HashMap<String, SkillEntryConfig>,
}

impl SkillRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            skills: HashMap::new(),
            entry_configs: HashMap::new(),
        }
    }

    /// Load all skills from bundled, local, and workspace directories.
    /// Applies precedence: bundled → local → workspace (last overwrites).
    /// Also loads per-skill enabled/disabled config from skills-config.json.
    pub fn load_all(workspace_dir: Option<&Path>) -> Self {
        let mut registry = Self::new();

        // Load per-skill config (enabled/disabled) from data_dir/skills-config.json
        let config_path = dirs::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join(".pylot")
            .join("data")
            .join("skills-config.json");
        if config_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&config_path) {
                if let Ok(configs) =
                    serde_json::from_str::<HashMap<String, SkillEntryConfig>>(&content)
                {
                    tracing::info!("Loaded skills-config.json with {} entries", configs.len());
                    registry.entry_configs = configs;
                }
            }
        }

        // 1. Bundled skills (lowest precedence)
        if let Some(dir) = SkillLoader::bundled_skills_dir() {
            let bundled = SkillLoader::scan_directory(&dir, SkillSource::Bundled);
            tracing::info!(
                "Loaded {} bundled skills from {}",
                bundled.len(),
                dir.display()
            );
            for skill in bundled {
                registry.insert_skill(skill);
            }
        }

        // 2. Local skills (~/.pylot/skills/)
        if let Some(dir) = SkillLoader::local_skills_dir() {
            let local = SkillLoader::scan_directory(&dir, SkillSource::Local);
            if !local.is_empty() {
                tracing::info!("Loaded {} local skills from {}", local.len(), dir.display());
            }
            for skill in local {
                registry.insert_skill(skill);
            }
        }

        // 3. Workspace skills (highest precedence)
        if let Some(dir) = SkillLoader::workspace_skills_dir(workspace_dir) {
            let workspace = SkillLoader::scan_directory(&dir, SkillSource::Workspace);
            if !workspace.is_empty() {
                tracing::info!(
                    "Loaded {} workspace skills from {}",
                    workspace.len(),
                    dir.display()
                );
            }
            for skill in workspace {
                registry.insert_skill(skill);
            }
        }

        registry
    }

    /// Insert a skill, overwriting any existing skill with the same name.
    pub fn insert_skill(&mut self, skill: Skill) {
        let name = skill.meta.name.clone();
        if let Some(existing) = self.skills.get(&name) {
            tracing::debug!(
                "Skill '{}' overridden: {:?} → {:?}",
                name,
                existing.source,
                skill.source
            );
        }
        self.skills.insert(name, skill);
    }

    /// Get all eligible skills (filtered by OS, required bins, env, and enabled/disabled config).
    pub fn eligible_skills(&self) -> Vec<&Skill> {
        self.skills
            .values()
            .filter(|s| {
                // Check if skill is disabled in skills-config.json
                if let Some(config) = self.entry_configs.get(&s.meta.name) {
                    if config.is_disabled() {
                        tracing::debug!("Skill '{}' skipped: disabled in config", s.meta.name);
                        return false;
                    }
                }
                is_skill_eligible(s)
            })
            .collect()
    }

    /// Get all skills (including ineligible ones).
    pub fn all_skills(&self) -> Vec<&Skill> {
        self.skills.values().collect()
    }

    /// Get a skill by name.
    pub fn get(&self, name: &str) -> Option<&Skill> {
        self.skills.get(name)
    }

    /// Match eligible skills against a user message.
    pub fn match_for_message(&self, user_message: &str, top_k: usize) -> Vec<&Skill> {
        let eligible: Vec<Skill> = self
            .skills
            .values()
            .filter(|s| {
                // Check disabled in config
                if let Some(config) = self.entry_configs.get(&s.meta.name) {
                    if config.is_disabled() {
                        return false;
                    }
                }
                is_skill_eligible(s)
            })
            .cloned()
            .collect();
        // Re-collect into owned to satisfy lifetimes, then map back to refs
        let matched_names: Vec<String> = SkillMatcher::match_skills(&eligible, user_message, top_k)
            .into_iter()
            .map(|s| s.meta.name.clone())
            .collect();
        matched_names
            .iter()
            .filter_map(|name| self.skills.get(name))
            .collect()
    }

    /// Build a skills section for injection into the system prompt.
    /// Lists available skill names + descriptions so the agent knows what's available.
    pub fn build_skills_overview(&self) -> String {
        let eligible = self.eligible_skills();
        if eligible.is_empty() {
            return String::new();
        }

        let mut section = String::from("\n\n## Available Skills\n\n");
        section.push_str("You have the following specialized skills available. ");
        section.push_str("When a user's request matches a skill, follow its instructions.\n\n");

        for skill in &eligible {
            section.push_str(&format!(
                "- **{}**: {}\n",
                skill.meta.name, skill.meta.description
            ));
        }

        section
    }

    /// Build a detailed prompt section for matched skills.
    /// Injects the full skill content for skills that are relevant to the user message.
    /// Reloads skills-config.json to respect any toggle changes from the frontend.
    pub fn build_matched_prompt(&mut self, user_message: &str, top_k: usize) -> String {
        // Reload config to pick up any toggle changes from the frontend
        self.reload_config();

        let matched = self.match_for_message(user_message, top_k);
        if matched.is_empty() {
            return String::new();
        }

        let mut section = String::from("\n\n## Active Skill Instructions\n\n");
        section.push_str(
            "IMPORTANT: You MUST follow these skill-specific instructions for this request. ",
        );
        section.push_str("Do NOT use alternative approaches. Follow the steps below exactly:\n\n");

        for skill in &matched {
            section.push_str(&format!("### Skill: {}\n\n", skill.meta.name));
            section.push_str(&skill.content);
            section.push_str("\n\n");
        }

        section
    }

    /// Reload the skills-config.json (e.g., after a toggle from the frontend).
    pub fn reload_config(&mut self) {
        let config_path = dirs::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join(".pylot")
            .join("data")
            .join("skills-config.json");
        if config_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&config_path) {
                if let Ok(configs) =
                    serde_json::from_str::<HashMap<String, SkillEntryConfig>>(&content)
                {
                    self.entry_configs = configs;
                }
            }
        }
    }

    /// Number of loaded skills.
    pub fn len(&self) -> usize {
        self.skills.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
    }

    /// List all skill names.
    pub fn names(&self) -> Vec<&str> {
        self.skills.keys().map(|s| s.as_str()).collect()
    }
}

impl Default for SkillRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::types::{SkillMeta, SkillSource};
    use std::path::PathBuf;

    fn make_skill(name: &str, source: SkillSource, description: &str) -> Skill {
        Skill {
            meta: SkillMeta {
                name: name.to_string(),
                description: description.to_string(),
                version: "1.0.0".to_string(),
                author: None,
                category: Some("coding".to_string()),
                tags: vec![],
                os: vec![],
                requires: None,
                install: vec![],
                examples: vec![],
            },
            content: format!("Instructions for {}", name),
            source_path: PathBuf::new(),
            source,
        }
    }

    #[test]
    fn test_precedence_workspace_over_bundled() {
        let mut registry = SkillRegistry::new();
        registry.insert_skill(make_skill(
            "test-skill",
            SkillSource::Bundled,
            "bundled version",
        ));
        registry.insert_skill(make_skill(
            "test-skill",
            SkillSource::Workspace,
            "workspace version",
        ));

        let skill = registry.get("test-skill").unwrap();
        assert_eq!(skill.meta.description, "workspace version");
        assert_eq!(skill.source, SkillSource::Workspace);
    }

    #[test]
    fn test_precedence_local_over_bundled() {
        let mut registry = SkillRegistry::new();
        registry.insert_skill(make_skill("debug", SkillSource::Bundled, "bundled"));
        registry.insert_skill(make_skill("debug", SkillSource::Local, "local"));

        let skill = registry.get("debug").unwrap();
        assert_eq!(skill.meta.description, "local");
    }

    #[test]
    fn test_multiple_skills() {
        let mut registry = SkillRegistry::new();
        registry.insert_skill(make_skill("a", SkillSource::Bundled, "skill a"));
        registry.insert_skill(make_skill("b", SkillSource::Local, "skill b"));
        registry.insert_skill(make_skill("c", SkillSource::Workspace, "skill c"));

        assert_eq!(registry.len(), 3);
        assert!(registry.get("a").is_some());
        assert!(registry.get("b").is_some());
        assert!(registry.get("c").is_some());
    }

    #[test]
    fn test_skills_overview() {
        let mut registry = SkillRegistry::new();
        registry.insert_skill(make_skill(
            "code-review",
            SkillSource::Bundled,
            "Review code for quality",
        ));

        let overview = registry.build_skills_overview();
        assert!(overview.contains("code-review"));
        assert!(overview.contains("Review code for quality"));
    }

    #[test]
    fn test_empty_registry() {
        let registry = SkillRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.build_skills_overview(), "");
    }
}
