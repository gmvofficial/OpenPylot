# 03 — Streaming Responses

## Objective

Add real-time token-by-token streaming for LLM responses via SSE (Server-Sent Events) and WebSocket. Currently, OpenPylot returns batch responses only — the user waits for the entire response.

---

## Current State

- **LLM calls**: `src/llm/` — Makes API calls and returns complete response
- **API server**: `src/api/` — Axum-based, returns JSON responses
- **WebSocket**: Exists but sends complete messages
- **Streaming**: Not implemented

---

## Reference Implementations

### Claw Code
- **Path**: `extra_repos/claw-code-main/rust/src/`
- **Events**: TextDelta, ToolUse, ToolResult, Usage, MessageStop
- **Format**: JSON-structured SSE events

### IronClaw
- **Path**: `extra_repos/ironclaw-staging/src/`
- **Features**: SSE + WebSocket streaming, typing indicators, real-time event broadcast

---

## Architecture

### Event Types

```rust
// File: src/llm/stream.rs

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum StreamEvent {
    #[serde(rename = "text_delta")]
    TextDelta { text: String },

    #[serde(rename = "tool_use_start")]
    ToolUseStart { tool_name: String, tool_id: String },

    #[serde(rename = "tool_input_delta")]
    ToolInputDelta { text: String },

    #[serde(rename = "tool_result")]
    ToolResult { tool_id: String, result: String, is_error: bool },

    #[serde(rename = "thinking")]
    Thinking { text: String },

    #[serde(rename = "usage")]
    Usage { input_tokens: u32, output_tokens: u32 },

    #[serde(rename = "message_stop")]
    MessageStop,

    #[serde(rename = "error")]
    Error { message: String },
}
```

### Stream Channel

```rust
// Use tokio broadcast channel for multi-subscriber streaming
use tokio::sync::broadcast;

pub type StreamSender = broadcast::Sender<StreamEvent>;
pub type StreamReceiver = broadcast::Receiver<StreamEvent>;
```

---

## Implementation Steps

### Step 1: Add streaming to LLM providers (Day 1 morning)

**File**: `src/llm/openai.rs` and `src/llm/anthropic.rs`

Both OpenAI and Anthropic APIs support streaming via `stream: true`. Currently the code likely sets `stream: false` or doesn't set it.

For OpenAI:
```rust
// Add to request body: "stream": true
// Parse SSE response line by line:
// data: {"choices":[{"delta":{"content":"Hello"}}]}

pub async fn chat_stream(
    &self,
    messages: &[Message],
    tools: &[ToolDefinition],
    tx: StreamSender,
) -> Result<String> {
    let mut request = self.build_request(messages, tools)?;
    request["stream"] = json!(true);

    let response = self.client.post(&self.url)
        .json(&request)
        .send()
        .await?;

    let mut stream = response.bytes_stream();
    let mut full_text = String::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        let text = String::from_utf8_lossy(&chunk);
        
        for line in text.lines() {
            if let Some(data) = line.strip_prefix("data: ") {
                if data == "[DONE]" { break; }
                let parsed: Value = serde_json::from_str(data)?;
                
                if let Some(delta) = parsed["choices"][0]["delta"]["content"].as_str() {
                    full_text.push_str(delta);
                    let _ = tx.send(StreamEvent::TextDelta { text: delta.to_string() });
                }
                
                if let Some(tool_calls) = parsed["choices"][0]["delta"]["tool_calls"].as_array() {
                    // Handle streaming tool calls
                }
            }
        }
    }

    let _ = tx.send(StreamEvent::MessageStop);
    Ok(full_text)
}
```

For Anthropic — similar pattern using `stream: true` in the Messages API.

### Step 2: Add SSE endpoint (Day 1 afternoon)

**File**: Modify `src/api/mod.rs` or create `src/api/stream.rs`

```rust
use axum::{
    response::sse::{Event, Sse},
    extract::State,
};
use futures::stream::Stream;

pub async fn chat_stream_sse(
    State(app): State<AppState>,
    Json(req): Json<ChatRequest>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let (tx, mut rx) = broadcast::channel::<StreamEvent>(100);

    // Spawn agent task
    tokio::spawn(async move {
        app.agent.handle_message_streaming(&req.message, tx).await;
    });

    let stream = async_stream::stream! {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    let data = serde_json::to_string(&event).unwrap();
                    yield Ok(Event::default().data(data));
                    if matches!(event, StreamEvent::MessageStop) {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Closed) => break,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
            }
        }
    };

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("ping")
    )
}
```

**Route**: `POST /api/chat/stream` → SSE endpoint

### Step 3: Update WebSocket handler (Day 1 afternoon)

**File**: Modify existing WebSocket handler in `src/api/`

```rust
// In the WebSocket message handler, use streaming:
async fn handle_ws_message(
    ws_tx: &mut SplitSink<WebSocket, WsMessage>,
    agent: &Agent,
    user_msg: &str,
) {
    let (tx, mut rx) = broadcast::channel::<StreamEvent>(100);

    tokio::spawn({
        let agent = agent.clone();
        let msg = user_msg.to_string();
        async move { agent.handle_message_streaming(&msg, tx).await; }
    });

    while let Ok(event) = rx.recv().await {
        let data = serde_json::to_string(&event).unwrap();
        let _ = ws_tx.send(WsMessage::Text(data)).await;
        if matches!(event, StreamEvent::MessageStop) { break; }
    }
}
```

### Step 4: Update agent to support streaming (Day 1)

**File**: Modify `src/agent.rs`

```rust
impl Agent {
    /// Existing method (kept for backward compatibility)
    pub async fn handle_message(&self, message: &str) -> Result<String> {
        // ... existing batch logic
    }

    /// New streaming method
    pub async fn handle_message_streaming(
        &self,
        message: &str,
        tx: StreamSender,
    ) -> Result<String> {
        // Same logic as handle_message but passes tx to LLM provider
        // Tool results also sent via tx
    }
}
```

---

## Cargo.toml Dependencies

```toml
async-stream = "0.3"
futures = "0.3"   # likely already present
# tokio broadcast already available in tokio
```

---

## Testing

- `test_sse_events` — Verify event format
- `test_stream_text_delta` — Token-by-token delivery
- `test_stream_tool_use` — Tool call events
- `test_stream_message_stop` — Clean termination
- `test_websocket_streaming` — WS receives stream events
- `test_batch_fallback` — Non-stream endpoint still works

---

## Acceptance Criteria

- [ ] SSE endpoint streams TextDelta events token-by-token
- [ ] WebSocket sends stream events in real-time
- [ ] Tool use events emitted (start, input, result)
- [ ] MessageStop event terminates stream cleanly
- [ ] Keep-alive prevents SSE timeout
- [ ] Existing batch `/api/chat` endpoint unchanged
- [ ] CLI REPL shows text as it arrives (character by character)
