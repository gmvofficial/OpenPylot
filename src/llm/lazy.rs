//! Lazily-configured LLM provider.
//!
//! Used when the backend starts before any API key has been configured
//! (e.g. first launch from the web dashboard). Each request re-checks the
//! environment and the secrets vault, so a key saved from the frontend
//! setup wizard takes effect immediately — no restart required.

use std::sync::Arc;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::llm::anthropic::AnthropicProvider;
use crate::llm::openai::OpenAIProvider;
use crate::llm::providers::{self, Kind, Provider};
use crate::llm::{LlmProvider, LlmResponse, Message};
use crate::streaming::StreamSender;
use crate::tools::ToolDefinition;

pub struct LazyProvider {
    provider: String,
    model: String,
    max_tokens: u32,
    temperature: f64,
    /// User-supplied API root, from `llm.base_url`. Lets `custom` be pointed
    /// anywhere and lets a hosted provider be aimed at a proxy.
    base_url: Option<String>,
    inner: RwLock<Option<Arc<dyn LlmProvider>>>,
}

impl LazyProvider {
    pub fn new(provider: String, model: String, max_tokens: u32, temperature: f64) -> Self {
        Self {
            provider,
            model,
            max_tokens,
            temperature,
            base_url: None,
            inner: RwLock::new(None),
        }
    }

    pub fn with_base_url(mut self, base_url: Option<String>) -> Self {
        self.base_url = base_url;
        self
    }

    /// Find this provider's API key, from the environment first and the
    /// encrypted vault second.
    fn find_key(&self, spec: &Provider) -> Option<String> {
        if let Ok(value) = std::env::var(spec.env_key) {
            if !value.is_empty() {
                return Some(value);
            }
        }
        crate::secrets::SecretsVault::open(&crate::secrets::default_secrets_path(), None)
            .ok()
            .and_then(|v| v.get(spec.vault_key))
    }

    async fn resolve(&self) -> Result<Arc<dyn LlmProvider>> {
        if let Some(p) = self.inner.read().await.as_ref() {
            return Ok(Arc::clone(p));
        }

        // Exhaustive, not `_ => OpenAI`. A typo used to build an OpenAI client
        // that then failed with an authentication error about a key the user
        // had never been asked for.
        let spec = providers::find(&self.provider)
            .ok_or_else(|| anyhow!("{}", providers::unknown_provider_error(&self.provider)))?;

        let api_key = match self.find_key(spec) {
            Some(key) => key,
            None if !spec.needs_key => {
                // Ollama and LM Studio ignore the header; send a placeholder
                // rather than refusing to start.
                "not-needed".to_string()
            }
            None => {
                return Err(anyhow!(
                    "No {} API key configured yet. Add it from the web dashboard setup wizard, \
                     run 'pylot init' to store it in the encrypted vault, or set {}.",
                    spec.label,
                    spec.env_key
                ));
            }
        };

        let base_url = providers::resolve_base_url(spec, self.base_url.as_deref());

        let built: Arc<dyn LlmProvider> = match spec.kind {
            Kind::Anthropic => Arc::new(AnthropicProvider::new(
                api_key,
                self.model.clone(),
                self.max_tokens,
            )),
            Kind::OpenAiCompatible => {
                let provider = OpenAIProvider::new(
                    api_key,
                    self.model.clone(),
                    self.max_tokens,
                    self.temperature,
                );
                Arc::new(match base_url {
                    Some(url) => provider.with_endpoint(&url, spec.id),
                    None => provider,
                })
            }
        };

        *self.inner.write().await = Some(Arc::clone(&built));
        tracing::info!(
            "LLM provider '{}' configured — picked up without restart",
            spec.id
        );
        Ok(built)
    }
}

#[async_trait]
impl LlmProvider for LazyProvider {
    async fn chat(&self, messages: &[Message], tools: &[ToolDefinition]) -> Result<LlmResponse> {
        self.resolve().await?.chat(messages, tools).await
    }

    async fn chat_stream(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        stream_tx: StreamSender,
    ) -> Result<LlmResponse> {
        self.resolve().await?.chat_stream(messages, tools, stream_tx).await
    }

    fn supports_streaming(&self) -> bool {
        // Before the real provider exists we must answer without blocking;
        // both concrete providers stream, so report true optimistically.
        match self.inner.try_read() {
            Ok(guard) => guard.as_ref().map(|p| p.supports_streaming()).unwrap_or(true),
            Err(_) => true,
        }
    }

    fn name(&self) -> &str {
        &self.provider
    }

    fn model(&self) -> &str {
        &self.model
    }
}
