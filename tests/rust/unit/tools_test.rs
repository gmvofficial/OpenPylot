/// Unit tests for the tool registry and tool types (src/tools/mod.rs).
///
/// Validates ToolResult construction, ToolRegistry operations, and ToolDefinition serialization.

use gmv_agent::tools::{ToolDefinition, ToolRegistry, ToolResult};

// ── ToolResult ───────────────────────────────────────────────────────

#[test]
fn test_tool_result_ok() {
    let result = ToolResult::ok("Event created successfully");
    assert!(result.success);
    assert_eq!(result.output, "Event created successfully");
}

#[test]
fn test_tool_result_err() {
    let result = ToolResult::err("Calendar API returned 403");
    assert!(!result.success);
    assert_eq!(result.output, "Calendar API returned 403");
}

#[test]
fn test_tool_result_serialization() {
    let result = ToolResult::ok("done");
    let json = serde_json::to_string(&result).unwrap();
    let parsed: ToolResult = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.success, result.success);
    assert_eq!(parsed.output, result.output);
}

// ── ToolDefinition ───────────────────────────────────────────────────

#[test]
fn test_tool_definition_serialization() {
    let def = ToolDefinition {
        name: "create_note".into(),
        description: "Create a new note".into(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "title": { "type": "string" },
                "content": { "type": "string" }
            },
            "required": ["title", "content"]
        }),
    };

    let json = serde_json::to_string(&def).unwrap();
    let parsed: ToolDefinition = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.name, "create_note");
    assert_eq!(parsed.description, "Create a new note");
}

// ── ToolRegistry ─────────────────────────────────────────────────────

#[test]
fn test_empty_registry() {
    let registry = ToolRegistry::new();
    assert!(registry.is_empty());
    assert_eq!(registry.len(), 0);
    assert!(registry.definitions().is_empty());
    assert!(registry.names().is_empty());
}

#[test]
fn test_registry_execute_unknown_tool() {
    let registry = ToolRegistry::new();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let result = rt.block_on(registry.execute("nonexistent", serde_json::json!({})));
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("not found"),
        "Should report tool not found"
    );
}
