pub mod auth;
pub mod handlers;
pub mod ws;

use axum::{
    extract::DefaultBodyLimit,
    routing::{any, delete, get, patch, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
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
    /// Companion apps (OpenDbPylot and friends) this server can host and proxy.
    pub companions: crate::companions::CompanionRegistry,
}

// ── Router builder ───────────────────────────────────────────────────

/// Build the full API router with optional static file serving.
///
/// Routes:
/// - `/api/*`  → REST handlers (token-gated)
/// - `/ws/*`   → WebSocket endpoints (token-gated)
/// - `/*`      → Static files from `frontend_dir` (if Some), unauthenticated
///
/// `token` gates everything that reads or mutates state. Static assets stay
/// open: they carry no user data, and gating them would leave the browser with
/// no way to bootstrap itself from `?token=…`. See [`auth`] for the rationale.
///
/// `allowed_origins` is the CORS allowlist. It is deliberately *not* `Any` —
/// with a credential in play, a wildcard origin would let any website the user
/// visits drive the agent from their browser.
pub fn api_router(
    state: ApiState,
    frontend_dir: Option<PathBuf>,
    token: auth::ApiToken,
    allowed_origins: Vec<String>,
) -> Router {
    let origins: Vec<axum::http::HeaderValue> = allowed_origins
        .iter()
        .filter_map(|o| match o.parse() {
            Ok(v) => Some(v),
            Err(_) => {
                tracing::warn!("Ignoring unparseable CORS origin: {o}");
                None
            }
        })
        .collect();

    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
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
        // Companions
        .route("/companions", get(handlers::list_companions))
        .route("/companions/{name}/start", post(handlers::start_companion))
        .route("/companions/{name}/stop", post(handlers::stop_companion))
        // SSE streaming chat
        .route("/chat/stream", post(handlers::chat_stream));

    let ws_routes = Router::new()
        .route("/chat", get(ws::ws_chat_handler))
        .route("/notifications", get(ws::ws_notifications_handler));

    // Token gate. `route_layer` (not `layer`) so it runs only for requests that
    // actually match a route in these sub-routers — an unmatched path falls
    // through to the static handler instead of returning 401 for a missing file.
    let token_gate = axum::middleware::from_fn_with_state(token, auth::require_token);
    let api_routes = api_routes.route_layer(token_gate.clone());
    let ws_routes = ws_routes.route_layer(token_gate.clone());

    // Set body size limit to 100MB for large file uploads
    let body_limit = DefaultBodyLimit::max(100 * 1024 * 1024); // 100MB

    // Static serving for user-uploaded media (images, PDFs) used in social posts.
    // The directory is created lazily by the upload handler; we pre-create here
    // so ServeDir doesn't 404 on first request.
    // Uploaded media is user data, so it is gated too. Browsers cannot put a
    // header on an `<img src>`, so the frontend appends `?token=` to these URLs
    // (see `assetUrl` in frontend/src/lib/api.ts).
    let uploads_dir = state.config.data_dir.join("uploads");
    let _ = std::fs::create_dir_all(&uploads_dir);
    // `layer`, not `route_layer`: this sub-router serves everything from its
    // fallback, and `route_layer` deliberately skips fallbacks — using it here
    // would leave uploads completely ungated.
    let uploads_service = Router::new()
        .fallback_service(ServeDir::new(&uploads_dir))
        .layer(token_gate.clone());

    // Companion reverse proxy. Behind the same token gate as everything else —
    // a companion binds loopback on an ephemeral port, so this is the only way
    // to reach it, and it inherits the parent's access control.
    // A single fallback rather than route patterns: the companion's own paths
    // are arbitrary, and `/{name}` + `/{name}/{*rest}` misses `/{name}/` — the
    // exact URL a browser lands on.
    let companion_proxy = Router::new()
        .fallback(crate::companions::proxy::handle)
        .with_state(state.companions.clone())
        .layer(token_gate);

    let mut app = Router::new()
        .nest("/api", api_routes)
        .nest("/ws", ws_routes)
        .nest_service("/uploads", uploads_service)
        .nest_service(crate::companions::MOUNT_PREFIX, companion_proxy)
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

/// How the server should expose itself on the network.
#[derive(Debug, Clone)]
pub struct ServeBinding {
    /// Interface to bind. Defaults to loopback.
    pub host: std::net::IpAddr,
    pub port: u16,
}

impl ServeBinding {
    /// Loopback-only: reachable from this machine and nothing else.
    pub fn loopback(port: u16) -> Self {
        Self {
            host: std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
            port,
        }
    }

    /// Whether this binding is reachable from outside the machine.
    pub fn is_public(&self) -> bool {
        !self.host.is_loopback()
    }

    /// The origins a browser may legitimately talk to this server from.
    ///
    /// Both loopback spellings are included because the user may open either,
    /// and `localhost` and `127.0.0.1` are distinct origins to the browser.
    /// The Next.js dev server is included so `npm run dev` can hit a running
    /// backend without disabling the allowlist.
    pub fn allowed_origins(&self) -> Vec<String> {
        let mut origins = vec![
            format!("http://127.0.0.1:{}", self.port),
            format!("http://localhost:{}", self.port),
            format!("http://[::1]:{}", self.port),
            // Frontend dev server.
            "http://localhost:3000".to_string(),
            "http://127.0.0.1:3000".to_string(),
        ];
        if self.is_public() {
            origins.push(format!("http://{}:{}", self.host, self.port));
        }
        origins
    }

    pub fn socket_addr(&self) -> std::net::SocketAddr {
        std::net::SocketAddr::new(self.host, self.port)
    }

    /// The URL a human should open, token included so the browser can bootstrap.
    pub fn browser_url(&self, token: &auth::ApiToken) -> String {
        let host = if self.host.is_loopback() {
            "127.0.0.1".to_string()
        } else {
            self.host.to_string()
        };
        format!("http://{}:{}/?token={}", host, self.port, token.as_str())
    }
}

/// Start the API + frontend server.
///
/// Binds loopback unless `binding.host` says otherwise. Historically this bound
/// `0.0.0.0` unconditionally with no authentication, which exposed shell access
/// to the whole local network; see [`auth`].
pub async fn start_api_server(
    binding: ServeBinding,
    state: ApiState,
    frontend_dir: Option<PathBuf>,
    token: auth::ApiToken,
) -> anyhow::Result<()> {
    let origins = binding.allowed_origins();
    let app = api_router(state, frontend_dir, token, origins);

    let addr = binding.socket_addr();
    if binding.is_public() {
        tracing::warn!(
            "API server bound to {} — reachable from the network. \
             The access token is the only thing protecting shell access.",
            addr
        );
    }
    tracing::info!("API server listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
