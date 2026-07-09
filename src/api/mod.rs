pub mod handlers;
pub mod ws;

use axum::{
    extract::DefaultBodyLimit,
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
use crate::learning::PromptEvolution;
use crate::llm::LlmProvider;
use crate::mcp::McpRegistry;
use crate::scheduler::AgentScheduler;
use crate::smart_memory::SmartMemory;
use crate::social::SocialManager;
use crate::sub_agents::AgentOrchestrator;

// ── Conversation persistence ─────────────────────────────────────────

/// A stored message inside a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredMessage {
    pub id: String,
    pub role: String, // "user" | "assistant"
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
        if let Err(e) = std::fs::create_dir_all(&dir) {
            tracing::error!(
                "ConversationStore: failed to create directory {}: {}. \
                 Conversation history will NOT persist.",
                dir.display(),
                e
            );
        }
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
    ///
    /// IMPORTANT: any I/O failure here is logged at ERROR level — historically
    /// the error was swallowed, which silently broke conversation history when
    /// the disk filled up (`ENOSPC`) or the data dir lost write permissions.
    pub fn save(&self, convo: &StoredConversation) {
        let path = self.dir.join(format!("{}.json", convo.id));
        let json = match serde_json::to_string_pretty(convo) {
            Ok(j) => j,
            Err(e) => {
                tracing::error!(
                    "ConversationStore: failed to serialize conversation {}: {}",
                    convo.id,
                    e
                );
                return;
            }
        };

        // Atomic-ish write: write to a temp file in the same dir, then rename.
        // This avoids leaving a half-written JSON file if the process is
        // killed mid-write, which would corrupt the conversation on next load.
        let tmp = self.dir.join(format!("{}.json.tmp", convo.id));
        if let Err(e) = std::fs::write(&tmp, &json) {
            tracing::error!(
                "ConversationStore: failed to write {} ({}). \
                 Conversation history will NOT persist this turn. \
                 Most common cause: disk full (run `df -h /`) or no write \
                 permission on the data dir.",
                tmp.display(),
                e
            );
            // Best-effort cleanup of the partial temp file.
            let _ = std::fs::remove_file(&tmp);
            return;
        }
        if let Err(e) = std::fs::rename(&tmp, &path) {
            tracing::error!(
                "ConversationStore: failed to rename {} -> {}: {}",
                tmp.display(),
                path.display(),
                e
            );
            let _ = std::fs::remove_file(&tmp);
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
    pub llm: Arc<dyn LlmProvider>,
    pub scheduler: Arc<Mutex<AgentScheduler>>,
    pub start_time: std::time::Instant,
    pub conversations: Arc<ConversationStore>,
    pub smart_memory: Option<Arc<SmartMemory>>,
    pub mcp_registry: Option<Arc<tokio::sync::Mutex<McpRegistry>>>,
    pub orchestrator: Option<Arc<AgentOrchestrator>>,
    pub social_manager: Option<Arc<tokio::sync::Mutex<SocialManager>>>,
    pub prompt_evolution: Option<Arc<tokio::sync::Mutex<PromptEvolution>>>,
    pub memory_v2_store: Option<Arc<crate::memory_v2::MemoryStore>>,
    pub sub_agent_store: Option<Arc<crate::sub_agents::SubAgentStore>>,
    /// Shared slot for the current conversation ID (set by WS handler, read by SpawnSubAgentTool).
    pub spawn_conversation_id: Arc<std::sync::Mutex<Option<String>>>,
    /// Broadcast channel for pushing notifications to connected WebSocket clients.
    pub notification_tx: tokio::sync::broadcast::Sender<String>,
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
        .route("/knowledge/collections", post(handlers::create_collection))
        .route(
            "/knowledge/collections/{id}",
            delete(handlers::delete_collection),
        )
        .route(
            "/knowledge/collections/{id}/documents",
            get(handlers::list_documents),
        )
        .route("/knowledge/documents", get(handlers::list_all_documents))
        .route("/knowledge/documents", post(handlers::upload_document))
        .route(
            "/knowledge/documents/upload-stream",
            post(handlers::upload_document_stream),
        )
        .route(
            "/knowledge/documents/{id}",
            delete(handlers::delete_document),
        )
        .route("/knowledge/search", post(handlers::search_knowledge))
        // Document extraction (preview before upload)
        .route(
            "/knowledge/extract-document",
            post(handlers::extract_document_multipart),
        )
        // Setup wizard
        .route("/setup/status", get(handlers::get_setup_status))
        .route("/setup/llm", post(handlers::setup_llm))
        .route("/setup/identity", post(handlers::setup_identity))
        .route("/setup/telegram", post(handlers::setup_telegram))
        .route("/setup/whatsapp", post(handlers::setup_whatsapp))
        .route("/setup/google", post(handlers::setup_google))
        .route("/setup/validate-key", post(handlers::validate_api_key))
        // Skills
        .route("/skills", get(handlers::list_skills_api))
        .route("/skills/status", get(handlers::skills_status_api))
        .route("/skills/update", post(handlers::skills_update_api))
        .route("/skills/delete/{name}", delete(handlers::skill_delete_api))
        .route("/skills/scan", post(handlers::skill_scan_api))
        .route("/skills/detail/{name}", get(handlers::skill_detail_api))
        // Learning
        .route("/learning/rules", get(handlers::list_learned_rules))
        .route("/learning/feedback", post(handlers::submit_feedback))
        // MCP
        .route("/mcp/servers", get(handlers::list_mcp_servers))
        .route("/mcp/tools", get(handlers::list_mcp_tools))
        // Social
        .route("/social/posts", get(handlers::list_social_posts))
        .route("/social/posts", post(handlers::create_social_post))
        .route(
            "/social/posts/{id}",
            axum::routing::delete(handlers::delete_social_post),
        )
        .route(
            "/social/posts/{id}/publish",
            post(handlers::publish_social_post),
        )
        .route("/social/improve-post", post(handlers::improve_social_post))
        .route("/social/upload", post(handlers::upload_social_media))
        .route("/social/campaigns", get(handlers::list_campaigns))
        .route("/social/campaigns", post(handlers::create_campaign))
        .route("/social/platforms", get(handlers::list_social_platforms))
        .route(
            "/social/connect/{platform}",
            post(handlers::connect_social_platform),
        )
        .route(
            "/social/disconnect/{platform}",
            post(handlers::disconnect_social_platform),
        )
        // Sub-agents
        .route("/agents", get(handlers::list_sub_agents))
        .route("/agents", post(handlers::spawn_sub_agent))
        .route("/agents/presets", get(handlers::list_agent_presets))
        .route("/agents/presets/{name}", get(handlers::get_agent_preset))
        .route("/agents/{id}", get(handlers::get_sub_agent))
        .route("/agents/{id}", delete(handlers::cancel_sub_agent))
        .route("/agents/{id}/runs", get(handlers::list_sub_agent_runs))
        .route("/agents/{id}/runs", delete(handlers::clear_sub_agent_runs))
        .route(
            "/agents/{id}/permanent",
            delete(handlers::delete_sub_agent_permanent),
        )
        // Memory v2
        .route("/memory/v2/search", post(handlers::memory_v2_search))
        .route("/memory/v2/units", get(handlers::memory_v2_list))
        // SSE streaming chat
        .route("/chat/stream", post(handlers::chat_stream));

    let ws_routes = Router::new()
        .route("/chat", get(ws::ws_chat_handler))
        .route("/notifications", get(ws::ws_notifications_handler));

    // Set body size limit to 100MB for large file uploads
    let body_limit = DefaultBodyLimit::max(100 * 1024 * 1024); // 100MB

    // Static serving for user-uploaded media (images, PDFs) used in social posts.
    // The directory is created lazily by the upload handler; we pre-create here
    // so ServeDir doesn't 404 on first request.
    let uploads_dir = state.config.data_dir.join("uploads");
    let _ = std::fs::create_dir_all(&uploads_dir);
    let uploads_service = ServeDir::new(&uploads_dir);

    let mut app = Router::new()
        .nest("/api", api_routes)
        .nest("/ws", ws_routes)
        .nest_service("/uploads", uploads_service)
        .with_state(state)
        .layer(cors)
        .layer(body_limit);

    // Serve the frontend. An on-disk build (dev override via PYLOT_FRONTEND_DIR
    // or ./frontend/out) takes priority so local UI changes show up without
    // recompiling; otherwise fall back to the frontend embedded in the binary
    // at build time, so `pylot serve` is self-contained on every install.
    match frontend_dir {
        Some(dir) if dir.exists() => {
            let index = dir.join("index.html");
            // Serve static files, falling back to index.html for SPA routing
            let serve_dir = ServeDir::new(&dir).not_found_service(ServeFile::new(&index));
            app = app.fallback_service(serve_dir);
            tracing::info!("Serving frontend from disk: {}", dir.display());
        }
        _ => {
            if crate::frontend_assets::has_embedded_frontend() {
                app = app.fallback(crate::frontend_assets::static_handler);
                tracing::info!("Serving embedded frontend");
            } else {
                tracing::warn!(
                    "No frontend embedded and no build directory found. \
                     Build the UI with 'cd frontend && npm run build'."
                );
            }
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
