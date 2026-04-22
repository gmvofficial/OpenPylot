use serde::{Deserialize, Serialize};

/// Events emitted during a streaming LLM response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum StreamEvent {
    /// A chunk of text from the LLM response.
    #[serde(rename = "text_delta")]
    TextDelta { text: String },

    /// The LLM is starting a tool call.
    #[serde(rename = "tool_use_start")]
    ToolUseStart { id: String, name: String },

    /// Incremental JSON input for a tool call.
    #[serde(rename = "tool_input_delta")]
    ToolInputDelta { id: String, delta: String },

    /// Result of a tool execution.
    #[serde(rename = "tool_result")]
    ToolResult {
        id: String,
        name: String,
        success: bool,
        output: String,
    },

    /// LLM thinking/reasoning content (for models that support it).
    #[serde(rename = "thinking")]
    Thinking { text: String },

    /// Token usage statistics.
    #[serde(rename = "usage")]
    Usage {
        input_tokens: u32,
        output_tokens: u32,
    },

    /// The LLM response is complete.
    #[serde(rename = "message_stop")]
    MessageStop,

    /// An error occurred during streaming.
    #[serde(rename = "error")]
    Error { message: String },
}

/// A sender handle for emitting stream events.
pub type StreamSender = tokio::sync::mpsc::UnboundedSender<StreamEvent>;

/// A receiver handle for consuming stream events.
pub type StreamReceiver = tokio::sync::mpsc::UnboundedReceiver<StreamEvent>;

/// Create a new stream event channel.
pub fn stream_channel() -> (StreamSender, StreamReceiver) {
    tokio::sync::mpsc::unbounded_channel()
}
