use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Arc;

use crate::smart_memory::SmartMemory;
use crate::tools::{Tool, ToolDefinition, ToolResult};

// ════════════════════════════════════════════════════════════════════
//  RememberFact — Explicitly store a fact about the user
// ════════════════════════════════════════════════════════════════════

pub struct RememberFact {
    smart_memory: Arc<SmartMemory>,
    user_id: String,
}

impl RememberFact {
    pub fn new(smart_memory: Arc<SmartMemory>, user_id: String) -> Self {
        Self {
            smart_memory,
            user_id,
        }
    }
}

#[async_trait]
impl Tool for RememberFact {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "remember_fact".into(),
            description: "Store an important fact about the user for future reference. Use this when the user tells you something you should remember (preferences, personal details, recurring needs).".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "fact": {
                        "type": "string",
                        "description": "The fact to remember about the user"
                    },
                    "category": {
                        "type": "string",
                        "description": "Optional category (e.g., 'preference', 'personal', 'work')"
                    }
                },
                "required": ["fact"]
            }),
        }
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let fact = params["fact"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'fact' parameter"))?;

        let category = params["category"].as_str();

        match self.smart_memory.remember(fact, &self.user_id, category, None).await {
            Ok(id) => Ok(ToolResult::ok(format!(
                "Remembered: \"{}\" (id: {})",
                fact, id
            ))),
            Err(e) => Ok(ToolResult::err(format!("Failed to remember: {e}"))),
        }
    }
}

// ════════════════════════════════════════════════════════════════════
//  RecallMemories — Search user memories semantically
// ════════════════════════════════════════════════════════════════════

pub struct RecallMemories {
    smart_memory: Arc<SmartMemory>,
    user_id: String,
}

impl RecallMemories {
    pub fn new(smart_memory: Arc<SmartMemory>, user_id: String) -> Self {
        Self {
            smart_memory,
            user_id,
        }
    }
}

#[async_trait]
impl Tool for RecallMemories {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "recall_memories".into(),
            description: "Search your memory for relevant facts about the user. Use this when you need to look up specific information about the user that wasn't automatically loaded into context.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "What to search for in memories"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of results (default: 10)"
                    }
                },
                "required": ["query"]
            }),
        }
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let query = params["query"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'query' parameter"))?;
        let limit = params["limit"].as_u64().unwrap_or(10) as usize;

        match self.smart_memory.recall(query, &self.user_id, limit).await {
            Ok(memories) => {
                if memories.is_empty() {
                    Ok(ToolResult::ok("No relevant memories found."))
                } else {
                    let mut output = format!("Found {} relevant memories:\n", memories.len());
                    for (i, mem) in memories.iter().enumerate() {
                        output.push_str(&format!(
                            "{}. [score: {:.2}] {}\n",
                            i + 1,
                            mem.score,
                            mem.content
                        ));
                    }
                    Ok(ToolResult::ok(output))
                }
            }
            Err(e) => Ok(ToolResult::err(format!("Memory search failed: {e}"))),
        }
    }
}

// ════════════════════════════════════════════════════════════════════
//  SearchKnowledgeTool — Search uploaded documents semantically
// ════════════════════════════════════════════════════════════════════

pub struct SearchKnowledgeTool {
    smart_memory: Arc<SmartMemory>,
}

impl SearchKnowledgeTool {
    pub fn new(smart_memory: Arc<SmartMemory>) -> Self {
        Self { smart_memory }
    }
}

#[async_trait]
impl Tool for SearchKnowledgeTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "search_knowledge".into(),
            description: "Search the user's uploaded documents and knowledge base for relevant information. Use this when you need specific information from documents the user has uploaded.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "What to search for in the knowledge base"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of results (default: 5)"
                    }
                },
                "required": ["query"]
            }),
        }
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let query = params["query"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'query' parameter"))?;
        let limit = params["limit"].as_u64().unwrap_or(5) as usize;

        match self.smart_memory.search_knowledge(query, limit).await {
            Ok(results) => {
                if results.is_empty() {
                    Ok(ToolResult::ok(
                        "No relevant documents found in the knowledge base.",
                    ))
                } else {
                    let mut output =
                        format!("Found {} relevant knowledge chunks:\n\n", results.len());
                    for (i, doc) in results.iter().enumerate() {
                        let source = doc.source.as_deref().unwrap_or("unknown");
                        let title = doc.title.as_deref().unwrap_or("Untitled");
                        output.push_str(&format!(
                            "--- Result {} [score: {:.2}] ---\nSource: {} ({})\n{}\n\n",
                            i + 1,
                            doc.score,
                            title,
                            source,
                            doc.content
                        ));
                    }
                    Ok(ToolResult::ok(output))
                }
            }
            Err(e) => Ok(ToolResult::err(format!("Knowledge search failed: {e}"))),
        }
    }
}

// ════════════════════════════════════════════════════════════════════
//  ForgetFact — Remove an outdated or incorrect memory
// ════════════════════════════════════════════════════════════════════

pub struct ForgetFact {
    smart_memory: Arc<SmartMemory>,
}

impl ForgetFact {
    pub fn new(smart_memory: Arc<SmartMemory>) -> Self {
        Self { smart_memory }
    }
}

#[async_trait]
impl Tool for ForgetFact {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "forget_fact".into(),
            description: "Remove an outdated or incorrect memory by its ID. Use this when the user tells you something is wrong or no longer true.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "memory_id": {
                        "type": "string",
                        "description": "The UUID of the memory to remove"
                    }
                },
                "required": ["memory_id"]
            }),
        }
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let id = params["memory_id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing 'memory_id' parameter"))?;

        match self.smart_memory.forget(id).await {
            Ok(()) => Ok(ToolResult::ok(format!("Memory {} deleted successfully.", id))),
            Err(e) => Ok(ToolResult::err(format!("Failed to forget: {e}"))),
        }
    }
}
