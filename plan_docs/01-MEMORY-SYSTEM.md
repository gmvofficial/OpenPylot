# 01 — Advanced Memory Management System

## Objective

Replace the current single-type smart memory (SQLite + OpenAI embeddings) with a 6-type memory system supporting hybrid search (FTS5 + vector), automatic extraction, consolidation, and cross-session recall. This is the foundation that the learning system (docs 06, 10) builds upon.

---

## Current State

**File**: `src/smart_memory.rs`  
**Database**: SQLite via `rusqlite`  
**Embeddings**: OpenAI `text-embedding-3-small`  
**What works**: Store/retrieve facts, similarity search, auto-extraction every N messages  
**What's missing**: Memory types, hybrid search, consolidation, decay, retrieval policies

**Existing trait** (in `src/traits.rs`):
```rust
pub trait MemoryProvider: Send + Sync {
    // Already designed but not wired up
}
```

---

## Reference Implementations

### MetaClaw (Primary Reference)
- **Path**: `extra_repos/MetaClaw-main/metaclaw/memory/`
- **Key files**:
  - `store.py` — SQLite storage with 20+ column schema
  - `retriever.py` — 4 retrieval modes (keyword/embedding/hybrid/auto)
  - `extractor.py` — Automatic extraction from conversation turns
  - `consolidator.py` — Dedup, merge, decay
  - `types.py` — 6 memory types enum
  - `upgrade_pipeline.py` — Self-evolving retrieval policy

### IronClaw (Secondary Reference)
- **Path**: `extra_repos/ironclaw-staging/src/memory/`
- **Key features**:
  - Workspace file structure (MEMORY.md, IDENTITY.md, etc.)
  - Hybrid RRF search (FTS5 + pgvector)
  - Memory tools: `memory_search`, `memory_read`, `memory_write`, `memory_tree`
  - Compaction strategies: MoveToWorkspace, Summarize, Truncate

---

## Architecture

### Memory Types (6 total)

```rust
// File: src/memory/types.rs

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MemoryType {
    /// Specific events/interactions from sessions
    Episodic,
    /// General facts, domain knowledge, project info
    Semantic,
    /// User preferences, style, requirements
    Preference,
    /// Current project goals, decisions, blockers
    ProjectState,
    /// Rolling compressed context summary
    WorkingSummary,
    /// Learned patterns, successful workflows
    ProceduralObservation,
}
```

### Database Schema

```sql
-- File: src/memory/schema.sql (embed in Rust via include_str!)

CREATE TABLE IF NOT EXISTS memory_units (
    id TEXT PRIMARY KEY,                    -- UUID
    memory_type TEXT NOT NULL,              -- enum as string
    content TEXT NOT NULL,                  -- main content
    summary TEXT,                           -- compressed version
    source_session TEXT,                    -- which session created this
    source_turn INTEGER,                    -- which turn in session
    entities TEXT DEFAULT '[]',             -- JSON array of entities
    topics TEXT DEFAULT '[]',              -- JSON array of topics
    tags TEXT DEFAULT '[]',               -- JSON array of tags
    importance REAL DEFAULT 0.5,           -- 0.0-1.0
    confidence REAL DEFAULT 0.5,           -- 0.0-1.0
    access_count INTEGER DEFAULT 0,        -- retrieval frequency
    last_accessed TEXT,                     -- ISO timestamp
    supersedes TEXT DEFAULT '[]',          -- JSON array of IDs this replaces
    embedding BLOB,                        -- serialized f32 vector
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE VIRTUAL TABLE IF NOT EXISTS memory_units_fts USING fts5(
    content, summary, entities, topics, tags,
    content='memory_units',
    content_rowid='rowid'
);

-- Trigger to keep FTS in sync
CREATE TRIGGER memory_units_ai AFTER INSERT ON memory_units BEGIN
    INSERT INTO memory_units_fts(rowid, content, summary, entities, topics, tags)
    VALUES (new.rowid, new.content, new.summary, new.entities, new.topics, new.tags);
END;

CREATE TRIGGER memory_units_ad AFTER DELETE ON memory_units BEGIN
    INSERT INTO memory_units_fts(memory_units_fts, rowid, content, summary, entities, topics, tags)
    VALUES ('delete', old.rowid, old.content, old.summary, old.entities, old.topics, old.tags);
END;

CREATE TRIGGER memory_units_au AFTER UPDATE ON memory_units BEGIN
    INSERT INTO memory_units_fts(memory_units_fts, rowid, content, summary, entities, topics, tags)
    VALUES ('delete', old.rowid, old.content, old.summary, old.entities, old.topics, old.tags);
    INSERT INTO memory_units_fts(rowid, content, summary, entities, topics, tags)
    VALUES (new.rowid, new.content, new.summary, new.entities, new.topics, new.tags);
END;
```

