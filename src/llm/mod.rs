pub mod openai;
pub mod anthropic;
pub mod fallback;
pub mod lazy;

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::streaming::StreamSender;
use crate::tools::ToolDefinition;

// ── Message types ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Role::System => write!(f, "system"),
            Role::User => write!(f, "user"),
            Role::Assistant => write!(f, "assistant"),
            Role::Tool => write!(f, "tool"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Message {
    pub role: Role,
    pub content: String,
    /// For role=Tool – the id of the tool call this result corresponds to.
    pub tool_call_id: Option<String>,
    /// For role=Assistant – tool calls the model wants to make.
    pub tool_calls: Option<Vec<ToolCall>>,
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: content.into(),
            tool_call_id: None,
            tool_calls: None,
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
            tool_call_id: None,
            tool_calls: None,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: content.into(),
            tool_call_id: None,
            tool_calls: None,
        }
    }

    pub fn assistant_tool_calls(calls: Vec<ToolCall>) -> Self {
        Self {
            role: Role::Assistant,
            content: String::new(),
            tool_call_id: None,
            tool_calls: Some(calls),
        }
    }

    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: content.into(),
            tool_call_id: Some(tool_call_id.into()),
            tool_calls: None,
        }
    }
}

// ── Tool call (from LLM response) ───────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

// ── LLM response enum ───────────────────────────────────────────────

#[derive(Debug)]
pub enum LlmResponse {
    /// The model produced a text response.
    Text(String),
    /// The model wants to call one or more tools.
    ToolCalls(Vec<ToolCall>),
    /// The model produced a text response with thinking/reasoning content.
    TextWithThinking {
        text: String,
        thinking: String,
    },
}

// ── LLM provider trait ──────────────────────────────────────────────

#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Send messages (with optional tool definitions) and get a response.
    async fn chat(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
    ) -> Result<LlmResponse>;

    /// Send messages with streaming — emits StreamEvents to the sender.
    /// Returns the final assembled response.
    async fn chat_stream(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        stream_tx: StreamSender,
    ) -> Result<LlmResponse> {
        // Default: fall back to non-streaming
        let _ = stream_tx;
        self.chat(messages, tools).await
    }

    /// Whether this provider supports streaming.
    fn supports_streaming(&self) -> bool {
        false
    }

    /// Provider name (for display).
    #[allow(dead_code)]
    fn name(&self) -> &str;

    /// Model name (for display).
    #[allow(dead_code)]
    fn model(&self) -> &str;
}
