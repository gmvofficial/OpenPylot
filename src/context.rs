use crate::llm::Message;

/// Manages the conversation context — system prompt + message history
/// with a bounded window to respect token limits.
pub struct ConversationContext {
    system_prompt: String,
    messages: Vec<Message>,
    max_messages: usize,
}

impl ConversationContext {
    pub fn new(system_prompt: String, max_messages: usize) -> Self {
        Self {
            system_prompt,
            messages: Vec::new(),
            max_messages,
        }
    }

    /// Build the full message list for the LLM: system + history.
    pub fn build_messages(&self) -> Vec<Message> {
        let mut msgs = vec![Message::system(&self.system_prompt)];
        msgs.extend(self.messages.clone());
        msgs
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
    }

    /// Current number of messages (excluding system).
    pub fn len(&self) -> usize {
        self.messages.len()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }
}
