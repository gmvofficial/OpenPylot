//! MCP stdio server: `pylot mcp serve`.
//!
//! The other direction from the rest of this module. `mcp::client` lets
//! OpenPylot *consume* MCP servers; this lets OpenPylot *be* one, so Claude
//! Desktop, Claude Code, the MCP Inspector — or another OpenPylot — can reach
//! the assistant, its memory and its skills.
//!
//! That closes a real gap: Goose and OpenHands are both addressable this way,
//! and OpenPylot was not, which made it a leaf rather than a node.
//!
//! # Protocol invariants
//!
//! These are not stylistic — breaking any of them hangs or confuses a host:
//!
//! - **stdout carries only JSON-RPC.** Logs go to stderr. Nothing in this path
//!   may `println!`.
//! - **Exactly one response per message carrying an `id`; silence for messages
//!   without one.** Spec-strict hosts send `notifications/initialized` with no
//!   id; some hosts send it *with* an id and then block for a reply. Keying on
//!   the presence of `id` satisfies both.
//! - **An unconfigured agent never kills the server.** `initialize` and
//!   `tools/list` always answer, so a host can connect and show the tools
//!   before an API key exists.

use std::sync::Arc;

use anyhow::Result;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;

use crate::agent::Agent;

/// MCP revision this speaks. A client proposing another is accepted — it
/// decides compatibility, not us.
pub const PROTOCOL_VERSION: &str = "2024-11-05";

/// Server name and version reported in `initialize`.
pub const SERVER_NAME: &str = "openpylot";

/// Cap on characters returned from a single tool, so a long answer cannot blow
/// up the host's context window.
const MAX_RESULT_CHARS: usize = 40_000;

/// Memory is single-user on this install, matching the HTTP handlers' default.
const DEFAULT_USER: &str = "default";

/// What the server needs to answer calls.
pub struct ServerState {
    /// `None` until an agent is available. Kept behind a mutex because the
    /// agent's chat loop takes `&mut self`.
    agent: Mutex<Option<Agent>>,
    agent_name: String,
}

impl ServerState {
    pub fn new(agent: Option<Agent>, agent_name: impl Into<String>) -> Self {
        Self {
            agent: Mutex::new(agent),
            agent_name: agent_name.into(),
        }
    }

    pub fn is_configured(&self) -> bool {
        self.agent.try_lock().map(|a| a.is_some()).unwrap_or(true)
    }
}

/// The tools this server exposes.
///
/// A stable contract: hosts reference these names in their own configuration,
/// so renaming one breaks every install that used it.
pub fn tool_definitions() -> Value {
    json!([
        {
            "name": "ask_assistant",
            "description":
                "Ask the OpenPylot assistant a question. Runs the full agent loop — its tools, \
                 skills, memory and any connected MCP servers — and returns the final answer. \
                 Use this for anything that needs the user's own context, integrations, or \
                 long-term memory.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "message": {
                        "type": "string",
                        "description": "What to ask the assistant."
                    }
                },
                "required": ["message"]
            }
        },
        {
            "name": "search_memory",
            "description":
                "Search the assistant's long-term memory for what it knows about the user — \
                 preferences, people, projects, past decisions. Read-only.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "What to look for." },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum results (default 10).",
                        "default": 10
                    }
                },
                "required": ["query"]
            }
        },
        {
            "name": "remember",
            "description":
                "Store a fact in the assistant's long-term memory so it is available in future \
                 conversations, including ones started elsewhere.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "fact": { "type": "string", "description": "The fact to remember." }
                },
                "required": ["fact"]
            }
        },
        {
            "name": "list_capabilities",
            "description":
                "List the assistant's registered tools and loaded skills. Call this to find out \
                 what it can actually do before asking it to do something.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "health",
            "description":
                "Check whether the assistant is configured and ready. Call this first if other \
                 tools fail.",
            "inputSchema": { "type": "object", "properties": {} }
        }
    ])
}

/// Serve MCP over stdin/stdout until the host closes the pipe.
pub async fn serve_stdio(state: Arc<ServerState>) -> Result<()> {
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut stdout = tokio::io::stdout();

    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        if let Some(response) = handle_line(&state, &line).await {
            stdout.write_all(response.to_string().as_bytes()).await?;
            stdout.write_all(b"\n").await?;
            stdout.flush().await?;
        }
    }
    Ok(())
}

