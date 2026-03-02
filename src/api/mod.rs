pub mod handlers;
pub mod ws;

use axum::{
    routing::{delete, get, patch, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};

use crate::agent::Agent;
use crate::config::AppConfig;
use crate::scheduler::AgentScheduler;

// ── Conversation persistence ─────────────────────────────────────────

/// A stored message inside a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredMessage {
    pub id: String,
    pub role: String,      // "user" | "assistant"
    pub content: String,
    pub timestamp: String,
}

/// A persisted conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredConversation {
    pub id: String,
    pub title: String,
    pub messages: Vec<StoredMessage>,
    pub created_at: String,
    pub updated_at: String,
}

/// Simple file-backed conversation store.
/// Each conversation is a JSON file in `<data_dir>/conversations/<id>.json`.
#[derive(Clone)]
pub struct ConversationStore {
    dir: PathBuf,
}

impl ConversationStore {
    pub fn new(data_dir: &std::path::Path) -> Self {
        let dir = data_dir.join("conversations");
        std::fs::create_dir_all(&dir).ok();
        Self { dir }
    }

    /// List all conversations (meta only, no full messages).
    pub fn list(&self) -> Vec<StoredConversation> {
        let mut convos: Vec<StoredConversation> = std::fs::read_dir(&self.dir)
            .into_iter()
            .flatten()
            .filter_map(|entry| {
                let entry = entry.ok()?;
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("json") {
                    return None;
                }
                let data = std::fs::read_to_string(&path).ok()?;
                serde_json::from_str::<StoredConversation>(&data).ok()
            })
            .collect();
        convos.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        convos
    }

    /// Get a full conversation by ID.
    pub fn get(&self, id: &str) -> Option<StoredConversation> {
        let path = self.dir.join(format!("{}.json", id));
        let data = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str(&data).ok()
    }

    /// Save a conversation (create or update).
    pub fn save(&self, convo: &StoredConversation) {
        let path = self.dir.join(format!("{}.json", convo.id));
        if let Ok(json) = serde_json::to_string_pretty(convo) {
            let _ = std::fs::write(&path, json);
        }
    }

    /// Delete a conversation.
    pub fn delete(&self, id: &str) -> bool {
        let path = self.dir.join(format!("{}.json", id));
        std::fs::remove_file(&path).is_ok()
    }

    /// Add a message to a conversation (creates it if it doesn't exist).
    pub fn add_message(&self, conversation_id: &str, msg: StoredMessage) {
        let now = chrono::Utc::now().to_rfc3339();
        let mut convo = self.get(conversation_id).unwrap_or_else(|| {
            // Derive a title from the first user message
            let title = if msg.role == "user" {
                let t = msg.content.chars().take(60).collect::<String>();
                if msg.content.len() > 60 {
                    format!("{}…", t)
                } else {
                    t
                }
            } else {
                "New conversation".into()
            };
            StoredConversation {
                id: conversation_id.into(),
                title,
                messages: Vec::new(),
                created_at: now.clone(),
                updated_at: now.clone(),
            }
        });

        convo.updated_at = chrono::Utc::now().to_rfc3339();
        convo.messages.push(msg);
        self.save(&convo);
    }
}

// ── Shared API state ─────────────────────────────────────────────────

#[derive(Clone)]
pub struct ApiState {
    pub agent: Arc<Mutex<Agent>>,
    pub config: Arc<AppConfig>,
    pub scheduler: Arc<Mutex<AgentScheduler>>,
    pub start_time: std::time::Instant,
    pub conversations: Arc<ConversationStore>,
}

// ── Router builder ───────────────────────────────────────────────────

/// Build the full API router with optional static file serving.
///
/// Routes:
/// - `/api/*`  → REST handlers
/// - `/ws/*`   → WebSocket endpoints
/// - `/*`      → Static files from `frontend_dir` (if Some)
pub fn api_router(state: ApiState, frontend_dir: Option<PathBuf>) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let api_routes = Router::new()
        // Status
        .route("/status", get(handlers::get_status))
        // Chat
        .route("/chat", post(handlers::send_message))
        // Conversations
        .route("/conversations", get(handlers::list_conversations))
        .route("/conversations/{id}", get(handlers::get_conversation))
        .route("/conversations/{id}", delete(handlers::delete_conversation))
        // Tools
        .route("/tools", get(handlers::list_tools))
        // Integrations
        .route("/integrations", get(handlers::list_integrations))
        .route(
            "/integrations/{service}/connect",
            post(handlers::connect_integration),
        )
        .route(
            "/integrations/{service}",
            delete(handlers::disconnect_integration),
        )
        .route(
            "/integrations/{service}/test",
            post(handlers::test_integration),
        )
        // Settings
        .route("/settings", get(handlers::get_settings))
        .route("/settings", patch(handlers::update_settings))
        // Memory
        .route("/memory", get(handlers::get_memory))
        .route("/memory/{id}", patch(handlers::update_memory_fact))
        .route("/memory/{id}", delete(handlers::delete_memory_fact))
        // Jobs
        .route("/jobs", get(handlers::list_jobs))
        .route("/jobs/{id}", patch(handlers::update_job))
        .route("/jobs/{id}/run", post(handlers::run_job))
        // Logs
        .route("/logs", get(handlers::get_logs))
        // Knowledge
        .route("/knowledge/collections", get(handlers::list_collections))
        .route(
            "/knowledge/collections",
            post(handlers::create_collection),
        )
        .route(
            "/knowledge/collections/{id}",
            delete(handlers::delete_collection),
        )
        .route(
            "/knowledge/collections/{id}/documents",
            get(handlers::list_documents),
        )
        .route(
            "/knowledge/documents",
            get(handlers::list_all_documents),
        )
        .route(
            "/knowledge/documents",
            post(handlers::upload_document),
        )
        .route(
            "/knowledge/documents/{id}",
            delete(handlers::delete_document),
        )
        .route("/knowledge/search", post(handlers::search_knowledge))
        // Setup wizard
        .route("/setup/status", get(handlers::get_setup_status))
        .route("/setup/llm", post(handlers::setup_llm))
        .route("/setup/identity", post(handlers::setup_identity))
        .route("/setup/telegram", post(handlers::setup_telegram))
        .route("/setup/whatsapp", post(handlers::setup_whatsapp))
        .route("/setup/google", post(handlers::setup_google))
        .route("/setup/validate-key", post(handlers::validate_api_key));

    let ws_routes = Router::new()
        .route("/chat", get(ws::ws_chat_handler))
        .route("/notifications", get(ws::ws_notifications_handler));

    let mut app = Router::new()
        .nest("/api", api_routes)
        .nest("/ws", ws_routes)
        .with_state(state)
        .layer(cors);

    // Serve static frontend files if the build directory exists
    if let Some(dir) = frontend_dir {
        if dir.exists() {
            let index = dir.join("index.html");
            // Serve static files, falling back to index.html for SPA routing
            let serve_dir = ServeDir::new(&dir).not_found_service(ServeFile::new(&index));
            app = app.fallback_service(serve_dir);
            tracing::info!("Serving frontend from {}", dir.display());
        } else {
            tracing::warn!(
                "Frontend directory not found: {}. Run 'cd frontend && npm run build'.",
                dir.display()
            );
        }
    }

    app
}

// ── Server startup ───────────────────────────────────────────────────

/// Start the API + frontend server.
pub async fn start_api_server(
    port: u16,
    state: ApiState,
    frontend_dir: Option<PathBuf>,
) -> anyhow::Result<()> {
    let app = api_router(state, frontend_dir);

    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("API server listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
