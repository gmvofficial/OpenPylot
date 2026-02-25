use anyhow::{Context, Result};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::llm::{LlmProvider, LlmResponse, Message, Role, ToolCall};
use crate::tools::ToolDefinition;

// ── OpenAI API types ─────────────────────────────────────────────────

#[derive(Serialize)]
struct OpenAIRequest {
    model: String,
    messages: Vec<OpenAIMessage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<OpenAITool>,
    max_tokens: u32,
    temperature: f64,
}

#[derive(Serialize, Deserialize, Debug)]
struct OpenAIMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OpenAIToolCallResponse>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
struct OpenAITool {
    r#type: String,
    function: OpenAIFunction,
}

#[derive(Serialize, Deserialize, Debug)]
struct OpenAIFunction {
    name: String,
    description: String,
    parameters: Value,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct OpenAIToolCallResponse {
    id: String,
    r#type: String,
    function: OpenAIFunctionCall,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct OpenAIFunctionCall {
    name: String,
    arguments: String,
}

#[derive(Deserialize, Debug)]
struct OpenAIResponse {
    choices: Vec<OpenAIChoice>,
}

#[derive(Deserialize, Debug)]
struct OpenAIChoice {
    message: OpenAIResponseMessage,
}

#[derive(Deserialize, Debug)]
struct OpenAIResponseMessage {
    content: Option<String>,
    tool_calls: Option<Vec<OpenAIToolCallResponse>>,
}

#[derive(Deserialize, Debug)]
struct OpenAIErrorResponse {
    error: Option<OpenAIError>,
}

#[derive(Deserialize, Debug)]
struct OpenAIError {
    message: String,
}

// ── Provider implementation ──────────────────────────────────────────

pub struct OpenAIProvider {
    client: Client,
    api_key: String,
    model: String,
    max_tokens: u32,
    temperature: f64,
}

impl OpenAIProvider {
    pub fn new(api_key: String, model: String, max_tokens: u32, temperature: f64) -> Self {
        Self {
            client: Client::new(),
            api_key,
            model,
            max_tokens,
            temperature,
        }
    }

    fn convert_messages(messages: &[Message]) -> Vec<OpenAIMessage> {
        messages
            .iter()
            .map(|msg| {
                let role = match msg.role {
                    Role::System => "system",
                    Role::User => "user",
                    Role::Assistant => "assistant",
                    Role::Tool => "tool",
                };

                let tool_calls = msg.tool_calls.as_ref().map(|calls| {
                    calls
                        .iter()
                        .map(|tc| OpenAIToolCallResponse {
                            id: tc.id.clone(),
                            r#type: "function".to_string(),
                            function: OpenAIFunctionCall {
                                name: tc.name.clone(),
                                arguments: serde_json::to_string(&tc.arguments)
                                    .unwrap_or_default(),
                            },
                        })
                        .collect()
                });

                // For assistant messages with tool calls, content should be null
                let content = if tool_calls.is_some() && msg.content.is_empty() {
                    None
                } else {
                    Some(msg.content.clone())
                };

                OpenAIMessage {
                    role: role.to_string(),
                    content,
                    tool_calls,
                    tool_call_id: msg.tool_call_id.clone(),
                }
            })
            .collect()
    }

    fn convert_tools(tools: &[ToolDefinition]) -> Vec<OpenAITool> {
        tools
            .iter()
            .map(|t| OpenAITool {
                r#type: "function".to_string(),
                function: OpenAIFunction {
                    name: t.name.clone(),
                    description: t.description.clone(),
                    parameters: t.parameters.clone(),
                },
            })
            .collect()
    }
}

#[async_trait]
impl LlmProvider for OpenAIProvider {
    async fn chat(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
    ) -> Result<LlmResponse> {
        let request = OpenAIRequest {
            model: self.model.clone(),
            messages: Self::convert_messages(messages),
            tools: Self::convert_tools(tools),
            max_tokens: self.max_tokens,
            temperature: self.temperature,
        };

        let response = self
            .client
            .post("https://api.openai.com/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&request)
            .send()
            .await
            .context("Failed to send request to OpenAI")?;

        let status = response.status();
        let body = response.text().await.context("Failed to read OpenAI response")?;

        if !status.is_success() {
            if let Ok(err) = serde_json::from_str::<OpenAIErrorResponse>(&body) {
                if let Some(e) = err.error {
                    anyhow::bail!("OpenAI API error ({}): {}", status, e.message);
                }
            }
            anyhow::bail!("OpenAI API error ({}): {}", status, body);
        }

        let resp: OpenAIResponse =
            serde_json::from_str(&body).context("Failed to parse OpenAI response")?;

        let choice = resp
            .choices
            .into_iter()
            .next()
            .context("No choices in OpenAI response")?;

        // Check for tool calls
        if let Some(tool_calls) = choice.message.tool_calls {
            let calls: Vec<ToolCall> = tool_calls
                .into_iter()
                .map(|tc| {
                    let args: Value =
                        serde_json::from_str(&tc.function.arguments).unwrap_or(Value::Object(
                            serde_json::Map::new(),
                        ));
                    ToolCall {
                        id: tc.id,
                        name: tc.function.name,
                        arguments: args,
                    }
                })
                .collect();
            Ok(LlmResponse::ToolCalls(calls))
        } else {
            Ok(LlmResponse::Text(
                choice.message.content.unwrap_or_default(),
            ))
        }
    }

    fn name(&self) -> &str {
        "OpenAI"
    }

    fn model(&self) -> &str {
        &self.model
    }
}
