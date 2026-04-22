use crate::learning::types::{LearnedRule, RuleSource};
use chrono::Utc;
use rusqlite::Connection;
use uuid::Uuid;

const MAX_PROMPT_CHARS: usize = 4000;
const CONFIDENCE_DECAY: f64 = 0.05;

/// Manages learned rules that evolve the system prompt over time.
pub struct PromptEvolution {
    db: Connection,
}

impl PromptEvolution {
    pub fn new(db_path: &str) -> Result<Self, String> {
        let db = Connection::open(db_path).map_err(|e| format!("Failed to open DB: {e}"))?;
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS learned_rules (
                id TEXT PRIMARY KEY,
                rule_text TEXT NOT NULL,
                source TEXT NOT NULL,
                confidence REAL NOT NULL DEFAULT 0.5,
                created_at TEXT NOT NULL,
                last_applied TEXT,
                success_count INTEGER NOT NULL DEFAULT 0,
                failure_count INTEGER NOT NULL DEFAULT 0
            );",
        )
        .map_err(|e| format!("Failed to create table: {e}"))?;
        Ok(Self { db })
    }

    /// Add a new learned rule.
    pub fn add_rule(&self, rule_text: &str, source: RuleSource) -> Result<String, String> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let source_str = serde_json::to_string(&source).unwrap_or_default();

        self.db
            .execute(
                "INSERT INTO learned_rules (id, rule_text, source, confidence, created_at)
                 VALUES (?1, ?2, ?3, 0.5, ?4)",
                rusqlite::params![id, rule_text, source_str, now],
            )
            .map_err(|e| format!("Failed to insert rule: {e}"))?;

        Ok(id)
    }

    /// Record success for a rule, boosting confidence.
    pub fn record_success(&self, rule_id: &str) -> Result<(), String> {
        let now = Utc::now().to_rfc3339();
        self.db
            .execute(
                "UPDATE learned_rules SET
                    success_count = success_count + 1,
                    confidence = MIN(1.0, confidence + 0.1),
                    last_applied = ?1
                 WHERE id = ?2",
                rusqlite::params![now, rule_id],
            )
            .map_err(|e| format!("Failed to update rule: {e}"))?;
        Ok(())
    }

    /// Record failure for a rule, decaying confidence.
    pub fn record_failure(&self, rule_id: &str) -> Result<(), String> {
        self.db
            .execute(
                &format!(
                    "UPDATE learned_rules SET
                        failure_count = failure_count + 1,
                        confidence = MAX(0.0, confidence - {CONFIDENCE_DECAY})
                     WHERE id = ?1"
                ),
                rusqlite::params![rule_id],
            )
            .map_err(|e| format!("Failed to update rule: {e}"))?;
        Ok(())
    }

    /// Get all active rules (confidence > 0.1) sorted by confidence desc.
    pub fn active_rules(&self) -> Result<Vec<LearnedRule>, String> {
        let mut stmt = self
            .db
            .prepare(
                "SELECT id, rule_text, source, confidence, created_at, last_applied,
                        success_count, failure_count
                 FROM learned_rules
                 WHERE confidence > 0.1
                 ORDER BY confidence DESC",
            )
            .map_err(|e| format!("Query failed: {e}"))?;

        let rules = stmt
            .query_map([], |row| {
                let source_str: String = row.get(2)?;
                let source: RuleSource =
                    serde_json::from_str(&source_str).unwrap_or(RuleSource::Manual);
                let created_str: String = row.get(4)?;
                let last_str: Option<String> = row.get(5)?;

                Ok(LearnedRule {
                    id: row.get(0)?,
                    rule_text: row.get(1)?,
                    source,
                    confidence: row.get(3)?,
                    created_at: chrono::DateTime::parse_from_rfc3339(&created_str)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                    last_applied: last_str.and_then(|s| {
                        chrono::DateTime::parse_from_rfc3339(&s)
                            .ok()
                            .map(|dt| dt.with_timezone(&Utc))
                    }),
                    success_count: row.get(6)?,
                    failure_count: row.get(7)?,
                })
            })
            .map_err(|e| format!("Failed to iterate rules: {e}"))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(rules)
    }

    /// Build the learned-rules section for the system prompt, capped at MAX_PROMPT_CHARS.
    pub fn build_prompt_section(&self) -> Result<String, String> {
        let rules = self.active_rules()?;
        if rules.is_empty() {
            return Ok(String::new());
        }

        let mut section = String::from("\n## Learned Behavioral Rules\n");
        for rule in &rules {
            let line = format!("- [confidence: {:.2}] {}\n", rule.confidence, rule.rule_text);
            if section.len() + line.len() > MAX_PROMPT_CHARS {
                break;
            }
            section.push_str(&line);
        }
        Ok(section)
    }

    /// Prune rules with zero confidence.
    pub fn prune_dead_rules(&self) -> Result<usize, String> {
        let count = self
            .db
            .execute("DELETE FROM learned_rules WHERE confidence <= 0.0", [])
            .map_err(|e| format!("Prune failed: {e}"))?;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_and_retrieve_rule() {
        let pe = PromptEvolution::new(":memory:").unwrap();
        let id = pe.add_rule("Always use markdown", RuleSource::Manual).unwrap();
        let rules = pe.active_rules().unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].id, id);
        assert_eq!(rules[0].confidence, 0.5);
    }

    #[test]
    fn test_confidence_boost_and_decay() {
        let pe = PromptEvolution::new(":memory:").unwrap();
        let id = pe.add_rule("Be concise", RuleSource::UserFeedback).unwrap();
        pe.record_success(&id).unwrap();
        let rules = pe.active_rules().unwrap();
        assert!((rules[0].confidence - 0.6).abs() < 0.01);

        pe.record_failure(&id).unwrap();
        let rules = pe.active_rules().unwrap();
        assert!((rules[0].confidence - 0.55).abs() < 0.01);
    }

    #[test]
    fn test_prompt_section() {
        let pe = PromptEvolution::new(":memory:").unwrap();
        pe.add_rule("Use bullet points", RuleSource::ConversationInsight).unwrap();
        let section = pe.build_prompt_section().unwrap();
        assert!(section.contains("Use bullet points"));
        assert!(section.contains("Learned Behavioral Rules"));
    }
}
