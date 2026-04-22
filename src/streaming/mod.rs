//! # Streaming Module
//!
//! Token-by-token streaming for LLM responses via SSE and WebSocket.
//! StreamEvent types, channel utilities, and SSE serialization.

pub mod types;

pub use types::{StreamEvent, StreamReceiver, StreamSender, stream_channel};

/// Serialize a StreamEvent to SSE format (text/event-stream).
pub fn event_to_sse(event: &StreamEvent) -> String {
    let json = serde_json::to_string(event).unwrap_or_default();
    let event_type = match event {
        StreamEvent::TextDelta { .. } => "text_delta",
        StreamEvent::ToolUseStart { .. } => "tool_use_start",
        StreamEvent::ToolInputDelta { .. } => "tool_input_delta",
        StreamEvent::ToolResult { .. } => "tool_result",
        StreamEvent::Thinking { .. } => "thinking",
        StreamEvent::Usage { .. } => "usage",
        StreamEvent::MessageStop => "message_stop",
        StreamEvent::Error { .. } => "error",
    };
    format!("event: {}\ndata: {}\n\n", event_type, json)
}
