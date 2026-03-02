use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};

use super::ApiState;

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
                let _ = send_json(&mut socket, &WsServerMessage::error(&format!("Invalid message: {}", e))).await;
                continue;
            }
        };

        match client_msg.msg_type.as_str() {
            "message" | "send_message" => {
                let content = match client_msg.content {
                    Some(c) if !c.trim().is_empty() => c,
                    _ => {
                        let _ = send_json(&mut socket, &WsServerMessage::error("Empty message")).await;
                        continue;
                    }
                };

                let conv_id = client_msg
                    .conversation_id
                    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

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

                // Process through the agent (synchronous for now)
                let mut agent = state.agent.lock().await;
                match agent.chat(&content).await {
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

                        // Send the full response as a text_delta followed by message_end.
                        let _ = send_json(
                            &mut socket,
                            &WsServerMessage::text_delta(&response),
                        )
                        .await;

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

    tracing::info!("WebSocket chat connection ended");
}

// ── Notifications WebSocket ──────────────────────────────────────────

pub async fn ws_notifications_handler(
    ws: WebSocketUpgrade,
    State(_state): State<ApiState>,
) -> impl IntoResponse {
    ws.on_upgrade(handle_notifications_ws)
}

async fn handle_notifications_ws(mut socket: WebSocket) {
    tracing::info!("WebSocket notifications connection established");

    // Send a welcome message
    let welcome = serde_json::json!({
        "type": "connected",
        "message": "Notifications channel connected"
    });
    let _ = socket
        .send(Message::Text(serde_json::to_string(&welcome).unwrap().into()))
        .await;

    // Keep the connection alive by responding to pings.
    // In a full implementation, this would broadcast events from the
    // webhook server and scheduler to connected clients.
    while let Some(msg) = socket.recv().await {
        match msg {
            Ok(Message::Close(_)) => break,
            Ok(Message::Text(text)) => {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&*text) {
                    if parsed.get("type").and_then(|t| t.as_str()) == Some("ping") {
                        let pong = serde_json::json!({ "type": "pong" });
                        let _ = socket
                            .send(Message::Text(serde_json::to_string(&pong).unwrap().into()))
                            .await;
                    }
                }
            }
            Ok(_) => continue,
            Err(_) => break,
        }
    }

    tracing::info!("WebSocket notifications connection ended");
}

// ── Helpers ──────────────────────────────────────────────────────────

async fn send_json(socket: &mut WebSocket, msg: &WsServerMessage) -> Result<(), axum::Error> {
    let text = serde_json::to_string(msg).unwrap_or_default();
    socket.send(Message::Text(text.into())).await
}
