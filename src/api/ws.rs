use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};

use super::ApiState;
use crate::streaming::{stream_channel, StreamEvent};

// ── WebSocket message types ──────────────────────────────────────────

#[derive(Deserialize)]
struct WsClientMessage {
    #[serde(rename = "type")]
    msg_type: String,
    content: Option<String>,
    #[serde(alias = "conversationId")]
    conversation_id: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WsServerMessage {
    #[serde(rename = "type")]
    msg_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    conversation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "message")]
    error: Option<String>,
}

impl WsServerMessage {
    fn thinking() -> Self {
        Self {
            msg_type: "thinking".into(),
            content: None,
            conversation_id: None,
            id: None,
            tool_call: None,
            error: None,
        }
    }

    fn text_delta(content: &str) -> Self {
        Self {
            msg_type: "text_delta".into(),
            content: Some(content.into()),
            conversation_id: None,
            id: None,
            tool_call: None,
            error: None,
        }
    }

    fn message_end(message_id: &str, conversation_id: &str, full_content: &str) -> Self {
        Self {
            msg_type: "message_end".into(),
            content: Some(full_content.into()),
            conversation_id: Some(conversation_id.into()),
            id: Some(message_id.into()),
            tool_call: None,
            error: None,
        }
    }

    fn error(msg: &str) -> Self {
        Self {
            msg_type: "error".into(),
            content: None,
            conversation_id: None,
            id: None,
            tool_call: None,
            error: Some(msg.into()),
        }
    }
}

// ── Chat WebSocket ───────────────────────────────────────────────────

pub async fn ws_chat_handler(
    ws: WebSocketUpgrade,
    State(state): State<ApiState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_chat_ws(socket, state))
}

async fn handle_chat_ws(mut socket: WebSocket, state: ApiState) {
    tracing::info!("WebSocket chat connection established");

    let mut last_conversation_id: Option<String> = None;

    while let Some(msg) = socket.recv().await {
        let msg = match msg {
            Ok(Message::Text(text)) => text.to_string(),
            Ok(Message::Close(_)) => {
                tracing::info!("WebSocket chat connection closed by client");
                break;
            }
            Ok(_) => continue,
            Err(e) => {
                tracing::error!("WebSocket receive error: {}", e);
                break;
            }
        };

        let client_msg: WsClientMessage = match serde_json::from_str(&msg) {
            Ok(m) => m,
            Err(e) => {
                let _ = send_json(
                    &mut socket,
                    &WsServerMessage::error(&format!("Invalid message: {}", e)),
                )
                .await;
                continue;
            }
        };

        match client_msg.msg_type.as_str() {
            "message" | "send_message" => {
                let content = match client_msg.content {
                    Some(c) if !c.trim().is_empty() => c,
                    _ => {
                        let _ =
                            send_json(&mut socket, &WsServerMessage::error("Empty message")).await;
                        continue;
                    }
                };

                let conv_id = client_msg
                    .conversation_id
                    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

                last_conversation_id = Some(conv_id.clone());
                let msg_id = uuid::Uuid::new_v4().to_string();

                // Persist the user message
                state.conversations.add_message(
                    &conv_id,
                    super::StoredMessage {
                        id: uuid::Uuid::new_v4().to_string(),
                        role: "user".into(),
                        content: content.clone(),
                        timestamp: chrono::Utc::now().to_rfc3339(),
                    },
                );

                // Send thinking indicator
                let _ = send_json(&mut socket, &WsServerMessage::thinking()).await;

                // Set current conversation_id so SpawnSubAgentTool can inject results
                {
                    let mut cid = state.spawn_conversation_id.lock().unwrap();
                    *cid = Some(conv_id.clone());
                }

                // Set up a stream channel so the LLM emits tokens in real time.
                let (stream_tx, mut stream_rx) = stream_channel();

                // Run the agent in a spawned task so we can forward events concurrently.
                let agent_handle = {
                    let agent = state.agent.clone();
                    let content = content.clone();
                    tokio::spawn(async move {
                        let mut guard = agent.lock().await;
                        guard.set_streaming(true);
                        guard.set_stream_sender(stream_tx);
                        let result = guard.chat(&content).await;
                        // Clear sender so the receiver sees channel-closed.
                        guard.clear_stream_sender();
                        result
                    })
                };

                // Forward stream events to the WS client as they arrive.
                let mut streamed_text = String::new();
                loop {
                    tokio::select! {
                        event = stream_rx.recv() => {
                            match event {
                                Some(StreamEvent::TextDelta { text }) => {
                                    streamed_text.push_str(&text);
                                    let _ = send_json(
                                        &mut socket,
                                        &WsServerMessage::text_delta(&text),
                                    ).await;
                                }
                                Some(StreamEvent::ToolUseStart { id, name }) => {
                                    let payload = serde_json::json!({
                                        "id": id,
                                        "name": name,
                                        "status": "running",
                                    });
                                    let _ = send_json(&mut socket, &WsServerMessage {
                                        msg_type: "tool_call_start".into(),
                                        content: None,
                                        conversation_id: None,
                                        id: Some(id),
                                        tool_call: Some(payload),
                                        error: None,
                                    }).await;
                                }
                                Some(StreamEvent::ToolResult { id, name, success, output }) => {
                                    let payload = serde_json::json!({
                                        "id": id,
                                        "name": name,
                                        "status": if success { "success" } else { "error" },
                                        "result": output,
                                    });
                                    let _ = send_json(&mut socket, &WsServerMessage {
                                        msg_type: "tool_call_end".into(),
                                        content: None,
                                        conversation_id: None,
                                        id: Some(id),
                                        tool_call: Some(payload),
                                        error: None,
                                    }).await;
                                }
                                Some(StreamEvent::Thinking { text }) => {
                                    let _ = send_json(&mut socket, &WsServerMessage {
                                        msg_type: "thinking".into(),
                                        content: Some(text),
                                        conversation_id: None,
                                        id: None,
                                        tool_call: None,
                                        error: None,
                                    }).await;
                                }
                                Some(StreamEvent::Error { message }) => {
                                    let _ = send_json(
                                        &mut socket,
                                        &WsServerMessage::error(&message),
                                    ).await;
                                }
                                Some(StreamEvent::MessageStop) | None => {
                                    break;
                                }
                                // Usage / ToolInputDelta — skip for WS, not needed.
                                _ => {}
                            }
                        }
                    }
                }

                // Await the final agent result.
                let agent_result = match agent_handle.await {
                    Ok(r) => r,
                    Err(e) => Err(anyhow::anyhow!("Agent task panicked: {e}")),
                };

                match agent_result {
                    Ok(response) => {
                        // Persist the assistant message
                        state.conversations.add_message(
                            &conv_id,
                            super::StoredMessage {
                                id: msg_id.clone(),
                                role: "assistant".into(),
                                content: response.clone(),
                                timestamp: chrono::Utc::now().to_rfc3339(),
                            },
                        );

                        let _ = send_json(
                            &mut socket,
                            &WsServerMessage::message_end(&msg_id, &conv_id, &response),
                        )
                        .await;
                    }
                    Err(e) => {
                        let _ = send_json(
                            &mut socket,
                            &WsServerMessage::error(&format!("Agent error: {}", e)),
                        )
                        .await;
                    }
                }
            }
            "ping" => {
                let _ = send_json(
                    &mut socket,
                    &WsServerMessage {
                        msg_type: "pong".into(),
                        content: None,
                        conversation_id: None,
                        id: None,
                        tool_call: None,
                        error: None,
                    },
                )
                .await;
            }
            other => {
                let _ = send_json(
                    &mut socket,
                    &WsServerMessage::error(&format!("Unknown message type: {}", other)),
                )
                .await;
            }
        }
    }

    // Summarize the conversation in the background when connection ends
    if let (Some(conv_id), Some(ref smart_mem)) = (last_conversation_id, &state.smart_memory) {
        if let Some(convo) = state.conversations.get(&conv_id) {
            if convo.messages.len() >= 4 {
                let llm_messages: Vec<crate::llm::Message> = convo
                    .messages
                    .iter()
                    .map(|m| crate::llm::Message {
                        role: match m.role.as_str() {
                            "user" => crate::llm::Role::User,
                            "assistant" => crate::llm::Role::Assistant,
                            "system" => crate::llm::Role::System,
                            _ => crate::llm::Role::User,
                        },
                        content: m.content.clone(),
                        tool_call_id: None,
                        tool_calls: None,
                    })
                    .collect();
                let user_id = state.config.agent_name.clone();
                let smart_mem = smart_mem.clone();
                tokio::spawn(async move {
                    if let Err(e) = smart_mem
                        .summarize_conversation(&llm_messages, &user_id, &conv_id)
                        .await
                    {
                        tracing::warn!("Conversation summarization failed: {e}");
                    }
                });
            }
        }
    }

    tracing::info!("WebSocket chat connection ended");
}

