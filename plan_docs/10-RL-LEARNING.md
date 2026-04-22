# 10 — Reinforcement Learning (GRPO) System

## Objective

Implement a MetaClaw-inspired RL learning system using Group Relative Policy Optimization (GRPO). The agent learns from live conversations: a Process Reward Model (PRM) scores responses, gradients are computed, and model behavior improves via LoRA weight updates. Support both local and cloud-based training backends.

---

## Current State

- **No learning system**: Agent behavior is static
- **Traditional learning** (doc 06): Provides prompt evolution and skill extraction
- **This doc**: Adds gradient-based RL on top

---

## Reference Implementation

### MetaClaw (Primary — GRPO System)
- **Path**: `extra_repos/MetaClaw-main/metaclaw/`
- **Key files**:
  - `rl/trainer.py` — GRPO training loop with batch collection
  - `rl/reward.py` — Process Reward Model (PRM) scoring
  - `rl/advantage.py` — Advantage computation (importance sampling, PPO, CISPO)
  - `rl/scheduler.py` — Idle-window gating (sleep hours, keyboard idle, calendar)
  - `proxy.py` — Proxy server that intercepts LLM calls
  - `rl/opd.py` — On-Policy Distillation (teacher guidance)

### How MetaClaw's GRPO Works

```
1. Proxy intercepts agent ↔ LLM traffic
2. Captures: prompt tokens, response tokens, logprobs
3. PRM (judge LLM) scores response: -1 / 0 / +1
4. Batch accumulated (default 4-32 samples)
5. Compute advantages via importance sampling
6. Convert to training datums with loss masks
7. Forward-backward pass on cloud GPU (Tinker)
8. LoRA weights updated (hot-swap, no downtime)
9. Next request uses updated weights
```

---

## Architecture

### Three Operating Modes (from MetaClaw)

1. **`skills_only`** — No RL, just skill injection + memory (no GPU needed)
2. **`rl`** — Full RL with immediate training (requires GPU backend)
3. **`auto`** — RL deferred to idle windows (sleep/inactive periods)

### Module Structure

```
src/learning/rl/
├── mod.rs              -- Public API, RLEngine
├── collector.rs        -- Data collection from conversations
├── reward.rs           -- PRM scoring (judge LLM)
├── advantage.rs        -- Advantage computation
├── trainer.rs          -- Training loop (GRPO)
├── scheduler.rs        -- Idle-window gating
├── backend/
│   ├── mod.rs         -- Training backend trait
│   ├── local.rs       -- Local GPU training (optional)
│   └── cloud.rs       -- Cloud API training (Tinker-like)
└── types.rs            -- TrainingSample, Datum, etc.
```

---

## Implementation Steps

### Step 1: Define types (Day 1)

