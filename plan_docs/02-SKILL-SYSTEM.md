# 02 — Skill System (SKILL.md Format)

## Objective

Implement a zero-friction skill system that allows users to extend agent capabilities by dropping SKILL.md files into a directory. No compilation, no restart needed. Support skill discovery, hot reload, a CLI interface, and future marketplace integration.

---

## Current State

**Tools**: `src/tools/` — Static tool registry with Rust `Tool` trait implementations  
**No skill system exists**. Agent behavior is fixed at compile time.

---

## Reference Implementations

### OpenClaw (Primary Reference — SKILL.md format)
- **Path**: `extra_repos/openclaw-main/`
- **Format**: SKILL.md files with YAML frontmatter + markdown body
- **Features**: 
  - Three locations: workspace (override) > local > bundled
  - Load-time gating (OS, bins, env vars, config)
  - Bundled installers (brew, node, go, uv)
  - Hot reload with file watcher
  - ClawHub registry (public, versioned)

### IronClaw (Secondary — Skill discovery)
- **Path**: `extra_repos/ironclaw-staging/skills/`
- **Format**: YAML frontmatter (title, author, version, tags, category) + markdown
- **Features**:
  - Recursive directory scanning
  - CLI: `ironclaw skills list/search/info`
  - Web API: `/api/skills/list`, `/api/skills/search`
  - Deterministic selection (no LLM call)
  - 9 bundled skills

### MetaClaw (Skill retrieval modes)
- **Path**: `extra_repos/MetaClaw-main/metaclaw/skills/`
- **Features**:
  - Template mode (keyword → category → top-k skills)
  - Embedding mode (semantic search over skill descriptions)
  - Auto-evolve: failed samples → LLM generates new skills
  - 40+ built-in skills across 9 categories

---

## Architecture

### SKILL.md Format

```markdown
---
name: debug-systematically
description: Use when diagnosing a bug or unexpected behavior
version: 1.0.0
author: pylot-team
category: coding
tags: [debug, troubleshoot, error]
os: [darwin, linux, windows]           # optional OS filter
requires:                               # optional requirements
  bins: [git]                          # required binaries
  env: [GITHUB_TOKEN]                  # required env vars
  tools: [github_search]              # required tools
install:                                # optional installers
  - id: brew
    kind: brew
    formula: git
    bins: [git]
    label: "Install git (brew)"
---

# Debug Systematically

When the user reports a bug or unexpected behavior:

1. **Reproduce consistently**
   - Isolate the minimal failing case
   - Confirm expected vs actual behavior

2. **Read error messages carefully**
   - Check full stack trace
   - Identify the failing module

3. **Form a hypothesis**
   - What changed recently?
   - Is this environment-specific?

4. **Test the hypothesis**
   - Add logging/assertions
   - Run with debug configuration
```

### Module Structure

```
src/skills/
├── mod.rs              -- Public API, SkillManager
├── types.rs            -- Skill struct, SkillMeta (frontmatter)
├── loader.rs           -- Directory scanner, YAML parser
├── matcher.rs          -- Skill matching: keyword + embedding + auto
├── registry.rs         -- Bundled + local + workspace skill registry
└── tools.rs            -- Skill-related tools for the agent

skills/                 -- Bundled skills directory (shipped with binary)
├── coding/
│   ├── debug-systematically/SKILL.md
│   ├── code-review/SKILL.md
│   └── refactoring/SKILL.md
├── research/
│   ├── web-research/SKILL.md
│   └── document-analysis/SKILL.md
├── productivity/
│   ├── task-planning/SKILL.md
│   └── calendar-management/SKILL.md
├── communication/
│   ├── email-drafting/SKILL.md
│   └── social-media/SKILL.md
└── agentic/
    ├── delegation/SKILL.md
    ├── plan-mode/SKILL.md
    └── multi-step-reasoning/SKILL.md
```

### Data Structures

```rust
// File: src/skills/types.rs

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMeta {
    pub name: String,
    pub description: String,
    #[serde(default = "default_version")]
    pub version: String,
    pub author: Option<String>,
    pub category: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub os: Vec<String>,              // empty = all platforms
    pub requires: Option<SkillRequirements>,
    #[serde(default)]
    pub install: Vec<SkillInstaller>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillRequirements {
    #[serde(default)]
    pub bins: Vec<String>,
    #[serde(default)]
    pub env: Vec<String>,
    #[serde(default)]
    pub tools: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillInstaller {
    pub id: String,
    pub kind: String,     // brew | npm | pip | cargo | download
    pub formula: Option<String>,
    pub package: Option<String>,
    #[serde(default)]
    pub bins: Vec<String>,
    pub label: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Skill {
    pub meta: SkillMeta,
    pub content: String,           // markdown body (instructions)
    pub source_path: PathBuf,      // where the SKILL.md was loaded from
    pub source: SkillSource,
    pub embedding: Option<Vec<f32>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SkillSource {
    Bundled,    // ships with binary
    Local,      // ~/.pylot/skills/
    Workspace,  // ./skills/ in current project
}
```

