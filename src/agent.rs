use anyhow::Result;
use colored::Colorize;
use std::sync::Arc;

use crate::context::ConversationContext;
use crate::learning::{ErrorDiagnoser, FeedbackProcessor, PromptEvolution};
use crate::learning::types::{RuleSource, UserFeedback};
use crate::llm::{LlmProvider, LlmResponse, Message};
use crate::mcp::McpRegistry;
use crate::memory::MemoryStore;
use crate::memory_v2;
use crate::skills::SkillRegistry;
use crate::social::SocialManager;
use crate::streaming::{stream_channel, StreamEvent, StreamSender};
use crate::sub_agents::AgentOrchestrator;
use crate::tools::ToolRegistry;
use crate::traits::MemoryProvider;

/// Patterns for dangerous commands that require user approval.
const DANGEROUS_PATTERNS: &[&str] = &[
    "rm -rf",
    "rm -r /",
    "rmdir",
    "mkfs",
    "dd if=",
    "chmod 777",
    "chmod -R 777",
    "> /dev/",
    "format ",
    "del /f",
    "DROP TABLE",
    "DROP DATABASE",
    "DELETE FROM",
    "sudo rm",
    "sudo dd",
    "git push --force",
    "git reset --hard",
    "docker rm",
    "docker rmi",
    "docker system prune",
    "shutdown",
    "reboot",
    "halt",
    "curl | sh",
    "curl | bash",
    "wget | sh",
];

/// Patterns for secrets that should be redacted in tool output.
const SECRET_PATTERNS: &[&str] = &[
    "sk-",
    "api_key=",
    "apikey=",
    "api-key:",
    "token=",
    "secret=",
    "password=",
    "passwd=",
    "AWS_SECRET",
    "PRIVATE_KEY",
    "Bearer ",
    "Basic ",
];

/// The main Agent orchestrator — ties together LLM, tools, context, and memory.
pub struct Agent {
    llm: Arc<dyn LlmProvider>,
    tools: ToolRegistry,
    skill_registry: SkillRegistry,
    context: ConversationContext,
    memory: MemoryStore,
    smart_memory: Option<Arc<dyn MemoryProvider>>,
    data_dir: std::path::PathBuf,
    max_iterations: usize,
    quiet_mode: bool,
    message_count: usize,
    user_id: String,
    /// Enable streaming mode for LLM responses.
    streaming_enabled: bool,
    /// External stream sender for forwarding events to callers (API/WS/terminal).
    stream_tx: Option<StreamSender>,
    /// Whether to require approval for dangerous commands.
    approval_enabled: bool,
    /// Approval callback: returns true if approved, false if denied.
    approval_fn: Option<Box<dyn Fn(&str, &str) -> bool + Send + Sync>>,
    /// MCP registry for external tool servers.
    mcp_registry: Option<Arc<tokio::sync::Mutex<McpRegistry>>>,
    /// Sub-agent orchestrator.
    orchestrator: Option<Arc<AgentOrchestrator>>,
    /// Social media manager.
    social_manager: Option<Arc<tokio::sync::Mutex<SocialManager>>>,
    /// Learning: prompt evolution engine.
    prompt_evolution: Option<Arc<tokio::sync::Mutex<PromptEvolution>>>,
    /// Memory v2 store.
    memory_v2_store: Option<Arc<memory_v2::MemoryStore>>,
    /// Memory v2 retriever.
    memory_v2_retriever: Option<Arc<memory_v2::MemoryRetriever>>,
}

impl Agent {
    pub fn new(
        llm: Arc<dyn LlmProvider>,
        tools: ToolRegistry,
        skill_registry: SkillRegistry,
        system_prompt: String,
        max_context_messages: usize,
        max_iterations: usize,
        data_dir: std::path::PathBuf,
        smart_memory: Option<Arc<dyn MemoryProvider>>,
    ) -> Result<Self> {
        let memory = MemoryStore::load(&data_dir).unwrap_or_default();

        // Build full system prompt with legacy memory context (if no smart memory)
        let full_prompt = if smart_memory.is_some() {
            // Smart memory injects context dynamically per-message
            system_prompt
        } else {
            format!("{}{}", system_prompt, memory.context_string())
        };

        // Append skills overview to system prompt
        let skills_overview = skill_registry.build_skills_overview();
        let full_prompt = format!("{}{}", full_prompt, skills_overview);

        let context = ConversationContext::new(full_prompt, max_context_messages);

        Ok(Self {
            llm,
            tools,
            skill_registry,
            context,
            memory,
            smart_memory,
            data_dir,
            max_iterations,
            quiet_mode: false,
            message_count: 0,
            user_id: "default".to_string(),
            streaming_enabled: false,
            stream_tx: None,
            approval_enabled: false,
            approval_fn: None,
            mcp_registry: None,
            orchestrator: None,
            social_manager: None,
            prompt_evolution: None,
            memory_v2_store: None,
            memory_v2_retriever: None,
        })
    }

