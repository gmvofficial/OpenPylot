use crate::llm::{LlmProvider, Message};
use crate::marketing::types::*;
use chrono::Utc;
use uuid::Uuid;

/// Marketing agent that generates content and manages campaigns.
pub struct MarketingAgent {
    strategies: Vec<ContentStrategy>,
    content_queue: Vec<ContentPiece>,
}

impl MarketingAgent {
    pub fn new() -> Self {
        Self {
            strategies: Vec::new(),
            content_queue: Vec::new(),
        }
    }

    /// Create a content strategy.
    pub fn create_strategy(
        &mut self,
        name: &str,
        target_audience: &str,
        tone: ContentTone,
        platforms: Vec<String>,
        content_pillars: Vec<String>,
    ) -> String {
        let id = Uuid::new_v4().to_string();
        let strategy = ContentStrategy {
            id: id.clone(),
            name: name.to_string(),
            target_audience: target_audience.to_string(),
            tone,
            platforms,
            content_pillars,
            posting_frequency: std::collections::HashMap::new(),
            created_at: Utc::now(),
        };
        self.strategies.push(strategy);
        id
    }

    /// Generate content for a strategy using an LLM.
    pub async fn generate_content(
        &mut self,
        strategy_id: &str,
        platform: &str,
        llm: &dyn LlmProvider,
    ) -> Result<String, String> {
        let strategy = self
            .strategies
            .iter()
            .find(|s| s.id == strategy_id)
            .ok_or("Strategy not found")?
            .clone();

        let prompt = format!(
            "Generate a social media post for {platform}.\n\
             Target audience: {}\n\
             Tone: {:?}\n\
             Content pillars: {}\n\
             Strategy: {}\n\n\
             Requirements:\n\
             - Platform-appropriate length and formatting\n\
             - Include relevant hashtags\n\
             - Be engaging and on-brand\n\n\
             Return ONLY the post content (no explanations).",
            strategy.target_audience,
            strategy.tone,
            strategy.content_pillars.join(", "),
            strategy.name,
        );

        let messages = vec![Message::user(&prompt)];
        let response = llm.chat(&messages, &[]).await.map_err(|e| e.to_string())?;

        let content_text = match response {
            crate::llm::LlmResponse::Text(t) => t,
            _ => return Err("Unexpected response type".to_string()),
        };

        let piece = ContentPiece {
            id: Uuid::new_v4().to_string(),
            strategy_id: strategy_id.to_string(),
            platform: platform.to_string(),
            content: content_text.clone(),
            hashtags: extract_hashtags(&content_text),
            media_suggestions: vec![],
            generated_at: Utc::now(),
            approved: false,
        };
        let piece_id = piece.id.clone();
        self.content_queue.push(piece);
        Ok(piece_id)
    }

    /// Approve a content piece for publishing.
    pub fn approve_content(&mut self, content_id: &str) -> Result<(), String> {
        let piece = self
            .content_queue
            .iter_mut()
            .find(|p| p.id == content_id)
            .ok_or("Content piece not found")?;
        piece.approved = true;
        Ok(())
    }

    /// Get all approved content ready for publishing.
    pub fn approved_content(&self) -> Vec<&ContentPiece> {
        self.content_queue.iter().filter(|p| p.approved).collect()
    }

    /// Get all strategies.
    pub fn strategies(&self) -> &[ContentStrategy] {
        &self.strategies
    }

    /// Get content queue.
    pub fn content_queue(&self) -> &[ContentPiece] {
        &self.content_queue
    }
}

/// Extract hashtags from content.
fn extract_hashtags(content: &str) -> Vec<String> {
    content
        .split_whitespace()
        .filter(|w| w.starts_with('#') && w.len() > 1)
        .map(|w| w.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_strategy() {
        let mut agent = MarketingAgent::new();
        let id = agent.create_strategy(
            "Launch Campaign",
            "developers",
            ContentTone::Technical,
            vec!["twitter".into(), "linkedin".into()],
            vec!["AI".into(), "Rust".into()],
        );
        assert_eq!(agent.strategies().len(), 1);
        assert_eq!(agent.strategies()[0].id, id);
    }

    #[test]
    fn test_extract_hashtags() {
        let content = "Check out our new #AI tool built with #Rust! #OpenSource";
        let tags = extract_hashtags(content);
        assert_eq!(tags, vec!["#AI", "#Rust!", "#OpenSource"]);
    }
}