### Module Structure

```
src/memory/
├── mod.rs              -- Public API, re-exports
├── types.rs            -- MemoryType enum, MemoryUnit struct
├── store.rs            -- SQLite CRUD operations
├── extractor.rs        -- Auto-extract memories from conversation
├── retriever.rs        -- 4 retrieval modes (keyword/embedding/hybrid/auto)
├── consolidator.rs     -- Dedup, merge, decay
├── embeddings.rs       -- Embedding generation (extracted from smart_memory.rs)
└── tools.rs            -- Memory tools for the agent (search/read/write/tree)
```

---

## Implementation Steps

### Step 1: Create module structure (Day 1 morning)

1. Create `src/memory/` directory
2. Create `src/memory/mod.rs` with module declarations
3. Create `src/memory/types.rs` with:

```rust
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MemoryType {
    Episodic,
    Semantic,
    Preference,
    ProjectState,
    WorkingSummary,
    ProceduralObservation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryUnit {
    pub id: String,
    pub memory_type: MemoryType,
    pub content: String,
    pub summary: Option<String>,
    pub source_session: Option<String>,
    pub source_turn: Option<i64>,
    pub entities: Vec<String>,
    pub topics: Vec<String>,
    pub tags: Vec<String>,
    pub importance: f64,
    pub confidence: f64,
    pub access_count: i64,
    pub last_accessed: Option<String>,
    pub supersedes: Vec<String>,
    pub embedding: Option<Vec<f32>>,
    pub created_at: String,
    pub updated_at: String,
}

impl MemoryUnit {
    pub fn new(memory_type: MemoryType, content: String) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            id: Uuid::new_v4().to_string(),
            memory_type,
            content,
            summary: None,
            source_session: None,
            source_turn: None,
            entities: vec![],
            topics: vec![],
            tags: vec![],
            importance: 0.5,
            confidence: 0.5,
            access_count: 0,
            last_accessed: None,
            supersedes: vec![],
            embedding: None,
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySearchResult {
    pub unit: MemoryUnit,
    pub score: f64,
    pub match_source: MatchSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MatchSource {
    Keyword,
    Embedding,
    Hybrid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RetrievalMode {
    Keyword,
    Embedding,
    Hybrid,
    Auto,
}
```

### Step 2: Implement MemoryStore (Day 1 afternoon)

**File**: `src/memory/store.rs`

Migrate logic from `src/smart_memory.rs` but with the new schema:

1. `pub async fn init(db_path: &str) -> Result<Self>` — Create tables + FTS + triggers
2. `pub async fn insert(&self, unit: &MemoryUnit) -> Result<()>` — Insert with embedding
3. `pub async fn get(&self, id: &str) -> Result<Option<MemoryUnit>>` — By ID
4. `pub async fn update(&self, unit: &MemoryUnit) -> Result<()>` — Full update
5. `pub async fn delete(&self, id: &str) -> Result<()>` — Soft or hard delete
6. `pub async fn list_by_type(&self, t: MemoryType) -> Result<Vec<MemoryUnit>>` — Filter
7. `pub async fn record_access(&self, id: &str) -> Result<()>` — Bump access_count + last_accessed
8. `pub async fn count(&self) -> Result<usize>` — Total count

