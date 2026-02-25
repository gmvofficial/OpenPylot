pub mod notes;
pub mod calendar;
pub mod telegram;
pub mod whatsapp;
pub mod reminder;

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
