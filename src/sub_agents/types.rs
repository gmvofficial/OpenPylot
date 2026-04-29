use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration for spawning a sub-agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubAgentConfig {
    pub id: String,
    pub name: String,
    pub agent_type: SubAgentType,
    pub system_prompt: String,
    /// Override the default LLM model for this agent.
    pub model_override: Option<String>,
    /// Restrict which tools this agent can use.
    pub allowed_tools: Option<Vec<String>>,
    /// Maximum execution time in seconds.
    pub timeout_secs: u64,
    /// Maximum number of tool call iterations.
    pub max_iterations: usize,
    /// Parent agent id (for nesting).
    pub parent_id: Option<String>,
    /// If `Some(n)`, this agent runs recurrently every `n` seconds (background updates).
    /// If `None`, the agent runs exactly once. Recurrent agents only stop on
    /// explicit cancel (via `AgentOrchestrator::cancel` / `cancel_by_name`).
    #[serde(default)]
    pub interval_secs: Option<u64>,
}

impl Default for SubAgentConfig {
    fn default() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: "sub-agent".to_string(),
            agent_type: SubAgentType::Task,
            system_prompt: String::new(),
            model_override: None,
            allowed_tools: None,
            timeout_secs: 300,
            max_iterations: 10,
            parent_id: None,
            interval_secs: None,
        }
    }
}

/// Type of sub-agent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SubAgentType {
    /// One-shot task execution.
    Task,
    /// Long-running background job.
    Background,
    /// Specialized domain expert.
    Specialist,
}

/// Current status of a sub-agent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SubAgentStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
    TimedOut,
}

/// Runtime state of a sub-agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubAgentState {
    pub config: SubAgentConfig,
    pub status: SubAgentStatus,
    pub result: Option<String>,
    pub error: Option<String>,
    pub messages: Vec<SubAgentMessage>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

/// A message in the sub-agent's conversation history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubAgentMessage {
    pub role: String,
    pub content: String,
    pub timestamp: String,
}

impl SubAgentState {
    pub fn new(config: SubAgentConfig) -> Self {
        Self {
            config,
            status: SubAgentStatus::Pending,
            result: None,
            error: None,
            messages: Vec::new(),
            started_at: None,
            completed_at: None,
        }
    }
}
