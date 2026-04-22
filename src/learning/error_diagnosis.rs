use crate::learning::types::{ErrorCategory, LearnedRule, RuleSource};
use crate::learning::prompt_evolution::PromptEvolution;

/// Diagnoses errors and auto-generates rules to prevent recurrence.
pub struct ErrorDiagnoser;

impl ErrorDiagnoser {
    /// Categorize an error message.
    pub fn categorize(error_msg: &str) -> ErrorCategory {
        let lower = error_msg.to_lowercase();
        if lower.contains("timeout") || lower.contains("connection refused") || lower.contains("503") {
            ErrorCategory::ExternalService
        } else if lower.contains("not found") && (lower.contains("tool") || lower.contains("command")) {
            ErrorCategory::ToolFailure
        } else if lower.contains("config") || lower.contains("missing key") || lower.contains("invalid setting") {
            ErrorCategory::ConfigError
        } else if lower.contains("context") || lower.contains("token limit") || lower.contains("too long") {
            ErrorCategory::ContextOverflow
        } else if lower.contains("syntax") || lower.contains("compile") || lower.contains("parse error") {
            ErrorCategory::CodeError
        } else {
            ErrorCategory::PromptMismatch
        }
    }

    /// Generate a rule suggestion from an error.
    pub fn suggest_rule(error_msg: &str, context: &str) -> Option<String> {
        let category = Self::categorize(error_msg);
        match category {
            ErrorCategory::ToolFailure => Some(format!(
                "Before using a tool, verify it exists and is available. Context: {context}"
            )),
            ErrorCategory::ContextOverflow => Some(
                "Summarize long outputs before including them in context. Use chunking for large documents.".to_string()
            ),
            ErrorCategory::ConfigError => Some(format!(
                "Check configuration values before proceeding. Error seen: {error_msg}"
            )),
            ErrorCategory::ExternalService => Some(
                "Add retry logic with backoff for external service calls.".to_string()
            ),
            ErrorCategory::CodeError => Some(format!(
                "Validate code syntax before execution. Previous error: {error_msg}"
            )),
            ErrorCategory::PromptMismatch => None, // Too vague to auto-generate
        }
    }

    /// Diagnose an error and optionally add a rule.
    pub fn diagnose_and_learn(
        prompt_evolution: &PromptEvolution,
        error_msg: &str,
        context: &str,
    ) -> Result<Option<String>, String> {
        if let Some(rule_text) = Self::suggest_rule(error_msg, context) {
            let id = prompt_evolution.add_rule(&rule_text, RuleSource::ErrorDiagnosis)?;
            Ok(Some(id))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_categorize_errors() {
        assert!(matches!(
            ErrorDiagnoser::categorize("connection refused to API"),
            ErrorCategory::ExternalService
        ));
        assert!(matches!(
            ErrorDiagnoser::categorize("tool not found: ripgrep"),
            ErrorCategory::ToolFailure
        ));
        assert!(matches!(
            ErrorDiagnoser::categorize("context token limit exceeded"),
            ErrorCategory::ContextOverflow
        ));
    }

    #[test]
    fn test_suggest_rule() {
        let rule = ErrorDiagnoser::suggest_rule("tool not found: fzf", "searching files");
        assert!(rule.is_some());
        assert!(rule.unwrap().contains("verify"));
    }
}
