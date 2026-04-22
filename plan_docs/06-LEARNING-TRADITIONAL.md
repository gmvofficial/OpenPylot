# 06 — Traditional Learning Approaches

## Objective

Implement non-RL learning mechanisms: prompt evolution (auto-append rules from experience), skill extraction (capture reusable patterns from successful multi-step tasks), and conversation insights (extract user preferences and workflow patterns). These provide immediate learning without requiring GPU/cloud backends.

---

## Current State

- **Static behavior**: Agent uses fixed system prompt, no adaptation
- **Memory extracts facts** but doesn't learn behavioral patterns
- **No feedback loop**: No mechanism to improve from past interactions

---

## Reference Implementations

### IronClaw (Primary — Self-Improvement System)
- **Path**: `extra_repos/ironclaw-staging/src/`
- **Level 1**: Prompt Evolution — runtime overlay appends rules (capped 4000 chars)
- **Level 1.5**: Orchestrator Patching — versioned code with auto-rollback (3-failure threshold)
- **Skill Extraction**: Captures reusable CodeAct snippets after 5+ steps, 3+ tools
- **Conversation Insights**: Extracts preferences + workflow patterns every 5 completed threads
- **Error Diagnosis**: Categorizes issues (tool_error, prompt_error, code_error)

### MetaClaw (Secondary — Skill Evolution)
- **Path**: `extra_repos/MetaClaw-main/metaclaw/skills/`
- **Auto-evolve**: Failed samples → LLM generates new skills
- **Generation versioning**: MAML-style to prevent stale data pollution

---

## Architecture

### Module Structure

```
src/learning/
├── mod.rs                  -- Public API, LearningEngine
├── prompt_evolution.rs     -- Dynamic prompt rules from experience
├── skill_extraction.rs     -- Extract reusable patterns from successful tasks
├── conversation_insights.rs -- Extract user preferences & workflows
├── error_diagnosis.rs      -- Categorize and learn from errors
└── feedback.rs             -- Explicit user feedback collection
```

### Learning Engine

```rust
// File: src/learning/mod.rs

pub struct LearningEngine {
    pub prompt_evolution: PromptEvolution,
    pub skill_extractor: SkillExtractor,
    pub insight_extractor: ConversationInsightExtractor,
    pub error_diagnoser: ErrorDiagnoser,
    pub feedback_collector: FeedbackCollector,
}

impl LearningEngine {
    /// Run after each conversation turn (lightweight check)
    pub async fn on_turn_complete(&self, turn: &ConversationTurn) -> Result<()>;

    /// Run after a full conversation/thread completes
    pub async fn on_thread_complete(&self, thread: &ConversationThread) -> Result<()>;

    /// Run when an error occurs
    pub async fn on_error(&self, error: &AgentError, context: &ErrorContext) -> Result<()>;

    /// Process explicit user feedback
    pub async fn on_feedback(&self, feedback: &UserFeedback) -> Result<()>;
}
```

---

## Implementation Steps

### Step 1: Prompt Evolution (Day 1 morning)

**File**: `src/learning/prompt_evolution.rs`

The agent accumulates "rules" from experience that get appended to its system prompt.

