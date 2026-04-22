use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::llm::{LlmProvider, LlmResponse, Message, Role, ToolCall};
use crate::streaming::{StreamEvent, StreamSender};
use crate::tools::ToolDefinition;

// ── Anthropic API types ──────────────────────────────────────────────

#[derive(Serialize, Debug)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<AnthropicTool>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    stream: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct AnthropicMessage {
    role: String,
    content: AnthropicContent,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(untagged)]
enum AnthropicContent {
    Text(String),
    Blocks(Vec<AnthropicContentBlock>),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
enum AnthropicContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
    },
}

#[derive(Serialize, Debug)]
struct AnthropicTool {
    name: String,
    description: String,
    input_schema: Value,
}

#[derive(Deserialize, Debug)]
struct AnthropicResponse {
    content: Vec<AnthropicContentBlock>,
    #[allow(dead_code)]
    stop_reason: Option<String>,
}

#[derive(Deserialize, Debug)]
struct AnthropicErrorResponse {
    error: Option<AnthropicError>,
}

#[derive(Deserialize, Debug)]
struct AnthropicError {
    message: String,
}

// ── Provider implementation ──────────────────────────────────────────

pub struct AnthropicProvider {
    client: Client,
    api_key: String,
    model: String,
    max_tokens: u32,
}

impl AnthropicProvider {
    pub fn new(api_key: String, model: String, max_tokens: u32) -> Self {
        Self {
            client: Client::new(),
            api_key,
            model,
            max_tokens,
        }
    }

    /// Convert internal messages → Anthropic format.
    /// Returns (system_prompt, messages).
    fn convert_messages(messages: &[Message]) -> (Option<String>, Vec<AnthropicMessage>) {
        let mut system: Option<String> = None;
        let mut out: Vec<AnthropicMessage> = Vec::new();

        for msg in messages {
            match msg.role {
                Role::System => {
                    system = Some(msg.content.clone());
                }
                Role::User => {
                    out.push(AnthropicMessage {
                        role: "user".into(),
                        content: AnthropicContent::Text(msg.content.clone()),
                    });
                }
                Role::Assistant => {
                    if let Some(ref calls) = msg.tool_calls {
                        // Assistant message with tool calls
                        let mut blocks: Vec<AnthropicContentBlock> = Vec::new();
                        if !msg.content.is_empty() {
                            blocks.push(AnthropicContentBlock::Text {
                                text: msg.content.clone(),
                            });
                        }
                        for tc in calls {
                            blocks.push(AnthropicContentBlock::ToolUse {
                                id: tc.id.clone(),
                                name: tc.name.clone(),
                                input: tc.arguments.clone(),
                            });
                        }
                        out.push(AnthropicMessage {
                            role: "assistant".into(),
                            content: AnthropicContent::Blocks(blocks),
                        });
                    } else {
                        out.push(AnthropicMessage {
                            role: "assistant".into(),
                            content: AnthropicContent::Text(msg.content.clone()),
                        });
                    }
                }
                Role::Tool => {
                    // In Anthropic, tool results are user messages with tool_result blocks.
                    // We need to check if the previous message in `out` is already a user
                    // message with tool_result blocks and merge if so.
                    let block = AnthropicContentBlock::ToolResult {
                        tool_use_id: msg.tool_call_id.clone().unwrap_or_default(),
                        content: msg.content.clone(),
                    };

                    // Check if last message is a user message with blocks (tool results)
                    let should_merge = matches!(
                        out.last(),
                        Some(AnthropicMessage {
                            role,
                            content: AnthropicContent::Blocks(_),
                        }) if role == "user"
                    );

                    if should_merge {
                        if let Some(last) = out.last_mut() {
                            if let AnthropicContent::Blocks(ref mut blocks) = last.content {
                                blocks.push(block);
                            }
                        }
                    } else {
                        out.push(AnthropicMessage {
                            role: "user".into(),
                            content: AnthropicContent::Blocks(vec![block]),
                        });
                    }
                }
            }
        }

        (system, out)
    }

    fn convert_tools(tools: &[ToolDefinition]) -> Vec<AnthropicTool> {
        tools
            .iter()
            .map(|t| AnthropicTool {
                name: t.name.clone(),
                description: t.description.clone(),
                input_schema: t.parameters.clone(),
            })
            .collect()
    }
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    async fn chat(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
    ) -> Result<LlmResponse> {
        let (system, api_messages) = Self::convert_messages(messages);

        let request = AnthropicRequest {
            model: self.model.clone(),
            max_tokens: self.max_tokens,
            system,
            messages: api_messages,
            tools: Self::convert_tools(tools),
            stream: false,
        };

        let response = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .await
            .context("Failed to send request to Anthropic")?;

        let status = response.status();
        let body = response
            .text()
            .await
            .context("Failed to read Anthropic response")?;

        if !status.is_success() {
            if let Ok(err) = serde_json::from_str::<AnthropicErrorResponse>(&body) {
                if let Some(e) = err.error {
                    anyhow::bail!("Anthropic API error ({}): {}", status, e.message);
                }
            }
            anyhow::bail!("Anthropic API error ({}): {}", status, body);
        }

        let resp: AnthropicResponse =
            serde_json::from_str(&body).context("Failed to parse Anthropic response")?;

        // Check if response contains tool_use blocks
        let mut tool_calls = Vec::new();
        let mut text_parts = Vec::new();

        for block in &resp.content {
            match block {
                AnthropicContentBlock::ToolUse { id, name, input } => {
                    tool_calls.push(ToolCall {
                        id: id.clone(),
                        name: name.clone(),
                        arguments: input.clone(),
                    });
                }
                AnthropicContentBlock::Text { text } => {
                    text_parts.push(text.clone());
                }
                _ => {}
            }
        }

        if !tool_calls.is_empty() {
            Ok(LlmResponse::ToolCalls(tool_calls))
        } else {
            Ok(LlmResponse::Text(text_parts.join("\n")))
        }
    }

