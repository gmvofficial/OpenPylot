//! The catalogue of LLM providers OpenPylot can talk to.
//!
//! # Why this exists
//!
//! There were two providers, Anthropic and OpenAI, and the provider name was
//! matched with `_ => OpenAI` in both the key lookup and the constructor. So
//! setting `llm.provider = "ollama"` did not error — it quietly built an OpenAI
//! client and then failed with an authentication message about a key the user
//! had never been asked for. A typo behaved the same way.
//!
//! Two things fix that. First, the match is exhaustive: an unrecognised
//! provider is a named error that lists the ones that exist. Second, the
//! OpenAI wire format is the de-facto standard — Ollama, OpenRouter, Groq,
//! Together, DeepSeek, Mistral, LM Studio and vLLM all serve
//! `/v1/chat/completions` — so supporting them is a base URL, not a new client.
//!
//! Google Gemini is deliberately absent: its API is a different shape and needs
//! its own implementation rather than an entry here.

/// A provider OpenPylot knows how to reach.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provider {
    /// Canonical id, as written in config.
    pub id: &'static str,
    /// Name for humans.
    pub label: &'static str,
    /// Wire protocol.
    pub kind: Kind,
    /// API root. `None` means the provider's own default.
    pub base_url: Option<&'static str>,
    /// Environment variable holding the key.
    pub env_key: &'static str,
    /// Key path in the encrypted vault.
    pub vault_key: &'static str,
    /// A sensible default model.
    pub default_model: &'static str,
    /// Whether a key is needed at all. Local servers do not take one.
    pub needs_key: bool,
}

/// Which client implementation a provider uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Anthropic's Messages API.
    Anthropic,
    /// OpenAI's chat-completions API, or anything that serves the same shape.
    OpenAiCompatible,
}

/// Everything OpenPylot can talk to, in the order `pylot init` offers them.
pub const PROVIDERS: &[Provider] = &[
    Provider {
        id: "anthropic",
        label: "Anthropic (Claude)",
        kind: Kind::Anthropic,
        base_url: None,
        env_key: "ANTHROPIC_API_KEY",
        vault_key: "llm.anthropic.api_key",
        default_model: "claude-sonnet-5",
        needs_key: true,
    },
    Provider {
        id: "openai",
        label: "OpenAI",
        kind: Kind::OpenAiCompatible,
        base_url: None,
        env_key: "OPENAI_API_KEY",
        vault_key: "llm.openai.api_key",
        default_model: "gpt-4o",
        needs_key: true,
    },
    Provider {
        id: "ollama",
        label: "Ollama (local, no key)",
        kind: Kind::OpenAiCompatible,
        base_url: Some("http://localhost:11434/v1"),
        env_key: "OLLAMA_API_KEY",
        vault_key: "llm.ollama.api_key",
        default_model: "llama3.1",
        // Ollama ignores the Authorization header entirely. Requiring a key
        // would make the one provider that needs no account the hardest to set
        // up, which is backwards.
        needs_key: false,
    },
    Provider {
        id: "openrouter",
        label: "OpenRouter (many models, one key)",
        kind: Kind::OpenAiCompatible,
        base_url: Some("https://openrouter.ai/api/v1"),
        env_key: "OPENROUTER_API_KEY",
        vault_key: "llm.openrouter.api_key",
        default_model: "anthropic/claude-sonnet-4.5",
        needs_key: true,
    },
    Provider {
        id: "groq",
        label: "Groq",
        kind: Kind::OpenAiCompatible,
        base_url: Some("https://api.groq.com/openai/v1"),
        env_key: "GROQ_API_KEY",
        vault_key: "llm.groq.api_key",
        default_model: "llama-3.3-70b-versatile",
        needs_key: true,
    },
    Provider {
        id: "together",
        label: "Together AI",
        kind: Kind::OpenAiCompatible,
        base_url: Some("https://api.together.xyz/v1"),
        env_key: "TOGETHER_API_KEY",
        vault_key: "llm.together.api_key",
        default_model: "meta-llama/Llama-3.3-70B-Instruct-Turbo",
        needs_key: true,
    },
    Provider {
        id: "deepseek",
        label: "DeepSeek",
        kind: Kind::OpenAiCompatible,
        base_url: Some("https://api.deepseek.com/v1"),
        env_key: "DEEPSEEK_API_KEY",
        vault_key: "llm.deepseek.api_key",
        default_model: "deepseek-chat",
        needs_key: true,
    },
    Provider {
        id: "mistral",
        label: "Mistral",
        kind: Kind::OpenAiCompatible,
        base_url: Some("https://api.mistral.ai/v1"),
        env_key: "MISTRAL_API_KEY",
        vault_key: "llm.mistral.api_key",
        default_model: "mistral-large-latest",
        needs_key: true,
    },
    Provider {
        id: "lmstudio",
        label: "LM Studio (local, no key)",
        kind: Kind::OpenAiCompatible,
        base_url: Some("http://localhost:1234/v1"),
        env_key: "LMSTUDIO_API_KEY",
        vault_key: "llm.lmstudio.api_key",
        default_model: "local-model",
        needs_key: false,
    },
    Provider {
        id: "custom",
        label: "Custom OpenAI-compatible endpoint",
        kind: Kind::OpenAiCompatible,
        // Supplied by the user through `llm.base_url`; see `resolve_base_url`.
        base_url: None,
        env_key: "LLM_API_KEY",
        vault_key: "llm.custom.api_key",
        default_model: "default",
        needs_key: false,
    },
];

