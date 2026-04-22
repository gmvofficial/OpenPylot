use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The 6 memory types supported by the advanced memory system.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum MemoryType {
    /// Specific events/interactions from sessions
    Episodic,
    /// General facts, domain knowledge, project info
    Semantic,
    /// User preferences, style, requirements
    Preference,
    /// Current project goals, decisions, blockers
    ProjectState,
    /// Rolling compressed context summary
    WorkingSummary,
    /// Learned patterns, successful workflows
    ProceduralObservation,
}

impl MemoryType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Episodic => "episodic",
            Self::Semantic => "semantic",
            Self::Preference => "preference",
            Self::ProjectState => "project_state",
            Self::WorkingSummary => "working_summary",
            Self::ProceduralObservation => "procedural_observation",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "episodic" => Some(Self::Episodic),
            "semantic" => Some(Self::Semantic),
            "preference" => Some(Self::Preference),
            "project_state" => Some(Self::ProjectState),
            "working_summary" => Some(Self::WorkingSummary),
            "procedural_observation" => Some(Self::ProceduralObservation),
            _ => None,
        }
    }
}

impl std::fmt::Display for MemoryType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A single memory unit stored in the database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryUnit {
    pub id: String,
    pub memory_type: MemoryType,
    pub content: String,
    pub summary: Option<String>,
    pub user_id: String,
    pub source_session: Option<String>,
    pub source_turn: Option<i64>,
    pub entities: Vec<String>,
    pub topics: Vec<String>,
    pub tags: Vec<String>,
    pub importance: f64,
    pub confidence: f64,
    pub access_count: i64,
    pub last_accessed: Option<String>,
    pub supersedes: Vec<String>,
    pub embedding: Option<Vec<f32>>,
    pub created_at: String,
    pub updated_at: String,
}

impl MemoryUnit {
    pub fn new(memory_type: MemoryType, content: String, user_id: String) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            id: Uuid::new_v4().to_string(),
            memory_type,
            content,
            summary: None,
            user_id,
            source_session: None,
            source_turn: None,
            entities: vec![],
            topics: vec![],
            tags: vec![],
            importance: 0.5,
            confidence: 0.5,
            access_count: 0,
            last_accessed: None,
            supersedes: vec![],
            embedding: None,
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

/// A search result with scoring metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySearchResult {
    pub unit: MemoryUnit,
    pub score: f64,
    pub match_source: MatchSource,
}

/// Which retrieval method produced the match.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MatchSource {
    Keyword,
    Embedding,
    Hybrid,
}

/// Retrieval strategy selection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RetrievalMode {
    Keyword,
    Embedding,
    Hybrid,
    Auto,
}

impl RetrievalMode {
    pub fn from_str(s: &str) -> Self {
        match s {
            "keyword" => Self::Keyword,
            "embedding" => Self::Embedding,
            "hybrid" => Self::Hybrid,
            _ => Self::Auto,
        }
    }
}

/// Report from a consolidation run.
#[derive(Debug, Default, Serialize)]
pub struct ConsolidationReport {
    pub exact_dupes_removed: usize,
    pub near_dupes_merged: usize,
    pub decayed_count: usize,
    pub stale_summaries_pruned: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_type_roundtrip() {
        for mt in [
            MemoryType::Episodic, MemoryType::Semantic, MemoryType::Preference,
            MemoryType::ProjectState, MemoryType::WorkingSummary, MemoryType::ProceduralObservation,
        ] {
            let s = mt.as_str();
            assert_eq!(MemoryType::from_str(s), Some(mt));
        }
    }

    #[test]
    fn test_memory_unit_new() {
        let unit = MemoryUnit::new(MemoryType::Semantic, "Rust is great".into(), "user1".into());
        assert_eq!(unit.memory_type, MemoryType::Semantic);
        assert_eq!(unit.importance, 0.5);
        assert!(!unit.id.is_empty());
    }
}