/// Handle one JSON-RPC line.
///
/// Returns `Some(response)` exactly when the message carried an `id`, or when
/// it could not be parsed at all (which gets a parse error with a null id).
pub async fn handle_line(state: &ServerState, line: &str) -> Option<Value> {
    let message: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(_) => return Some(rpc_error(Value::Null, -32700, "Parse error")),
    };

    // Ids may be numbers or strings; keep whatever the client sent.
    let id = message.get("id").cloned()?;
    let method = message.get("method").and_then(Value::as_str).unwrap_or("");
    let params = message.get("params").cloned().unwrap_or(Value::Null);

    let result = match method {
        "initialize" => Ok(json!({
            "protocolVersion": params
                .get("protocolVersion")
                .and_then(Value::as_str)
                .unwrap_or(PROTOCOL_VERSION),
            "capabilities": { "tools": {} },
            "serverInfo": {
                "name": SERVER_NAME,
                "version": env!("CARGO_PKG_VERSION"),
            }
        })),

        // Some hosts send these with an id and wait for a reply.
        "notifications/initialized" | "initialized" | "ping" => Ok(json!({})),

        "tools/list" => Ok(json!({ "tools": tool_definitions() })),

        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let arguments = params.get("arguments").cloned().unwrap_or(json!({}));
            Ok(call_tool(state, name, &arguments).await)
        }

        // Declared in `capabilities`, so a host may probe for them.
        "resources/list" => Ok(json!({ "resources": [] })),
        "prompts/list" => Ok(json!({ "prompts": [] })),

        other => Err(format!("Unknown method: {other}")),
    };

    Some(match result {
        Ok(value) => json!({ "jsonrpc": "2.0", "id": id, "result": value }),
        Err(message) => rpc_error(id, -32601, &message),
    })
}