/// Look up a provider by id. Aliases are accepted for the spellings people
/// actually type.
pub fn find(id: &str) -> Option<&'static Provider> {
    let id = id.trim().to_ascii_lowercase();
    let canonical = match id.as_str() {
        "claude" => "anthropic",
        "gpt" | "chatgpt" => "openai",
        "local" | "llama" => "ollama",
        "lm-studio" | "lm_studio" => "lmstudio",
        "open-router" | "open_router" => "openrouter",
        other => other,
    };
    PROVIDERS.iter().find(|p| p.id == canonical)
}

/// Every provider id, for error messages and completion.
pub fn ids() -> Vec<&'static str> {
    PROVIDERS.iter().map(|p| p.id).collect()
}

/// The error a bad provider id produces.
///
/// Names the alternatives, because the situation this replaces was a silent
/// fallback to OpenAI followed by a confusing authentication failure.
pub fn unknown_provider_error(id: &str) -> String {
    format!(
        "Unknown LLM provider '{id}'. Available: {}.\nSet one with: pylot config set llm.provider <name>",
        ids().join(", ")
    )
}

/// Resolve the API root for a provider, honouring a user override.
///
/// `configured` comes from `llm.base_url` and always wins — that is what makes
/// `custom` usable, and lets someone point `ollama` at another machine.
pub fn resolve_base_url(provider: &Provider, configured: Option<&str>) -> Option<String> {
    configured
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .or_else(|| provider.base_url.map(str::to_string))
}

/// Build a client for `spec` with a key already in hand.
///
/// One place where a provider id becomes a client, so the `_ => OpenAI`
/// fallback cannot come back in a third copy of this logic.
pub fn build(
    spec: &Provider,
    api_key: String,
    model: String,
    max_tokens: u32,
    temperature: f64,
    configured_base_url: Option<&str>,
) -> std::sync::Arc<dyn crate::llm::LlmProvider> {
    use std::sync::Arc;

    match spec.kind {
        Kind::Anthropic => Arc::new(crate::llm::anthropic::AnthropicProvider::new(
            api_key, model, max_tokens,
        )),
        Kind::OpenAiCompatible => {
            let provider =
                crate::llm::openai::OpenAIProvider::new(api_key, model, max_tokens, temperature);
            match resolve_base_url(spec, configured_base_url) {
                Some(url) => Arc::new(provider.with_endpoint(&url, spec.id)),
                None => Arc::new(provider),
            }
        }
    }
}