    /// Set the MCP registry for external tool access.
    pub fn set_mcp_registry(&mut self, registry: Arc<tokio::sync::Mutex<McpRegistry>>) {
        self.mcp_registry = Some(registry);
    }

    /// Set the sub-agent orchestrator.
    pub fn set_orchestrator(&mut self, orch: Arc<AgentOrchestrator>) {
        self.orchestrator = Some(orch);
    }

    /// Set the social media manager.
    pub fn set_social_manager(&mut self, sm: Arc<tokio::sync::Mutex<SocialManager>>) {
        self.social_manager = Some(sm);
    }

    /// Set the prompt evolution engine.
    pub fn set_prompt_evolution(&mut self, pe: Arc<tokio::sync::Mutex<PromptEvolution>>) {
        self.prompt_evolution = Some(pe);
    }

    /// Set the memory v2 system.
    pub fn set_memory_v2(&mut self, store: Arc<memory_v2::MemoryStore>, retriever: Arc<memory_v2::MemoryRetriever>) {
        self.memory_v2_store = Some(store);
        self.memory_v2_retriever = Some(retriever);
    }

    /// Enable streaming mode for LLM responses.
    pub fn set_streaming(&mut self, enabled: bool) {
        self.streaming_enabled = enabled && self.llm.supports_streaming();
    }

    /// Set an external stream sender for forwarding events.
    pub fn set_stream_sender(&mut self, tx: StreamSender) {
        self.stream_tx = Some(tx);
    }

    /// Clear the stream sender so the receiver side sees channel-closed.
    pub fn clear_stream_sender(&mut self) {
        self.stream_tx = None;
    }

    /// Enable the dangerous command approval system.
    pub fn set_approval_enabled(&mut self, enabled: bool) {
        self.approval_enabled = enabled;
    }

    /// Set a custom approval callback.
    pub fn set_approval_fn(&mut self, f: impl Fn(&str, &str) -> bool + Send + Sync + 'static) {
        self.approval_fn = Some(Box::new(f));
    }

    /// Enable quiet mode (suppress tool call output)
    pub fn set_quiet_mode(&mut self, quiet: bool) {
        self.quiet_mode = quiet;
    }

    /// Process a user message through the full agent loop:
    ///   user msg → LLM → [tool calls → LLM]* → final text response
    pub async fn chat(&mut self, user_input: &str) -> Result<String> {
        let result = self.chat_inner(user_input).await;
        // Always clear the stream sender so the receiver channel closes.
        // This is critical: callers (SSE handler, WS handler, terminal)
        // wait for the channel to close to know the stream is complete.
        self.stream_tx = None;
        result
    }

