use crate::learning::auto_scorer::ScoreResult;
use crate::llm::{LlmProvider, LlmResponse, Message};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Minimum failures before considering skill evolution.
const MIN_FAILURES_FOR_EVOLUTION: usize = 3;
/// Success rate threshold below which evolution triggers.
const EVOLUTION_THRESHOLD: f64 = 0.4;
/// Maximum number of skills generated per evolution step.
const MAX_NEW_SKILLS: usize = 3;

/// A recorded conversation outcome for tracking failure patterns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationOutcome {
    pub user_instruction: String,
    pub assistant_response: String,
    pub score: i8,
    pub timestamp: DateTime<Utc>,
}

/// A generated skill ready to be written as a SKILL.md file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedSkill {
    pub name: String,
    pub description: String,
    pub category: String,
    pub tags: Vec<String>,
    pub content: String,
}

/// Tracks conversation outcomes and auto-generates SKILL.md files when
/// the agent's success rate drops below a threshold.
///
/// Inspired by MetaClaw's SkillEvolver — analyses failed conversations
/// via LLM and produces concrete SKILL.md files that get loaded by the
/// existing `SkillRegistry` on the next scan.
pub struct SkillEvolver {
    /// Rolling window of recent conversation outcomes.
    outcomes: Vec<ConversationOutcome>,
    /// Maximum outcomes to keep in memory.
    window_size: usize,
    /// Directory where auto-generated skills are written.
    output_dir: PathBuf,
    /// Names of skills already generated (avoid duplicates).
    generated_names: Vec<String>,
}

impl SkillEvolver {
    pub fn new(output_dir: &Path) -> Self {
        Self {
            outcomes: Vec::new(),
            window_size: 50,
            output_dir: output_dir.to_path_buf(),
            generated_names: Vec::new(),
        }
    }

    /// Record a conversation outcome from an auto-score result.
    pub fn record_outcome(
        &mut self,
        user_instruction: &str,
        assistant_response: &str,
        score_result: &ScoreResult,
    ) {
        self.outcomes.push(ConversationOutcome {
            user_instruction: truncate_string(user_instruction, 1000),
            assistant_response: truncate_string(assistant_response, 1500),
            score: score_result.score,
            timestamp: Utc::now(),
        });

        // Keep window bounded
        if self.outcomes.len() > self.window_size {
            self.outcomes.drain(0..self.outcomes.len() - self.window_size);
        }
    }

    /// Check whether the recent failure rate warrants skill evolution.
    pub fn should_evolve(&self) -> bool {
        if self.outcomes.len() < MIN_FAILURES_FOR_EVOLUTION {
            return false;
        }
        let successes = self.outcomes.iter().filter(|o| o.score > 0).count();
        let rate = successes as f64 / self.outcomes.len() as f64;
        rate < EVOLUTION_THRESHOLD
    }

    /// Get recent failures for analysis.
    fn recent_failures(&self) -> Vec<&ConversationOutcome> {
        self.outcomes
            .iter()
            .filter(|o| o.score < 0)
            .rev()
            .take(6)
            .collect()
    }

    /// Current success rate across the outcome window.
    pub fn success_rate(&self) -> f64 {
        if self.outcomes.is_empty() {
            return 1.0;
        }
        let successes = self.outcomes.iter().filter(|o| o.score > 0).count();
        successes as f64 / self.outcomes.len() as f64
    }

    /// Number of tracked outcomes.
    pub fn outcome_count(&self) -> usize {
        self.outcomes.len()
    }

    /// Names of skills generated so far.
    pub fn generated_skill_names(&self) -> &[String] {
        &self.generated_names
    }

