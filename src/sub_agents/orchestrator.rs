use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use anyhow::Result;

use super::store::SubAgentStore;
use super::types::*;
use crate::agent::Agent;
use crate::api::ConversationStore;
use crate::llm::LlmProvider;
use crate::skills::SkillRegistry;
use crate::tools::ToolRegistry;

/// Orchestrates multiple sub-agents: spawn, track, cancel, collect results.
pub struct AgentOrchestrator {
    agents: Arc<Mutex<HashMap<String, SubAgentState>>>,
    max_concurrent: usize,
    handles: Arc<Mutex<HashMap<String, tokio::task::JoinHandle<()>>>>,
    store: Option<Arc<SubAgentStore>>,
    conversations: Option<Arc<ConversationStore>>,
}

impl AgentOrchestrator {
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            agents: Arc::new(Mutex::new(HashMap::new())),
            max_concurrent,
            handles: Arc::new(Mutex::new(HashMap::new())),
            store: None,
            conversations: None,
        }
    }

    /// Attach a persistent store for sub-agent state.
    pub fn set_store(&mut self, store: Arc<SubAgentStore>) {
        self.store = Some(store);
    }

    /// Attach a conversation store to inject results back into chat.
    pub fn set_conversations(&mut self, conversations: Arc<ConversationStore>) {
        self.conversations = Some(conversations);
    }

    /// Spawn a new sub-agent with the given config and task message.
    pub async fn spawn(
        &self,
        config: SubAgentConfig,
        task_message: String,
        llm: Arc<dyn LlmProvider>,
        tools: ToolRegistry,
        skill_registry: SkillRegistry,
        data_dir: std::path::PathBuf,
        conversation_id: Option<String>,
    ) -> Result<String> {
        let agents = self.agents.lock().await;
        let running_count = agents
            .values()
            .filter(|s| s.status == SubAgentStatus::Running)
            .count();
        if running_count >= self.max_concurrent {
            anyhow::bail!(
                "Maximum concurrent sub-agents reached ({}/{})",
                running_count,
                self.max_concurrent
            );
        }
        drop(agents);

        let id = config.id.clone();
        let state = SubAgentState::new(config.clone());

        self.agents.lock().await.insert(id.clone(), state);

        // Persist to SQLite
        if let Some(ref store) = self.store {
            if let Err(e) = store.insert(&config, &task_message, conversation_id.as_deref()) {
                tracing::warn!("Failed to persist sub-agent to SQLite: {e}");
            }
        }

        let agents_ref = Arc::clone(&self.agents);
        let agent_id = id.clone();
        let timeout = config.timeout_secs;
        let max_iters = config.max_iterations;
        let store_ref = self.store.clone();
        let conversations_ref = self.conversations.clone();
        let agent_name = config.name.clone();
        let conv_id = conversation_id.clone();

        let handle = tokio::spawn(async move {
            // Mark as running
            let started_at = chrono::Utc::now().to_rfc3339();
            {
                let mut agents = agents_ref.lock().await;
                if let Some(state) = agents.get_mut(&agent_id) {
                    state.status = SubAgentStatus::Running;
                    state.started_at = Some(started_at.clone());
                }
            }
            if let Some(ref store) = store_ref {
                let _ = store.update_status(&agent_id, "Running", Some(&started_at), None);
            }

            let result = tokio::time::timeout(
                std::time::Duration::from_secs(timeout),
                run_sub_agent(
                    llm,
                    tools,
                    skill_registry,
                    config,
                    task_message,
                    data_dir,
                    max_iters,
                ),
            )
            .await;

            let mut agents = agents_ref.lock().await;
            if let Some(state) = agents.get_mut(&agent_id) {
                state.completed_at = Some(chrono::Utc::now().to_rfc3339());
                match &result {
                    Ok(Ok(output)) => {
                        state.status = SubAgentStatus::Completed;
                        state.result = Some(output.clone());
                    }
                    Ok(Err(e)) => {
                        state.status = SubAgentStatus::Failed;
                        state.error = Some(e.to_string());
                    }
                    Err(_) => {
                        state.status = SubAgentStatus::TimedOut;
                        state.error = Some("Sub-agent timed out".into());
                    }
                }
            }
            drop(agents);

            // Persist final state to SQLite
            match &result {
                Ok(Ok(output)) => {
                    // Always record the run (even one-shot agents get a Run #1
                    // entry in the panel).
                    let run_number = if let Some(ref store) = store_ref {
                        let _ = store.set_result(&agent_id, &output);
                        store.append_run(&agent_id, &output).unwrap_or(1)
                    } else {
                        1
                    };

                    // Post the FULL output to chat so the user can read it
                    // directly without switching panels.
                    if let (Some(ref convos), Some(ref cid)) = (&conversations_ref, &conv_id) {
                        convos.add_message(
                            cid,
                            crate::api::StoredMessage {
                                id: uuid::Uuid::new_v4().to_string(),
                                role: "assistant".into(),
                                content: format!(
                                    "✅ **{agent_name}** (Run #{run_number}) completed:\n\n{output}"
                                ),
                                timestamp: chrono::Utc::now().to_rfc3339(),
                            },
                        );
                        tracing::info!(
                            "Sub-agent '{}' run #{} completed; full output posted to conversation {}",
                            agent_name,
                            run_number,
                            cid
                        );
                    }
                }
                Ok(Err(e)) => {
                    if let Some(ref store) = store_ref {
                        let _ = store.set_error(&agent_id, &e.to_string(), "Failed");
                    }
                    if let (Some(ref convos), Some(ref cid)) = (&conversations_ref, &conv_id) {
                        convos.add_message(
                            cid,
                            crate::api::StoredMessage {
                                id: uuid::Uuid::new_v4().to_string(),
                                role: "assistant".into(),
                                content: format!(
                                    "❌ {agent_name} failed — see Sub-Agent panel ({e})"
                                ),
                                timestamp: chrono::Utc::now().to_rfc3339(),
                            },
                        );
                    }
                }
                Err(_) => {
                    if let Some(ref store) = store_ref {
                        let _ = store.set_error(&agent_id, "Sub-agent timed out", "TimedOut");
                    }
                    if let (Some(ref convos), Some(ref cid)) = (&conversations_ref, &conv_id) {
                        convos.add_message(
                            cid,
                            crate::api::StoredMessage {
                                id: uuid::Uuid::new_v4().to_string(),
                                role: "assistant".into(),
                                content: format!("⏰ {agent_name} timed out — see Sub-Agent panel"),
                                timestamp: chrono::Utc::now().to_rfc3339(),
                            },
                        );
                    }
                }
            }
        });

        self.handles.lock().await.insert(id.clone(), handle);
        Ok(id)
    }

    /// Spawn a **recurring** sub-agent that re-runs the same task every
    /// `interval_secs` seconds, sending each iteration's result back into the
    /// originating conversation.
    ///
    /// The task loops forever inside a single `tokio::spawn`. The JoinHandle
    /// is stored in `self.handles`, so dropping the orchestrator (or calling
    /// `cancel(id)` / `cancel_by_name(name)`) is the only way to stop it.
    ///
    /// `tools_factory` is invoked once per iteration so each run gets its own
    /// fresh `ToolRegistry` + `SkillRegistry` (neither type is `Clone`).
    pub async fn spawn_recurring<F>(
        &self,
        mut config: SubAgentConfig,
        task_message: String,
        llm: Arc<dyn LlmProvider>,
        tools_factory: F,
        data_dir: std::path::PathBuf,
        conversation_id: Option<String>,
        interval_secs: u64,
    ) -> Result<String>
    where
        F: Fn() -> (ToolRegistry, SkillRegistry) + Send + Sync + 'static,
    {
        if interval_secs == 0 {
            anyhow::bail!("Recurring sub-agent interval must be > 0 seconds");
        }
        config.interval_secs = Some(interval_secs);

        // Concurrency check (same rule as one-shot spawn).
        let agents = self.agents.lock().await;
        let running_count = agents
            .values()
            .filter(|s| s.status == SubAgentStatus::Running)
            .count();
        if running_count >= self.max_concurrent {
            anyhow::bail!(
                "Maximum concurrent sub-agents reached ({}/{})",
                running_count,
                self.max_concurrent
            );
        }
        drop(agents);

        let id = config.id.clone();
        let state = SubAgentState::new(config.clone());
        self.agents.lock().await.insert(id.clone(), state);

        if let Some(ref store) = self.store {
            if let Err(e) = store.insert(&config, &task_message, conversation_id.as_deref()) {
                tracing::warn!("Failed to persist recurring sub-agent to SQLite: {e}");
            }
        }

        let agents_ref = Arc::clone(&self.agents);
        let agent_id = id.clone();
        let timeout = config.timeout_secs;
        let max_iters = config.max_iterations;
        let store_ref = self.store.clone();
        let conversations_ref = self.conversations.clone();
        let agent_name = config.name.clone();
        let conv_id = conversation_id.clone();
        let factory = Arc::new(tools_factory);

        let handle = tokio::spawn(async move {
            let started_at = chrono::Utc::now().to_rfc3339();
            {
                let mut agents = agents_ref.lock().await;
                if let Some(state) = agents.get_mut(&agent_id) {
                    state.status = SubAgentStatus::Running;
                    state.started_at = Some(started_at.clone());
                }
            }
            if let Some(ref store) = store_ref {
                let _ = store.update_status(&agent_id, "Running", Some(&started_at), None);
            }

            // First tick fires immediately — the user sees update #1 right away,
            // then subsequent ticks are spaced by `interval_secs`.
            let mut ticker = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
            let mut iteration: u64 = 0;

            tracing::info!(
                "Recurring sub-agent '{}' (id={}) loop started: interval={}s, timeout={}s, max_iters={}, conv_id={:?}",
                agent_name, agent_id, interval_secs, timeout, max_iters, conv_id
            );

            loop {
                tracing::debug!(
                    "Recurring sub-agent '{}' awaiting next tick (iter so far: {})",
                    agent_name,
                    iteration
                );
                ticker.tick().await;
                iteration += 1;
                tracing::info!(
                    "Recurring sub-agent '{}' tick #{} fired — starting iteration",
                    agent_name,
                    iteration
                );

                // Build a fresh tools+skills set for this iteration.
                let (tools, skills) = factory();
                let cfg = config.clone();
                let llm_clone = Arc::clone(&llm);
                let task = task_message.clone();
                let dd = data_dir.clone();

                let res = tokio::time::timeout(
                    std::time::Duration::from_secs(timeout),
                    run_sub_agent(llm_clone, tools, skills, cfg, task, dd, max_iters),
                )
                .await;

                match res {
                    Ok(Ok(output)) => {
                        tracing::info!(
                            "Recurring sub-agent '{}' iteration #{} completed ({} chars)",
                            agent_name,
                            iteration,
                            output.len()
                        );

                        // Update last result; status stays Running for recurring agents.
                        {
                            let mut agents = agents_ref.lock().await;
                            if let Some(state) = agents.get_mut(&agent_id) {
                                state.result = Some(output.clone());
                                state.completed_at = Some(chrono::Utc::now().to_rfc3339());
                            }
                        }

                        // Persist the run to its own history row, then post the
                        // full output to chat so the user sees it immediately.
                        let run_number = if let Some(ref store) = store_ref {
                            store
                                .append_run(&agent_id, &output)
                                .unwrap_or(iteration as i64)
                        } else {
                            iteration as i64
                        };

                        if let (Some(ref convos), Some(ref cid)) = (&conversations_ref, &conv_id) {
                            convos.add_message(
                                cid,
                                crate::api::StoredMessage {
                                    id: uuid::Uuid::new_v4().to_string(),
                                    role: "assistant".into(),
                                    content: format!(
                                        "✅ **{agent_name}** (Run #{run_number}) completed:\n\n{output}"
                                    ),
                                    timestamp: chrono::Utc::now().to_rfc3339(),
                                },
                            );
                        }
                    }
                    Ok(Err(e)) => {
                        tracing::warn!(
                            "Recurring sub-agent '{}' iteration #{} failed: {}",
                            agent_name,
                            iteration,
                            e
                        );
                        // Don't break the loop on transient errors; surface and continue.
                        if let (Some(ref convos), Some(ref cid)) = (&conversations_ref, &conv_id) {
                            convos.add_message(
                                cid,
                                crate::api::StoredMessage {
                                    id: uuid::Uuid::new_v4().to_string(),
                                    role: "assistant".into(),
                                    content: format!(
                                        "⚠️ {agent_name} Run #{iteration} failed — see Sub-Agent panel ({e})"
                                    ),
                                    timestamp: chrono::Utc::now().to_rfc3339(),
                                },
                            );
                        }
                    }
                    Err(_) => {
                        tracing::warn!(
                            "Recurring sub-agent '{}' iteration #{} timed out after {}s",
                            agent_name,
                            iteration,
                            timeout
                        );
                    }
                }
                tracing::debug!(
                    "Recurring sub-agent '{}' end of iteration #{} — next tick in ~{}s",
                    agent_name,
                    iteration,
                    interval_secs
                );
                // Loop continues; only `handle.abort()` (cancel) ends it.
            }
        });

        self.handles.lock().await.insert(id.clone(), handle);
        Ok(id)
    }

    /// Cancel any running sub-agent(s) whose `name` matches (case-insensitive).
    /// Returns the list of cancelled agent ids. Used to power "stop updates"
    /// requests from the user (`StopRecurringSubAgentTool`).
    pub async fn cancel_by_name(&self, name: &str) -> Result<Vec<String>> {
        let needle = name.trim().to_lowercase();
        let ids: Vec<String> = self
            .agents
            .lock()
            .await
            .iter()
            .filter(|(_, s)| {
                s.config.name.to_lowercase().contains(&needle)
                    && matches!(s.status, SubAgentStatus::Running | SubAgentStatus::Pending)
            })
            .map(|(id, _)| id.clone())
            .collect();

        for id in &ids {
            self.cancel(id).await?;
        }
        Ok(ids)
    }

    /// Get the current status of a sub-agent.
    pub async fn status(&self, id: &str) -> Option<SubAgentStatus> {
        self.agents.lock().await.get(id).map(|s| s.status.clone())
    }

    /// Get the full state of a sub-agent.
    pub async fn get_state(&self, id: &str) -> Option<SubAgentState> {
        self.agents.lock().await.get(id).cloned()
    }

    /// Get the result of a completed sub-agent.
    pub async fn get_result(&self, id: &str) -> Option<String> {
        self.agents
            .lock()
            .await
            .get(id)
            .and_then(|s| s.result.clone())
    }

    /// Cancel a running sub-agent.
    pub async fn cancel(&self, id: &str) -> Result<()> {
        if let Some(handle) = self.handles.lock().await.remove(id) {
            handle.abort();
        }
        if let Some(state) = self.agents.lock().await.get_mut(id) {
            state.status = SubAgentStatus::Cancelled;
            state.completed_at = Some(chrono::Utc::now().to_rfc3339());
        }
        if let Some(ref store) = self.store {
            let _ = store.set_error(id, "Cancelled by user", "Cancelled");
        }
        Ok(())
    }

    /// List all sub-agents (in-memory active + persisted history).
    pub async fn list(&self) -> Vec<SubAgentState> {
        let in_memory: Vec<SubAgentState> = self.agents.lock().await.values().cloned().collect();

        // If we have a store, also load persisted agents not in memory
        if let Some(ref store) = self.store {
            if let Ok(persisted) = store.list(100) {
                let in_memory_ids: std::collections::HashSet<String> =
                    in_memory.iter().map(|a| a.config.id.clone()).collect();

                let mut all = in_memory;
                for p in persisted {
                    if !in_memory_ids.contains(&p.id) {
                        // Convert StoredSubAgent → SubAgentState for display
                        all.push(SubAgentState {
                            config: SubAgentConfig {
                                id: p.id,
                                name: p.name,
                                interval_secs: p.interval_secs,
                                ..Default::default()
                            },
                            status: match p.status.as_str() {
                                "Running" => SubAgentStatus::Running,
                                "Completed" => SubAgentStatus::Completed,
                                "Failed" => SubAgentStatus::Failed,
                                "Cancelled" => SubAgentStatus::Cancelled,
                                "TimedOut" => SubAgentStatus::TimedOut,
                                _ => SubAgentStatus::Pending,
                            },
                            result: p.result,
                            error: p.error,
                            messages: Vec::new(),
                            started_at: p.started_at,
                            completed_at: p.completed_at,
                        });
                    }
                }
                return all;
            }
        }

        in_memory
    }

    /// Wait for a specific sub-agent to complete.
    pub async fn wait_for(&self, id: &str) -> Result<String> {
        loop {
            let state = self.agents.lock().await.get(id).cloned();
            match state {
                Some(s) if s.status == SubAgentStatus::Completed => {
                    return Ok(s.result.unwrap_or_default());
                }
                Some(s) if s.status == SubAgentStatus::Failed => {
                    anyhow::bail!("Sub-agent failed: {}", s.error.unwrap_or_default());
                }
                Some(s) if s.status == SubAgentStatus::TimedOut => {
                    anyhow::bail!("Sub-agent timed out");
                }
                Some(s) if s.status == SubAgentStatus::Cancelled => {
                    anyhow::bail!("Sub-agent was cancelled");
                }
                None => {
                    anyhow::bail!("Sub-agent not found: {}", id);
                }
                _ => {
                    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                }
            }
        }
    }
}