    /// Inner chat implementation. Separated so `chat()` can guarantee
    /// `stream_tx` cleanup regardless of how this returns (Ok, Err, or ?).
    async fn chat_inner(&mut self, user_input: &str) -> Result<String> {
        // Add user message to context
        self.context.push(Message::user(user_input));
        self.message_count += 1;

        // Inject matched skill instructions for this message
        let skill_prompt = self.skill_registry.build_matched_prompt(user_input, 2);
        if !skill_prompt.is_empty() {
            tracing::info!(
                "Injecting {} chars of skill instructions",
                skill_prompt.len()
            );
            self.context.push(Message::system(&skill_prompt));
        }

        // Inject learned rules from prompt evolution
        if let Some(ref pe) = self.prompt_evolution {
            match pe.lock().await.build_prompt_section() {
                Ok(section) if !section.is_empty() => {
                    tracing::info!("Injecting {} chars of learned rules", section.len());
                    self.context.push(Message::system(&section));
                }
                _ => {}
            }
        }

        // Dynamic context injection from memory v2 (preferred) or smart memory (legacy)
        if let Some(ref retriever) = self.memory_v2_retriever {
            match retriever.build_context(user_input, &self.user_id, 10, 2000).await {
                Ok(ctx) if !ctx.is_empty() => {
                    tracing::info!("Injecting {} chars of memory_v2 context", ctx.len());
                    self.context.set_dynamic_context(ctx);
                }
                Ok(_) => {}
                Err(e) => tracing::warn!("Memory v2 context build failed: {e}"),
            }
        } else if let Some(ref smart_mem) = self.smart_memory {
            match smart_mem.build_context(user_input, &self.user_id).await {
                Ok(ctx) if !ctx.is_empty() => {
                    tracing::info!(
                        "Injecting {} chars of dynamic context from smart memory",
                        ctx.len()
                    );
                    self.context.set_dynamic_context(ctx);
                }
                Ok(_) => {
                    tracing::info!("Smart memory returned empty context for this query");
                }
                Err(e) => {
                    tracing::warn!("Smart memory context build failed: {e}");
                }
            }
        }

        let mut iterations = 0;

        loop {
            iterations += 1;
            tracing::info!(
                "Agent loop iteration {}/{}",
                iterations,
                self.max_iterations
            );
            if iterations > self.max_iterations {
                let msg = "Reached maximum tool call iterations. Stopping.";
                tracing::warn!("{}", msg);
                return Ok(msg.to_string());
            }

            // Build messages and call LLM
            let messages = self.context.build_messages();
            let tool_defs = self.tools.definitions();

            tracing::debug!(
                "Sending {} messages to LLM with {} tool definitions",
                messages.len(),
                tool_defs.len()
            );

            // Use streaming if enabled and supported
            let response = if self.streaming_enabled {
                let (tx, _rx) = stream_channel();
                // If there's an external stream sender, use it; otherwise use a default
                let sender = self.stream_tx.clone().unwrap_or(tx);
                self.llm.chat_stream(&messages, &tool_defs, sender).await?
            } else {
                self.llm.chat(&messages, &tool_defs).await?
            };

            match response {
                LlmResponse::Text(text) => {
                    tracing::info!("LLM returned text response (iteration {})", iterations);
                    self.context.push(Message::assistant(&text));
                    self.post_response_tasks().await;
                    return Ok(text);
                }
                LlmResponse::TextWithThinking { text, thinking } => {
                    tracing::info!("LLM returned text with thinking (iteration {})", iterations);
                    if !self.quiet_mode && !thinking.is_empty() {
                        println!(
                            "\n  {} {}",
                            "💭".dimmed(),
                            format!(
                                "[Thinking] {}",
                                if thinking.len() > 200 {
                                    format!("{}...", &thinking[..200])
                                } else {
                                    thinking.clone()
                                }
                            )
                            .dimmed()
                        );
                    }
                    // Emit thinking event if streaming
                    if let Some(ref tx) = self.stream_tx {
                        let _ = tx.send(StreamEvent::Thinking { text: thinking });
                    }
                    self.context.push(Message::assistant(&text));
                    self.post_response_tasks().await;
                    return Ok(text);
                }
                LlmResponse::ToolCalls(calls) => {
                    tracing::info!(
                        "LLM requested {} tool call(s) on iteration {}: {}",
                        calls.len(),
                        iterations,
                        calls
                            .iter()
                            .map(|c| format!(
                                "{}({})",
                                c.name,
                                serde_json::to_string(&c.arguments).unwrap_or_default()
                            ))
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                    // Record the assistant's tool-call message in context
                    self.context
                        .push(Message::assistant_tool_calls(calls.clone()));

                    // Execute each tool call
                    for call in &calls {
                        // --- Dangerous command approval check ---
                        if self.approval_enabled {
                            let args_str =
                                serde_json::to_string(&call.arguments).unwrap_or_default();
                            if let Some(pattern) = Self::check_dangerous(&call.name, &args_str) {
                                if !self.quiet_mode {
                                    println!(
                                        "\n  {} {} {}",
                                        "⚠️".bright_yellow(),
                                        "Dangerous operation detected:".bright_red().bold(),
                                        format!("'{}' matches pattern '{}'", call.name, pattern)
                                            .bright_yellow()
                                    );
                                }

                                let approved = if let Some(ref approval) = self.approval_fn {
                                    approval(&call.name, &args_str)
                                } else {
                                    false // Default: deny if no approval callback
                                };

                                if !approved {
                                    let deny_msg = format!("Tool '{}' was blocked: dangerous operation requires approval", call.name);
                                    if !self.quiet_mode {
                                        println!("  {} {}", "🚫", deny_msg.bright_red());
                                    }
                                    // Emit tool result event for blocked tool
                                    if let Some(ref tx) = self.stream_tx {
                                        let _ = tx.send(StreamEvent::ToolResult {
                                            id: call.id.clone(),
                                            name: call.name.clone(),
                                            success: false,
                                            output: deny_msg.clone(),
                                        });
                                    }
                                    self.context.push(Message::tool_result(&call.id, &deny_msg));
                                    continue;
                                }
                            }
                        }

                        // Emit tool use start event for streaming consumers
                        if let Some(ref tx) = self.stream_tx {
                            let _ = tx.send(StreamEvent::ToolUseStart {
                                id: call.id.clone(),
                                name: call.name.clone(),
                            });
                        }

                        if !self.quiet_mode {
                            println!(
                                "\n  {} {} ({})",
                                "🔧".bright_yellow(),
                                format!("Calling tool: {}", call.name).bright_yellow(),
                                serde_json::to_string(&call.arguments)
                                    .unwrap_or_default()
                                    .dimmed()
                            );
                        }

                        let result =
                            match self.tools.execute(&call.name, call.arguments.clone()).await {
                                Ok(mut result) => {
                                    // --- Secret redaction ---
                                    result.output = Self::redact_secrets(&result.output);

                                    if !self.quiet_mode {
                                        let status = if result.success {
                                            "✅".to_string()
                                        } else {
                                            "❌".to_string()
                                        };

                                        // Safely truncate output respecting character boundaries
                                        let display_output = if result.output.chars().count() > 200
                                        {
                                            let truncated: String =
                                                result.output.chars().take(200).collect();
                                            format!("{}...", truncated)
                                        } else {
                                            result.output.clone()
                                        };

                                        println!("  {} {}", status, display_output.dimmed());
                                    }
                                    result
                                }
                                Err(e) => {
                                    let err_msg = format!("Tool error: {}", e);
                                    if !self.quiet_mode {
                                        println!("  {} {}", "❌", err_msg.bright_red());
                                    }
                                    crate::tools::ToolResult::err(err_msg)
                                }
                            };

                        // Emit tool result event for streaming consumers
                        if let Some(ref tx) = self.stream_tx {
                            let _ = tx.send(StreamEvent::ToolResult {
                                id: call.id.clone(),
                                name: call.name.clone(),
                                success: result.success,
                                output: result.output.clone(),
                            });
                        }

                        // Add tool result to context
                        self.context
                            .push(Message::tool_result(&call.id, &result.output));
                    }

                    // Continue the loop — LLM will process tool results
                }
            }
        }
    }

    /// Get the list of loaded tool names.
    pub fn tool_names(&self) -> Vec<String> {
        self.tools.names()
    }

    /// Register an additional tool after construction.
    pub fn register_tool(&mut self, tool: Box<dyn crate::tools::Tool>) {
        self.tools.register(tool);
    }

    /// Clear conversation context (start fresh).
    pub fn clear_context(&mut self) {
        self.context.clear();
    }

    /// Push a message into the conversation context (for session restoration).
    pub fn push_context_message(&mut self, msg: Message) {
        self.context.push(msg);
    }

    /// Run post-response housekeeping tasks (memory persistence, extraction, learning).
    async fn post_response_tasks(&mut self) {
        // Persist legacy memory periodically
        if self.context.len() % 10 == 0 {
            let _ = self.memory.save(&self.data_dir);
        }

        // Memory v2 extraction (preferred over smart memory extraction)
        if let (Some(ref store), Some(ref retriever)) = (&self.memory_v2_store, &self.memory_v2_retriever) {
            let interval = 5; // extract every 5 messages
            if self.message_count % interval == 0 {
                let recent = self.context.recent_messages(interval * 2);
                if !recent.is_empty() {
                    let store_clone = Arc::clone(store);
                    let llm_clone = Arc::clone(&self.llm);
                    let embeddings_clone = retriever.embeddings().cloned();
                    let uid = self.user_id.clone();
                    let msgs = recent.to_vec();
                    tokio::spawn(async move {
                        match memory_v2::MemoryExtractor::extract(llm_clone.as_ref(), &msgs, &uid).await {
                            Ok(mut units) if !units.is_empty() => {
                                let count = units.len();
                                // Generate embeddings for each unit if client available
                                if let Some(ref emb_client) = embeddings_clone {
                                    for unit in &mut units {
                                        match emb_client.embed(&unit.content).await {
                                            Ok(embedding) => unit.embedding = Some(embedding),
                                            Err(e) => tracing::debug!("Embedding generation failed for unit: {e}"),
                                        }
                                    }
                                }
                                // Store extracted units
                                for unit in &units {
                                    if let Err(e) = store_clone.insert(unit) {
                                        tracing::warn!("Failed to store memory unit: {e}");
                                    }
                                }
                                tracing::info!("Memory v2 extracted and stored {count} units");
                            }
                            Ok(_) => {} // no units extracted
                            Err(e) => tracing::warn!("Memory v2 extraction failed: {e}"),
                        }
                    });
                }
            }
        } else if let Some(ref smart_mem) = self.smart_memory {
            if smart_mem.auto_extract_enabled()
                && self.message_count % smart_mem.extraction_interval() == 0
            {
                let recent = self
                    .context
                    .recent_messages(smart_mem.extraction_interval() * 2);

                if !recent.is_empty() {
                    let sm = Arc::clone(smart_mem);
                    let uid = self.user_id.clone();
                    let msgs = recent.to_vec();
                    tokio::spawn(async move {
                        match sm.extract_and_store(&msgs, &uid).await {
                            Ok(n) => {
                                if n > 0 {
                                    tracing::info!("Background memory extraction stored {n} facts");
                                }
                            }
                            Err(e) => {
                                tracing::warn!("Background memory extraction failed: {e}");
                            }
                        }
                    });
                }
            }
        }

        // Learning hook: periodically prune low-performing rules
        if let Some(ref pe) = self.prompt_evolution {
            if self.message_count > 0 && self.message_count % 50 == 0 {
                let pe_guard = pe.lock().await;
                match pe_guard.prune_dead_rules() {
                    Ok(n) if n > 0 => tracing::info!("Pruned {n} underperforming learned rules"),
                    _ => {}
                }
            }
        }
    }

    /// Check if a tool call matches a dangerous pattern.
    /// Returns the matched pattern if dangerous, None if safe.
    fn check_dangerous(tool_name: &str, args: &str) -> Option<&'static str> {
        let combined = format!("{} {}", tool_name, args).to_lowercase();
        for pattern in DANGEROUS_PATTERNS {
            if combined.contains(&pattern.to_lowercase()) {
                return Some(pattern);
            }
        }
        None
    }