/// Run one tool and wrap the outcome as an MCP tool result.
async fn call_tool(state: &ServerState, name: &str, arguments: &Value) -> Value {
    match name {
        "health" => {
            let ready = state.agent.lock().await.is_some();
            ok_result(if ready {
                format!("{} is configured and ready.", state.agent_name)
            } else {
                "OpenPylot has no LLM configured yet. Run 'pylot init' to set one up."
                    .to_string()
            })
        }

        "ask_assistant" => {
            let Some(message) = arguments.get("message").and_then(Value::as_str) else {
                return error_result("'message' is required.");
            };
            if message.trim().is_empty() {
                return error_result("'message' must not be empty.");
            }

            let mut slot = state.agent.lock().await;
            let Some(agent) = slot.as_mut() else {
                return error_result(
                    "OpenPylot has no LLM configured yet. Run 'pylot init', then try again.",
                );
            };
            match agent.chat(message).await {
                Ok(answer) => ok_result(truncate(&answer)),
                Err(e) => error_result(&format!("{e:#}")),
            }
        }

        "search_memory" => {
            let Some(query) = arguments.get("query").and_then(Value::as_str) else {
                return error_result("'query' is required.");
            };
            let limit = arguments
                .get("limit")
                .and_then(Value::as_u64)
                .unwrap_or(10)
                .clamp(1, 50) as usize;

            let slot = state.agent.lock().await;
            let Some(agent) = slot.as_ref() else {
                return error_result("OpenPylot has no LLM configured yet.");
            };
            let Some(store) = agent.memory_v2_store() else {
                return error_result("Long-term memory is not enabled on this install.");
            };

            match store.search_keyword(query, DEFAULT_USER, limit) {
                Ok(hits) if hits.is_empty() => {
                    ok_result(format!("Nothing remembered about '{query}'."))
                }
                Ok(hits) => {
                    let body = hits
                        .iter()
                        .map(|(unit, score)| {
                            format!("- [{:?}, {score:.2}] {}", unit.memory_type, unit.content)
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    ok_result(truncate(&body))
                }
                Err(e) => error_result(&format!("Memory search failed: {e}")),
            }
        }

        "remember" => {
            let Some(fact) = arguments.get("fact").and_then(Value::as_str) else {
                return error_result("'fact' is required.");
            };
            if fact.trim().is_empty() {
                return error_result("'fact' must not be empty.");
            }

            let slot = state.agent.lock().await;
            let Some(agent) = slot.as_ref() else {
                return error_result("OpenPylot has no LLM configured yet.");
            };
            let Some(store) = agent.memory_v2_store() else {
                return error_result("Long-term memory is not enabled on this install.");
            };

            // Semantic: a fact asserted by the user, not an episode from a
            // session. Confidence is high because it was stated deliberately.
            let mut unit = crate::memory_v2::types::MemoryUnit::new(
                crate::memory_v2::types::MemoryType::Semantic,
                fact.trim().to_string(),
                DEFAULT_USER.to_string(),
            );
            unit.confidence = 0.9;
            unit.importance = 0.7;

            match store.insert(&unit) {
                Ok(()) => ok_result("Remembered.".to_string()),
                Err(e) => error_result(&format!("Could not store that: {e}")),
            }
        }

        "list_capabilities" => {
            let slot = state.agent.lock().await;
            let Some(agent) = slot.as_ref() else {
                return error_result("OpenPylot has no LLM configured yet.");
            };
            let tools = agent.tool_names();
            let body = format!(
                "{} tool(s):\n{}",
                tools.len(),
                tools
                    .iter()
                    .map(|t| format!("- {t}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
            ok_result(truncate(&body))
        }

        "" => error_result("Missing tool name."),
        other => error_result(&format!("Unknown tool: {other}")),
    }
}

/// Cut a long result down, saying so rather than truncating silently.
fn truncate(text: &str) -> String {
    if text.chars().count() <= MAX_RESULT_CHARS {
        return text.to_string();
    }
    let kept: String = text.chars().take(MAX_RESULT_CHARS).collect();
    format!("{kept}\n\n[truncated — the full answer was longer]")
}

fn ok_result(text: String) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "isError": false })
}

fn error_result(message: &str) -> Value {
    // MCP convention: a tool failure is a *result* with isError, not a
    // JSON-RPC error — the host shows it to the model to react to.
    json!({ "content": [{ "type": "text", "text": message }], "isError": true })
}

fn rpc_error(id: Value, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A server with no agent — the unconfigured case every host hits first.
    fn unconfigured() -> ServerState {
        ServerState::new(None, "Pylot")
    }

    async fn send(state: &ServerState, message: Value) -> Option<Value> {
        handle_line(state, &message.to_string()).await
    }

    // ── Protocol framing ─────────────────────────────────────────────

    #[tokio::test]
    async fn initialize_reports_the_server_and_its_tools_capability() {
        let response = send(
            &unconfigured(),
            json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}),
        )
        .await
        .expect("initialize carries an id and must be answered");

        assert_eq!(response["id"], 1);
        assert_eq!(response["result"]["serverInfo"]["name"], SERVER_NAME);
        assert!(response["result"]["capabilities"]["tools"].is_object());
    }

    #[tokio::test]
    async fn initialize_echoes_the_clients_protocol_version() {
        // Let the client decide compatibility rather than forcing ours.
        let response = send(
            &unconfigured(),
            json!({
                "jsonrpc": "2.0", "id": 1, "method": "initialize",
                "params": {"protocolVersion": "2025-06-18"}
            }),
        )
        .await
        .unwrap();
        assert_eq!(response["result"]["protocolVersion"], "2025-06-18");
    }

    #[tokio::test]
    async fn a_message_without_an_id_gets_no_response() {
        // Writing anything back for a true notification corrupts the stream.
        let response = send(
            &unconfigured(),
            json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        )
        .await;
        assert!(response.is_none());
    }

    #[tokio::test]
    async fn a_notification_that_carries_an_id_is_answered_anyway() {
        // Some hosts send this with an id and then block waiting for a reply.
        let response = send(
            &unconfigured(),
            json!({"jsonrpc": "2.0", "id": 7, "method": "notifications/initialized"}),
        )
        .await
        .expect("a message with an id must always be answered");
        assert_eq!(response["id"], 7);
        assert!(response.get("error").is_none());
    }

    #[tokio::test]
    async fn a_string_id_comes_back_unchanged() {
        let response = send(
            &unconfigured(),
            json!({"jsonrpc": "2.0", "id": "abc-123", "method": "ping"}),
        )
        .await
        .unwrap();
        assert_eq!(response["id"], "abc-123");
    }

    #[tokio::test]
    async fn unparseable_input_gets_a_parse_error() {
        let response = handle_line(&unconfigured(), "{not json").await.unwrap();
        assert_eq!(response["error"]["code"], -32700);
        assert!(response["id"].is_null());
    }

    #[tokio::test]
    async fn an_unknown_method_is_a_method_not_found_error() {
        let response = send(
            &unconfigured(),
            json!({"jsonrpc": "2.0", "id": 1, "method": "does/not/exist"}),
        )
        .await
        .unwrap();
        assert_eq!(response["error"]["code"], -32601);
    }

    #[tokio::test]
    async fn resources_and_prompts_answer_empty_rather_than_erroring() {
        // They are probed by hosts; an error there reads as a broken server.
        for method in ["resources/list", "prompts/list"] {
            let response = send(
                &unconfigured(),
                json!({"jsonrpc": "2.0", "id": 1, "method": method}),
            )
            .await
            .unwrap();
            assert!(response.get("error").is_none(), "{method} should not error");
        }
    }

    // ── Tool listing ─────────────────────────────────────────────────

    #[tokio::test]
    async fn tools_list_works_before_anything_is_configured() {
        // A host must be able to connect and show the tools without a key.
        let response = send(
            &unconfigured(),
            json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}),
        )
        .await
        .unwrap();

        let tools = response["result"]["tools"].as_array().unwrap();
        assert!(!tools.is_empty());
    }

    #[test]
    fn every_tool_is_fully_described() {
        for tool in tool_definitions().as_array().unwrap() {
            let name = tool["name"].as_str().expect("a name");
            assert!(!name.is_empty());
            assert!(
                tool["description"].as_str().is_some_and(|d| d.len() > 30),
                "{name}: the description is what the model uses to choose the tool"
            );
            assert_eq!(
                tool["inputSchema"]["type"], "object",
                "{name}: MCP requires an object schema"
            );
        }
    }

    #[test]
    fn tool_names_are_unique() {
        let tools = tool_definitions();
        let names: Vec<&str> = tools
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        let mut unique = names.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), names.len());
    }

    #[test]
    fn required_arguments_are_declared() {
        for tool in tool_definitions().as_array().unwrap() {
            let name = tool["name"].as_str().unwrap();
            let properties = tool["inputSchema"]["properties"].as_object().unwrap();
            // A tool taking arguments must say which are required, or a host
            // will happily call it with none.
            if !properties.is_empty() {
                assert!(
                    tool["inputSchema"].get("required").is_some(),
                    "{name} takes arguments but declares none required"
                );
            }
        }
    }

    // ── Tool calls against an unconfigured agent ─────────────────────

    async fn call(state: &ServerState, name: &str, arguments: Value) -> Value {
        send(
            state,
            json!({
                "jsonrpc": "2.0", "id": 1, "method": "tools/call",
                "params": {"name": name, "arguments": arguments}
            }),
        )
        .await
        .unwrap()["result"]
            .clone()
    }

    #[tokio::test]
    async fn health_reports_the_unconfigured_state_rather_than_failing() {
        let result = call(&unconfigured(), "health", json!({})).await;
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("pylot init"), "should say how to fix it: {text}");
    }

    #[tokio::test]
    async fn asking_an_unconfigured_assistant_returns_a_tool_error_not_a_crash() {
        let result = call(&unconfigured(), "ask_assistant", json!({"message": "hi"})).await;
        assert_eq!(result["isError"], true);
        assert!(result["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("pylot init"));
    }

    #[tokio::test]
    async fn a_missing_required_argument_is_a_tool_error() {
        // MCP convention: the host shows this to the model, which can retry.
        let result = call(&unconfigured(), "ask_assistant", json!({})).await;
        assert_eq!(result["isError"], true);
        assert!(result["content"][0]["text"].as_str().unwrap().contains("message"));
    }

    #[tokio::test]
    async fn an_empty_message_is_rejected() {
        let result = call(&unconfigured(), "ask_assistant", json!({"message": "   "})).await;
        assert_eq!(result["isError"], true);
    }

    #[tokio::test]
    async fn an_unknown_tool_is_a_tool_error_not_a_protocol_error() {
        let result = call(&unconfigured(), "no_such_tool", json!({})).await;
        assert_eq!(result["isError"], true);
        assert!(result["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("no_such_tool"));
    }

    #[tokio::test]
    async fn a_missing_tool_name_is_reported() {
        let result = call(&unconfigured(), "", json!({})).await;
        assert_eq!(result["isError"], true);
    }

    // ── Result shaping ───────────────────────────────────────────────

    #[test]
    fn a_short_result_is_returned_whole() {
        assert_eq!(truncate("short"), "short");
    }

    #[test]
    fn a_long_result_is_cut_and_says_so() {
        // Silently truncating would let a host act on a half-answer believing
        // it complete.
        let long = "x".repeat(MAX_RESULT_CHARS + 100);
        let out = truncate(&long);
        assert!(out.contains("truncated"));
        assert!(out.chars().count() < long.chars().count() + 100);
    }

    #[test]
    fn truncation_counts_characters_not_bytes() {
        // Byte slicing would panic on the first multi-byte character.
        let text = "é".repeat(MAX_RESULT_CHARS + 10);
        let _ = truncate(&text);
    }

    #[test]
    fn error_results_are_flagged_for_the_model() {
        let result = error_result("nope");
        assert_eq!(result["isError"], true);
        assert_eq!(result["content"][0]["type"], "text");
    }
}