---

## Implementation Steps

### Step 1: Create types and YAML parser (Day 1 morning)

**File**: `src/skills/types.rs` — Structs above  
**File**: `src/skills/loader.rs`:

```rust
use std::path::{Path, PathBuf};

pub struct SkillLoader;

impl SkillLoader {
    /// Parse a SKILL.md file: split YAML frontmatter from markdown body
    pub fn parse_skill_file(path: &Path) -> Result<Skill> {
        let content = std::fs::read_to_string(path)?;
        let (frontmatter, body) = Self::split_frontmatter(&content)?;
        let meta: SkillMeta = serde_yaml::from_str(&frontmatter)?;
        Ok(Skill {
            meta,
            content: body,
            source_path: path.to_path_buf(),
            source: SkillSource::Local,
            embedding: None,
        })
    }

    /// Split "---\nyaml\n---\nmarkdown" into (yaml, markdown)
    fn split_frontmatter(content: &str) -> Result<(String, String)> {
        // Find first "---" then next "---"
        // Everything between = YAML, everything after = markdown
    }

    /// Recursively scan directory for SKILL.md files
    pub fn scan_directory(dir: &Path) -> Result<Vec<Skill>> {
        // Walk dir, find all SKILL.md files, parse each
    }
}
```

### Step 2: Implement SkillRegistry with precedence (Day 1 afternoon)

**File**: `src/skills/registry.rs`

```rust
pub struct SkillRegistry {
    skills: Vec<Skill>,
    /// Precedence: workspace > local > bundled (higher index = higher priority)
}

impl SkillRegistry {
    pub fn new() -> Self;

    /// Load skills from all 3 sources with precedence
    pub fn load_all(&mut self, workspace_dir: Option<&Path>) -> Result<()> {
        // 1. Load bundled skills (compiled into binary via include_dir! or from exe dir)
        let bundled_dir = self.bundled_skills_dir()?;
        let bundled = SkillLoader::scan_directory(&bundled_dir)?;

        // 2. Load local skills from ~/.pylot/skills/
        let local_dir = dirs::home_dir().unwrap().join(".pylot").join("skills");
        let local = SkillLoader::scan_directory(&local_dir).unwrap_or_default();

        // 3. Load workspace skills from ./skills/ (if exists)
        let workspace = if let Some(ws) = workspace_dir {
            SkillLoader::scan_directory(&ws.join("skills")).unwrap_or_default()
        } else {
            vec![]
        };

        // Merge with precedence: workspace overrides local overrides bundled
        // Skills with same name: keep highest priority
        self.skills = Self::merge_with_precedence(bundled, local, workspace);
        Ok(())
    }

    /// Check requirements before making skill available
    fn gate_skill(skill: &Skill) -> bool {
        // Check OS: if skill.meta.os is non-empty, current OS must be in list
        // Check bins: all required binaries must exist (which <bin>)
        // Check env: all required env vars must be set
        // Check tools: all required tools must be registered
        true
    }

    pub fn list(&self) -> &[Skill];
    pub fn get(&self, name: &str) -> Option<&Skill>;
    pub fn search(&self, query: &str) -> Vec<&Skill>;
}
```

### Step 3: Implement skill matching/selection (Day 2 morning)

**File**: `src/skills/matcher.rs`

```rust
pub struct SkillMatcher {
    registry: Arc<SkillRegistry>,
}

impl SkillMatcher {
    /// Select relevant skills for the current user message
    /// Uses keyword matching (fast) — no LLM call needed
    pub fn match_skills(&self, user_message: &str, top_k: usize) -> Vec<&Skill> {
        // 1. Extract task type from message keywords
        //    e.g., "debug" → coding, "research" → research
        // 2. Filter skills by matching category
        // 3. Score by tag overlap + description similarity
        // 4. Return top-k
    }

    /// Category detection from keywords (from MetaClaw)
    fn detect_category(&self, message: &str) -> Option<String> {
        let task_keywords: HashMap<&str, Vec<&str>> = HashMap::from([
            ("coding", vec!["code", "debug", "implement", "function", "bug", "error", "fix", "refactor"]),
            ("research", vec!["research", "paper", "find", "search", "look up", "investigate"]),
            ("productivity", vec!["plan", "schedule", "organize", "task", "todo", "calendar"]),
            ("communication", vec!["email", "draft", "write", "reply", "message", "social"]),
            ("agentic", vec!["delegate", "multi-step", "complex", "break down", "plan"]),
        ]);
        // Score each category, return highest if > threshold
    }
}
```

