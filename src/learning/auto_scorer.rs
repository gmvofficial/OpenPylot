use crate::learning::prompt_evolution::PromptEvolution;
use crate::learning::types::RuleSource;
use crate::llm::{LlmProvider, LlmResponse, Message};
use serde::{Deserialize, Serialize};

/// Number of LLM judge votes per evaluation (majority voting).
const DEFAULT_VOTES: usize = 3;

/// Score result from LLM-as-judge evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreResult {
    /// Final score after majority vote: +1 (helpful), -1 (unhelpful), 0 (unclear).
    pub score: i8,
    /// Individual vote values from each judge call.
    pub votes: Vec<i8>,
    /// Brief explanation from the first judge call (if available).
    pub explanation: Option<String>,
}

/// Automatic response quality scorer using LLM-as-judge with majority voting.
///
/// Inspired by MetaClaw's PRM scorer — sends the user instruction + assistant
/// response to the same LLM (or a separate judge model) multiple times, then
/// takes the majority vote. The result is fed into `PromptEvolution` to
/// automatically boost or decay learned rules without manual user feedback.
pub struct AutoScorer {
    /// Number of judge votes per evaluation.
    num_votes: usize,
}

impl AutoScorer {
    pub fn new() -> Self {
        Self {
            num_votes: DEFAULT_VOTES,
        }
    }

    pub fn with_votes(mut self, n: usize) -> Self {
        self.num_votes = n.max(1);
        self
    }

    /// Evaluate a single turn: score the assistant's response given the user's instruction.
    pub async fn evaluate(
        &self,
        llm: &dyn LlmProvider,
        user_instruction: &str,
        assistant_response: &str,
    ) -> Result<ScoreResult, String> {
        let judge_messages = Self::build_judge_prompt(user_instruction, assistant_response);

        let mut votes: Vec<i8> = Vec::with_capacity(self.num_votes);
        let mut first_explanation: Option<String> = None;

        for i in 0..self.num_votes {
            match llm.chat(&judge_messages, &[]).await {
                Ok(LlmResponse::Text(text)) => {
                    let score = Self::parse_score(&text);
                    votes.push(score);
                    if i == 0 {
                        first_explanation = Some(text);
                    }
                }
                Ok(_) => {
                    // Tool call response — treat as unclear
                    votes.push(0);
                }
                Err(e) => {
                    // LLM error — skip this vote
                    tracing::warn!("AutoScorer judge call {i} failed: {e}");
                }
            }
        }

        let score = Self::majority_vote(&votes);

        Ok(ScoreResult {
            score,
            votes,
            explanation: first_explanation,
        })
    }

    /// Evaluate and automatically feed the result into prompt evolution.
    ///
    /// - Score +1: boost all active rules (they're working)
    /// - Score -1: create an avoidance rule from the explanation, decay active rules
    /// - Score  0: no action
    pub async fn evaluate_and_learn(
        &self,
        llm: &dyn LlmProvider,
        prompt_evolution: &PromptEvolution,
        user_instruction: &str,
        assistant_response: &str,
    ) -> Result<ScoreResult, String> {
        let result = self.evaluate(llm, user_instruction, assistant_response).await?;

        match result.score {
            1 => {
                // Positive: boost active rules
                let rules = prompt_evolution.active_rules()?;
                for rule in &rules {
                    let _ = prompt_evolution.record_success(&rule.id);
                }
            }
            -1 => {
                // Negative: create an avoidance rule from the judge explanation
                let rule_text = if let Some(ref explanation) = result.explanation {
                    let summary = Self::extract_failure_reason(explanation);
                    format!("Auto-detected issue: {summary}")
                } else {
                    format!(
                        "Avoid approach used for: {}",
                        truncate(user_instruction, 100)
                    )
                };
                prompt_evolution.add_rule(&rule_text, RuleSource::ConversationInsight)?;

                // Decay active rules slightly
                let rules = prompt_evolution.active_rules()?;
                for rule in &rules {
                    let _ = prompt_evolution.record_failure(&rule.id);
                }
            }
            _ => {
                // Neutral: no action
            }
        }

        Ok(result)
    }