**Important**: Use `tokio::task::spawn_blocking` for all rusqlite calls (it's sync).

### Step 3: Implement Retriever with 4 modes (Day 2 morning)

**File**: `src/memory/retriever.rs`

```rust
pub struct MemoryRetriever {
    store: Arc<MemoryStore>,
    embeddings: Arc<EmbeddingClient>,
    mode: RetrievalMode,
}

impl MemoryRetriever {
    /// Keyword search using FTS5 BM25 ranking
    pub async fn search_keyword(&self, query: &str, limit: usize) -> Result<Vec<MemorySearchResult>>

    /// Embedding search using cosine similarity
    pub async fn search_embedding(&self, query: &str, limit: usize) -> Result<Vec<MemorySearchResult>>

    /// Hybrid search using Reciprocal Rank Fusion (RRF)
    /// Combined score = Σ 1/(k + rank) for each method, k=60
    pub async fn search_hybrid(&self, query: &str, limit: usize) -> Result<Vec<MemorySearchResult>>

    /// Auto-select: <4 tokens → keyword, 4+ tokens → hybrid
    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<MemorySearchResult>>
}
```

**RRF Algorithm** (from IronClaw):
```rust
fn reciprocal_rank_fusion(
    keyword_results: &[MemorySearchResult],
    embedding_results: &[MemorySearchResult],
    k: f64,  // default 60.0
) -> Vec<MemorySearchResult> {
    let mut scores: HashMap<String, f64> = HashMap::new();

    for (rank, result) in keyword_results.iter().enumerate() {
        *scores.entry(result.unit.id.clone()).or_default() += 1.0 / (k + rank as f64 + 1.0);
    }
    for (rank, result) in embedding_results.iter().enumerate() {
        *scores.entry(result.unit.id.clone()).or_default() += 1.0 / (k + rank as f64 + 1.0);
    }

    // Sort by fused score descending
    // Apply type boosts: WorkingSummary × 1.2, Preference × 1.1
    // Apply importance, recency, confidence weights
}
```

### Step 4: Implement Extractor (Day 2 afternoon)

**File**: `src/memory/extractor.rs`

Extract memory units from conversation turns using LLM:

```rust
pub struct MemoryExtractor {
    llm: Arc<dyn LlmProvider>,
}

impl MemoryExtractor {
    /// Extract memory units from recent conversation messages
    pub async fn extract(&self, messages: &[Message]) -> Result<Vec<MemoryUnit>>
}
```

**Extraction prompt** (adapt from MetaClaw):
```
Analyze the following conversation and extract distinct memory units.
For each unit, provide:
- type: one of [episodic, semantic, preference, project_state, working_summary, procedural_observation]
- content: the factual content (1-3 sentences)
- entities: key entities mentioned (people, projects, technologies)
- topics: relevant topic tags
- importance: 0.0-1.0 (how important is this to remember)

Conversation:
{messages}

Return as JSON array:
[{"type": "...", "content": "...", "entities": [...], "topics": [...], "importance": 0.8}]
```

**Integration point**: Call extraction every N messages (configurable, default 5) in `src/agent.rs`. Check `config.memory.extraction_interval`.

### Step 5: Implement Consolidator (Day 3 morning)

**File**: `src/memory/consolidator.rs`

```rust
pub struct MemoryConsolidator {
    store: Arc<MemoryStore>,
}

impl MemoryConsolidator {
    /// Run all consolidation passes
    pub async fn consolidate(&self) -> Result<ConsolidationReport>

    /// Remove exact content duplicates (content hash)
    async fn dedup_exact(&self) -> Result<usize>

    /// Merge near-duplicates (Jaccard similarity > 0.80)
    async fn merge_near_duplicates(&self) -> Result<usize>

    /// Decay importance of old, unused memories
    /// importance *= 0.95 for memories older than 30 days with access_count < 3
    async fn apply_importance_decay(&self) -> Result<usize>

    /// Keep only newest WorkingSummary per session
    async fn prune_stale_summaries(&self) -> Result<usize>
}
```

**Jaccard similarity** for near-duplicate detection:
```rust
fn jaccard_similarity(a: &str, b: &str) -> f64 {
    let set_a: HashSet<&str> = a.split_whitespace().collect();
    let set_b: HashSet<&str> = b.split_whitespace().collect();
    let intersection = set_a.intersection(&set_b).count() as f64;
    let union = set_a.union(&set_b).count() as f64;
    if union == 0.0 { 0.0 } else { intersection / union }
}
```

### Step 6: Implement Memory Tools (Day 3 afternoon)

**File**: `src/memory/tools.rs`

Add 4 new tools to the tool registry for the agent to use:

```rust
// 1. memory_search — Hybrid search across all memory types
pub struct MemorySearchTool;
// Input: { "query": "string", "limit": 10, "memory_type": "optional filter" }
// Output: Ranked results with scores

// 2. memory_read — Read specific memory by ID or path
pub struct MemoryReadTool;
// Input: { "id": "uuid" } or { "path": "memories/project/goals.md" }
// Output: Full memory unit content

// 3. memory_write — Create or update memory
pub struct MemoryWriteTool;
// Input: { "content": "...", "type": "semantic", "entities": [...], "tags": [...] }
// Output: Created memory ID

// 4. memory_tree — List memory structure
pub struct MemoryTreeTool;
// Input: { "type": "optional filter", "limit": 50 }
// Output: Tree of memory units grouped by type
```

Register these in `src/tools/mod.rs` alongside existing tools.

### Step 7: Migrate from smart_memory.rs (Day 3)

1. Keep `src/smart_memory.rs` as deprecated wrapper
2. Wire `src/memory/mod.rs` into `src/agent.rs`
3. Update `src/context_builder.rs` to use new retriever for context injection
4. Update `src/config.rs` to add memory config section:

```toml
[memory]
enabled = true
auto_extract = true
extraction_interval = 5
retrieval_mode = "auto"  # keyword | embedding | hybrid | auto
similarity_threshold = 0.35
max_injected_units = 8
max_injected_tokens = 800
consolidation_enabled = true
consolidation_interval_hours = 24
importance_decay_days = 30
```

---

## Testing Requirements

### Unit Tests (src/memory/)
- `test_memory_unit_creation` — All 6 types
- `test_store_crud` — Insert, get, update, delete
- `test_fts_search` — Keyword search with FTS5
- `test_embedding_search` — Vector similarity
- `test_hybrid_search_rrf` — RRF fusion scoring
- `test_auto_mode_selection` — Short query → keyword, long → hybrid
- `test_dedup_exact` — Exact duplicate removal
- `test_near_duplicate_merge` — Jaccard threshold
- `test_importance_decay` — Old memories decay
- `test_extraction_parsing` — LLM response parsing
- `test_memory_tools` — All 4 tools via agent

### Integration Tests (tests/rust/)
- `test_memory_lifecycle` — Extract → store → search → consolidate
- `test_cross_session_recall` — Memory persists across sessions
- `test_context_injection` — Memories injected into LLM context

---

## Acceptance Criteria

- [ ] 6 memory types stored and searchable
- [ ] FTS5 keyword search returns results ranked by BM25
- [ ] Embedding search returns results ranked by cosine similarity
- [ ] Hybrid RRF search combines both with proper fusion
- [ ] Auto mode selects correct strategy based on query length
- [ ] Extraction runs every N messages, produces typed memory units
- [ ] Consolidation removes duplicates, merges near-duplicates, decays old memories
- [ ] 4 memory tools available to agent (search/read/write/tree)
- [ ] Context builder injects relevant memories into LLM prompt
- [ ] All existing tests still pass (backward compatible)
- [ ] Config options respected from `config.toml`
