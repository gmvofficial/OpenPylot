use anyhow::Result;
use colored::Colorize;

use crate::context::ConversationContext;
use crate::llm::{LlmProvider, LlmResponse, Message};
use crate::memory::MemoryStore;
use crate::tools::ToolRegistry;

/// The main Agent orchestrator — ties together LLM, tools, context, and memory.
pub struct Agent {
    llm: Box<dyn LlmProvider>,
    tools: ToolRegistry,
    context: ConversationContext,
    memory: MemoryStore,
    data_dir: std::path::PathBuf,
    max_iterations: usize,
}

impl Agent {
    pub fn new(
        llm: Box<dyn LlmProvider>,
        tools: ToolRegistry,
        system_prompt: String,
        max_context_messages: usize,
        max_iterations: usize,
        data_dir: std::path::PathBuf,
    ) -> Result<Self> {
        let memory = MemoryStore::load(&data_dir).unwrap_or_default();

        // Build full system prompt with memory context
        let full_prompt = format!("{}{}", system_prompt, memory.context_string());
        let context = ConversationContext::new(full_prompt, max_context_messages);

        Ok(Self {
            llm,
            tools,
            context,
            memory,
            data_dir,
            max_iterations,
        })
    }

    /// Process a user message through the full agent loop:
    ///   user msg → LLM → [tool calls → LLM]* → final text response
    pub async fn chat(&mut self, user_input: &str) -> Result<String> {
        // Add user message to context
        self.context.push(Message::user(user_input));

        let mut iterations = 0;

        loop {
            iterations += 1;
            if iterations > self.max_iterations {
                let msg = "Reached maximum tool call iterations. Stopping.";
                tracing::warn!("{}", msg);
                return Ok(msg.to_string());
            }

            // Build messages and call LLM
            let messages = self.context.build_messages();
            let tool_defs = self.tools.definitions();

            let response = self.llm.chat(&messages, &tool_defs).await?;

            match response {
                LlmResponse::Text(text) => {
                    // Final text response from LLM
                    self.context.push(Message::assistant(&text));

                    // Persist memory periodically
                    if self.context.len() % 10 == 0 {
                        let _ = self.memory.save(&self.data_dir);
                    }

                    return Ok(text);
                }
                LlmResponse::ToolCalls(calls) => {
                    // Record the assistant's tool-call message in context
                    self.context
                        .push(Message::assistant_tool_calls(calls.clone()));

                    // Execute each tool call
                    for call in &calls {
                        println!(
                            "\n  {} {} ({})",
                            "🔧".bright_yellow(),
                            format!("Calling tool: {}", call.name).bright_yellow(),
                            serde_json::to_string(&call.arguments)
                                .unwrap_or_default()
                                .dimmed()
                        );

                        let result = match self.tools.execute(&call.name, call.arguments.clone()).await {
                            Ok(result) => {
                                let status = if result.success {
                                    "✅".to_string()
                                } else {
                                    "❌".to_string()
                                };
                                println!(
                                    "  {} {}",
                                    status,
                                    if result.output.len() > 200 {
                                        format!("{}...", &result.output[..200])
                                    } else {
                                        result.output.clone()
                                    }
                                    .dimmed()
                                );
                                result
                            }
                            Err(e) => {
                                let err_msg = format!("Tool error: {}", e);
                                println!("  {} {}", "❌", err_msg.bright_red());
                                crate::tools::ToolResult::err(err_msg)
                            }
                        };

                        // Add tool result to context
                        self.context.push(Message::tool_result(
                            &call.id,
                            &result.output,
                        ));
                    }

                    // Continue the loop — LLM will process tool results
                }
            }
        }
    }

    /// Get the list of loaded tool names.
    pub fn tool_names(&self) -> Vec<String> {
        self.tools.names()
    }

    /// Clear conversation context (start fresh).
    pub fn clear_context(&mut self) {
        self.context.clear();
    }
}
