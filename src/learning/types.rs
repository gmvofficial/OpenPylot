use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A learned behavioral rule appended to the system prompt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearnedRule {
    pub id: String,
    pub rule_text: String,
    pub source: RuleSource,
    pub confidence: f64,
    pub created_at: DateTime<Utc>,
    pub last_applied: Option<DateTime<Utc>>,
    pub success_count: u32,
    pub failure_count: u32,
}

/// Where a learned rule originated.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleSource {
    ErrorDiagnosis,
    UserFeedback,
    ConversationInsight,
    SkillExtraction,
    Manual,
}

/// Error category for diagnosis.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCategory {
    ToolFailure,
    PromptMismatch,
    CodeError,
    ContextOverflow,
    ConfigError,
    ExternalService,
}

/// User feedback on a response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserFeedback {
    pub session_id: String,
    pub turn_id: String,
    pub rating: i8, // -1, 0, 1
    pub comment: Option<String>,
    pub timestamp: DateTime<Utc>,
}

/// A conversation insight extracted from completed threads.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationInsight {
    pub id: String,
    pub insight_type: InsightType,
    pub description: String,
    pub confidence: f64,
    pub extracted_from_sessions: Vec<String>,
    pub created_at: DateTime<Utc>,
}

/// Type of conversation insight.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InsightType {
    UserPreference,
    WorkflowPattern,
    ToolUsagePattern,
    CommunicationStyle,
}
