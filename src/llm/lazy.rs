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
use crate::llm::{LlmProvider, LlmResponse, Message};
use crate::streaming::StreamSender;
use crate::tools::ToolDefinition;

pub struct LazyProvider {
    provider: String,
    model: String,
    max_tokens: u32,
    temperature: f64,
    inner: RwLock<Option<Arc<dyn LlmProvider>>>,
}

impl LazyProvider {
    pub fn new(provider: String, model: String, max_tokens: u32, temperature: f64) -> Self {
        Self {
            provider,
            model,
            max_tokens,
            temperature,
            inner: RwLock::new(None),
        }
    }

    fn find_key(&self) -> Option<String> {
        let (env_key, vault_key) = match self.provider.as_str() {
            "anthropic" => ("ANTHROPIC_API_KEY", "llm.anthropic.api_key"),
            _ => ("OPENAI_API_KEY", "llm.openai.api_key"),
        };
        if let Ok(v) = std::env::var(env_key) {
            if !v.is_empty() {
                return Some(v);
            }
        }
        crate::secrets::SecretsVault::open(&crate::secrets::default_secrets_path(), None)
            .ok()
            .and_then(|v| v.get(vault_key))
    }

    async fn resolve(&self) -> Result<Arc<dyn LlmProvider>> {
        if let Some(p) = self.inner.read().await.as_ref() {
            return Ok(Arc::clone(p));
        }
        let Some(api_key) = self.find_key() else {
            return Err(anyhow!(
                "No {} API key configured yet. Add it from the web dashboard setup wizard, \
                 or run 'pylot init' in a terminal to store it in the encrypted vault.",
                self.provider
            ));
        };
        let built: Arc<dyn LlmProvider> = match self.provider.as_str() {
            "anthropic" => Arc::new(AnthropicProvider::new(
                api_key,
                self.model.clone(),
                self.max_tokens,
            )),
            _ => Arc::new(OpenAIProvider::new(
                api_key,
                self.model.clone(),
                self.max_tokens,
                self.temperature,
            )),
        };
        *self.inner.write().await = Some(Arc::clone(&built));
        tracing::info!(
            "LLM provider '{}' configured from vault — picked up without restart",
            self.provider
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
