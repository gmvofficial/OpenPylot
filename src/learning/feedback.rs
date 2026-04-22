use crate::learning::prompt_evolution::PromptEvolution;
use crate::learning::types::{RuleSource, UserFeedback};
use chrono::Utc;

/// Handles user feedback and translates it into learned rules.
pub struct FeedbackProcessor;

impl FeedbackProcessor {
    /// Process user feedback. Positive feedback boosts related rules;
    /// negative feedback can generate new avoidance rules.
    pub fn process(
        prompt_evolution: &PromptEvolution,
        feedback: &UserFeedback,
    ) -> Result<(), String> {
        match feedback.rating {
            1 => {
                // Positive: if there's a comment, create a reinforcement rule
                if let Some(comment) = &feedback.comment {
                    if !comment.trim().is_empty() {
                        prompt_evolution.add_rule(
                            &format!("User preference: {comment}"),
                            RuleSource::UserFeedback,
                        )?;
                    }
                }
            }
            -1 => {
                // Negative: create an avoidance rule
                let rule_text = if let Some(comment) = &feedback.comment {
                    format!("Avoid: {comment}")
                } else {
                    "Review approach when user gives negative feedback.".to_string()
                };
                prompt_evolution.add_rule(&rule_text, RuleSource::UserFeedback)?;
            }
            _ => {
                // Neutral: no action
            }
        }
        Ok(())
    }

    /// Create a feedback record.
    pub fn create_feedback(
        session_id: &str,
        turn_id: &str,
        rating: i8,
        comment: Option<String>,
    ) -> UserFeedback {
        UserFeedback {
            session_id: session_id.to_string(),
            turn_id: turn_id.to_string(),
            rating: rating.clamp(-1, 1),
            comment,
            timestamp: Utc::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_positive_feedback_creates_rule() {
        let pe = PromptEvolution::new(":memory:").unwrap();
        let fb = FeedbackProcessor::create_feedback(
            "s1", "t1", 1, Some("I like concise answers".to_string()),
        );
        FeedbackProcessor::process(&pe, &fb).unwrap();
        let rules = pe.active_rules().unwrap();
        assert_eq!(rules.len(), 1);
        assert!(rules[0].rule_text.contains("concise"));
    }

    #[test]
    fn test_negative_feedback_creates_avoidance() {
        let pe = PromptEvolution::new(":memory:").unwrap();
        let fb = FeedbackProcessor::create_feedback(
            "s1", "t1", -1, Some("Too verbose".to_string()),
        );
        FeedbackProcessor::process(&pe, &fb).unwrap();
        let rules = pe.active_rules().unwrap();
        assert_eq!(rules.len(), 1);
        assert!(rules[0].rule_text.contains("Avoid"));
    }
}
