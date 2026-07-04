/// Unit tests for MemoryStore (src/memory.rs).
///
/// Validates fact storage, conversation summaries, persistence, and context formatting.

use openpylot::memory::MemoryStore;

// ── Default / empty state ────────────────────────────────────────────

#[test]
fn test_default_store_is_empty() {
    let store = MemoryStore::default();
    assert!(store.facts.is_empty());
    assert!(store.summaries.is_empty());
}

#[test]
fn test_empty_context_string_is_blank() {
    let store = MemoryStore::default();
    assert!(store.context_string().is_empty());
}

// ── Facts ────────────────────────────────────────────────────────────

#[test]
fn test_set_and_get_fact() {
    let mut store = MemoryStore::default();
    store.set_fact("timezone", "US/Eastern");

    assert_eq!(store.get_fact("timezone"), Some("US/Eastern"));
}

#[test]
fn test_get_nonexistent_fact_returns_none() {
    let store = MemoryStore::default();
    assert!(store.get_fact("missing_key").is_none());
}

#[test]
fn test_update_existing_fact() {
    let mut store = MemoryStore::default();
    store.set_fact("name", "Alice");
    store.set_fact("name", "Bob");

    assert_eq!(store.get_fact("name"), Some("Bob"));
    assert_eq!(store.facts.len(), 1, "Should update in-place, not duplicate");
}

#[test]
fn test_multiple_facts() {
    let mut store = MemoryStore::default();
    store.set_fact("timezone", "UTC");
    store.set_fact("language", "English");
    store.set_fact("preferred_model", "gpt-4o");

    assert_eq!(store.facts.len(), 3);
    assert_eq!(store.get_fact("language"), Some("English"));
}

// ── Summaries ────────────────────────────────────────────────────────

#[test]
fn test_add_summary() {
    let mut store = MemoryStore::default();
    store.add_summary("User asked about weather in NYC.");

    assert_eq!(store.summaries.len(), 1);
    assert_eq!(
        store.summaries[0].summary,
        "User asked about weather in NYC."
    );
}

#[test]
fn test_summaries_capped_at_100() {
    let mut store = MemoryStore::default();
    for i in 0..110 {
        store.add_summary(format!("Summary #{}", i));
    }

    assert_eq!(store.summaries.len(), 100);
    // Oldest should have been trimmed — first remaining should be #10
    assert_eq!(store.summaries[0].summary, "Summary #10");
    assert_eq!(store.summaries[99].summary, "Summary #109");
}

// ── context_string ───────────────────────────────────────────────────

#[test]
fn test_context_string_includes_facts() {
    let mut store = MemoryStore::default();
    store.set_fact("timezone", "US/Pacific");

    let ctx = store.context_string();
    assert!(ctx.contains("User Memory"));
    assert!(ctx.contains("timezone"));
    assert!(ctx.contains("US/Pacific"));
}

#[test]
fn test_context_string_includes_summaries() {
    let mut store = MemoryStore::default();
    store.add_summary("Discussed project deadlines.");

    let ctx = store.context_string();
    assert!(ctx.contains("conversation summaries"));
    assert!(ctx.contains("Discussed project deadlines."));
}

#[test]
fn test_context_string_shows_recent_summaries_only() {
    let mut store = MemoryStore::default();
    for i in 0..10 {
        store.add_summary(format!("Summary #{}", i));
    }

    let ctx = store.context_string();
    // Only last 5 summaries are shown in context
    assert!(ctx.contains("Summary #9"));
    assert!(ctx.contains("Summary #5"));
    // Summary #4 should NOT be in context (only last 5)
    assert!(!ctx.contains("Summary #4"));
}

// ── Persistence (save / load) ────────────────────────────────────────

#[test]
fn test_save_and_load_roundtrip() {
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().to_path_buf();

    let mut store = MemoryStore::default();
    store.set_fact("user_name", "TestUser");
    store.add_summary("First conversation about scheduling.");
    store.save(&data_dir).unwrap();

    let loaded = MemoryStore::load(&data_dir).unwrap();
    assert_eq!(loaded.get_fact("user_name"), Some("TestUser"));
    assert_eq!(loaded.summaries.len(), 1);
    assert_eq!(
        loaded.summaries[0].summary,
        "First conversation about scheduling."
    );
}

#[test]
fn test_load_missing_file_returns_default() {
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().to_path_buf();

    let loaded = MemoryStore::load(&data_dir).unwrap();
    assert!(loaded.facts.is_empty());
    assert!(loaded.summaries.is_empty());
}
