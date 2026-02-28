/// Unit tests for ConversationContext (src/context.rs).
///
/// Validates message management, trimming, and system prompt handling.

use gmv_agent::context::ConversationContext;
use gmv_agent::llm::Message;

// ── Construction ─────────────────────────────────────────────────────

#[test]
fn test_new_context_is_empty() {
    let ctx = ConversationContext::new("You are a helpful assistant.".into(), 50);
    assert!(ctx.is_empty());
    assert_eq!(ctx.len(), 0);
}

// ── Push & len ───────────────────────────────────────────────────────

#[test]
fn test_push_increments_len() {
    let mut ctx = ConversationContext::new("system".into(), 50);
    ctx.push(Message::user("hello"));
    assert_eq!(ctx.len(), 1);

    ctx.push(Message::assistant("hi there"));
    assert_eq!(ctx.len(), 2);
}

// ── build_messages includes system prompt ────────────────────────────

#[test]
fn test_build_messages_starts_with_system() {
    let mut ctx = ConversationContext::new("You are GMV Agent.".into(), 50);
    ctx.push(Message::user("hi"));

    let msgs = ctx.build_messages();
    assert_eq!(msgs.len(), 2); // system + user
    assert_eq!(msgs[0].content, "You are GMV Agent.");
    assert_eq!(msgs[1].content, "hi");
}

#[test]
fn test_build_messages_preserves_order() {
    let mut ctx = ConversationContext::new("sys".into(), 50);
    ctx.push(Message::user("first"));
    ctx.push(Message::assistant("second"));
    ctx.push(Message::user("third"));

    let msgs = ctx.build_messages();
    assert_eq!(msgs.len(), 4); // system + 3 messages
    assert_eq!(msgs[1].content, "first");
    assert_eq!(msgs[2].content, "second");
    assert_eq!(msgs[3].content, "third");
}

// ── Trimming ─────────────────────────────────────────────────────────

#[test]
fn test_trim_removes_oldest_messages() {
    let mut ctx = ConversationContext::new("sys".into(), 3);

    ctx.push(Message::user("msg1"));
    ctx.push(Message::assistant("msg2"));
    ctx.push(Message::user("msg3"));
    assert_eq!(ctx.len(), 3);

    // Pushing a 4th should trim the oldest
    ctx.push(Message::assistant("msg4"));
    assert_eq!(ctx.len(), 3);

    let msgs = ctx.build_messages();
    // system + last 3 messages
    assert_eq!(msgs.len(), 4);
    assert_eq!(msgs[1].content, "msg2");
    assert_eq!(msgs[2].content, "msg3");
    assert_eq!(msgs[3].content, "msg4");
}

#[test]
fn test_trim_boundary_exact_max() {
    let mut ctx = ConversationContext::new("sys".into(), 2);
    ctx.push(Message::user("a"));
    ctx.push(Message::assistant("b"));
    assert_eq!(ctx.len(), 2);

    // At exactly max — no trimming yet
    let msgs = ctx.build_messages();
    assert_eq!(msgs[1].content, "a");
    assert_eq!(msgs[2].content, "b");
}

// ── Extend ───────────────────────────────────────────────────────────

#[test]
fn test_extend_adds_multiple() {
    let mut ctx = ConversationContext::new("sys".into(), 50);
    ctx.extend(vec![
        Message::user("q1"),
        Message::assistant("a1"),
        Message::user("q2"),
    ]);
    assert_eq!(ctx.len(), 3);
}

#[test]
fn test_extend_triggers_trim() {
    let mut ctx = ConversationContext::new("sys".into(), 2);
    ctx.extend(vec![
        Message::user("old1"),
        Message::assistant("old2"),
        Message::user("new1"),
    ]);
    // Only the latest 2 should remain
    assert_eq!(ctx.len(), 2);

    let msgs = ctx.build_messages();
    assert_eq!(msgs[1].content, "old2");
    assert_eq!(msgs[2].content, "new1");
}

// ── Clear ────────────────────────────────────────────────────────────

#[test]
fn test_clear_empties_messages() {
    let mut ctx = ConversationContext::new("sys".into(), 50);
    ctx.push(Message::user("hello"));
    ctx.push(Message::assistant("hi"));
    assert_eq!(ctx.len(), 2);

    ctx.clear();
    assert_eq!(ctx.len(), 0);
    assert!(ctx.is_empty());
}

#[test]
fn test_clear_preserves_system_prompt() {
    let mut ctx = ConversationContext::new("My System Prompt".into(), 50);
    ctx.push(Message::user("hello"));
    ctx.clear();

    let msgs = ctx.build_messages();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].content, "My System Prompt");
}
