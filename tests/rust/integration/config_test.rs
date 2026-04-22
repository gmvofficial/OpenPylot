// Config tests must run serially because they modify process-level env vars.
// Use `cargo test --test config_test -- --test-threads=1`
// or run them as a single combined test to avoid interference.

/// Combined config test — runs all assertions sequentially in one function
/// to avoid env var race conditions between parallel tests.
#[test]
fn test_config_loading() {
    // ── Test 1: Defaults ──
    // Clear env vars that might interfere.
    // Use set_var("") instead of remove_var so that dotenvy
    // (which only sets vars not already present) won't re-load
    // values from .env.  env_opt filters empty strings to None.
    for key in &[
        "OPENAI_API_KEY",
        "ANTHROPIC_API_KEY",
        "LLM_PROVIDER",
        "LLM_MODEL",
        "PYLOT_DATA_DIR",
        "AGENT_NAME",
        "AGENT_PERSONA",
        "GOOGLE_CLIENT_ID",
        "GOOGLE_CLIENT_SECRET",
        "GOOGLE_CALENDAR_ID",
        "TELEGRAM_BOT_TOKEN",
        "TWILIO_ACCOUNT_SID",
        "PYLOT_SCHEDULER_ENABLED",
    ] {
        std::env::set_var(key, "");
    }

    let cfg = pylot::config::AppConfig::load()
        .expect("Config loading should succeed with defaults");
    assert_eq!(cfg.agent_name, "Pylot");
    assert_eq!(cfg.llm_provider, "openai");
    assert_eq!(cfg.llm_max_tokens, 4096);
    assert!(cfg.google_calendar_enabled); // enabled = true in default.toml
    assert!(cfg.telegram_enabled); // enabled = true in default.toml + bot token in .env
    assert!(!cfg.whatsapp_enabled);
    assert!(!cfg.scheduler_enabled);

    // ── Test 2: Env override ──
    std::env::set_var("AGENT_NAME", "Test Agent");
    std::env::set_var("LLM_PROVIDER", "anthropic");
    std::env::set_var("ANTHROPIC_API_KEY", "test-key-123");
    std::env::set_var("LLM_MODEL", "claude-sonnet-4-20250514");

    let cfg = pylot::config::AppConfig::load().unwrap();
    assert_eq!(cfg.agent_name, "Test Agent");
    assert_eq!(cfg.llm_provider, "anthropic");
    assert_eq!(cfg.anthropic_api_key.as_deref(), Some("test-key-123"));
    assert!(
        cfg.llm_model.contains("claude"),
        "Model should be Claude when explicitly set, got: {}",
        cfg.llm_model
    );

    // Cleanup
    std::env::remove_var("AGENT_NAME");
    std::env::remove_var("LLM_PROVIDER");
    std::env::remove_var("ANTHROPIC_API_KEY");

    // ── Test 3: OpenAI default model ──
    std::env::set_var("LLM_PROVIDER", "openai");
    std::env::remove_var("LLM_MODEL");
    let cfg = pylot::config::AppConfig::load().unwrap();
    assert!(
        cfg.llm_model.contains("gpt"),
        "OpenAI provider should default to a GPT model, got: {}",
        cfg.llm_model
    );
    std::env::remove_var("LLM_PROVIDER");

    // ── Test 4: Data dir ──
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().join("pylot_test_data");
    std::env::set_var("PYLOT_DATA_DIR", data_dir.to_str().unwrap());

    let cfg = pylot::config::AppConfig::load().unwrap();
    assert_eq!(cfg.data_dir, data_dir);
    assert!(data_dir.exists());
    std::env::remove_var("PYLOT_DATA_DIR");

    // ── Test 5: Google auto-enable ──
    std::env::set_var("GOOGLE_CLIENT_ID", "test-client-id");
    std::env::set_var("GOOGLE_CLIENT_SECRET", "test-client-secret");

    let cfg = pylot::config::AppConfig::load().unwrap();
    assert!(cfg.google_calendar_enabled);

    std::env::remove_var("GOOGLE_CLIENT_ID");
    std::env::remove_var("GOOGLE_CLIENT_SECRET");

    // ── Test 6: Telegram auto-enable ──
    std::env::set_var("TELEGRAM_BOT_TOKEN", "123:ABC");

    let cfg = pylot::config::AppConfig::load().unwrap();
    assert!(cfg.telegram_enabled);
    assert_eq!(cfg.telegram_bot_token.as_deref(), Some("123:ABC"));

    std::env::remove_var("TELEGRAM_BOT_TOKEN");
}