**File**: `src/learning/rl/types.rs`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationSample {
    pub id: String,
    pub prompt_tokens: Vec<u32>,        // Tokenized prompt
    pub response_tokens: Vec<u32>,      // Tokenized response
    pub logprobs: Vec<f64>,             // Log probabilities of response tokens
    pub reward: f64,                     // PRM score: -1.0, 0.0, or 1.0
    pub skill_generation: u64,           // For MAML versioning
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingDatum {
    pub model_input: Vec<u32>,          // all_tokens[:-1]
    pub target_tokens: Vec<u32>,        // all_tokens[1:] (left-shifted)
    pub logprobs: Vec<f64>,             // [0..0, sampled_logprobs] (masked prompt)
    pub advantages: Vec<f64>,           // [0..0, reward * mask] (masked prompt)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingBatch {
    pub datums: Vec<TrainingDatum>,
    pub skill_generation: u64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RLMode {
    SkillsOnly,     // No RL
    RL,             // Immediate training
    Auto,           // Deferred to idle windows
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RLConfig {
    pub mode: RLMode,
    pub batch_size: usize,              // 4-32
    pub learning_rate: f64,             // 1e-5
    pub lora_rank: usize,              // 8-16
    pub prm_model: String,             // e.g., "gpt-4o-mini"
    pub prm_majority_votes: usize,     // 3
    pub backend: TrainingBackend,
    pub opd_enabled: bool,             // On-Policy Distillation
    pub opd_teacher_model: Option<String>,
    pub opd_kl_penalty: f64,           // 0.1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TrainingBackend {
    Local,          // Local GPU
    Cloud { api_url: String, api_key: String },
}
```

### Step 2: Implement data collector (Day 1)

**File**: `src/learning/rl/collector.rs`

The collector captures conversation samples during normal agent operation.

```rust
pub struct DataCollector {
    samples: RwLock<Vec<ConversationSample>>,
    current_generation: AtomicU64,
    config: RLConfig,
}

impl DataCollector {
    /// Record a conversation turn for RL training
    /// Called after each agent response
    pub async fn record(
        &self,
        prompt: &str,
        response: &str,
        logprobs: Option<Vec<f64>>,
    ) -> Result<()> {
        // 1. Tokenize prompt and response
        let prompt_tokens = self.tokenize(prompt)?;
        let response_tokens = self.tokenize(response)?;

        // 2. Use provided logprobs or estimate
        let logprobs = logprobs.unwrap_or_else(|| vec![0.0; response_tokens.len()]);

        let sample = ConversationSample {
            id: uuid::Uuid::new_v4().to_string(),
            prompt_tokens,
            response_tokens,
            logprobs,
            reward: 0.0,  // Will be scored by PRM
            skill_generation: self.current_generation.load(Ordering::Relaxed),
            timestamp: chrono::Utc::now().to_rfc3339(),
        };

        self.samples.write().await.push(sample);
        Ok(())
    }

    /// Get samples ready for scoring (unscored)
    pub async fn get_unscored(&self) -> Vec<ConversationSample> {
        self.samples.read().await
            .iter()
            .filter(|s| s.reward == 0.0)
            .cloned()
            .collect()
    }

    /// Flush samples for a given generation (MAML: discard stale data)
    pub async fn flush_generation(&self, generation: u64) {
        let mut samples = self.samples.write().await;
        samples.retain(|s| s.skill_generation > generation);
    }

    /// Take a batch of scored samples for training
    pub async fn take_batch(&self, batch_size: usize) -> Option<Vec<ConversationSample>> {
        let mut samples = self.samples.write().await;
        let scored: Vec<_> = samples.iter()
            .filter(|s| s.reward != 0.0)
            .cloned()
            .collect();

        if scored.len() < batch_size { return None; }

        // Remove taken samples
        let batch: Vec<_> = scored.into_iter().take(batch_size).collect();
        let batch_ids: HashSet<_> = batch.iter().map(|s| s.id.clone()).collect();
        samples.retain(|s| !batch_ids.contains(&s.id));

        Some(batch)
    }
}
```

### Step 3: Implement PRM (Process Reward Model) (Day 1)

**File**: `src/learning/rl/reward.rs`

```rust
pub struct ProcessRewardModel {
    llm: Arc<dyn LlmProvider>,      // Judge LLM (can be different from agent LLM)
    majority_votes: usize,          // Default 3
}

impl ProcessRewardModel {
    /// Score a conversation sample using judge LLM
    /// Returns -1.0 (bad), 0.0 (neutral), or 1.0 (good)
    pub async fn score(&self, sample: &ConversationSample) -> Result<f64> {
        let prompt_text = self.detokenize(&sample.prompt_tokens)?;
        let response_text = self.detokenize(&sample.response_tokens)?;

        let scoring_prompt = format!(
            "You are evaluating an AI assistant's response.\n\n\
             User instruction:\n{}\n\n\
             Assistant response:\n{}\n\n\
             Was the response helpful, accurate, and appropriate for this instruction?\n\
             Score: 1 (good), 0 (unclear/mediocre), -1 (bad/harmful/wrong)\n\n\
             Respond with ONLY the score number.",
            prompt_text, response_text
        );

        // Majority voting: ask multiple times, take majority
        let mut scores = Vec::new();
        for _ in 0..self.majority_votes {
            let response = self.llm.chat_simple(&scoring_prompt).await?;
            let score: f64 = response.trim().parse().unwrap_or(0.0);
            scores.push(score.clamp(-1.0, 1.0));
        }

        // Return majority score
        let avg = scores.iter().sum::<f64>() / scores.len() as f64;
        Ok(if avg > 0.3 { 1.0 } else if avg < -0.3 { -1.0 } else { 0.0 })
    }

    /// Score a batch of samples (parallel)
    pub async fn score_batch(&self, samples: &mut [ConversationSample]) -> Result<()> {
        let futures: Vec<_> = samples.iter().map(|s| self.score(s)).collect();
        let scores = futures::future::join_all(futures).await;

        for (sample, score_result) in samples.iter_mut().zip(scores) {
            sample.reward = score_result.unwrap_or(0.0);
        }
        Ok(())
    }
}
```

### Step 4: Implement advantage computation (Day 2)

**File**: `src/learning/rl/advantage.rs`

```rust
pub enum AdvantageMethod {
    ImportanceSampling,     // Default, most stable
    PPO,                    // PPO-style clipped
}

pub fn compute_advantages(
    samples: &[ConversationSample],
    method: AdvantageMethod,
) -> Vec<TrainingDatum> {
    samples.iter().map(|sample| {
        let prompt_len = sample.prompt_tokens.len();
        let response_len = sample.response_tokens.len();
        let total_len = prompt_len + response_len;

        // Build full token sequence
        let mut all_tokens = sample.prompt_tokens.clone();
        all_tokens.extend(&sample.response_tokens);

        // Model input: all_tokens[:-1]
        let model_input = all_tokens[..total_len - 1].to_vec();

        // Target: all_tokens[1:]
        let target_tokens = all_tokens[1..].to_vec();

        // Logprobs: zero for prompt, actual for response
        let mut logprobs = vec![0.0; prompt_len];
        logprobs.extend(&sample.logprobs);

        // Advantages: zero for prompt, reward * mask for response
        let mut advantages = vec![0.0; prompt_len];
        let response_advantages = match method {
            AdvantageMethod::ImportanceSampling => {
                // advantage = reward (simple IS)
                vec![sample.reward; response_len]
            }
            AdvantageMethod::PPO => {
                // PPO-style: clip(ratio, 1-eps, 1+eps) * advantage
                vec![sample.reward; response_len]
            }
        };
        advantages.extend(response_advantages);

        TrainingDatum {
            model_input,
            target_tokens,
            logprobs,
            advantages,
        }
    }).collect()
}
```

### Step 5: Implement training backend (Day 2)

**File**: `src/learning/rl/backend/cloud.rs`

```rust
pub struct CloudTrainingBackend {
    api_url: String,
    api_key: String,
    client: reqwest::Client,
}

#[async_trait]
impl TrainingBackendTrait for CloudTrainingBackend {
    /// Submit a training batch and wait for weight update
    async fn train(&self, batch: &TrainingBatch) -> Result<TrainingResult> {
        let resp = self.client.post(&format!("{}/train", self.api_url))
            .bearer_auth(&self.api_key)
            .json(&batch)
            .send().await?;

        let result: TrainingResult = resp.json().await?;
        Ok(result)
    }

    /// Check if backend is available
    async fn health(&self) -> Result<bool> {
        let resp = self.client.get(&format!("{}/health", self.api_url))
            .send().await;
        Ok(resp.is_ok())
    }
}
```

**File**: `src/learning/rl/backend/local.rs`

```rust
// For local GPU training, use candle or tch-rs (PyTorch bindings)
// This is optional and requires GPU

pub struct LocalTrainingBackend {
    model_path: PathBuf,
    lora_rank: usize,
}

#[async_trait]
impl TrainingBackendTrait for LocalTrainingBackend {
    async fn train(&self, batch: &TrainingBatch) -> Result<TrainingResult> {
        // Use candle-core for Rust-native ML training
        // Or shell out to a Python training script
        todo!("Implement with candle or tch-rs")
    }
}
```

### Step 6: Implement idle-window scheduler (Day 2)

**File**: `src/learning/rl/scheduler.rs`

```rust
pub struct RLScheduler {
    state: RwLock<SchedulerState>,
    sleep_start: u32,       // Hour (e.g., 23)
    sleep_end: u32,         // Hour (e.g., 7)
    idle_threshold_secs: u64,  // e.g., 1800 (30 min)
}

#[derive(Debug)]
pub enum SchedulerState {
    IdleWait,       // Waiting for idle window
    WindowOpen,     // Window detected, ready to train
    Updating,       // Training in progress
    Pausing,        // User returned, pausing
}

impl RLScheduler {
    /// Check if an idle window is currently open
    pub async fn is_window_open(&self) -> bool {
        let now = chrono::Local::now().hour();

        // Sleep hours check
        let in_sleep = if self.sleep_start > self.sleep_end {
            now >= self.sleep_start || now < self.sleep_end
        } else {
            now >= self.sleep_start && now < self.sleep_end
        };

        if in_sleep { return true; }

        // System idle check (platform-specific)
        #[cfg(target_os = "macos")]
        {
            // Use IOKit to check system idle time
            if let Ok(idle_secs) = get_system_idle_time() {
                if idle_secs >= self.idle_threshold_secs {
                    return true;
                }
            }
        }

        false
    }

    /// Run the scheduler loop
    pub async fn run(&self, collector: Arc<DataCollector>, trainer: Arc<Trainer>) {
        loop {
            match *self.state.read().await {
                SchedulerState::IdleWait => {
                    if self.is_window_open().await {
                        *self.state.write().await = SchedulerState::WindowOpen;
                    }
                }
                SchedulerState::WindowOpen => {
                    // Start training if batch available
                    if let Some(batch) = collector.take_batch(trainer.batch_size()).await {
                        *self.state.write().await = SchedulerState::Updating;
                        match trainer.train_batch(&batch).await {
                            Ok(_) => log::info!("RL training batch completed"),
                            Err(e) => log::error!("RL training failed: {}", e),
                        }
                        *self.state.write().await = SchedulerState::IdleWait;
                    }
                }
                SchedulerState::Updating => {
                    // Check if user returned (window closed)
                    if !self.is_window_open().await {
                        *self.state.write().await = SchedulerState::Pausing;
                        // Signal trainer to save partial state
                    }
                }
                SchedulerState::Pausing => {
                    *self.state.write().await = SchedulerState::IdleWait;
                }
            }

            tokio::time::sleep(Duration::from_secs(60)).await;
        }
    }
}
```

### Step 7: Main RL Engine (Day 3)

**File**: `src/learning/rl/mod.rs`

```rust
pub struct RLEngine {
    config: RLConfig,
    collector: Arc<DataCollector>,
    prm: Arc<ProcessRewardModel>,
    trainer: Arc<Trainer>,
    scheduler: Arc<RLScheduler>,
}

impl RLEngine {
    /// Called after each agent response to collect training data
    pub async fn on_response(
        &self,
        prompt: &str,
        response: &str,
        logprobs: Option<Vec<f64>>,
    ) -> Result<()> {
        if matches!(self.config.mode, RLMode::SkillsOnly) { return Ok(()); }

        // 1. Record sample
        self.collector.record(prompt, response, logprobs).await?;

        // 2. Score unscored samples (async, background)
        let collector = self.collector.clone();
        let prm = self.prm.clone();
        tokio::spawn(async move {
            let mut unscored = collector.get_unscored().await;
            if let Err(e) = prm.score_batch(&mut unscored).await {
                log::warn!("PRM scoring failed: {}", e);
            }
        });

        // 3. In RL mode, train immediately when batch ready
        if matches!(self.config.mode, RLMode::RL) {
            if let Some(batch) = self.collector.take_batch(self.config.batch_size).await {
                let datums = compute_advantages(&batch, AdvantageMethod::ImportanceSampling);
                let training_batch = TrainingBatch {
                    datums,
                    skill_generation: self.collector.current_generation(),
                    created_at: chrono::Utc::now().to_rfc3339(),
                };
                self.trainer.train_batch(&training_batch).await?;
            }
        }

        // In Auto mode, scheduler handles training in idle windows
        Ok(())
    }

    /// Notify that skills have evolved (MAML: flush stale data)
    pub async fn on_skill_evolution(&self) {
        let gen = self.collector.bump_generation();
        self.collector.flush_generation(gen - 1).await;
    }

    /// Start the RL system (scheduler + background tasks)
    pub async fn start(&self) {
        if matches!(self.config.mode, RLMode::Auto) {
            let scheduler = self.scheduler.clone();
            let collector = self.collector.clone();
            let trainer = self.trainer.clone();
            tokio::spawn(async move {
                scheduler.run(collector, trainer).await;
            });
        }
    }
}
```

---

## Config Additions

```toml
[learning.rl]
enabled = false                         # Disabled by default
mode = "auto"                          # skills_only | rl | auto
batch_size = 8
learning_rate = 0.00001
lora_rank = 8

[learning.rl.prm]
model = "gpt-4o-mini"                  # Judge model (cheap is fine)
majority_votes = 3

[learning.rl.backend]
type = "cloud"                         # local | cloud
api_url = "https://api.tinker.dev/v1"  # Cloud training API
api_key = "${TINKER_API_KEY}"

[learning.rl.scheduler]
sleep_start = 23                       # 11 PM
sleep_end = 7                          # 7 AM
idle_threshold_secs = 1800             # 30 minutes

[learning.rl.opd]
enabled = false
teacher_model = "gpt-4o"
kl_penalty = 0.1
```

---

## Testing

- `test_data_collection` — Samples recorded correctly
- `test_prm_scoring` — Judge returns valid scores
- `test_majority_voting` — Multiple votes aggregated correctly
- `test_advantage_computation` — Correct masking and values
- `test_batch_formation` — Batch taken when ready
- `test_generation_flush` — MAML flush discards stale data
- `test_scheduler_window` — Sleep hours detected correctly
- `test_training_batch_format` — Datums formatted correctly
- `test_skills_only_mode` — No RL when in skills_only mode

---

## Acceptance Criteria

- [ ] Data collected from conversation turns (prompt, response, logprobs)
- [ ] PRM scores responses using judge LLM with majority voting
- [ ] Advantages computed with proper prompt masking
- [ ] Training batches formed when enough scored samples exist
- [ ] Cloud backend submits batches and receives weight updates
- [ ] Idle-window scheduler defers training to sleep/inactive periods
- [ ] MAML generation versioning flushes stale data on skill evolution
- [ ] Three modes work: skills_only (no RL), rl (immediate), auto (deferred)
- [ ] RL disabled by default (opt-in via config)
- [ ] All learning integrates with doc 06 (traditional learning)
