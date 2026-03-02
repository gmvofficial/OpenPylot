use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Simple persistent memory — stores conversation summaries and user preferences.
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct MemoryStore {
    /// Key facts / preferences the agent has learned about the user.
    pub facts: Vec<MemoryFact>,
    /// Conversation summaries for long-term recall.
    pub summaries: Vec<ConversationSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryFact {
    pub key: String,
    pub value: String,
    pub learned_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationSummary {
    pub summary: String,
    pub timestamp: DateTime<Utc>,
}

impl MemoryStore {
    fn file_path(data_dir: &PathBuf) -> PathBuf {
        data_dir.join("memory.json")
    }

    pub fn load(data_dir: &PathBuf) -> Result<Self> {
        let path = Self::file_path(data_dir);
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read memory from {}", path.display()))?;
        let store: MemoryStore =
            serde_json::from_str(&content).with_context(|| "Failed to parse memory file")?;
        Ok(store)
    }

    pub fn save(&self, data_dir: &PathBuf) -> Result<()> {
        let path = Self::file_path(data_dir);
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, content)
            .with_context(|| format!("Failed to write memory to {}", path.display()))?;
        Ok(())
    }

    /// Add or update a fact.
    #[allow(dead_code)]
    pub fn set_fact(&mut self, key: impl Into<String>, value: impl Into<String>) {
        let key = key.into();
        let value = value.into();

        // Update existing or push new
        if let Some(fact) = self.facts.iter_mut().find(|f| f.key == key) {
            fact.value = value;
            fact.learned_at = Utc::now();
        } else {
            self.facts.push(MemoryFact {
                key,
                value,
                learned_at: Utc::now(),
            });
        }
    }

    /// Get a fact by key.
    #[allow(dead_code)]
    pub fn get_fact(&self, key: &str) -> Option<&str> {
        self.facts
            .iter()
            .find(|f| f.key == key)
            .map(|f| f.value.as_str())
    }

    /// Add a conversation summary.
    #[allow(dead_code)]
    pub fn add_summary(&mut self, summary: impl Into<String>) {
        self.summaries.push(ConversationSummary {
            summary: summary.into(),
            timestamp: Utc::now(),
        });

        // Keep only the last 100 summaries
        if self.summaries.len() > 100 {
            self.summaries.drain(..self.summaries.len() - 100);
        }
    }

    /// Build a context string from memory for inclusion in the system prompt.
    pub fn context_string(&self) -> String {
        if self.facts.is_empty() && self.summaries.is_empty() {
            return String::new();
        }

        let mut ctx = String::from("\n\n--- User Memory ---\n");

        if !self.facts.is_empty() {
            ctx.push_str("Known facts about the user:\n");
            for fact in &self.facts {
                ctx.push_str(&format!("- {}: {}\n", fact.key, fact.value));
            }
        }

        if !self.summaries.is_empty() {
            ctx.push_str("\nRecent conversation summaries:\n");
            for summary in self.summaries.iter().rev().take(5) {
                ctx.push_str(&format!(
                    "- [{}] {}\n",
                    summary.timestamp.format("%Y-%m-%d"),
                    summary.summary
                ));
            }
        }

        ctx
    }

    /// Return a reference to all stored facts.
    pub fn all_facts(&self) -> &[MemoryFact] {
        &self.facts
    }

    /// Update a fact at the given index. Returns true if successful.
    pub fn update_fact_at(&mut self, index: usize, new_value: &str) -> bool {
        if let Some(fact) = self.facts.get_mut(index) {
            fact.value = new_value.to_string();
            fact.learned_at = Utc::now();
            true
        } else {
            false
        }
    }

    /// Remove a fact at the given index. Returns true if successful.
    pub fn remove_fact_at(&mut self, index: usize) -> bool {
        if index < self.facts.len() {
            self.facts.remove(index);
            true
        } else {
            false
        }
    }
}