    /// Build the judge prompt messages (system + user).
    fn build_judge_prompt(user_instruction: &str, assistant_response: &str) -> Vec<Message> {
        let system = Message::system(
            "You are a quality reviewer for AI assistant responses.\n\
             You will be shown a user instruction and the assistant's response.\n\
             Evaluate whether the response is helpful and correctly addresses the instruction.\n\n\
             Scoring criteria:\n\
             - Score: 1 — Response clearly follows and substantially completes the instruction.\n\
             - Score: -1 — Response is off-task, wrong, or fails to complete core requirements.\n\
             - Score: 0 — Completion is ambiguous or evidence is insufficient.\n\n\
             Think briefly (2-3 sentences), then end your reply with exactly:\n\
             Score: 1, Score: -1, or Score: 0",
        );

        let user = Message::user(format!(
            "Instruction:\n{}\n\nResponse:\n{}\n\nWas the response helpful? End with Score: 1, Score: -1, or Score: 0.",
            truncate(user_instruction, 2000),
            truncate(assistant_response, 3000),
        ));

        vec![system, user]
    }

    /// Parse the judge's score from the response text.
    fn parse_score(text: &str) -> i8 {
        // Look for "Score: N" pattern (last occurrence)
        let lower = text.to_lowercase();
        if let Some(pos) = lower.rfind("score:") {
            let after = &text[pos + 6..];
            let trimmed = after.trim();
            if trimmed.starts_with("-1") {
                return -1;
            } else if trimmed.starts_with('1') {
                return 1;
            } else if trimmed.starts_with('0') {
                return 0;
            }
        }
        // Fallback: unclear
        0
    }

    /// Majority vote: most common score wins. Ties → 0.
    fn majority_vote(votes: &[i8]) -> i8 {
        if votes.is_empty() {
            return 0;
        }
        let pos = votes.iter().filter(|&&v| v == 1).count();
        let neg = votes.iter().filter(|&&v| v == -1).count();
        let neu = votes.iter().filter(|&&v| v == 0).count();

        if pos > neg && pos > neu {
            1
        } else if neg > pos && neg > neu {
            -1
        } else {
            0
        }
    }

    /// Extract a concise failure reason from the judge explanation.
    fn extract_failure_reason(explanation: &str) -> String {
        // Take the first sentence before "Score:" as the reason
        let before_score = if let Some(pos) = explanation.to_lowercase().find("score:") {
            &explanation[..pos]
        } else {
            explanation
        };
        let trimmed = before_score.trim();
        // Take up to 200 chars
        truncate(trimmed, 200).to_string()
    }
}

fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max {
        s
    } else {
        // Find a char boundary
        let mut end = max;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        &s[..end]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_score_positive() {
        assert_eq!(AutoScorer::parse_score("The response was good. Score: 1"), 1);
    }

    #[test]
    fn test_parse_score_negative() {
        assert_eq!(
            AutoScorer::parse_score("Off-topic answer. Score: -1"),
            -1
        );
    }

    #[test]
    fn test_parse_score_neutral() {
        assert_eq!(AutoScorer::parse_score("Hard to tell. Score: 0"), 0);
    }

    #[test]
    fn test_parse_score_no_match() {
        assert_eq!(AutoScorer::parse_score("No score here"), 0);
    }

    #[test]
    fn test_majority_vote() {
        assert_eq!(AutoScorer::majority_vote(&[1, 1, -1]), 1);
        assert_eq!(AutoScorer::majority_vote(&[-1, -1, 1]), -1);
        assert_eq!(AutoScorer::majority_vote(&[1, -1, 0]), 0); // tie
        assert_eq!(AutoScorer::majority_vote(&[0, 0, 1]), 0);
        assert_eq!(AutoScorer::majority_vote(&[]), 0);
    }

    #[test]
    fn test_extract_failure_reason() {
        let explanation = "The response failed to address the core question about configuration. It went off on a tangent about unrelated features. Score: -1";
        let reason = AutoScorer::extract_failure_reason(explanation);
        assert!(reason.contains("failed to address"));
        assert!(!reason.contains("Score:"));
    }

    #[test]
    fn test_truncate_utf8() {
        assert_eq!(truncate("hello world", 5), "hello");
        assert_eq!(truncate("hi", 10), "hi");
    }
}
