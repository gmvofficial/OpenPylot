use crate::llm::Message;

/// Manages the conversation context — system prompt + message history
/// with a bounded window to respect token limits.
/// Supports context compression via LLM summarization (inspired by Hermes).
pub struct ConversationContext {
    system_prompt: String,
    messages: Vec<Message>,
    max_messages: usize,
    /// Transient context (memories/knowledge) injected before each LLM call, then cleared.
    dynamic_context: Option<String>,
    /// Compressed summaries of older conversation segments.
    compressed_summaries: Vec<String>,
    /// Threshold at which to trigger compression (percentage of max_messages).
    compression_threshold: f64,
    /// Whether compression is enabled.
    compression_enabled: bool,
    /// Number of recent messages to preserve (not compress).
    preserve_recent: usize,
}

impl ConversationContext {
    pub fn new(system_prompt: String, max_messages: usize) -> Self {
        Self {
            system_prompt,
            messages: Vec::new(),
            max_messages,
            dynamic_context: None,
            compressed_summaries: Vec::new(),
            compression_threshold: 0.8,
            compression_enabled: true,
            preserve_recent: 10,
        }
    }

    /// Enable/disable context compression.
    pub fn set_compression_enabled(&mut self, enabled: bool) {
        self.compression_enabled = enabled;
    }

    /// Build the full message list for the LLM: system + summaries + dynamic context + history.
    pub fn build_messages(&mut self) -> Vec<Message> {
        let mut msgs = vec![Message::system(&self.system_prompt)];

        // Inject compressed summaries as context
        if !self.compressed_summaries.is_empty() {
            let summary_ctx = format!(
                "\n## Previous Conversation Summary\n{}\n",
                self.compressed_summaries.join("\n---\n")
            );
            msgs.push(Message::system(&summary_ctx));
        }

        // Inject dynamic memory/knowledge context (consumed on use)
        if let Some(ctx) = self.dynamic_context.take() {
            msgs.push(Message::system(&ctx));
        }

        msgs.extend(self.messages.clone());
        msgs
    }

    /// Set transient context to be included in the next `build_messages()` call.
    pub fn set_dynamic_context(&mut self, context: String) {
        self.dynamic_context = Some(context);
    }

    /// Add a message to the conversation.
    pub fn push(&mut self, msg: Message) {
        self.messages.push(msg);
        self.trim();
    }

    /// Add multiple messages (e.g., after a tool-call round).
    #[allow(dead_code)]
    pub fn extend(&mut self, msgs: impl IntoIterator<Item = Message>) {
        self.messages.extend(msgs);
        self.trim();
    }

    /// Check if context needs compression (past threshold but before hard limit).
    pub fn needs_compression(&self) -> bool {
        self.compression_enabled
            && self.messages.len() as f64 > self.max_messages as f64 * self.compression_threshold
    }

    /// Get the messages that should be compressed (oldest messages, excluding recent).
    pub fn messages_to_compress(&self) -> &[Message] {
        if self.messages.len() <= self.preserve_recent {
            return &[];
        }
        let end = self.messages.len() - self.preserve_recent;
        &self.messages[..end]
    }

    /// Apply compression: store the summary and remove compressed messages.
    pub fn apply_compression(&mut self, summary: String) {
        if self.messages.len() <= self.preserve_recent {
            return;
        }
        let drain_end = self.messages.len() - self.preserve_recent;
        self.messages.drain(..drain_end);
        self.compressed_summaries.push(summary);

        // Keep at most 5 summaries (oldest get merged/dropped)
        if self.compressed_summaries.len() > 5 {
            let merged = self.compressed_summaries[..2].join(" ");
            self.compressed_summaries.drain(..2);
            self.compressed_summaries.insert(0, merged);
        }

        tracing::info!(
            "Context compressed: {} messages remaining, {} summaries stored",
            self.messages.len(),
            self.compressed_summaries.len()
        );
    }

    /// Trim old messages to stay within the bound.
    /// Keeps the most recent messages. Never trims tool-call/result pairs
    /// that are in-flight (we trim from the front).
    fn trim(&mut self) {
        if self.messages.len() > self.max_messages {
            let excess = self.messages.len() - self.max_messages;
            self.messages.drain(..excess);
        }
    }

    /// Clear the conversation (start fresh).
    pub fn clear(&mut self) {
        self.messages.clear();
        self.compressed_summaries.clear();
    }

    /// Current number of messages (excluding system).
    pub fn len(&self) -> usize {
        self.messages.len()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    /// Get the N most recent messages (for memory extraction).
    pub fn recent_messages(&self, n: usize) -> &[Message] {
        let start = self.messages.len().saturating_sub(n);
        &self.messages[start..]
    }

    /// Get total context size including summaries (rough estimate).
    pub fn estimated_tokens(&self) -> usize {
        let msg_chars: usize = self.messages.iter().map(|m| m.content.len()).sum();
        let summary_chars: usize = self.compressed_summaries.iter().map(|s| s.len()).sum();
        let system_chars = self.system_prompt.len();
        // Rough estimate: ~4 chars per token
        (msg_chars + summary_chars + system_chars) / 4
    }
}