### Step 4: Wire skills into agent loop (Day 2 afternoon)

**File**: Modify `src/agent.rs`

In the agent's message handling loop, before sending to LLM:

```rust
// In agent.rs, after building context but before LLM call:

// 1. Match relevant skills for user message
let matched_skills = self.skill_matcher.match_skills(&user_message, 3);

// 2. Inject skill content into system prompt
let skill_context = matched_skills
    .iter()
    .map(|s| format!("## Skill: {}\n{}", s.meta.name, s.content))
    .collect::<Vec<_>>()
    .join("\n\n");

// 3. Append to system prompt (within token budget)
if !skill_context.is_empty() {
    system_prompt.push_str("\n\n# Active Skills\n\n");
    system_prompt.push_str(&skill_context);
}
```

### Step 5: Implement CLI commands (Day 2)

**File**: Modify `src/main.rs` or `src/terminal.rs`

Add skill management commands:

```
pylot skills list                    # List all available skills
pylot skills search <query>          # Search skills by keyword
pylot skills info <name>             # Show skill details
pylot skills install <path|url>      # Install a skill from path or URL
pylot skills remove <name>           # Remove an installed skill
pylot skills create <name>           # Scaffold a new SKILL.md
```

### Step 6: Add API endpoints (Day 2)

**File**: Modify `src/api/` routes

```
GET  /api/skills/list                # List all skills
GET  /api/skills/search?q=<query>    # Search skills
GET  /api/skills/:name               # Get skill details
POST /api/skills/install             # Install from path/URL
DELETE /api/skills/:name             # Remove skill
```

### Step 7: Create bundled skills (Day 2)

Create 10-15 bundled SKILL.md files in `skills/` directory:

**Coding**:
- `debug-systematically` — Bug diagnosis workflow
- `code-review` — Code review checklist
- `refactoring` — Safe refactoring patterns

**Research**:
- `web-research` — Structured web research approach
- `document-analysis` — Analyze documents methodically

**Productivity**:
- `task-planning` — Break down complex tasks
- `calendar-management` — Manage calendar effectively

**Communication**:
- `email-drafting` — Professional email writing
- `social-media` — Social media content creation

**Agentic**:
- `delegation` — When and how to delegate to sub-agents
- `plan-mode` — Multi-step planning approach
- `multi-step-reasoning` — Complex reasoning patterns

### Step 8: Optional — File watcher for hot reload

```rust
// In src/skills/registry.rs, add optional notify watcher:
use notify::{RecommendedWatcher, RecursiveMode, Watcher};

pub fn watch_for_changes(&self, tx: mpsc::Sender<()>) -> Result<RecommendedWatcher> {
    let mut watcher = notify::recommended_watcher(move |_| {
        let _ = tx.send(());
    })?;
    watcher.watch(&self.local_dir, RecursiveMode::Recursive)?;
    if let Some(ws) = &self.workspace_dir {
        watcher.watch(ws, RecursiveMode::Recursive)?;
    }
    Ok(watcher)
}
```

**Cargo.toml dependency**: `notify = "6"`

---

## Config Additions

```toml
# config/default.toml

[skills]
enabled = true
retrieval_mode = "keyword"    # keyword | embedding | auto
top_k = 3                     # max skills injected per message
max_tokens = 500              # token budget for skill content
auto_evolve = false           # generate new skills from failures (requires learning system)
watch = false                 # hot reload on file changes
extra_dirs = []               # additional skill directories
```

---

## Testing Requirements

- `test_frontmatter_parsing` — Parse valid YAML + markdown
- `test_frontmatter_invalid` — Handle malformed YAML gracefully
- `test_skill_loading` — Load from single file
- `test_directory_scan` — Recursive discovery
- `test_precedence` — Workspace overrides local overrides bundled
- `test_os_gating` — Filter by OS
- `test_binary_gating` — Filter by required binaries
- `test_keyword_matching` — Category detection + scoring
- `test_cli_list` — `pylot skills list` output
- `test_api_list` — GET /api/skills/list response

---

## Acceptance Criteria

- [ ] SKILL.md files parsed correctly (YAML frontmatter + markdown)
- [ ] Skills loaded from 3 locations with proper precedence
- [ ] OS gating filters skills for current platform
- [ ] Binary/env/tool gating filters unavailable skills
- [ ] Keyword matching selects relevant skills for user messages
- [ ] Skills injected into system prompt within token budget
- [ ] CLI commands work: list, search, info, install, remove
- [ ] API endpoints work: list, search, get, install, delete
- [ ] 10+ bundled skills shipped with binary
- [ ] Hot reload (optional) picks up new skills without restart