    /// Run the evolution step: analyse failures via LLM, generate SKILL.md files,
    /// and write them to the output directory.
    ///
    /// Returns the list of generated skills (empty if evolution isn't needed or fails).
    pub async fn evolve(
        &mut self,
        llm: &dyn LlmProvider,
        existing_skill_names: &[String],
    ) -> Result<Vec<GeneratedSkill>, String> {
        if !self.should_evolve() {
            return Ok(Vec::new());
        }

        let failures = self.recent_failures();
        if failures.is_empty() {
            return Ok(Vec::new());
        }

        // Build analysis prompt
        let prompt = self.build_analysis_prompt(&failures, existing_skill_names);

        let messages = vec![Message::user(prompt)];

        let response_text = match llm.chat(&messages, &[]).await {
            Ok(LlmResponse::Text(text)) => text,
            Ok(_) => return Ok(Vec::new()),
            Err(e) => return Err(format!("LLM call failed during skill evolution: {e}")),
        };

        // Parse generated skills from JSON response
        let skills = self.parse_skills_response(&response_text);

        if skills.is_empty() {
            return Ok(Vec::new());
        }

        // Write SKILL.md files
        std::fs::create_dir_all(&self.output_dir)
            .map_err(|e| format!("Failed to create skills dir: {e}"))?;

        let mut written = Vec::new();
        for skill in skills.into_iter().take(MAX_NEW_SKILLS) {
            // Skip duplicates
            if existing_skill_names.contains(&skill.name)
                || self.generated_names.contains(&skill.name)
            {
                continue;
            }

            let skill_dir = self.output_dir.join(&skill.name);
            std::fs::create_dir_all(&skill_dir)
                .map_err(|e| format!("Failed to create skill dir: {e}"))?;

            let skill_md = self.render_skill_md(&skill);
            let skill_path = skill_dir.join("SKILL.md");
            std::fs::write(&skill_path, skill_md)
                .map_err(|e| format!("Failed to write SKILL.md: {e}"))?;

            tracing::info!(
                "Auto-generated skill '{}' at {}",
                skill.name,
                skill_path.display()
            );

            self.generated_names.push(skill.name.clone());
            written.push(skill);
        }

        // Clear failures that were addressed
        if !written.is_empty() {
            self.outcomes.retain(|o| o.score >= 0);
        }

        Ok(written)
    }

    /// Build the LLM prompt for analysing failures and generating skills.
    fn build_analysis_prompt(
        &self,
        failures: &[&ConversationOutcome],
        existing_names: &[String],
    ) -> String {
        let mut failure_blocks = String::new();
        for (i, f) in failures.iter().enumerate() {
            failure_blocks.push_str(&format!(
                "### Failure {}\n**User instruction:**\n{}\n\n**Assistant response (excerpt):**\n{}\n\n",
                i + 1,
                f.user_instruction,
                f.assistant_response,
            ));
        }

        format!(
            r#"You are a skill engineer for an AI assistant. Analyse the failed conversations below and generate NEW skills that would prevent these failures.

## Failed Conversations

{failure_blocks}
## Existing Skills (do NOT duplicate)

{existing_names:?}

## Instructions

Generate 1 to {MAX_NEW_SKILLS} new skills. Each skill must be a JSON object with:
- "name": lowercase-hyphenated slug (e.g. "verify-file-paths")
- "description": one sentence — when to trigger this skill
- "category": e.g. "coding", "research", "communication", "automation", "general"
- "tags": array of keyword strings for matching
- "content": 6-15 lines of actionable Markdown with numbered steps and an Anti-pattern section

Return ONLY a valid JSON array. No markdown fences, no prose outside the JSON."#
        )
    }

    /// Parse the LLM response into GeneratedSkill objects.
    fn parse_skills_response(&self, response: &str) -> Vec<GeneratedSkill> {
        // Strip markdown code fences if present
        let clean = response
            .replace("```json", "")
            .replace("```", "");

        let arr_start = clean.find('[');
        let arr_end = clean.rfind(']');

        let json_str = match (arr_start, arr_end) {
            (Some(s), Some(e)) if e > s => &clean[s..=e],
            _ => return Vec::new(),
        };

        let parsed: Vec<serde_json::Value> = match serde_json::from_str(json_str) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("Failed to parse skill evolution response: {e}");
                return Vec::new();
            }
        };

        parsed
            .into_iter()
            .filter_map(|v| {
                let name = v.get("name")?.as_str()?.to_string();
                let description = v.get("description")?.as_str()?.to_string();
                let content = v.get("content")?.as_str()?.to_string();
                let category = v
                    .get("category")
                    .and_then(|c| c.as_str())
                    .unwrap_or("general")
                    .to_string();
                let tags = v
                    .get("tags")
                    .and_then(|t| t.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|t| t.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();

                // Validate name is a proper slug
                if name.is_empty() || name.contains(' ') || description.is_empty() {
                    return None;
                }

                Some(GeneratedSkill {
                    name,
                    description,
                    category,
                    tags,
                    content,
                })
            })
            .collect()
    }

    /// Render a GeneratedSkill as a SKILL.md file with YAML frontmatter.
    fn render_skill_md(&self, skill: &GeneratedSkill) -> String {
        let tags_yaml = if skill.tags.is_empty() {
            "[]".to_string()
        } else {
            let inner = skill
                .tags
                .iter()
                .map(|t| format!("\"{}\"", t.replace('"', "")))
                .collect::<Vec<_>>()
                .join(", ");
            format!("[{inner}]")
        };

        let desc_escaped = skill.description.replace('"', "'");
        format!(
            "---\nname: {name}\ndescription: \"{desc}\"\nversion: \"1.0.0\"\nauthor: \"auto-evolved\"\ncategory: \"{cat}\"\ntags: {tags}\n---\n\n{content}\n",
            name = skill.name,
            desc = desc_escaped,
            cat = skill.category,
            tags = tags_yaml,
            content = skill.content,
        )
    }
}