    /// Redact potential secrets/credentials from tool output.
    fn redact_secrets(output: &str) -> String {
        let mut result = output.to_string();
        for pattern in SECRET_PATTERNS {
            if let Some(pos) = result.to_lowercase().find(&pattern.to_lowercase()) {
                // Find the value after the pattern and redact it
                let start = pos + pattern.len();
                // Find end of the secret value (whitespace, quote, newline, comma, etc.)
                let end = result[start..]
                    .find(|c: char| {
                        c.is_whitespace()
                            || c == '"'
                            || c == '\''
                            || c == ','
                            || c == '}'
                            || c == ')'
                    })
                    .map(|e| start + e)
                    .unwrap_or(result.len());
                if end > start && (end - start) >= 8 {
                    // Only redact if the value is long enough to look like a secret
                    let prefix: String = result[start..].chars().take(4).collect();
                    result = format!(
                        "{}{}{}***REDACTED***{}",
                        &result[..pos],
                        pattern,
                        prefix,
                        &result[end..]
                    );
                }
            }
        }
        result
    }

    /// Get total conversation length.
    pub fn context_len(&self) -> usize {
        self.context.len()
    }

    /// Get the MCP registry (if set).
    pub fn mcp_registry(&self) -> Option<&Arc<tokio::sync::Mutex<McpRegistry>>> {
        self.mcp_registry.as_ref()
    }

    /// Get the sub-agent orchestrator (if set).
    pub fn orchestrator(&self) -> Option<&Arc<AgentOrchestrator>> {
        self.orchestrator.as_ref()
    }

    /// Get the social manager (if set).
    pub fn social_manager(&self) -> Option<&Arc<tokio::sync::Mutex<SocialManager>>> {
        self.social_manager.as_ref()
    }

    /// Get the prompt evolution engine (if set).
    pub fn prompt_evolution(&self) -> Option<&Arc<tokio::sync::Mutex<PromptEvolution>>> {
        self.prompt_evolution.as_ref()
    }

    /// Get the memory v2 store (if set).
    pub fn memory_v2_store(&self) -> Option<&Arc<memory_v2::MemoryStore>> {
        self.memory_v2_store.as_ref()
    }

    /// Get the memory v2 retriever (if set).
    pub fn memory_v2_retriever(&self) -> Option<&Arc<memory_v2::MemoryRetriever>> {
        self.memory_v2_retriever.as_ref()
    }
}