```rust
use std::path::PathBuf;

pub struct PromptEvolution {
    rules_path: PathBuf,               // ~/.pylot/learned_rules.json
    rules: RwLock<Vec<LearnedRule>>,
    max_total_chars: usize,            // 4000 (prevents prompt bloat)
    llm: Arc<dyn LlmProvider>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearnedRule {
    pub id: String,
    pub rule: String,                   // e.g., "Always check file exists before reading"
    pub source: RuleSource,
    pub confidence: f64,                // 0.0-1.0, increases with validation
    pub created_at: String,
    pub applied_count: u32,
    pub failure_count: u32,             // If rule causes failures, decay confidence
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RuleSource {
    ErrorRecovery,          // Generated from repeated errors
    UserFeedback,           // Generated from explicit feedback
    SuccessPattern,         // Generated from successful multi-step tasks
    AutoDiagnosis,          // Generated from error diagnosis
}

impl PromptEvolution {
    /// Generate rules from a completed thread with issues
    pub async fn evolve_from_thread(&self, thread: &ConversationThread) -> Result<Vec<LearnedRule>> {
        // Only trigger if thread had errors or user corrections
        if !thread.had_issues() { return Ok(vec![]); }

        let prompt = format!(
            "Analyze this conversation and extract 1-3 behavioral rules the assistant should follow \
             to avoid similar issues in the future.\n\n\
             Conversation:\n{}\n\n\
             Issues encountered:\n{}\n\n\
             Return as JSON array: [{{\"rule\": \"...\", \"source\": \"error_recovery\"}}]",
            thread.summary(),
            thread.issues_summary(),
        );

        let response = self.llm.chat_simple(&prompt).await?;
        let new_rules: Vec<LearnedRule> = serde_json::from_str(&response)?;

        // Add rules if within budget
        self.add_rules(new_rules).await
    }

    /// Get the rules overlay for system prompt injection
    pub async fn get_prompt_overlay(&self) -> String {
        let rules = self.rules.read().await;
        let active_rules: Vec<_> = rules.iter()
            .filter(|r| r.confidence >= 0.3 && r.failure_count < 3)
            .collect();

        if active_rules.is_empty() { return String::new(); }

        let mut overlay = String::from("\n\n# Learned Behavioral Rules\n\n");
        let mut total_chars = 0;

        for rule in active_rules {
            if total_chars + rule.rule.len() > self.max_total_chars { break; }
            overlay.push_str(&format!("- {}\n", rule.rule));
            total_chars += rule.rule.len();
        }

        overlay
    }

    /// Decay rule confidence if it caused an error
    pub async fn report_rule_failure(&self, rule_id: &str) {
        if let Some(rule) = self.rules.write().await.iter_mut().find(|r| r.id == rule_id) {
            rule.failure_count += 1;
            rule.confidence *= 0.8; // 20% decay per failure
        }
    }

    /// Persist rules to disk
    async fn save(&self) -> Result<()>;
    /// Load rules from disk
    async fn load(&self) -> Result<()>;
}
```

**Integration** in `src/agent.rs`:
```rust
// When building system prompt:
let rules_overlay = self.learning.prompt_evolution.get_prompt_overlay().await;
system_prompt.push_str(&rules_overlay);
```

### Step 2: Skill Extraction (Day 1 afternoon)

**File**: `src/learning/skill_extraction.rs`

After successful multi-step tasks (5+ steps, 3+ tools), extract reusable skills.

```rust
pub struct SkillExtractor {
    llm: Arc<dyn LlmProvider>,
    skill_dir: PathBuf,         // ~/.pylot/skills/auto-generated/
    min_steps: usize,           // 5
    min_tools: usize,           // 3
}

impl SkillExtractor {
    /// Analyze a completed thread and extract skills if criteria met
    pub async fn extract_from_thread(&self, thread: &ConversationThread) -> Result<Vec<ExtractedSkill>> {
        // Check criteria
        if thread.step_count() < self.min_steps || thread.unique_tools().len() < self.min_tools {
            return Ok(vec![]);
        }

        let prompt = format!(
            "Analyze this successful multi-step conversation and extract 1-2 reusable procedural skills.\n\n\
             A skill is a general-purpose approach that could apply to similar future tasks.\n\n\
             Conversation summary:\n{}\n\
             Tools used: {:?}\n\
             Steps taken: {}\n\n\
             For each skill, provide:\n\
             - name: slug format (lowercase-with-hyphens)\n\
             - description: when to use this skill (1 sentence)\n\
             - category: coding | research | productivity | communication | agentic\n\
             - content: step-by-step instructions in markdown (the skill body)\n\n\
             Return as JSON array.",
            thread.summary(),
            thread.unique_tools(),
            thread.step_count(),
        );

        let response = self.llm.chat_simple(&prompt).await?;
        let skills: Vec<ExtractedSkill> = serde_json::from_str(&response)?;

        // Save as SKILL.md files
        for skill in &skills {
            self.save_skill(skill).await?;
        }

        Ok(skills)
    }

    async fn save_skill(&self, skill: &ExtractedSkill) -> Result<()> {
        let dir = self.skill_dir.join(&skill.name);
        tokio::fs::create_dir_all(&dir).await?;

        let content = format!(
            "---\nname: {}\ndescription: {}\ncategory: {}\ntags: [auto-generated]\nauthor: openpylot-learning\nversion: 1.0.0\n---\n\n{}",
            skill.name, skill.description, skill.category, skill.content
        );

        tokio::fs::write(dir.join("SKILL.md"), content).await?;
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExtractedSkill {
    pub name: String,
    pub description: String,
    pub category: String,
    pub content: String,
}
```

