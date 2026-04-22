# 04 — Sub-Agent System

## Objective

Implement a multi-agent orchestration system where the main agent can spawn isolated sub-agents for parallel tasks, background research, and specialized work. Each sub-agent gets its own session context, tool access, and can report results back.

---

## Current State

- **Single agent**: `src/agent.rs` — One agent handles all requests
- **No sub-agent spawning**: No way to delegate tasks to parallel workers
- **Background scheduler**: `src/scheduler.rs` — Exists for cron jobs but not agent tasks

---

## Reference Implementations

### OpenClaw (Primary — Sub-agent spawning)
- **Tool**: `sessions_spawn` — Creates isolated background agents
- **Pattern**: Parallel research + result announcements
- **Features**: Per-agent workspaces, separate auth, metadata persistence

### IronClaw (Secondary — Job system)
- **Tool**: `job` — Creates background LLM-driven executor
- **Features**: Full jobs (Docker worker), lightweight subtasks (tool exec), cancellation, self-repair
- **State tracking**: InProgress, Complete, Failed, Stuck with timestamps

### Claw Code (Agent tool)
- **Tool**: `Agent` — Launches sub-agent with metadata
- **Params**: description, prompt, subagent_type, name, model

---

## Architecture

### Sub-Agent Types

```rust
// File: src/agents/types.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SubAgentType {
    /// Short-lived task, returns result and terminates
    Task,
    /// Background worker, runs until completion or timeout
    Background,
    /// Specialized agent with specific tool access
    Specialist,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubAgentConfig {
    pub id: String,                         // UUID
    pub name: String,                       // Human-readable name
    pub agent_type: SubAgentType,
    pub prompt: String,                     // System prompt / instructions
    pub model: Option<String>,              // Override LLM model
    pub tools: Option<Vec<String>>,         // Restrict tool access (None = all)
    pub timeout_secs: Option<u64>,          // Max execution time
    pub parent_id: Option<String>,          // Parent agent ID (for nesting)
    pub metadata: HashMap<String, String>,  // Arbitrary metadata
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SubAgentStatus {
    Pending,
    Running,
    Completed { result: String },
    Failed { error: String },
    Cancelled,
    TimedOut,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubAgentState {
    pub config: SubAgentConfig,
    pub status: SubAgentStatus,
    pub created_at: String,
    pub updated_at: String,
    pub messages: Vec<Message>,     // Sub-agent's conversation history
}
```

### Module Structure

```
src/agents/
├── mod.rs              -- Public API, re-exports
├── types.rs            -- SubAgentConfig, SubAgentState, enums
├── orchestrator.rs     -- AgentOrchestrator: spawn, track, communicate
├── runner.rs           -- SubAgentRunner: execute sub-agent loop
└── tools.rs            -- Agent tools: spawn_agent, list_agents, get_result
```

---

## Implementation Steps

### Step 1: Define types and state tracking (Day 1 morning)

**File**: `src/agents/types.rs` — Structs defined above

**File**: `src/agents/mod.rs`:
```rust
pub mod types;
pub mod orchestrator;
pub mod runner;
pub mod tools;

pub use orchestrator::AgentOrchestrator;
pub use types::*;
```

### Step 2: Implement AgentOrchestrator (Day 1)

**File**: `src/agents/orchestrator.rs`