/// Execute a sub-agent's task in isolation.
async fn run_sub_agent(
    llm: Arc<dyn LlmProvider>,
    tools: ToolRegistry,
    skill_registry: SkillRegistry,
    config: SubAgentConfig,
    task_message: String,
    data_dir: std::path::PathBuf,
    max_iterations: usize,
) -> Result<String> {
    let system_prompt = if config.system_prompt.is_empty() {
        format!(
            "You are a sub-agent named '{name}'. Complete the assigned task thoroughly.\n\n\
             CRITICAL OUTPUT RULES — you MUST follow these every single run:\n\
             1. Do all your work (research, writing, analysis, etc.) first.\n\
             2. Your FINAL response MUST contain the COMPLETE, FULL content you produced.\n\
             3. Do NOT just say \"I created a note\" or \"I saved the result\" — \
                reproduce the ENTIRE output in your reply so it appears in the Run History panel.\n\
             4. If you wrote an article, include the full article text.\n\
             5. If you researched a topic, include all findings, facts, and sources.\n\
             6. Tools like create_note or write_file are optional side-effects. \
                The run result is ALWAYS your final text reply — make it complete.",
            name = config.name
        )
    } else {
        config.system_prompt
    };

    let mut agent = Agent::new(
        llm,
        tools,
        skill_registry,
        system_prompt,
        50, // sub-agents get smaller context window
        max_iterations,
        data_dir,
        None, // no smart memory for sub-agents
    )?;
    agent.set_quiet_mode(true);

    // Wrap the task so the agent always ends with the full content in its reply,
    // not just a tool-call confirmation.
    let wrapped_task = format!(
        "{task_message}\n\n\
         REMEMBER: Your final reply must contain the COMPLETE output (full article, \
         full research findings, full analysis, etc.) — not just \"Done\" or \
         \"I created a note\". The run panel shows your final text, so include everything."
    );

    agent.chat(&wrapped_task).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sub_agent_state_creation() {
        let config = SubAgentConfig {
            name: "test-agent".into(),
            ..Default::default()
        };
        let state = SubAgentState::new(config);
        assert_eq!(state.status, SubAgentStatus::Pending);
        assert!(state.result.is_none());
    }

    #[tokio::test]
    async fn test_orchestrator_max_concurrent() {
        let orch = AgentOrchestrator::new(0); // max 0
        let agents = orch.agents.lock().await;
        assert!(agents.is_empty());
    }

    #[tokio::test]
    async fn test_orchestrator_list_empty() {
        let orch = AgentOrchestrator::new(5);
        assert!(orch.list().await.is_empty());
    }
}
