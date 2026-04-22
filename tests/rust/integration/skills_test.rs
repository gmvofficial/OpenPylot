use pylot::skills::types::{Skill, SkillMeta, SkillSource};
use pylot::skills::registry::SkillRegistry;
use pylot::skills::loader::SkillLoader;
use pylot::skills::matcher::SkillMatcher;
use std::path::PathBuf;
use tempfile::TempDir;
use std::fs;

// ── Helper ───────────────────────────────────────────────────────────

fn make_skill(name: &str, category: &str, tags: &[&str], desc: &str, content: &str) -> Skill {
    Skill {
        meta: SkillMeta {
            name: name.to_string(),
            description: desc.to_string(),
            version: "1.0.0".to_string(),
            author: None,
            category: Some(category.to_string()),
            tags: tags.iter().map(|s| s.to_string()).collect(),
            os: vec![],
            requires: None,
            install: vec![],
        },
        content: content.to_string(),
        source_path: PathBuf::new(),
        source: SkillSource::Bundled,
    }
}

// ── Loader tests ─────────────────────────────────────────────────────

#[test]
fn test_parse_skill_file() {
    let tmp = TempDir::new().unwrap();
    let skill_dir = tmp.path().join("test-skill");
    fs::create_dir_all(&skill_dir).unwrap();

    let skill_path = skill_dir.join("SKILL.md");
    fs::write(&skill_path, r#"---
name: test-skill
description: A test skill for unit testing
version: "1.0.0"
category: coding
tags:
  - test
  - unit
---

# Test Skill

These are the instructions for the test skill.
Follow them carefully.
"#).unwrap();

    let skill = SkillLoader::parse_skill_file(&skill_path, SkillSource::Bundled).unwrap();
    assert_eq!(skill.meta.name, "test-skill");
    assert_eq!(skill.meta.description, "A test skill for unit testing");
    assert_eq!(skill.meta.category, Some("coding".to_string()));
    assert_eq!(skill.meta.tags, vec!["test", "unit"]);
    assert!(skill.content.contains("# Test Skill"));
    assert!(skill.content.contains("Follow them carefully."));
    assert_eq!(skill.source, SkillSource::Bundled);
}

#[test]
fn test_scan_directory_finds_nested_skills() {
    let tmp = TempDir::new().unwrap();

    // Create two skill directories
    let skill1_dir = tmp.path().join("coding").join("review");
    fs::create_dir_all(&skill1_dir).unwrap();
    fs::write(skill1_dir.join("SKILL.md"), r#"---
name: review
description: Code review
---
Review instructions
"#).unwrap();

    let skill2_dir = tmp.path().join("research").join("analyze");
    fs::create_dir_all(&skill2_dir).unwrap();
    fs::write(skill2_dir.join("SKILL.md"), r#"---
name: analyze
description: Analysis
---
Analysis instructions
"#).unwrap();

    let skills = SkillLoader::scan_directory(tmp.path(), SkillSource::Workspace);
    assert_eq!(skills.len(), 2);

    let names: Vec<&str> = skills.iter().map(|s| s.meta.name.as_str()).collect();
    assert!(names.contains(&"review"));
    assert!(names.contains(&"analyze"));
}

#[test]
fn test_scan_directory_skips_invalid_files() {
    let tmp = TempDir::new().unwrap();

    // Valid skill
    let valid_dir = tmp.path().join("valid");
    fs::create_dir_all(&valid_dir).unwrap();
    fs::write(valid_dir.join("SKILL.md"), r#"---
name: valid
description: Valid skill
---
Instructions
"#).unwrap();

    // Invalid skill (no frontmatter)
    let invalid_dir = tmp.path().join("invalid");
    fs::create_dir_all(&invalid_dir).unwrap();
    fs::write(invalid_dir.join("SKILL.md"), "No frontmatter here").unwrap();

    let skills = SkillLoader::scan_directory(tmp.path(), SkillSource::Local);
    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0].meta.name, "valid");
}

// ── Registry tests ───────────────────────────────────────────────────

#[test]
fn test_registry_precedence_workspace_wins() {
    let mut registry = SkillRegistry::new();

    let mut bundled = make_skill("debug", "coding", &[], "bundled debug", "old instructions");
    bundled.source = SkillSource::Bundled;
    registry.insert_skill(bundled);

    let mut workspace = make_skill("debug", "coding", &[], "workspace debug", "new instructions");
    workspace.source = SkillSource::Workspace;
    registry.insert_skill(workspace);

    assert_eq!(registry.len(), 1);
    let skill = registry.get("debug").unwrap();
    assert_eq!(skill.meta.description, "workspace debug");
    assert_eq!(skill.content, "new instructions");
}

#[test]
fn test_registry_load_all_from_workspace() {
    let tmp = TempDir::new().unwrap();
    let skills_dir = tmp.path().join("skills");
    let coding_dir = skills_dir.join("coding").join("test-load");
    fs::create_dir_all(&coding_dir).unwrap();
    fs::write(coding_dir.join("SKILL.md"), r#"---
name: test-load
description: Integration test skill
category: coding
---
Test instructions
"#).unwrap();

    let registry = SkillRegistry::load_all(Some(tmp.path()));
    // Should find at least the workspace skill we created
    assert!(registry.get("test-load").is_some());
}

// ── Matcher tests ────────────────────────────────────────────────────

#[test]
fn test_matcher_selects_relevant_skills() {
    let skills = vec![
        make_skill("code-review", "coding", &["review", "code"], "Review code quality", "Review instructions"),
        make_skill("email-drafting", "communication", &["email", "draft"], "Draft emails", "Email instructions"),
        make_skill("task-planning", "productivity", &["plan", "task"], "Plan tasks", "Planning instructions"),
    ];

    let matched = SkillMatcher::match_skills(&skills, "please review this code for bugs", 3);
    assert!(!matched.is_empty());
    assert_eq!(matched[0].meta.name, "code-review");
}

#[test]
fn test_matcher_returns_empty_for_unrelated() {
    let skills = vec![
        make_skill("code-review", "coding", &["review"], "Review code", "instructions"),
    ];

    let matched = SkillMatcher::match_skills(&skills, "what is the meaning of life", 3);
    assert!(matched.is_empty());
}

#[test]
fn test_matcher_respects_top_k() {
    let skills = vec![
        make_skill("s1", "coding", &["code"], "coding stuff", ""),
        make_skill("s2", "coding", &["code"], "more coding", ""),
        make_skill("s3", "coding", &["code"], "even more coding", ""),
    ];

    let matched = SkillMatcher::match_skills(&skills, "write some code for me", 1);
    assert_eq!(matched.len(), 1);
}

// ── Skills overview / prompt injection ───────────────────────────────

#[test]
fn test_build_skills_overview() {
    let mut registry = SkillRegistry::new();
    registry.insert_skill(make_skill("code-review", "coding", &[], "Review code quality", ""));
    registry.insert_skill(make_skill("email-drafting", "communication", &[], "Draft emails", ""));

    let overview = registry.build_skills_overview();
    assert!(overview.contains("code-review"));
    assert!(overview.contains("email-drafting"));
    assert!(overview.contains("Available Skills"));
}

#[test]
fn test_build_matched_prompt() {
    let mut registry = SkillRegistry::new();
    registry.insert_skill(make_skill(
        "debug-systematically", "coding",
        &["debug", "bug", "error"],
        "Systematic debugging",
        "Step 1: Reproduce\nStep 2: Isolate\nStep 3: Fix",
    ));

    let prompt = registry.build_matched_prompt("I have a bug to debug", 2);
    assert!(prompt.contains("debug-systematically"));
    assert!(prompt.contains("Step 1: Reproduce"));
}

// ── End-to-end: load bundled skills from workspace ───────────────────

#[test]
fn test_bundled_skills_directory() {
    // This test verifies the actual bundled skills in the project's skills/ directory
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let skills_dir = workspace.join("skills");

    if skills_dir.exists() {
        let skills = SkillLoader::scan_directory(&skills_dir, SkillSource::Bundled);
        // We created at least 5 bundled skills
        assert!(skills.len() >= 5, "Expected at least 5 bundled skills, found {}", skills.len());

        // Verify each has required fields
        for skill in &skills {
            assert!(!skill.meta.name.is_empty(), "Skill name should not be empty");
            assert!(!skill.meta.description.is_empty(), "Skill description should not be empty");
            assert!(!skill.content.is_empty(), "Skill content should not be empty");
        }
    }
}