    fn name(&self) -> &str {
        "Anthropic"
    }

    fn model(&self) -> &str {
        &self.model
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    async fn chat_stream(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        stream_tx: StreamSender,
    ) -> Result<LlmResponse> {
        let (system, api_messages) = Self::convert_messages(messages);

        let request = AnthropicRequest {
            model: self.model.clone(),
            max_tokens: self.max_tokens,
            system,
            messages: api_messages,
            tools: Self::convert_tools(tools),
            stream: true,
        };

        let response = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .await
            .context("Failed to send streaming request to Anthropic")?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Anthropic streaming API error ({}): {}", status, body);
        }

        let mut full_text = String::new();
        let mut thinking_text = String::new();
        let mut tool_calls: Vec<ToolCall> = Vec::new();
        let mut current_tool_id = String::new();
        let mut current_tool_name = String::new();
        let mut current_tool_input = String::new();
        let mut in_tool_use = false;

        use futures_util::StreamExt;
        let mut stream = response.bytes_stream();
        let mut buffer = String::new();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.context("Stream chunk error")?;
            buffer.push_str(&String::from_utf8_lossy(&chunk));

            while let Some(line_end) = buffer.find('\n') {
                let line = buffer[..line_end].trim().to_string();
                buffer = buffer[line_end + 1..].to_string();

                if line.is_empty() {
                    continue;
                }

                // Parse SSE event type
                if line.starts_with("event: ") {
                    continue; // We parse data lines
                }

                if let Some(data) = line.strip_prefix("data: ") {
                    if let Ok(parsed) = serde_json::from_str::<Value>(data) {
                        let event_type = parsed.get("type").and_then(|t| t.as_str()).unwrap_or("");

                        match event_type {
                            "content_block_start" => {
                                if let Some(block) = parsed.get("content_block") {
                                    let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
                                    if block_type == "tool_use" {
                                        in_tool_use = true;
                                        current_tool_id = block.get("id").and_then(|i| i.as_str()).unwrap_or("").to_string();
                                        current_tool_name = block.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
                                        current_tool_input.clear();
                                        let _ = stream_tx.send(StreamEvent::ToolUseStart {
                                            id: current_tool_id.clone(),
                                            name: current_tool_name.clone(),
                                        });
                                    }
                                }
                            }
                            "content_block_delta" => {
                                if let Some(delta) = parsed.get("delta") {
                                    let delta_type = delta.get("type").and_then(|t| t.as_str()).unwrap_or("");
                                    match delta_type {
                                        "text_delta" => {
                                            if let Some(text) = delta.get("text").and_then(|t| t.as_str()) {
                                                full_text.push_str(text);
                                                let _ = stream_tx.send(StreamEvent::TextDelta {
                                                    text: text.to_string(),
                                                });
                                            }
                                        }
                                        "thinking_delta" => {
                                            if let Some(thinking) = delta.get("thinking").and_then(|t| t.as_str()) {
                                                thinking_text.push_str(thinking);
                                                let _ = stream_tx.send(StreamEvent::Thinking {
                                                    text: thinking.to_string(),
                                                });
                                            }
                                        }
                                        "input_json_delta" => {
                                            if let Some(partial) = delta.get("partial_json").and_then(|p| p.as_str()) {
                                                current_tool_input.push_str(partial);
                                                let _ = stream_tx.send(StreamEvent::ToolInputDelta {
                                                    id: current_tool_id.clone(),
                                                    delta: partial.to_string(),
                                                });
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            "content_block_stop" => {
                                if in_tool_use {
                                    let args: Value = serde_json::from_str(&current_tool_input)
                                        .unwrap_or(Value::Object(serde_json::Map::new()));
                                    tool_calls.push(ToolCall {
                                        id: current_tool_id.clone(),
                                        name: current_tool_name.clone(),
                                        arguments: args,
                                    });
                                    in_tool_use = false;
                                }
                            }
                            "message_delta" => {
                                if let Some(usage) = parsed.get("usage") {
                                    let output = usage.get("output_tokens").and_then(|t| t.as_u64()).unwrap_or(0) as u32;
                                    let _ = stream_tx.send(StreamEvent::Usage {
                                        input_tokens: 0,
                                        output_tokens: output,
                                    });
                                }
                            }
                            "message_start" => {
                                if let Some(msg) = parsed.get("message") {
                                    if let Some(usage) = msg.get("usage") {
                                        let input = usage.get("input_tokens").and_then(|t| t.as_u64()).unwrap_or(0) as u32;
                                        let _ = stream_tx.send(StreamEvent::Usage {
                                            input_tokens: input,
                                            output_tokens: 0,
                                        });
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        let _ = stream_tx.send(StreamEvent::MessageStop);

        if !tool_calls.is_empty() {
            Ok(LlmResponse::ToolCalls(tool_calls))
        } else if !thinking_text.is_empty() {
            Ok(LlmResponse::TextWithThinking {
                text: full_text,
                thinking: thinking_text,
            })
        } else {
            Ok(LlmResponse::Text(full_text))
        }
    }
}