// ── Notifications WebSocket ──────────────────────────────────────────

pub async fn ws_notifications_handler(
    ws: WebSocketUpgrade,
    State(state): State<ApiState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_notifications_ws(socket, state))
}

async fn handle_notifications_ws(mut socket: WebSocket, state: ApiState) {
    tracing::info!("WebSocket notifications connection established");

    // Send a welcome message
    let welcome = serde_json::json!({
        "type": "connected",
        "message": "Notifications channel connected"
    });
    let _ = socket
        .send(Message::Text(
            serde_json::to_string(&welcome).unwrap().into(),
        ))
        .await;

    // Subscribe to the broadcast channel for real-time notifications
    let mut notif_rx = state.notification_tx.subscribe();

    loop {
        tokio::select! {
            // Forward broadcast notifications to the WebSocket client
            result = notif_rx.recv() => {
                match result {
                    Ok(payload) => {
                        if socket.send(Message::Text(payload.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("Notification WS lagged, skipped {n} messages");
                    }
                    Err(_) => break,
                }
            }
            // Handle incoming client messages (pings, close)
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&*text) {
                            if parsed.get("type").and_then(|t| t.as_str()) == Some("ping") {
                                let pong = serde_json::json!({ "type": "pong" });
                                let _ = socket
                                    .send(Message::Text(serde_json::to_string(&pong).unwrap().into()))
                                    .await;
                            }
                        }
                    }
                    Some(Err(_)) => break,
                    _ => continue,
                }
            }
        }
    }

    tracing::info!("WebSocket notifications connection ended");
}

// ── Helpers ──────────────────────────────────────────────────────────

async fn send_json(socket: &mut WebSocket, msg: &WsServerMessage) -> Result<(), axum::Error> {
    let text = serde_json::to_string(msg).unwrap_or_default();
    socket.send(Message::Text(text.into())).await
}