```rust
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct AgentOrchestrator {
    agents: Arc<RwLock<HashMap<String, SubAgentState>>>,
    /// Handles for spawned tasks, keyed by agent ID
    handles: Arc<RwLock<HashMap<String, tokio::task::JoinHandle<()>>>>,
    max_concurrent: usize,
    config: Arc<AppConfig>,
    llm: Arc<dyn LlmProvider>,
    tools: Arc<ToolRegistry>,
    memory: Arc<MemoryStore>,
}

impl AgentOrchestrator {
    pub fn new(config: Arc<AppConfig>, llm: Arc<dyn LlmProvider>, tools: Arc<ToolRegistry>, memory: Arc<MemoryStore>) -> Self {
        Self {
            agents: Arc::new(RwLock::new(HashMap::new())),
            handles: Arc::new(RwLock::new(HashMap::new())),
            max_concurrent: 5,
            config, llm, tools, memory,
        }
    }

    /// Spawn a new sub-agent. Returns the agent ID.
    pub async fn spawn(&self, config: SubAgentConfig) -> Result<String> {
        let id = config.id.clone();

        // Check concurrent limit
        let running = self.agents.read().await
            .values()
            .filter(|a| matches!(a.status, SubAgentStatus::Running))
            .count();
        if running >= self.max_concurrent {
            anyhow::bail!("Maximum concurrent sub-agents ({}) reached", self.max_concurrent);
        }

        // Create state
        let state = SubAgentState {
            config: config.clone(),
            status: SubAgentStatus::Pending,
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
            messages: vec![],
        };
        self.agents.write().await.insert(id.clone(), state);

        // Spawn runner task
        let runner = SubAgentRunner::new(
            config.clone(),
            self.llm.clone(),
            self.tools.clone(),
            self.memory.clone(),
            self.agents.clone(),
        );

        let handle = tokio::spawn(async move {
            runner.run().await;
        });

        self.handles.write().await.insert(id.clone(), handle);
        Ok(id)
    }

    /// Get status of a sub-agent
    pub async fn status(&self, id: &str) -> Option<SubAgentStatus> {
        self.agents.read().await.get(id).map(|a| a.status.clone())
    }

    /// Get result of a completed sub-agent
    pub async fn get_result(&self, id: &str) -> Option<String> {
        let agents = self.agents.read().await;
        match agents.get(id)?.status {
            SubAgentStatus::Completed { ref result } => Some(result.clone()),
            _ => None,
        }
    }

    /// Cancel a running sub-agent
    pub async fn cancel(&self, id: &str) -> Result<()> {
        if let Some(handle) = self.handles.write().await.remove(id) {
            handle.abort();
        }
        if let Some(agent) = self.agents.write().await.get_mut(id) {
            agent.status = SubAgentStatus::Cancelled;
            agent.updated_at = chrono::Utc::now().to_rfc3339();
        }
        Ok(())
    }

    /// List all sub-agents
    pub async fn list(&self) -> Vec<SubAgentState> {
        self.agents.read().await.values().cloned().collect()
    }

    /// Wait for a sub-agent to complete (with timeout)
    pub async fn wait_for(&self, id: &str, timeout: Duration) -> Result<SubAgentStatus> {
        let start = Instant::now();
        loop {
            if let Some(status) = self.status(id).await {
                match status {
                    SubAgentStatus::Completed { .. } |
                    SubAgentStatus::Failed { .. } |
                    SubAgentStatus::Cancelled |
                    SubAgentStatus::TimedOut => return Ok(status),
                    _ => {}
                }
            }
            if start.elapsed() > timeout {
                self.cancel(id).await?;
                return Ok(SubAgentStatus::TimedOut);
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }
}
```

### Step 3: Implement SubAgentRunner (Day 2 morning)

**File**: `src/agents/runner.rs`

```rust
pub struct SubAgentRunner {
    config: SubAgentConfig,
    llm: Arc<dyn LlmProvider>,
    tools: Arc<ToolRegistry>,
    memory: Arc<MemoryStore>,
    agents: Arc<RwLock<HashMap<String, SubAgentState>>>,
}

impl SubAgentRunner {
    pub async fn run(&self) {
        // Update status to Running
        self.update_status(SubAgentStatus::Running).await;

        // Build system prompt for sub-agent
        let system_prompt = format!(
            "You are a sub-agent named '{}'. Your task:\n{}\n\n\
             Complete the task and provide a clear, concise result.\
             You have access to the following tools: {}",
            self.config.name,
            self.config.prompt,
            self.available_tools_description(),
        );

        // Run agent loop (similar to main agent but isolated)
        let mut messages = vec![Message::system(&system_prompt)];
        messages.push(Message::user(&self.config.prompt));

        let max_iterations = 15;
        for _ in 0..max_iterations {
            // Call LLM
            let response = match self.llm.chat(&messages, &self.available_tools()).await {
                Ok(r) => r,
                Err(e) => {
                    self.update_status(SubAgentStatus::Failed {
                        error: e.to_string(),
                    }).await;
                    return;
                }
            };

            messages.push(Message::assistant(&response.content));

            // Check for tool calls
            if response.tool_calls.is_empty() {
                // No more tools → agent is done
                self.update_status(SubAgentStatus::Completed {
                    result: response.content.clone(),
                }).await;
                return;
            }

            // Execute tool calls
            for tool_call in &response.tool_calls {
                let result = self.tools.execute(&tool_call.name, &tool_call.arguments).await;
                messages.push(Message::tool_result(&tool_call.id, &result));
            }
        }

        // Max iterations reached
        let last_content = messages.last()
            .map(|m| m.content.clone())
            .unwrap_or_default();
        self.update_status(SubAgentStatus::Completed { result: last_content }).await;
    }

    fn available_tools(&self) -> Vec<ToolDefinition> {
        match &self.config.tools {
            Some(allowed) => self.tools.get_definitions()
                .into_iter()
                .filter(|t| allowed.contains(&t.name))
                .collect(),
            None => self.tools.get_definitions(),
        }
    }

    async fn update_status(&self, status: SubAgentStatus) {
        if let Some(agent) = self.agents.write().await.get_mut(&self.config.id) {
            agent.status = status;
            agent.updated_at = chrono::Utc::now().to_rfc3339();
        }
    }
}
```