### Step 3: Conversation Insights (Day 2 morning)

**File**: `src/learning/conversation_insights.rs`

Extract user preferences and workflow patterns periodically (every 5 completed threads).

```rust
pub struct ConversationInsightExtractor {
    llm: Arc<dyn LlmProvider>,
    memory: Arc<MemoryStore>,
    threads_since_last: AtomicU32,
    extraction_interval: u32,       // every 5 threads
}

impl ConversationInsightExtractor {
    /// Check if extraction is due, and if so, extract insights
    pub async fn maybe_extract(&self, thread: &ConversationThread) -> Result<()> {
        let count = self.threads_since_last.fetch_add(1, Ordering::Relaxed);
        if count < self.extraction_interval { return Ok(()); }
        self.threads_since_last.store(0, Ordering::Relaxed);

        self.extract_insights().await
    }

    async fn extract_insights(&self) -> Result<()> {
        // Gather recent threads (last 5)
        let recent_summaries = self.get_recent_thread_summaries(5).await?;

        let prompt = format!(
            "Analyze these recent conversations and extract user preferences and workflow patterns.\n\n\
             Recent conversations:\n{}\n\n\
             Extract:\n\
             1. Communication preferences (verbosity, format, style)\n\
             2. Technical preferences (languages, tools, frameworks)\n\
             3. Workflow patterns (how user typically approaches tasks)\n\
             4. Common requests (frequently asked topics)\n\n\
             Return as JSON: {{\"preferences\": [...], \"patterns\": [...], \"common_requests\": [...]}}",
            recent_summaries.join("\n---\n"),
        );

        let response = self.llm.chat_simple(&prompt).await?;
        let insights: Insights = serde_json::from_str(&response)?;

        // Store as PREFERENCE memories
        for pref in insights.preferences {
            let unit = MemoryUnit::new(MemoryType::Preference, pref);
            self.memory.insert(&unit).await?;
        }

        // Store as PROCEDURAL_OBSERVATION memories
        for pattern in insights.patterns {
            let unit = MemoryUnit::new(MemoryType::ProceduralObservation, pattern);
            self.memory.insert(&unit).await?;
        }

        Ok(())
    }
}
```

### Step 4: Error Diagnosis (Day 2 afternoon)

**File**: `src/learning/error_diagnosis.rs`

```rust
pub struct ErrorDiagnoser {
    llm: Arc<dyn LlmProvider>,
    prompt_evolution: Arc<PromptEvolution>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ErrorCategory {
    ToolError,          // Tool returned error
    PromptError,        // Agent misunderstood user intent
    CodeError,          // Generated code had bugs
    ContextError,       // Missing context/information
    ConfigError,        // Configuration issue
    ExternalError,      // External service failure
}

impl ErrorDiagnoser {
    /// Diagnose an error and recommend improvement action
    pub async fn diagnose(&self, error: &AgentError, context: &ErrorContext) -> Result<Diagnosis> {
        let prompt = format!(
            "Diagnose this agent error and recommend how to prevent it:\n\n\
             Error: {}\n\
             Context: {}\n\
             Recent messages: {}\n\n\
             Classify the error as: tool_error | prompt_error | code_error | context_error | config_error | external_error\n\
             Suggest a behavioral rule to prevent recurrence.\n\
             Return: {{\"category\": \"...\", \"explanation\": \"...\", \"rule\": \"...\"}}",
            error.message,
            context.summary(),
            context.recent_messages_summary(),
        );

        let response = self.llm.chat_simple(&prompt).await?;
        let diagnosis: Diagnosis = serde_json::from_str(&response)?;

        // Auto-create learned rule if confidence is high enough
        if diagnosis.category != ErrorCategory::ExternalError {
            let rule = LearnedRule {
                id: uuid::Uuid::new_v4().to_string(),
                rule: diagnosis.rule.clone(),
                source: RuleSource::AutoDiagnosis,
                confidence: 0.5,  // Start at 50%, needs validation
                created_at: chrono::Utc::now().to_rfc3339(),
                applied_count: 0,
                failure_count: 0,
            };
            self.prompt_evolution.add_rules(vec![rule]).await?;
        }

        Ok(diagnosis)
    }
}
```

