use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Arc;

use super::{Tool, ToolDefinition, ToolResult};
use crate::llm::LlmProvider;
use crate::skills::SkillRegistry;
use crate::sub_agents::types::SubAgentConfig;
use crate::sub_agents::AgentOrchestrator;

/// Tool that lets the LLM spawn sub-agents via the orchestrator.
pub struct SpawnSubAgentTool {
    orchestrator: Arc<AgentOrchestrator>,
    llm: Arc<dyn LlmProvider>,
    data_dir: std::path::PathBuf,
    /// Shared slot for the current conversation ID, set by the WS handler before each chat call.
    pub current_conversation_id: Arc<std::sync::Mutex<Option<String>>>,
}

impl SpawnSubAgentTool {
    pub fn new(
        orchestrator: Arc<AgentOrchestrator>,
        llm: Arc<dyn LlmProvider>,
        data_dir: std::path::PathBuf,
    ) -> Self {
        Self {
            orchestrator,
            llm,
            data_dir,
            current_conversation_id: Arc::new(std::sync::Mutex::new(None)),
        }
    }
}

#[async_trait]
impl Tool for SpawnSubAgentTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "spawn_sub_agent".to_string(),
            description: "Spawn an autonomous sub-agent to work on a task in the background. The sub-agent gets its own LLM context and can use tools like web search, document loading, file reading, and notes. Use this for tasks that can be delegated, like research, analysis, summarization, or background processing. Set `interval_minutes` to make the agent recurring (e.g. 'fetch AI news every 5 minutes' → interval_minutes=5); each run sends its result back to the chat. Recurring agents run until the user asks to stop them.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "A short descriptive name for the sub-agent (e.g., 'researcher', 'ai-news-fetcher'). Used as the cancel handle for recurring agents."
                    },
                    "task": {
                        "type": "string",
                        "description": "The detailed task description for the sub-agent to complete"
                    },
                    "system_prompt": {
                        "type": "string",
                        "description": "Optional custom system prompt for the sub-agent. If not provided, a default is used."
                    },
                    "interval_minutes": {
                        "type": "integer",
                        "minimum": 1,
                        "description": "Optional. If set, the sub-agent runs recurrently every N minutes and posts each result back to the chat, until cancelled with stop_recurring_sub_agent. Omit for one-shot tasks."
                    }
                },
                "required": ["name", "task"]
            }),
        }
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let name = params["name"].as_str().unwrap_or("sub-agent").to_string();
        let task = match params["task"].as_str() {
            Some(t) if !t.is_empty() => t.to_string(),
            _ => return Ok(ToolResult::err("Missing required parameter: task")),
        };
        let system_prompt = params["system_prompt"].as_str().unwrap_or("").to_string();
        let interval_minutes = params["interval_minutes"].as_u64();

        tracing::info!(
            "spawn_sub_agent tool called: name={}, task_len={}, interval_minutes={:?}",
            name,
            task.len(),
            interval_minutes
        );

        let config = SubAgentConfig {
            name: name.clone(),
            system_prompt,
            ..Default::default()
        };

        let llm = Arc::clone(&self.llm);
        let data_dir = self.data_dir.clone();
        let conversation_id = self.current_conversation_id.lock().unwrap().clone();

        // Recurring path: every `interval_minutes` minutes.
        if let Some(mins) = interval_minutes {
            if mins == 0 {
                return Ok(ToolResult::err("interval_minutes must be >= 1"));
            }
            let dd_for_factory = data_dir.clone();
            // Factory is invoked once per iteration so each run gets a fresh
            // ToolRegistry/SkillRegistry (neither type is Clone).
            let factory = move || {
                let tools = super::build_sub_agent_tools(dd_for_factory.clone());
                let skills = SkillRegistry::load_all(None);
                (tools, skills)
            };
            return match self
                .orchestrator
                .spawn_recurring(
                    config,
                    task,
                    llm,
                    factory,
                    data_dir,
                    conversation_id,
                    mins * 60,
                )
                .await
            {
                Ok(id) => Ok(ToolResult::ok(format!(
                    "✅ Recurring sub-agent '{}' started (id: {}). It will run now and then \
                     every {} minute(s), posting each update to this chat. Ask me to \"stop {}\" \
                     to cancel it.",
                    name, id, mins, name
                ))),
                Err(e) => Ok(ToolResult::err(format!(
                    "Failed to spawn recurring sub-agent: {}",
                    e
                ))),
            };
        }

        // One-shot path (unchanged).
        let tools = super::build_sub_agent_tools(data_dir.clone());
        let skills = SkillRegistry::load_all(None);

        tracing::info!(
            "Sub-agent '{}' will have {} tools: {:?}",
            name,
            tools.names().len(),
            tools.names()
        );

        match self
            .orchestrator
            .spawn(config, task, llm, tools, skills, data_dir, conversation_id)
            .await
        {
            Ok(id) => Ok(ToolResult::ok(format!(
                "Sub-agent '{}' spawned successfully (id: {}). It is now running in the background \
                 with access to web search, document loading, file reading, and notes tools. \
                 Check its status on the Sub-Agents page.",
                name, id
            ))),
            Err(e) => Ok(ToolResult::err(format!("Failed to spawn sub-agent: {}", e))),
        }
    }
}

/// Tool that lets the LLM stop a recurring sub-agent (e.g. when the user says
/// "stop updates" or "stop the news fetcher"). Matches by name (case-insensitive
/// substring), so users don't need to know the UUID.
pub struct StopRecurringSubAgentTool {
    orchestrator: Arc<AgentOrchestrator>,
}

impl StopRecurringSubAgentTool {
    pub fn new(orchestrator: Arc<AgentOrchestrator>) -> Self {
        Self { orchestrator }
    }
}

#[async_trait]
impl Tool for StopRecurringSubAgentTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "stop_recurring_sub_agent".to_string(),
            description: "Stop a running or recurring sub-agent by name. Use this when the user asks to stop updates, stop a recurring job, or cancel a sub-agent (e.g. 'stop updates', 'stop the news fetcher', 'cancel the AI news agent'). Pass the sub-agent's name (or any substring of it).".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Name (or substring of the name) of the sub-agent to stop. Matched case-insensitively. Pass an empty string or '*' to stop ALL running sub-agents."
                    }
                },
                "required": ["name"]
            }),
        }
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let name = params["name"].as_str().unwrap_or("").trim().to_string();
        // "*" or empty → match all running agents.
        let needle = if name.is_empty() || name == "*" {
            ""
        } else {
            name.as_str()
        };

        match self.orchestrator.cancel_by_name(needle).await {
            Ok(ids) if ids.is_empty() => Ok(ToolResult::ok(format!(
                "No running sub-agent matched '{}'. Nothing to stop.",
                name
            ))),
            Ok(ids) => Ok(ToolResult::ok(format!(
                "🛑 Stopped {} sub-agent(s) matching '{}': {}",
                ids.len(),
                name,
                ids.join(", ")
            ))),
            Err(e) => Ok(ToolResult::err(format!(
                "Failed to stop sub-agent(s): {}",
                e
            ))),
        }
    }
}