### Step 4: Implement agent tools (Day 2 afternoon)

**File**: `src/agents/tools.rs`

Register these as tools the main agent can call:

```rust
/// Tool: spawn_agent
/// Spawns a sub-agent to handle a delegated task
pub struct SpawnAgentTool;

// Input schema:
// {
//   "name": "researcher",
//   "prompt": "Research the latest AI safety papers from 2024",
//   "type": "task",           // task | background | specialist
//   "model": "gpt-4o-mini",  // optional, cheaper model for simple tasks
//   "tools": ["web_search", "web_fetch"],  // optional tool restriction
//   "timeout_secs": 300       // optional timeout
// }
// Output: { "agent_id": "uuid", "status": "running" }

/// Tool: list_agents
/// Lists all sub-agents and their statuses
pub struct ListAgentsTool;

// Output: [{ "id": "...", "name": "...", "status": "running", "created_at": "..." }]

/// Tool: get_agent_result
/// Gets the result of a completed sub-agent
pub struct GetAgentResultTool;

// Input: { "agent_id": "uuid" }
// Output: { "status": "completed", "result": "..." }

/// Tool: cancel_agent
/// Cancels a running sub-agent
pub struct CancelAgentTool;

// Input: { "agent_id": "uuid" }
// Output: { "status": "cancelled" }

/// Tool: wait_for_agent
/// Waits for a sub-agent to complete (blocks)
pub struct WaitForAgentTool;

// Input: { "agent_id": "uuid", "timeout_secs": 60 }
// Output: { "status": "completed", "result": "..." }
```

### Step 5: Wire into main agent (Day 2)

**File**: Modify `src/agent.rs`

```rust
pub struct Agent {
    // ... existing fields
    pub orchestrator: Arc<AgentOrchestrator>,
}

impl Agent {
    pub fn new(config: AppConfig, llm: Arc<dyn LlmProvider>, tools: Arc<ToolRegistry>, memory: Arc<MemoryStore>) -> Self {
        let orchestrator = Arc::new(AgentOrchestrator::new(
            Arc::new(config.clone()),
            llm.clone(),
            tools.clone(),
            memory.clone(),
        ));
        // Register sub-agent tools
        tools.register(SpawnAgentTool::new(orchestrator.clone()));
        tools.register(ListAgentsTool::new(orchestrator.clone()));
        tools.register(GetAgentResultTool::new(orchestrator.clone()));
        tools.register(CancelAgentTool::new(orchestrator.clone()));
        tools.register(WaitForAgentTool::new(orchestrator.clone()));
        // ...
    }
}
```

### Step 6: Add API endpoints (Day 3)

```
GET    /api/agents/list              # List all sub-agents
POST   /api/agents/spawn             # Spawn new sub-agent
GET    /api/agents/:id               # Get agent status
GET    /api/agents/:id/result        # Get agent result
POST   /api/agents/:id/cancel        # Cancel agent
DELETE /api/agents/:id               # Remove agent record
```

### Step 7: Add CLI commands (Day 3)

```
pylot agents list                    # List all sub-agents
pylot agents spawn "Research X"      # Spawn from CLI
pylot agents status <id>             # Check status
pylot agents result <id>             # Get result
pylot agents cancel <id>             # Cancel
```

---

## Config Additions

```toml
[agents]
max_concurrent = 5              # Maximum parallel sub-agents
default_timeout_secs = 300      # 5 minute default timeout
default_model = "gpt-4o-mini"   # Cheaper model for sub-agents
```

---

## Testing

- `test_spawn_and_complete` — Sub-agent runs and returns result
- `test_concurrent_limit` — Reject when max reached
- `test_cancellation` — Cancel running sub-agent
- `test_timeout` — Sub-agent times out
- `test_tool_restriction` — Sub-agent only sees allowed tools
- `test_parent_child` — Nested sub-agent spawning
- `test_result_retrieval` — Get result after completion
- `test_status_tracking` — Status transitions are correct

---

## Acceptance Criteria

- [ ] Main agent can spawn sub-agents via `spawn_agent` tool
- [ ] Sub-agents run in isolated async tasks with own context
- [ ] Sub-agents can use tools (with optional restriction)
- [ ] Results reported back to main agent
- [ ] Concurrent limit enforced
- [ ] Timeout kills long-running sub-agents
- [ ] CLI and API management endpoints work
- [ ] Status tracking accurate through all states