### Step 5: User Feedback Collection (Day 2)

**File**: `src/learning/feedback.rs`

```rust
pub struct FeedbackCollector {
    llm: Arc<dyn LlmProvider>,
    prompt_evolution: Arc<PromptEvolution>,
    memory: Arc<MemoryStore>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UserFeedback {
    pub rating: Option<i8>,         // -1 (bad), 0 (neutral), 1 (good)
    pub comment: Option<String>,    // Free text
    pub thread_id: String,
}

impl FeedbackCollector {
    pub async fn process(&self, feedback: &UserFeedback) -> Result<()> {
        if feedback.rating == Some(-1) {
            // Negative feedback → trigger prompt evolution + error diagnosis
            // Generate rules from the feedback
        }

        if feedback.rating == Some(1) {
            // Positive feedback → boost confidence of recent rules
            // Extract skill if multi-step task
        }

        // Store feedback as memory
        let content = format!(
            "User feedback on thread {}: rating={:?}, comment={:?}",
            feedback.thread_id, feedback.rating, feedback.comment
        );
        let unit = MemoryUnit::new(MemoryType::Episodic, content);
        self.memory.insert(&unit).await?;

        Ok(())
    }
}
```

### Step 6: Wire into agent lifecycle (Day 2)

**File**: Modify `src/agent.rs`

```rust
impl Agent {
    pub async fn handle_message(&self, message: &str) -> Result<String> {
        // ... existing logic ...

        // After LLM response:
        let turn = ConversationTurn { /* ... */ };
        self.learning.on_turn_complete(&turn).await?;

        // If this completes a thread:
        if is_thread_complete {
            let thread = self.build_thread_summary()?;
            self.learning.on_thread_complete(&thread).await?;
        }

        // If error occurred:
        if let Err(ref e) = result {
            self.learning.on_error(e, &error_context).await?;
        }

        result
    }
}
```

### Step 7: CLI commands for learning management (Day 2)

```
pylot learn status                   # Show learning stats
pylot learn rules list               # List all learned rules
pylot learn rules remove <id>        # Remove a rule
pylot learn insights list            # List extracted insights
pylot learn skills list              # List auto-generated skills
pylot learn reset                    # Reset all learned state
```

---

## Config Additions

```toml
[learning]
enabled = true
prompt_evolution = true
prompt_max_chars = 4000
skill_extraction = true
skill_min_steps = 5
skill_min_tools = 3
conversation_insights = true
insight_interval = 5              # every N threads
error_diagnosis = true
feedback_enabled = true
```

---

## Testing

- `test_rule_generation` — Rules generated from error patterns
- `test_rule_confidence_decay` — Failures decrease confidence
- `test_prompt_overlay` — Rules injected into system prompt within budget
- `test_skill_extraction_criteria` — Only extracts when criteria met
- `test_skill_file_creation` — Valid SKILL.md produced
- `test_insight_extraction` — Preferences and patterns saved as memories
- `test_error_diagnosis` — Correct error categorization
- `test_feedback_processing` — Positive/negative feedback handled

---

## Acceptance Criteria

- [ ] Prompt evolution generates rules from errors and stores them
- [ ] Rules injected into system prompt (capped at 4000 chars)
- [ ] Rule confidence decays on failures, removed after 3 failures
- [ ] Skills extracted from successful multi-step tasks
- [ ] Skills saved as SKILL.md in auto-generated directory
- [ ] Conversation insights extracted every 5 threads
- [ ] Insights stored as Preference and ProceduralObservation memories
- [ ] Error diagnosis categorizes errors and suggests rules
- [ ] User feedback influences rule confidence
- [ ] CLI commands for learning management work
- [ ] All learning can be disabled via config
