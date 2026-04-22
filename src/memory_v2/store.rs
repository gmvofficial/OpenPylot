use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::{Arc, Mutex};

use super::types::{MemoryType, MemoryUnit};

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS memory_units (
    id TEXT PRIMARY KEY,
    memory_type TEXT NOT NULL,
    content TEXT NOT NULL,
    summary TEXT,
    user_id TEXT NOT NULL,
    source_session TEXT,
    source_turn INTEGER,
    entities TEXT DEFAULT '[]',
    topics TEXT DEFAULT '[]',
    tags TEXT DEFAULT '[]',
    importance REAL DEFAULT 0.5,
    confidence REAL DEFAULT 0.5,
    access_count INTEGER DEFAULT 0,
    last_accessed TEXT,
    supersedes TEXT DEFAULT '[]',
    embedding BLOB,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_mu_user ON memory_units(user_id);
CREATE INDEX IF NOT EXISTS idx_mu_type ON memory_units(memory_type);
CREATE INDEX IF NOT EXISTS idx_mu_importance ON memory_units(importance);

-- FTS5 virtual table for keyword search (BM25)
CREATE VIRTUAL TABLE IF NOT EXISTS memory_units_fts USING fts5(
    content, summary, entities, topics, tags,
    content='memory_units',
    content_rowid='rowid'
);

-- Keep FTS in sync via triggers
CREATE TRIGGER IF NOT EXISTS mu_fts_ai AFTER INSERT ON memory_units BEGIN
    INSERT INTO memory_units_fts(rowid, content, summary, entities, topics, tags)
    VALUES (new.rowid, new.content, new.summary, new.entities, new.topics, new.tags);
END;

CREATE TRIGGER IF NOT EXISTS mu_fts_ad AFTER DELETE ON memory_units BEGIN
    INSERT INTO memory_units_fts(memory_units_fts, rowid, content, summary, entities, topics, tags)
    VALUES ('delete', old.rowid, old.content, old.summary, old.entities, old.topics, old.tags);
END;

CREATE TRIGGER IF NOT EXISTS mu_fts_au AFTER UPDATE ON memory_units BEGIN
    INSERT INTO memory_units_fts(memory_units_fts, rowid, content, summary, entities, topics, tags)
    VALUES ('delete', old.rowid, old.content, old.summary, old.entities, old.topics, old.tags);
    INSERT INTO memory_units_fts(rowid, content, summary, entities, topics, tags)
    VALUES (new.rowid, new.content, new.summary, new.entities, new.topics, new.tags);
END;
"#;

/// SQLite-backed store for the 6-type memory system with FTS5 support.
pub struct MemoryStore {
    db: Arc<Mutex<Connection>>,
}

impl MemoryStore {
    /// Open (or create) the memory database at the given path.
    pub fn open(db_path: &Path) -> Result<Self> {
        let conn = Connection::open(db_path)
            .with_context(|| format!("Failed to open memory DB: {}", db_path.display()))?;

        conn.execute_batch(SCHEMA)
            .context("Failed to initialize memory_units schema")?;

        Ok(Self {
            db: Arc::new(Mutex::new(conn)),
        })
    }

    /// Open an in-memory database (for testing).
    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self {
            db: Arc::new(Mutex::new(conn)),
        })
    }

    /// Insert a new memory unit.
    pub fn insert(&self, unit: &MemoryUnit) -> Result<()> {
        let db = self.db.lock().map_err(|e| anyhow::anyhow!("DB lock: {e}"))?;
        let entities_json = serde_json::to_string(&unit.entities)?;
        let topics_json = serde_json::to_string(&unit.topics)?;
        let tags_json = serde_json::to_string(&unit.tags)?;
        let supersedes_json = serde_json::to_string(&unit.supersedes)?;
        let emb_bytes = unit.embedding.as_ref().map(|e| embedding_to_bytes(e));

        db.execute(
            "INSERT INTO memory_units (id, memory_type, content, summary, user_id,
             source_session, source_turn, entities, topics, tags,
             importance, confidence, access_count, last_accessed, supersedes,
             embedding, created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18)",
            params![
                unit.id, unit.memory_type.as_str(), unit.content, unit.summary, unit.user_id,
                unit.source_session, unit.source_turn, entities_json, topics_json, tags_json,
                unit.importance, unit.confidence, unit.access_count, unit.last_accessed,
                supersedes_json, emb_bytes, unit.created_at, unit.updated_at,
            ],
        )?;
        Ok(())
    }

    /// Get a memory unit by ID.
    pub fn get(&self, id: &str) -> Result<Option<MemoryUnit>> {
        let db = self.db.lock().map_err(|e| anyhow::anyhow!("DB lock: {e}"))?;
        let mut stmt = db.prepare(
            "SELECT id, memory_type, content, summary, user_id, source_session, source_turn,
             entities, topics, tags, importance, confidence, access_count, last_accessed,
             supersedes, embedding, created_at, updated_at
             FROM memory_units WHERE id = ?1",
        )?;

        let mut rows = stmt.query_map(params![id], |row| Ok(row_to_unit(row)))?;
        match rows.next() {
            Some(Ok(unit)) => Ok(Some(unit)),
            Some(Err(e)) => Err(e.into()),
            None => Ok(None),
        }
    }

    /// Update an existing memory unit.
    pub fn update(&self, unit: &MemoryUnit) -> Result<()> {
        let db = self.db.lock().map_err(|e| anyhow::anyhow!("DB lock: {e}"))?;
        let entities_json = serde_json::to_string(&unit.entities)?;
        let topics_json = serde_json::to_string(&unit.topics)?;
        let tags_json = serde_json::to_string(&unit.tags)?;
        let supersedes_json = serde_json::to_string(&unit.supersedes)?;
        let emb_bytes = unit.embedding.as_ref().map(|e| embedding_to_bytes(e));
        let now = chrono::Utc::now().to_rfc3339();

        db.execute(
            "UPDATE memory_units SET content=?2, summary=?3, entities=?4, topics=?5, tags=?6,
             importance=?7, confidence=?8, supersedes=?9, embedding=?10, updated_at=?11
             WHERE id=?1",
            params![unit.id, unit.content, unit.summary, entities_json, topics_json, tags_json,
                    unit.importance, unit.confidence, supersedes_json, emb_bytes, now],
        )?;
        Ok(())
    }

    /// Delete a memory unit by ID.
    pub fn delete(&self, id: &str) -> Result<bool> {
        let db = self.db.lock().map_err(|e| anyhow::anyhow!("DB lock: {e}"))?;
        let affected = db.execute("DELETE FROM memory_units WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }

    /// List all memory units for a user, optionally filtered by type.
    pub fn list(&self, user_id: &str, memory_type: Option<&MemoryType>, limit: usize) -> Result<Vec<MemoryUnit>> {
        let db = self.db.lock().map_err(|e| anyhow::anyhow!("DB lock: {e}"))?;

        let (sql, type_str);
        if let Some(mt) = memory_type {
            type_str = mt.as_str().to_string();
            sql = format!(
                "SELECT id, memory_type, content, summary, user_id, source_session, source_turn,
                 entities, topics, tags, importance, confidence, access_count, last_accessed,
                 supersedes, embedding, created_at, updated_at
                 FROM memory_units WHERE user_id = ?1 AND memory_type = ?2
                 ORDER BY importance DESC, updated_at DESC LIMIT ?3"
            );
            let mut stmt = db.prepare(&sql)?;
            let rows = stmt.query_map(params![user_id, type_str, limit as i64], |row| Ok(row_to_unit(row)))?;
            return Ok(rows.filter_map(|r| r.ok()).collect());
        }

        sql = "SELECT id, memory_type, content, summary, user_id, source_session, source_turn,
               entities, topics, tags, importance, confidence, access_count, last_accessed,
               supersedes, embedding, created_at, updated_at
               FROM memory_units WHERE user_id = ?1
               ORDER BY importance DESC, updated_at DESC LIMIT ?2".to_string();
        let mut stmt = db.prepare(&sql)?;
        let rows = stmt.query_map(params![user_id, limit as i64], |row| Ok(row_to_unit(row)))?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    /// FTS5 keyword search (BM25 ranked).
    pub fn search_keyword(&self, query: &str, user_id: &str, limit: usize) -> Result<Vec<(MemoryUnit, f64)>> {
        let db = self.db.lock().map_err(|e| anyhow::anyhow!("DB lock: {e}"))?;

        let mut stmt = db.prepare(
            "SELECT mu.id, mu.memory_type, mu.content, mu.summary, mu.user_id,
             mu.source_session, mu.source_turn, mu.entities, mu.topics, mu.tags,
             mu.importance, mu.confidence, mu.access_count, mu.last_accessed,
             mu.supersedes, mu.embedding, mu.created_at, mu.updated_at,
             rank
             FROM memory_units_fts fts
             JOIN memory_units mu ON mu.rowid = fts.rowid
             WHERE memory_units_fts MATCH ?1 AND mu.user_id = ?2
             ORDER BY rank LIMIT ?3",
        )?;

        let results = stmt.query_map(params![query, user_id, limit as i64], |row| {
            let unit = row_to_unit(row);
            let rank: f64 = row.get(18)?;
            // FTS5 rank is negative (more negative = better match), negate for positive score
            Ok((unit, -rank))
        })?;

        Ok(results.filter_map(|r| r.ok()).collect())
    }

    /// Embedding-based cosine similarity search.
    pub fn search_embedding(&self, query_embedding: &[f32], user_id: &str, limit: usize, threshold: f32) -> Result<Vec<(MemoryUnit, f64)>> {
        let db = self.db.lock().map_err(|e| anyhow::anyhow!("DB lock: {e}"))?;

        let mut stmt = db.prepare(
            "SELECT id, memory_type, content, summary, user_id, source_session, source_turn,
             entities, topics, tags, importance, confidence, access_count, last_accessed,
             supersedes, embedding, created_at, updated_at
             FROM memory_units WHERE user_id = ?1 AND embedding IS NOT NULL",
        )?;

        let all: Vec<MemoryUnit> = stmt.query_map(params![user_id], |row| Ok(row_to_unit(row)))?
            .filter_map(|r| r.ok())
            .collect();

        let mut scored: Vec<(MemoryUnit, f64)> = all
            .into_iter()
            .filter_map(|unit| {
                let emb = unit.embedding.as_ref()?;
                let sim = cosine_similarity(query_embedding, emb);
                if sim >= threshold as f64 {
                    Some((unit, sim))
                } else {
                    None
                }
            })
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit);
        Ok(scored)
    }

    /// Bump access_count and set last_accessed.
    pub fn record_access(&self, id: &str) -> Result<()> {
        let db = self.db.lock().map_err(|e| anyhow::anyhow!("DB lock: {e}"))?;
        let now = chrono::Utc::now().to_rfc3339();
        db.execute(
            "UPDATE memory_units SET access_count = access_count + 1, last_accessed = ?2 WHERE id = ?1",
            params![id, now],
        )?;
        Ok(())
    }

    /// Count all memory units for a user.
    pub fn count(&self, user_id: &str) -> Result<usize> {
        let db = self.db.lock().map_err(|e| anyhow::anyhow!("DB lock: {e}"))?;
        let count: i64 = db.query_row(
            "SELECT COUNT(*) FROM memory_units WHERE user_id = ?1",
            params![user_id],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    }

    /// Count by type for a user.
    pub fn count_by_type(&self, user_id: &str) -> Result<std::collections::HashMap<String, usize>> {
        let db = self.db.lock().map_err(|e| anyhow::anyhow!("DB lock: {e}"))?;
        let mut stmt = db.prepare(
            "SELECT memory_type, COUNT(*) FROM memory_units WHERE user_id = ?1 GROUP BY memory_type",
        )?;
        let mut map = std::collections::HashMap::new();
        let rows = stmt.query_map(params![user_id], |row| {
            let mt: String = row.get(0)?;
            let count: i64 = row.get(1)?;
            Ok((mt, count as usize))
        })?;
        for row in rows.flatten() {
            map.insert(row.0, row.1);
        }
        Ok(map)
    }

    /// Get all memory units (for consolidation). Not user-scoped.
    pub fn all_units(&self) -> Result<Vec<MemoryUnit>> {
        let db = self.db.lock().map_err(|e| anyhow::anyhow!("DB lock: {e}"))?;
        let mut stmt = db.prepare(
            "SELECT id, memory_type, content, summary, user_id, source_session, source_turn,
             entities, topics, tags, importance, confidence, access_count, last_accessed,
             supersedes, embedding, created_at, updated_at
             FROM memory_units ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], |row| Ok(row_to_unit(row)))?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }
}

// ── Helpers ──────────────────────────────────────────────────────────

fn row_to_unit(row: &rusqlite::Row<'_>) -> MemoryUnit {
    let memory_type_str: String = row.get(1).unwrap_or_default();
    let entities_json: String = row.get(7).unwrap_or_else(|_| "[]".to_string());
    let topics_json: String = row.get(8).unwrap_or_else(|_| "[]".to_string());
    let tags_json: String = row.get(9).unwrap_or_else(|_| "[]".to_string());
    let supersedes_json: String = row.get(14).unwrap_or_else(|_| "[]".to_string());
    let emb_bytes: Option<Vec<u8>> = row.get(15).ok();

    MemoryUnit {
        id: row.get(0).unwrap_or_default(),
        memory_type: MemoryType::from_str(&memory_type_str).unwrap_or(MemoryType::Semantic),
        content: row.get(2).unwrap_or_default(),
        summary: row.get(3).ok(),
        user_id: row.get(4).unwrap_or_default(),
        source_session: row.get(5).ok(),
        source_turn: row.get(6).ok(),
        entities: serde_json::from_str(&entities_json).unwrap_or_default(),
        topics: serde_json::from_str(&topics_json).unwrap_or_default(),
        tags: serde_json::from_str(&tags_json).unwrap_or_default(),
        importance: row.get(10).unwrap_or(0.5),
        confidence: row.get(11).unwrap_or(0.5),
        access_count: row.get(12).unwrap_or(0),
        last_accessed: row.get(13).ok(),
        supersedes: serde_json::from_str(&supersedes_json).unwrap_or_default(),
        embedding: emb_bytes.map(|b| bytes_to_embedding(&b)),
        created_at: row.get(16).unwrap_or_default(),
        updated_at: row.get(17).unwrap_or_default(),
    }
}

pub fn embedding_to_bytes(embedding: &[f32]) -> Vec<u8> {
    embedding.iter().flat_map(|f| f.to_le_bytes()).collect()
}

pub fn bytes_to_embedding(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}

pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let (mut dot, mut norm_a, mut norm_b) = (0.0f64, 0.0f64, 0.0f64);
    for (x, y) in a.iter().zip(b.iter()) {
        let (x, y) = (*x as f64, *y as f64);
        dot += x * y;
        norm_a += x * x;
        norm_b += y * y;
    }
    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom == 0.0 { 0.0 } else { dot / denom }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_store_crud() {
        let store = MemoryStore::open_in_memory().unwrap();
        let mut unit = MemoryUnit::new(MemoryType::Semantic, "Rust is fast".into(), "user1".into());
        unit.entities = vec!["Rust".into()];
        unit.topics = vec!["programming".into()];

        store.insert(&unit).unwrap();
        let fetched = store.get(&unit.id).unwrap().unwrap();
        assert_eq!(fetched.content, "Rust is fast");
        assert_eq!(fetched.memory_type, MemoryType::Semantic);
        assert_eq!(fetched.entities, vec!["Rust"]);

        // Update
        let mut updated = fetched;
        updated.content = "Rust is fast and safe".into();
        store.update(&updated).unwrap();
        let fetched2 = store.get(&unit.id).unwrap().unwrap();
        assert_eq!(fetched2.content, "Rust is fast and safe");

        // Delete
        assert!(store.delete(&unit.id).unwrap());
        assert!(store.get(&unit.id).unwrap().is_none());
    }

    #[test]
    fn test_store_list_by_type() {
        let store = MemoryStore::open_in_memory().unwrap();
        store.insert(&MemoryUnit::new(MemoryType::Semantic, "fact1".into(), "u1".into())).unwrap();
        store.insert(&MemoryUnit::new(MemoryType::Preference, "pref1".into(), "u1".into())).unwrap();
        store.insert(&MemoryUnit::new(MemoryType::Semantic, "fact2".into(), "u1".into())).unwrap();

        let semantics = store.list("u1", Some(&MemoryType::Semantic), 100).unwrap();
        assert_eq!(semantics.len(), 2);
        let prefs = store.list("u1", Some(&MemoryType::Preference), 100).unwrap();
        assert_eq!(prefs.len(), 1);
    }

    #[test]
    fn test_fts_search() {
        let store = MemoryStore::open_in_memory().unwrap();
        store.insert(&MemoryUnit::new(MemoryType::Semantic, "Rust programming language".into(), "u1".into())).unwrap();
        store.insert(&MemoryUnit::new(MemoryType::Semantic, "Python data science".into(), "u1".into())).unwrap();

        let results = store.search_keyword("Rust programming", "u1", 10).unwrap();
        assert!(!results.is_empty());
        assert!(results[0].0.content.contains("Rust"));
    }

    #[test]
    fn test_embedding_search() {
        let store = MemoryStore::open_in_memory().unwrap();
        let mut u1 = MemoryUnit::new(MemoryType::Semantic, "close match".into(), "u1".into());
        u1.embedding = Some(vec![1.0, 0.0, 0.0]);
        store.insert(&u1).unwrap();

        let mut u2 = MemoryUnit::new(MemoryType::Semantic, "distant".into(), "u1".into());
        u2.embedding = Some(vec![0.0, 1.0, 0.0]);
        store.insert(&u2).unwrap();

        let query = [0.9, 0.1, 0.0];
        let results = store.search_embedding(&query, "u1", 10, 0.5).unwrap();
        assert!(!results.is_empty());
        assert!(results[0].0.content.contains("close"));
    }

    #[test]
    fn test_count_by_type() {
        let store = MemoryStore::open_in_memory().unwrap();
        store.insert(&MemoryUnit::new(MemoryType::Episodic, "e1".into(), "u1".into())).unwrap();
        store.insert(&MemoryUnit::new(MemoryType::Episodic, "e2".into(), "u1".into())).unwrap();
        store.insert(&MemoryUnit::new(MemoryType::Preference, "p1".into(), "u1".into())).unwrap();

        let counts = store.count_by_type("u1").unwrap();
        assert_eq!(counts.get("episodic"), Some(&2));
        assert_eq!(counts.get("preference"), Some(&1));
    }

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 1e-6);

        let c = vec![0.0, 1.0, 0.0];
        assert!((cosine_similarity(&a, &c)).abs() < 1e-6);
    }

    #[test]
    fn test_embedding_bytes_roundtrip() {
        let emb = vec![1.0f32, 2.5, -3.14, 0.0];
        let bytes = embedding_to_bytes(&emb);
        let back = bytes_to_embedding(&bytes);
        assert_eq!(emb.len(), back.len());
        for (a, b) in emb.iter().zip(back.iter()) {
            assert!((a - b).abs() < 1e-6);
        }
    }
}
