use anyhow::Result;
use async_trait::async_trait;

use crate::llm::{LlmProvider, LlmResponse, Message};
use crate::streaming::StreamSender;
use crate::tools::ToolDefinition;

/// A provider that tries multiple LLM providers in order,
/// falling back to the next one on failure. Inspired by Hermes'
/// ordered fallback provider chains with automatic rotation.
pub struct FallbackProvider {
    providers: Vec<Box<dyn LlmProvider>>,
    max_retries: usize,
}

impl FallbackProvider {
    pub fn new(providers: Vec<Box<dyn LlmProvider>>) -> Self {
        Self {
            providers,
            max_retries: 2,
        }
    }

    #[allow(dead_code)]
    pub fn with_retries(mut self, retries: usize) -> Self {
        self.max_retries = retries;
        self
    }
}

#[async_trait]
impl LlmProvider for FallbackProvider {
    async fn chat(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
    ) -> Result<LlmResponse> {
        let mut last_error = None;

        for (i, provider) in self.providers.iter().enumerate() {
            for attempt in 0..=self.max_retries {
                match provider.chat(messages, tools).await {
                    Ok(response) => return Ok(response),
                    Err(e) => {
                        let err_str = e.to_string();
                        tracing::warn!(
                            "Provider {} ({}) attempt {}/{} failed: {}",
                            provider.name(),
                            provider.model(),
                            attempt + 1,
                            self.max_retries + 1,
                            err_str
                        );

                        // Don't retry on auth errors (401/403), fall through to next provider
                        if err_str.contains("401") || err_str.contains("403") || err_str.contains("Unauthorized") {
                            last_error = Some(e);
                            break;
                        }

                        // Rate limit — wait before retry
                        if err_str.contains("429") || err_str.contains("rate") {
                            tokio::time::sleep(tokio::time::Duration::from_secs(2u64.pow(attempt as u32))).await;
                        }

                        last_error = Some(e);
                    }
                }
            }

            if i < self.providers.len() - 1 {
                tracing::info!(
                    "Falling back from {} to {}",
                    provider.name(),
                    self.providers[i + 1].name()
                );
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("No providers configured")))
    }

    async fn chat_stream(
        &self,
        messages: &[Message],
        tools: &[ToolDefinition],
        stream_tx: StreamSender,
    ) -> Result<LlmResponse> {
        let mut last_error = None;

        for (i, provider) in self.providers.iter().enumerate() {
            match provider.chat_stream(messages, tools, stream_tx.clone()).await {
                Ok(response) => return Ok(response),
                Err(e) => {
                    tracing::warn!(
                        "Streaming provider {} failed: {}",
                        provider.name(),
                        e
                    );
                    last_error = Some(e);

                    if i < self.providers.len() - 1 {
                        tracing::info!(
                            "Falling back from {} to {}",
                            provider.name(),
                            self.providers[i + 1].name()
                        );
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("No providers configured")))
    }

    fn supports_streaming(&self) -> bool {
        self.providers.iter().any(|p| p.supports_streaming())
    }

    fn name(&self) -> &str {
        self.providers
            .first()
            .map(|p| p.name())
            .unwrap_or("Fallback")
    }

    fn model(&self) -> &str {
        self.providers
            .first()
            .map(|p| p.model())
            .unwrap_or("unknown")
    }
}
