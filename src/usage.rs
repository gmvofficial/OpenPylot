use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Instant;

/// Token usage for a single LLM call.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
}

impl TokenUsage {
    pub fn total(&self) -> u64 {
        self.input_tokens + self.output_tokens
    }
}

/// Model pricing in USD per million tokens.
#[derive(Debug, Clone)]
pub struct ModelPricing {
    pub input_per_million: f64,
    pub output_per_million: f64,
    pub cache_read_per_million: f64,
    pub cache_write_per_million: f64,
}

impl ModelPricing {
    /// Get pricing for known models. Falls back to a default estimate.
    pub fn for_model(model: &str) -> Self {
        match model {
            // Anthropic models
            m if m.contains("claude-3-5-sonnet") || m.contains("claude-3.5-sonnet") => Self {
                input_per_million: 3.0,
                output_per_million: 15.0,
                cache_read_per_million: 0.3,
                cache_write_per_million: 3.75,
            },
            m if m.contains("claude-3-5-haiku") || m.contains("claude-3.5-haiku") => Self {
                input_per_million: 1.0,
                output_per_million: 5.0,
                cache_read_per_million: 0.1,
                cache_write_per_million: 1.25,
            },
            m if m.contains("claude-3-opus") => Self {
                input_per_million: 15.0,
                output_per_million: 75.0,
                cache_read_per_million: 1.5,
                cache_write_per_million: 18.75,
            },
            m if m.contains("claude-4") || m.contains("claude-sonnet-4") => Self {
                input_per_million: 3.0,
                output_per_million: 15.0,
                cache_read_per_million: 0.3,
                cache_write_per_million: 3.75,
            },
            // OpenAI models
            m if m.contains("gpt-4o-mini") => Self {
                input_per_million: 0.15,
                output_per_million: 0.60,
                cache_read_per_million: 0.075,
                cache_write_per_million: 0.15,
            },
            m if m.contains("gpt-4o") => Self {
                input_per_million: 2.5,
                output_per_million: 10.0,
                cache_read_per_million: 1.25,
                cache_write_per_million: 2.5,
            },
            m if m.contains("gpt-4-turbo") => Self {
                input_per_million: 10.0,
                output_per_million: 30.0,
                cache_read_per_million: 5.0,
                cache_write_per_million: 10.0,
            },
            m if m.contains("o1-mini") => Self {
                input_per_million: 3.0,
                output_per_million: 12.0,
                cache_read_per_million: 1.5,
                cache_write_per_million: 3.0,
            },
            m if m.contains("o1") => Self {
                input_per_million: 15.0,
                output_per_million: 60.0,
                cache_read_per_million: 7.5,
                cache_write_per_million: 15.0,
            },
            // Default fallback
            _ => Self {
                input_per_million: 3.0,
                output_per_million: 15.0,
                cache_read_per_million: 0.3,
                cache_write_per_million: 3.75,
            },
        }
    }

    /// Calculate cost in USD for given token usage.
    pub fn calculate_cost(&self, usage: &TokenUsage) -> f64 {
        let input_cost = (usage.input_tokens as f64 / 1_000_000.0) * self.input_per_million;
        let output_cost = (usage.output_tokens as f64 / 1_000_000.0) * self.output_per_million;
        let cache_read_cost =
            (usage.cache_read_tokens as f64 / 1_000_000.0) * self.cache_read_per_million;
        let cache_write_cost =
            (usage.cache_write_tokens as f64 / 1_000_000.0) * self.cache_write_per_million;
        input_cost + output_cost + cache_read_cost + cache_write_cost
    }
}

/// Tracks cumulative token usage and costs across a session.
#[derive(Debug)]
pub struct UsageTracker {
    model: String,
    pricing: ModelPricing,
    /// Cumulative totals for the session.
    cumulative: TokenUsage,
    /// Per-turn usage history: (turn_number, usage).
    turns: Vec<(usize, TokenUsage)>,
    /// Total turns.
    turn_count: usize,
    /// Session start time.
    session_start: Instant,
    /// Per-model breakdown (for fallback scenarios).
    per_model: HashMap<String, TokenUsage>,
}

impl UsageTracker {
    pub fn new(model: &str) -> Self {
        Self {
            model: model.to_string(),
            pricing: ModelPricing::for_model(model),
            cumulative: TokenUsage::default(),
            turns: Vec::new(),
            turn_count: 0,
            session_start: Instant::now(),
            per_model: HashMap::new(),
        }
    }

    /// Record usage for a single LLM call.
    pub fn record(&mut self, usage: TokenUsage, model: Option<&str>) {
        self.turn_count += 1;

        self.cumulative.input_tokens += usage.input_tokens;
        self.cumulative.output_tokens += usage.output_tokens;
        self.cumulative.cache_read_tokens += usage.cache_read_tokens;
        self.cumulative.cache_write_tokens += usage.cache_write_tokens;

        self.turns.push((self.turn_count, usage.clone()));

        // Track per-model (useful with fallback providers)
        let model_name = model.unwrap_or(&self.model).to_string();
        let entry = self.per_model.entry(model_name).or_default();
        entry.input_tokens += usage.input_tokens;
        entry.output_tokens += usage.output_tokens;
        entry.cache_read_tokens += usage.cache_read_tokens;
        entry.cache_write_tokens += usage.cache_write_tokens;
    }

    /// Set the primary model (e.g., after switching).
    pub fn set_model(&mut self, model: &str) {
        self.model = model.to_string();
        self.pricing = ModelPricing::for_model(model);
    }

    /// Get total cost estimate in USD.
    pub fn total_cost(&self) -> f64 {
        // Sum costs per model with their respective pricing
        self.per_model
            .iter()
            .map(|(model, usage)| {
                let pricing = ModelPricing::for_model(model);
                pricing.calculate_cost(usage)
            })
            .sum()
    }

    /// Get cumulative token totals.
    pub fn cumulative(&self) -> &TokenUsage {
        &self.cumulative
    }

    /// Get total turns.
    pub fn turn_count(&self) -> usize {
        self.turn_count
    }

    /// Generate a summary string for display (e.g., /cost command).
    pub fn summary(&self) -> String {
        let elapsed = self.session_start.elapsed();
        let minutes = elapsed.as_secs() / 60;
        let seconds = elapsed.as_secs() % 60;

        let mut lines = vec![
            format!("Session Usage ({}m {}s, {} turns):", minutes, seconds, self.turn_count),
            format!(
                "  Input:  {:>8} tokens",
                self.cumulative.input_tokens
            ),
            format!(
                "  Output: {:>8} tokens",
                self.cumulative.output_tokens
            ),
            format!(
                "  Total:  {:>8} tokens",
                self.cumulative.total()
            ),
        ];

        if self.cumulative.cache_read_tokens > 0 || self.cumulative.cache_write_tokens > 0 {
            lines.push(format!(
                "  Cache:  {:>8} read, {} write",
                self.cumulative.cache_read_tokens, self.cumulative.cache_write_tokens
            ));
        }

        let cost = self.total_cost();
        lines.push(format!("  Cost:   ${:.4}", cost));

        if self.per_model.len() > 1 {
            lines.push("  By model:".to_string());
            for (model, usage) in &self.per_model {
                let pricing = ModelPricing::for_model(model);
                let model_cost = pricing.calculate_cost(usage);
                lines.push(format!(
                    "    {}: {} tokens (${:.4})",
                    model,
                    usage.total(),
                    model_cost
                ));
            }
        }

        lines.join("\n")
    }
}
