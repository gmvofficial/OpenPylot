pub mod bash;
pub mod calendar;
pub mod document_loader;
pub mod file_ops;
pub mod gmail;
pub mod memory_tools;
pub mod notes;
pub mod reminder;
pub mod search;
pub mod spawn_agent;
pub mod telegram;
pub mod web;
pub mod whatsapp;

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

// ── Tool definition sent to LLM ──────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value, // JSON Schema
}

// ── Result returned from tool execution ──────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub success: bool,
    pub output: String,
}

impl ToolResult {
    pub fn ok(output: impl Into<String>) -> Self {
        Self {
            success: true,
            output: output.into(),
        }
    }

    pub fn err(output: impl Into<String>) -> Self {
        Self {
            success: false,
            output: output.into(),
        }
    }
}

// ── Tool trait ────────────────────────────────────────────────────────

#[async_trait]
pub trait Tool: Send + Sync {
    /// Returns the JSON-Schema–based definition of this tool for the LLM.
    fn definition(&self) -> ToolDefinition;

    /// Execute the tool with the given parameters (parsed from LLM output).
    async fn execute(&self, params: Value) -> Result<ToolResult>;
}

// ── Tool registry ────────────────────────────────────────────────────

pub struct ToolRegistry {
    tools: Vec<Box<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self { tools: Vec::new() }
    }

    pub fn register(&mut self, tool: Box<dyn Tool>) {
        tracing::info!("Registered tool: {}", tool.definition().name);
        self.tools.push(tool);
    }

    /// All tool definitions (for sending to LLM).
    pub fn definitions(&self) -> Vec<ToolDefinition> {
        self.tools.iter().map(|t| t.definition()).collect()
    }

    /// Tool names for display.
    pub fn names(&self) -> Vec<String> {
        self.tools.iter().map(|t| t.definition().name).collect()
    }

    /// Execute a tool by name.
    pub async fn execute(&self, name: &str, params: Value) -> Result<ToolResult> {
        for tool in &self.tools {
            if tool.definition().name == name {
                return tool.execute(params).await;
            }
        }
        anyhow::bail!("Tool '{}' not found in registry", name)
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.tools.len()
    }
}

/// Build a safe subset of tools for sub-agents.
///
/// Sub-agents get read-oriented and research tools:
/// - Web search & extract
/// - Document loader
/// - File reading & search (read-only, no write/bash)
/// - Notes (create, list, search — for persisting findings)
/// - List reminders (read-only)
///
/// Sub-agents do NOT get: bash, file writing, spawn_agent (no grandchildren),
/// messaging tools (Telegram/WhatsApp/Gmail/Calendar).
pub fn build_sub_agent_tools(data_dir: std::path::PathBuf) -> ToolRegistry {
    let mut tools = ToolRegistry::new();

    // Web tools (read-only, safe)
    tools.register(Box::new(web::WebSearchTool::new()));
    tools.register(Box::new(web::WebExtractTool::new()));

    // Document loader (read-only)
    tools.register(Box::new(document_loader::DocumentLoaderTool::new()));

    // File reading & search (read-only, no write/bash)
    let workspace_root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    tools.register(Box::new(file_ops::ReadFileTool::new(workspace_root.clone())));
    tools.register(Box::new(search::GlobSearchTool::new(workspace_root.clone())));
    tools.register(Box::new(search::GrepSearchTool::new(workspace_root.clone())));
    tools.register(Box::new(search::ListDirectoryTool::new(workspace_root)));

    // Notes (sub-agents can create/read notes to persist findings)
    tools.register(Box::new(notes::CreateNote::new(data_dir.clone())));
    tools.register(Box::new(notes::ListNotes::new(data_dir.clone())));
    tools.register(Box::new(notes::SearchNotes::new(data_dir.clone())));

    // Reminders (read-only for sub-agents)
    tools.register(Box::new(reminder::ListReminders::new(data_dir)));

    tools
}
