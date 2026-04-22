use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;

use crate::llm::Message;
use crate::smart_memory::{KnowledgeEntry, MemoryEntry};
use crate::tools::{ToolDefinition, ToolResult};

/// Memory backend trait — abstracts over smart memory implementations.
///
/// The default implementation wraps SmartMemory (SQLite + OpenAI embeddings).
/// Future backends: ChromaDB, Qdrant, Ollama embeddings, etc.
#[async_trait]
pub trait MemoryProvider: Send + Sync {
    /// Store a fact/memory for a user.
    async fn remember(
        &self,
        content: &str,
        user_id: &str,
        category: Option<&str>,
        metadata: Option<HashMap<String, Value>>,
    ) -> Result<String>;

    /// Search memories by semantic similarity.
    async fn recall(&self, query: &str, user_id: &str, limit: usize) -> Result<Vec<MemoryEntry>>;

    /// Delete a memory by ID.
    async fn forget(&self, id: &str) -> Result<()>;

    /// Build a context string from relevant memories + knowledge for injection.
    async fn build_context(&self, query: &str, user_id: &str) -> Result<String>;

    /// LLM-based extraction: pull facts from recent conversation and store them.
    async fn extract_and_store(&self, conversation: &[Message], user_id: &str) -> Result<usize>;

    /// Search the knowledge base (indexed documents).
    async fn search_knowledge(&self, query: &str, limit: usize) -> Result<Vec<KnowledgeEntry>>;

    /// Whether auto-extraction is enabled.
    fn auto_extract_enabled(&self) -> bool;

    /// How often (in message count) to trigger extraction.
    fn extraction_interval(&self) -> usize;
}

/// Tool provider trait — represents a bundle of tools (e.g., a plugin).
///
/// Built-in tools implement the existing `Tool` trait directly.
/// Plugins implement `ToolProvider` to expose multiple tools from an external process.
#[async_trait]
pub trait ToolProvider: Send + Sync {
    /// Provider name (e.g., plugin name).
    fn name(&self) -> &str;

    /// List all tool definitions this provider offers.
    fn tools(&self) -> Vec<ToolDefinition>;

    /// Execute a named tool with given arguments.
    async fn call(&self, tool_name: &str, args: Value) -> Result<ToolResult>;

    /// Health check — returns true if provider is operational.
    async fn health(&self) -> Result<bool>;
}

/// Sub-agent trait — enables multi-agent coordination.
///
/// Future: used by the plugin system to delegate to specialized sub-agents.
#[async_trait]
pub trait SubAgent: Send + Sync {
    /// Unique identifier for this agent.
    fn id(&self) -> &str;

    /// List capabilities that this agent provides.
    fn capabilities(&self) -> Vec<String>;

    /// List tools this agent exposes.
    fn tools(&self) -> Vec<ToolDefinition>;

    /// Send a request to this sub-agent and get a response.
    async fn invoke(&self, message: &str, context: Option<&str>) -> Result<String>;

    /// Health check.
    async fn health(&self) -> Result<bool>;
}