/// The key for `spec`, from the environment first and the vault second.
pub fn find_key(spec: &Provider) -> Option<String> {
    if let Ok(value) = std::env::var(spec.env_key) {
        if !value.is_empty() {
            return Some(value);
        }
    }
    crate::secrets::SecretsVault::open(&crate::secrets::default_secrets_path(), None)
        .ok()
        .and_then(|v| v.get(spec.vault_key))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn ids_are_unique() {
        let ids = ids();
        assert_eq!(ids.iter().collect::<HashSet<_>>().len(), ids.len());
    }

    #[test]
    fn every_provider_is_fully_described() {
        for p in PROVIDERS {
            assert!(!p.id.is_empty());
            assert!(!p.label.is_empty(), "{}: no label for the setup wizard", p.id);
            assert!(!p.env_key.is_empty(), "{}: no env var", p.id);
            assert!(!p.default_model.is_empty(), "{}: no default model", p.id);
            assert!(
                p.vault_key.starts_with("llm."),
                "{}: vault key should be namespaced, got {}",
                p.id,
                p.vault_key
            );
        }
    }

    #[test]
    fn ids_are_lowercase_so_lookup_is_predictable() {
        for p in PROVIDERS {
            assert_eq!(p.id, p.id.to_ascii_lowercase());
        }
    }

    #[test]
    fn base_urls_are_absolute_and_carry_no_trailing_path() {
        for p in PROVIDERS {
            let Some(url) = p.base_url else { continue };
            assert!(url.starts_with("http"), "{}: {url}", p.id);
            assert!(
                !url.ends_with('/'),
                "{}: trailing slash would double up when the path is appended",
                p.id
            );
            assert!(
                !url.contains("/chat/completions"),
                "{}: base_url is the API root, not the endpoint",
                p.id
            );
        }
    }

    #[test]
    fn local_providers_do_not_demand_a_key() {
        // Requiring one would make the providers that need no account the
        // hardest to set up.
        for id in ["ollama", "lmstudio"] {
            assert!(!find(id).unwrap().needs_key, "{id} should not need a key");
        }
    }

    #[test]
    fn hosted_providers_do_demand_a_key() {
        for id in ["openai", "anthropic", "openrouter", "groq"] {
            assert!(find(id).unwrap().needs_key, "{id} should need a key");
        }
    }

    #[test]
    fn only_anthropic_uses_the_anthropic_client() {
        let anthropic: Vec<&str> = PROVIDERS
            .iter()
            .filter(|p| p.kind == Kind::Anthropic)
            .map(|p| p.id)
            .collect();
        assert_eq!(anthropic, vec!["anthropic"]);
    }

    #[test]
    fn lookup_finds_every_listed_provider() {
        for id in ids() {
            assert!(find(id).is_some(), "{id} is listed but not findable");
        }
    }

    #[test]
    fn lookup_accepts_the_spellings_people_type() {
        assert_eq!(find("claude").unwrap().id, "anthropic");
        assert_eq!(find("ChatGPT").unwrap().id, "openai");
        assert_eq!(find("  OLLAMA  ").unwrap().id, "ollama");
        assert_eq!(find("lm-studio").unwrap().id, "lmstudio");
    }

    #[test]
    fn an_unknown_provider_is_not_silently_resolved() {
        // This is the bug: `_ => OpenAI` meant a typo became an OpenAI client
        // that then failed with an unrelated authentication error.
        assert!(find("gemini").is_none());
        assert!(find("typo").is_none());
        assert!(find("").is_none());
    }

    #[test]
    fn the_unknown_provider_error_lists_the_alternatives() {
        let message = unknown_provider_error("gemeni");
        assert!(message.contains("gemeni"));
        assert!(message.contains("ollama"), "{message}");
        assert!(message.contains("anthropic"), "{message}");
        assert!(message.contains("pylot config set"), "should say how to fix it");
    }

    #[test]
    fn a_configured_base_url_overrides_the_default() {
        let ollama = find("ollama").unwrap();
        assert_eq!(
            resolve_base_url(ollama, Some("http://gpu-box:11434/v1")).as_deref(),
            Some("http://gpu-box:11434/v1"),
            "pointing ollama at another machine must work"
        );
    }

    #[test]
    fn a_blank_base_url_falls_back_to_the_default() {
        let ollama = find("ollama").unwrap();
        assert_eq!(
            resolve_base_url(ollama, Some("   ")).as_deref(),
            Some("http://localhost:11434/v1")
        );
        assert_eq!(
            resolve_base_url(ollama, None).as_deref(),
            Some("http://localhost:11434/v1")
        );
    }

    #[test]
    fn openai_has_no_base_url_so_the_client_uses_its_own_default() {
        assert_eq!(resolve_base_url(find("openai").unwrap(), None), None);
    }

    #[test]
    fn custom_is_useless_without_a_configured_url() {
        // It exists precisely to be pointed somewhere, so it ships with none.
        let custom = find("custom").unwrap();
        assert_eq!(resolve_base_url(custom, None), None);
        assert_eq!(
            resolve_base_url(custom, Some("http://internal:8000/v1")).as_deref(),
            Some("http://internal:8000/v1")
        );
    }
}

