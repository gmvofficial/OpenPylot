use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Arc;

use super::{Tool, ToolDefinition, ToolResult};
use crate::llm::LlmProvider;
use crate::skills::SkillRegistry;
use crate::sub_agents::AgentOrchestrator;
use crate::sub_agents::types::SubAgentConfig;

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
            description: "Spawn an autonomous sub-agent to work on a task in the background. The sub-agent gets its own LLM context and can use tools like web search, document loading, file reading, and notes. Use this for tasks that can be delegated, like research, analysis, summarization, or background processing.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "A short descriptive name for the sub-agent (e.g., 'researcher', 'code-reviewer')"
                    },
                    "task": {
                        "type": "string",
                        "description": "The detailed task description for the sub-agent to complete"
                    },
                    "system_prompt": {
                        "type": "string",
                        "description": "Optional custom system prompt for the sub-agent. If not provided, a default is used."
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
        let system_prompt = params["system_prompt"]
            .as_str()
            .unwrap_or("")
            .to_string();

        tracing::info!("spawn_sub_agent tool called: name={}, task_len={}", name, task.len());

        let config = SubAgentConfig {
            name: name.clone(),
            system_prompt,
            ..Default::default()
        };

        let llm = Arc::clone(&self.llm);
        let tools = super::build_sub_agent_tools(self.data_dir.clone());
        let skills = SkillRegistry::load_all(None);
        let data_dir = self.data_dir.clone();

        tracing::info!("Sub-agent '{}' will have {} tools: {:?}", name, tools.names().len(), tools.names());

        let conversation_id = self.current_conversation_id.lock().unwrap().clone();

        match self.orchestrator.spawn(config, task, llm, tools, skills, data_dir, conversation_id).await {
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