fn truncate_string(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        let mut end = max;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        s[..end].to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_evolve_below_threshold() {
        let mut evolver = SkillEvolver::new(Path::new("/tmp/test-skills"));
        // Add 5 failures
        for _ in 0..5 {
            evolver.outcomes.push(ConversationOutcome {
                user_instruction: "do something".into(),
                assistant_response: "bad answer".into(),
                score: -1,
                timestamp: Utc::now(),
            });
        }
        assert!(evolver.should_evolve());
        assert!(evolver.success_rate() < 0.01);
    }

    #[test]
    fn test_should_not_evolve_above_threshold() {
        let mut evolver = SkillEvolver::new(Path::new("/tmp/test-skills"));
        // Add mostly successes
        for _ in 0..8 {
            evolver.outcomes.push(ConversationOutcome {
                user_instruction: "do something".into(),
                assistant_response: "good answer".into(),
                score: 1,
                timestamp: Utc::now(),
            });
        }
        for _ in 0..2 {
            evolver.outcomes.push(ConversationOutcome {
                user_instruction: "do something".into(),
                assistant_response: "bad".into(),
                score: -1,
                timestamp: Utc::now(),
            });
        }
        assert!(!evolver.should_evolve());
    }

    #[test]
    fn test_parse_skills_response() {
        let evolver = SkillEvolver::new(Path::new("/tmp/test-skills"));
        let response = "[\n{\"name\": \"verify-file-paths\", \"description\": \"Always verify file paths exist before reading\", \"category\": \"coding\", \"tags\": [\"files\", \"validation\"], \"content\": \"Check path exists before reading.\"}\n]";
        let skills = evolver.parse_skills_response(response);
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "verify-file-paths");
        assert_eq!(skills[0].tags, vec!["files", "validation"]);
    }

    #[test]
    fn test_parse_skills_response_with_fences() {
        let evolver = SkillEvolver::new(Path::new("/tmp/test-skills"));
        let response = "```json\n[{\"name\": \"handle-errors\", \"description\": \"Better error handling\", \"category\": \"coding\", \"tags\": [], \"content\": \"## Handle Errors\"}]\n```";
        let skills = evolver.parse_skills_response(response);
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "handle-errors");
    }

    #[test]
    fn test_render_skill_md() {
        let evolver = SkillEvolver::new(Path::new("/tmp/test-skills"));
        let skill = GeneratedSkill {
            name: "verify-paths".into(),
            description: "Check file paths before access".into(),
            category: "coding".into(),
            tags: vec!["files".into(), "safety".into()],
            content: "## Verify Paths\n\n1. Use exists() check\n2. Handle errors\n\n**Anti-pattern:** Opening without checking.".into(),
        };
        let md = evolver.render_skill_md(&skill);
        assert!(md.starts_with("---\n"));
        assert!(md.contains("name: verify-paths"));
        assert!(md.contains("author: \"auto-evolved\""));
        assert!(md.contains("## Verify Paths"));
    }

    #[test]
    fn test_window_bounded() {
        let mut evolver = SkillEvolver::new(Path::new("/tmp/test-skills"));
        evolver.window_size = 5;
        for i in 0..10 {
            evolver.record_outcome(
                &format!("instruction {i}"),
                "response",
                &ScoreResult { score: -1, votes: vec![-1], explanation: None },
            );
        }
        assert_eq!(evolver.outcome_count(), 5);
    }
}