#[cfg(test)]
mod build_tests {
    use super::*;

    fn client(id: &str, base_url: Option<&str>) -> std::sync::Arc<dyn crate::llm::LlmProvider> {
        let spec = find(id).expect("provider should exist");
        build(spec, "k".into(), "m".into(), 100, 0.7, base_url)
    }

    #[test]
    fn each_provider_reports_its_own_name() {
        // "Failed to reach OpenAI" on an Ollama outage sends people looking in
        // the wrong place.
        assert_eq!(client("ollama", None).name(), "ollama");
        assert_eq!(client("groq", None).name(), "groq");
        assert_eq!(client("openai", None).name(), "openai");
    }

    #[test]
    fn anthropic_gets_the_anthropic_client() {
        assert_eq!(client("anthropic", None).name(), "Anthropic");
    }

    #[test]
    fn the_model_is_carried_through() {
        assert_eq!(client("ollama", None).model(), "m");
    }

    #[test]
    fn a_configured_base_url_reaches_the_client() {
        // Not directly observable through the trait, so this asserts the call
        // is accepted and does not panic — the endpoint itself is covered by
        // `endpoint_is_built_from_the_base_url`.
        let _ = client("custom", Some("http://internal:8000/v1"));
    }

    #[test]
    fn endpoint_is_built_from_the_base_url() {
        use crate::llm::openai::OpenAIProvider;
        let p = OpenAIProvider::new("k".into(), "m".into(), 10, 0.0)
            .with_endpoint("http://localhost:11434/v1", "ollama");
        assert_eq!(p.endpoint_for_test(), "http://localhost:11434/v1/chat/completions");
    }

    #[test]
    fn a_trailing_slash_on_the_base_url_does_not_double_up() {
        use crate::llm::openai::OpenAIProvider;
        // Both spellings must produce the same endpoint; a `//chat/completions`
        // 404s on most servers.
        let with = OpenAIProvider::new("k".into(), "m".into(), 10, 0.0)
            .with_endpoint("http://x/v1/", "a");
        let without = OpenAIProvider::new("k".into(), "m".into(), 10, 0.0)
            .with_endpoint("http://x/v1", "a");
        assert_eq!(with.endpoint_for_test(), without.endpoint_for_test());
        assert_eq!(with.endpoint_for_test(), "http://x/v1/chat/completions");
    }

    #[test]
    fn every_catalogue_provider_builds() {
        for spec in PROVIDERS {
            let built = build(spec, "k".into(), "m".into(), 10, 0.0, Some("http://x/v1"));
            assert!(!built.name().is_empty(), "{} built namelessly", spec.id);
        }
    }
}
