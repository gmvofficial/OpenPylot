use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::llm::{LlmProvider, LlmResponse, Message, Role, ToolCall};
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
}
