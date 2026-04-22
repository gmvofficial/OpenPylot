use std::collections::HashMap;

use super::types::Skill;

/// Selects relevant skills for a given user message using keyword matching.
///
/// Inspired by MetaClaw's category→skill matching and OpenClaw's eligibility gating.
/// Does NOT use LLM calls — purely deterministic keyword + tag scoring.
pub struct SkillMatcher;

impl SkillMatcher {
    /// Select the top-k most relevant skills for the given user message.
    pub fn match_skills<'a>(skills: &'a [Skill], user_message: &str, top_k: usize) -> Vec<&'a Skill> {
        let lower = user_message.to_lowercase();

        let mut scored: Vec<(&Skill, f64)> = skills
            .iter()
            .map(|s| (s, Self::score_skill(s, &lower)))
            .filter(|(_, score)| *score > 0.0)
            .collect();

        // Sort descending by score
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(top_k);
        scored.into_iter().map(|(s, _)| s).collect()
    }

    /// Score a skill against a lowercased user message.
    /// Higher = more relevant.
    fn score_skill(skill: &Skill, message_lower: &str) -> f64 {
        let mut score = 0.0;

        // 1. Category match (strong signal)
        if let Some(ref cat) = skill.meta.category {
            if let Some(cat_keywords) = Self::category_keywords().get(cat.as_str()) {
                for kw in cat_keywords {
                    if message_lower.contains(kw) {
                        score += 3.0;
                    }
                }
            }
        }

        // 2. Tag match (medium signal)
        for tag in &skill.meta.tags {
            if message_lower.contains(&tag.to_lowercase()) {
                score += 2.0;
            }
        }

        // 3. Name match (strong signal)
        let name_lower = skill.meta.name.to_lowercase();
        // Match hyphenated name parts
        for part in name_lower.split('-') {
            if part.len() >= 3 && message_lower.contains(part) {
                score += 2.5;
            }
        }

        // 4. Description keyword overlap (weak signal)
        let desc_lower = skill.meta.description.to_lowercase();
        let desc_words: Vec<&str> = desc_lower.split_whitespace().collect();
        let msg_words: Vec<&str> = message_lower.split_whitespace().collect();
        let overlap = msg_words
            .iter()
            .filter(|w| w.len() >= 4 && desc_words.contains(w))
            .count();
        score += overlap as f64 * 0.5;

        score
    }

    /// Category → keyword mapping for task detection.
    /// Inspired by MetaClaw's task_type_keywords.
    fn category_keywords() -> HashMap<&'static str, Vec<&'static str>> {
        HashMap::from([
            (
                "coding",
                vec![
                    "code", "debug", "implement", "function", "bug", "error", "fix",
                    "refactor", "compile", "build", "test", "review", "lint", "type",
                    "class", "module", "api", "endpoint", "database", "sql", "rust",
                    "python", "javascript", "typescript",
                ],
            ),
            (
                "research",
                vec![
                    "research", "paper", "find", "search", "look up", "investigate",
                    "compare", "analyze", "study", "report", "summary",
                ],
            ),
            (
                "productivity",
                vec![
                    "plan", "schedule", "organize", "task", "todo", "calendar",
                    "meeting", "deadline", "prioritize", "goal", "project",
                ],
            ),
            (
                "communication",
                vec![
                    "email", "draft", "write", "reply", "message", "social",
                    "post", "blog", "newsletter", "announcement",
                ],
            ),
            (
                "agentic",
                vec![
                    "delegate", "multi-step", "complex", "break down", "parallel",
                    "background", "spawn", "agent", "orchestrate",
                ],
            ),
        ])
    }
}

// ── OS eligibility check ─────────────────────────────────────────────

/// Check whether a skill is eligible to run on the current platform.
pub fn is_skill_eligible(skill: &Skill) -> bool {
    // 1. Check OS filter
    if !skill.meta.os.is_empty() {
        let current_os = if cfg!(target_os = "macos") {
            "macos"
        } else if cfg!(target_os = "linux") {
            "linux"
        } else if cfg!(target_os = "windows") {
            "windows"
        } else {
            "unknown"
        };
        // Also accept "darwin" as alias for "macos"
        let os_matches = skill.meta.os.iter().any(|os| {
            let os_lower = os.to_lowercase();
            os_lower == current_os || (os_lower == "darwin" && current_os == "macos")
        });
        if !os_matches {
            return false;
        }
    }

    // 2. Check required binaries
    if let Some(ref reqs) = skill.meta.requires {
        for bin in &reqs.bins {
            if which::which(bin).is_err() {
                tracing::debug!("Skill '{}' skipped: missing binary '{}'", skill.meta.name, bin);
                return false;
            }
        }

        // 3. Check required env vars
        for env_var in &reqs.env {
            if std::env::var(env_var).is_err() {
                tracing::debug!("Skill '{}' skipped: env var '{}' not set", skill.meta.name, env_var);
                return false;
            }
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::types::{SkillMeta, SkillSource};
    use std::path::PathBuf;

    fn make_skill(name: &str, category: &str, tags: &[&str], description: &str) -> Skill {
        Skill {
            meta: SkillMeta {
                name: name.to_string(),
                description: description.to_string(),
                version: "1.0.0".to_string(),
                author: None,
                category: Some(category.to_string()),
                tags: tags.iter().map(|s| s.to_string()).collect(),
                os: vec![],
                requires: None,
                install: vec![],
                examples: vec![],
            },
            content: String::new(),
            source_path: PathBuf::new(),
            source: SkillSource::Bundled,
        }
    }

    #[test]
    fn test_match_coding_skill() {
        let skills = vec![
            make_skill("debug-systematically", "coding", &["debug", "error"], "Diagnose bugs"),
            make_skill("email-drafting", "communication", &["email"], "Write emails"),
        ];
        let matched = SkillMatcher::match_skills(&skills, "I have a bug to debug", 3);
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].meta.name, "debug-systematically");
    }

    #[test]
    fn test_match_multiple_skills() {
        let skills = vec![
            make_skill("code-review", "coding", &["review", "code"], "Review code"),
            make_skill("debug-systematically", "coding", &["debug", "error"], "Fix bugs"),
            make_skill("task-planning", "productivity", &["plan", "task"], "Plan tasks"),
        ];
        let matched = SkillMatcher::match_skills(&skills, "review this code and plan next steps", 3);
        assert!(matched.len() >= 2);
    }

    #[test]
    fn test_no_match() {
        let skills = vec![
            make_skill("debug-systematically", "coding", &["debug"], "Fix bugs"),
        ];
        let matched = SkillMatcher::match_skills(&skills, "what is the weather today", 3);
        assert!(matched.is_empty());
    }

    #[test]
    fn test_top_k_limit() {
        let skills = vec![
            make_skill("s1", "coding", &["code"], "code stuff"),
            make_skill("s2", "coding", &["code"], "code more"),
            make_skill("s3", "coding", &["code"], "code again"),
        ];
        let matched = SkillMatcher::match_skills(&skills, "write some code", 1);
        assert_eq!(matched.len(), 1);
    }
}
