use anyhow::Result;

use crate::llm::{LlmProvider, Message};
use super::types::{MemoryType, MemoryUnit};

/// Extracts structured memory units from conversation turns using LLM.
pub struct MemoryExtractor;

impl MemoryExtractor {
    const EXTRACTION_PROMPT: &'static str = r#"Analyze the following conversation and extract distinct memory units.
For each unit, provide a JSON object with:
- "type": one of ["episodic", "semantic", "preference", "project_state", "working_summary", "procedural_observation"]
- "content": the factual content (1-3 sentences)
- "entities": array of key entities mentioned (people, projects, technologies)
- "topics": array of relevant topic tags
- "importance": 0.0-1.0 (how important to remember)

Memory type guide:
- episodic: specific events, interactions, things that happened
- semantic: general facts, domain knowledge, project info
- preference: user preferences, style choices, requirements
- project_state: current project goals, decisions, blockers, progress
- working_summary: compressed context summary of the session
- procedural_observation: learned patterns, successful workflows, how-tos

Return ONLY a JSON array. If nothing worth remembering, return [].

Conversation:
"#;

    /// Extract memory units from recent messages.
    pub async fn extract(
        llm: &dyn LlmProvider,
        messages: &[Message],
        user_id: &str,
    ) -> Result<Vec<MemoryUnit>> {
        if messages.is_empty() {
            return Ok(vec![]);
        }

        // Build conversation text
        let mut conv_text = String::new();
        for msg in messages {
            let role = match msg.role {
                crate::llm::Role::User => "User",
                crate::llm::Role::Assistant => "Assistant",
                crate::llm::Role::System => continue,
                crate::llm::Role::Tool => continue,
            };
            if !msg.content.is_empty() {
                conv_text.push_str(&format!("{}: {}\n", role, msg.content));
            }
        }

        if conv_text.trim().is_empty() {
            return Ok(vec![]);
        }

        let prompt = format!("{}{}", Self::EXTRACTION_PROMPT, conv_text);
        let extract_messages = vec![Message::user(&prompt)];
        let response = llm.chat(&extract_messages, &[]).await?;

        let text = match response {
            crate::llm::LlmResponse::Text(t) => t,
            _ => return Ok(vec![]),
        };

        Self::parse_extraction_response(&text, user_id)
    }

    /// Parse the LLM's JSON array response into MemoryUnits.
    fn parse_extraction_response(text: &str, user_id: &str) -> Result<Vec<MemoryUnit>> {
        // Extract JSON array from potential markdown code blocks
        let json_str = text
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();

        let items: Vec<serde_json::Value> = match serde_json::from_str(json_str) {
            Ok(arr) => arr,
            Err(_) => {
                tracing::debug!("Failed to parse extraction response as JSON array");
                return Ok(vec![]);
            }
        };

        let mut units = Vec::new();
        for item in items {
            let type_str = item["type"].as_str().unwrap_or("semantic");
            let content = match item["content"].as_str() {
                Some(c) if !c.trim().is_empty() => c.to_string(),
                _ => continue,
            };
            let memory_type = MemoryType::from_str(type_str).unwrap_or(MemoryType::Semantic);

            let entities: Vec<String> = item["entities"]
                .as_array()
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default();

            let topics: Vec<String> = item["topics"]
                .as_array()
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default();

            let importance = item["importance"].as_f64().unwrap_or(0.5).clamp(0.0, 1.0);

            let mut unit = MemoryUnit::new(memory_type, content, user_id.to_string());
            unit.entities = entities;
            unit.topics = topics;
            unit.importance = importance;
            units.push(unit);
        }

        Ok(units)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_extraction_json() {
        let json = r#"[
            {"type": "semantic", "content": "User works with Rust", "entities": ["Rust"], "topics": ["programming"], "importance": 0.8},
            {"type": "preference", "content": "User prefers dark mode", "entities": [], "topics": ["ui"], "importance": 0.6}
        ]"#;

        let units = MemoryExtractor::parse_extraction_response(json, "user1").unwrap();
        assert_eq!(units.len(), 2);
        assert_eq!(units[0].memory_type, MemoryType::Semantic);
        assert_eq!(units[0].entities, vec!["Rust"]);
        assert_eq!(units[1].memory_type, MemoryType::Preference);
        assert_eq!(units[1].importance, 0.6);
    }

    #[test]
    fn test_parse_extraction_with_code_block() {
        let json = "```json\n[{\"type\": \"episodic\", \"content\": \"Had a meeting\", \"entities\": [], \"topics\": [], \"importance\": 0.5}]\n```";
        let units = MemoryExtractor::parse_extraction_response(json, "u1").unwrap();
        assert_eq!(units.len(), 1);
        assert_eq!(units[0].memory_type, MemoryType::Episodic);
    }

    #[test]
    fn test_parse_extraction_empty() {
        let units = MemoryExtractor::parse_extraction_response("[]", "u1").unwrap();
        assert!(units.is_empty());
    }

    #[test]
    fn test_parse_extraction_invalid() {
        let units = MemoryExtractor::parse_extraction_response("not json", "u1").unwrap();
        assert!(units.is_empty());
    }
}
