/// Unit tests for LLM message types (src/llm/mod.rs).
///
/// Validates Message constructors, Role enum, and ToolCall structures.

use pylot::llm::{Message, Role, ToolCall};

// ── Message constructors ─────────────────────────────────────────────

#[test]
fn test_system_message() {
    let msg = Message::system("You are helpful.");
    assert_eq!(msg.role, Role::System);
    assert_eq!(msg.content, "You are helpful.");
    assert!(msg.tool_call_id.is_none());
    assert!(msg.tool_calls.is_none());
}

#[test]
fn test_user_message() {
    let msg = Message::user("What time is it?");
    assert_eq!(msg.role, Role::User);
    assert_eq!(msg.content, "What time is it?");
    assert!(msg.tool_call_id.is_none());
    assert!(msg.tool_calls.is_none());
}

#[test]
fn test_assistant_message() {
    let msg = Message::assistant("It's 3 PM.");
    assert_eq!(msg.role, Role::Assistant);
    assert_eq!(msg.content, "It's 3 PM.");
    assert!(msg.tool_call_id.is_none());
    assert!(msg.tool_calls.is_none());
}

#[test]
fn test_tool_result_message() {
    let msg = Message::tool_result("call_123", "Event created");
    assert_eq!(msg.role, Role::Tool);
    assert_eq!(msg.content, "Event created");
    assert_eq!(msg.tool_call_id.as_deref(), Some("call_123"));
    assert!(msg.tool_calls.is_none());
}

#[test]
fn test_assistant_tool_calls_message() {
    let calls = vec![
        ToolCall {
            id: "tc_1".into(),
            name: "list_events".into(),
            arguments: serde_json::json!({"date": "2026-03-01"}),
        },
        ToolCall {
            id: "tc_2".into(),
            name: "create_note".into(),
            arguments: serde_json::json!({"title": "test"}),
        },
    ];

    let msg = Message::assistant_tool_calls(calls);
    assert_eq!(msg.role, Role::Assistant);
    assert!(msg.content.is_empty());
    assert!(msg.tool_call_id.is_none());

    let tool_calls = msg.tool_calls.unwrap();
    assert_eq!(tool_calls.len(), 2);
    assert_eq!(tool_calls[0].name, "list_events");
    assert_eq!(tool_calls[1].name, "create_note");
}

// ── Role enum ────────────────────────────────────────────────────────

#[test]
fn test_role_serialization() {
    let role = Role::Assistant;
    let json = serde_json::to_string(&role).unwrap();
    assert_eq!(json, r#""assistant""#);

    let parsed: Role = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, Role::Assistant);
}

#[test]
fn test_all_roles_roundtrip() {
    for role in [Role::System, Role::User, Role::Assistant, Role::Tool] {
        let json = serde_json::to_string(&role).unwrap();
        let parsed: Role = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, role);
    }
}

// ── ToolCall ─────────────────────────────────────────────────────────

#[test]
fn test_tool_call_serialization() {
    let call = ToolCall {
        id: "call_abc".into(),
        name: "search_notes".into(),
        arguments: serde_json::json!({"query": "meeting notes"}),
    };

    let json = serde_json::to_string(&call).unwrap();
    let parsed: ToolCall = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.id, "call_abc");
    assert_eq!(parsed.name, "search_notes");
    assert_eq!(parsed.arguments["query"], "meeting notes");
}
