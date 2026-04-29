//! Embedded smart memory using SQLite + OpenAI embeddings.
//!
//! Zero external dependencies - no Docker, no Qdrant, no servers.
//! Works in terminal mode, pip install, brew install, frontend mode - everywhere.

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

use crate::config::AppConfig;
use crate::memory::MemoryStore;

/// A memory entry (fact, preference, or auto-extracted user statement).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: String,
    pub content: String,
    pub user_id: String,
    pub category: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub score: f32,
    pub created_at: String,
    pub updated_at: String,
}

/// A knowledge chunk from an indexed document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeEntry {
    pub id: String,
    pub content: String,
    pub source: Option<String>,
    pub title: Option<String>,
    pub chunk_index: Option<i32>,
    pub metadata: Option<serde_json::Value>,
    pub score: f32,
    pub created_at: String,
}

pub struct SmartMemory {
    db: Arc<Mutex<Connection>>,
    api_key: String,
    embedding_model: String,
    http: reqwest::Client,
    config: SmartMemoryConfig,
    /// LRU cache of (model + query) → embedding. Bounded at EMBED_CACHE_CAP entries.
    embed_cache: Arc<Mutex<EmbedCache>>,
}

const EMBED_CACHE_CAP: usize = 256;

struct EmbedCache {
    map: HashMap<String, Vec<f32>>,
    order: VecDeque<String>,
}

impl EmbedCache {
    fn new() -> Self {
        Self {
            map: HashMap::with_capacity(EMBED_CACHE_CAP),
            order: VecDeque::with_capacity(EMBED_CACHE_CAP),
        }
    }
    fn get(&mut self, key: &str) -> Option<Vec<f32>> {
        if let Some(v) = self.map.get(key).cloned() {
            // Move-to-back for LRU semantics
            if let Some(pos) = self.order.iter().position(|k| k == key) {
                self.order.remove(pos);
            }
            self.order.push_back(key.to_string());
            Some(v)
        } else {
            None
        }
    }
    fn put(&mut self, key: String, val: Vec<f32>) {
        if self.map.contains_key(&key) {
            return;
        }
        if self.map.len() >= EMBED_CACHE_CAP {
            if let Some(oldest) = self.order.pop_front() {
                self.map.remove(&oldest);
            }
        }
        self.order.push_back(key.clone());
        self.map.insert(key, val);
    }
}

#[derive(Clone)]
struct SmartMemoryConfig {
    similarity_threshold: f32,
    max_memory_context: usize,
    max_knowledge_context: usize,
    auto_extract: bool,
    extraction_interval: usize,
}

impl SmartMemory {
    pub async fn new(app_config: &AppConfig) -> Result<Self> {
        let openai_key = app_config
            .openai_api_key
            .clone()
            .context("OPENAI_API_KEY required for smart memory embeddings")?;

        let db_path = app_config.data_dir.join(&app_config.memory_db_name);

        let conn = Connection::open(&db_path)
            .with_context(|| format!("Failed to open smart memory DB: {}", db_path.display()))?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS memories (
                id TEXT PRIMARY KEY,
                user_id TEXT NOT NULL,
                content TEXT NOT NULL,
                embedding BLOB,
                category TEXT,
                metadata TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS knowledge_chunks (
                id TEXT PRIMARY KEY,
                collection_id TEXT,
                content TEXT NOT NULL,
                embedding BLOB,
                source TEXT,
                title TEXT,
                chunk_index INTEGER,
                metadata TEXT,
                created_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_memories_user_id ON memories(user_id);
            CREATE INDEX IF NOT EXISTS idx_knowledge_collection ON knowledge_chunks(collection_id);
            -- FTS5 mirror of knowledge_chunks.content for BM25 keyword search.
            -- 'porter unicode61' tokenizer = Porter stemming + unicode-aware (handles accents, languages).
            CREATE VIRTUAL TABLE IF NOT EXISTS knowledge_chunks_fts USING fts5(
                chunk_id UNINDEXED,
                content,
                tokenize = 'porter unicode61'
            );",
        )
        .context("Failed to initialize smart memory tables")?;

        // Backfill FTS index from any existing rows (one-time migration on first run after upgrade).
        // Cheap no-op if FTS already populated.
        let fts_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM knowledge_chunks_fts", [], |r| {
                r.get(0)
            })
            .unwrap_or(0);
        let kb_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM knowledge_chunks", [], |r| r.get(0))
            .unwrap_or(0);
        if fts_count == 0 && kb_count > 0 {
            tracing::info!(
                "Backfilling FTS5 index for {} existing knowledge chunks...",
                kb_count
            );
            conn.execute(
                "INSERT INTO knowledge_chunks_fts (chunk_id, content)
                 SELECT id, content FROM knowledge_chunks",
                [],
            )
            .context("FTS backfill failed")?;
            tracing::info!("FTS5 backfill complete");
        }

        let smart = Self {
            db: Arc::new(Mutex::new(conn)),
            api_key: openai_key,
            embedding_model: app_config.memory_embedding_model.clone(),
            http: reqwest::Client::new(),
            config: SmartMemoryConfig {
                similarity_threshold: app_config.memory_similarity_threshold,
                max_memory_context: app_config.memory_max_memory_context,
                max_knowledge_context: app_config.memory_max_knowledge_context,
                auto_extract: app_config.memory_auto_extract,
                extraction_interval: app_config.memory_extraction_interval,
            },
            embed_cache: Arc::new(Mutex::new(EmbedCache::new())),
        };

        smart.migrate_legacy_memory(&app_config.data_dir).await;
        Ok(smart)
    }

    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        // ── LRU cache lookup (key = model + text) ────────────────────────
        let cache_key = format!("{}::{}", self.embedding_model, text);
        if let Ok(mut cache) = self.embed_cache.lock() {
            if let Some(hit) = cache.get(&cache_key) {
                tracing::debug!("Embedding cache HIT for query of len {}", text.len());
                return Ok(hit);
            }
        }

        let results = self.embed_batch(&[text.to_string()]).await?;
        let emb = results
            .into_iter()
            .next()
            .context("Empty batch embedding response")?;

        if let Ok(mut cache) = self.embed_cache.lock() {
            cache.put(cache_key, emb.clone());
        }
        Ok(emb)
    }

    /// Embed multiple texts in a single API call (up to ~2048 inputs per call).
    /// Includes exponential-backoff retry on 5xx / 429 / network errors.
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let body = serde_json::json!({
            "input": texts,
            "model": self.embedding_model,
        });

        // Up to 3 attempts: 0ms, 500ms, 2000ms (with jitter)
        let mut last_err: Option<anyhow::Error> = None;
        for attempt in 0u32..3 {
            if attempt > 0 {
                let base_ms = 500u64 * (1 << (attempt - 1)); // 500, 1000, 2000
                let jitter = (chrono::Utc::now().timestamp_subsec_micros() as u64) % 250;
                let delay = std::time::Duration::from_millis(base_ms + jitter);
                tracing::warn!("Embedding API retry {}/3 after {:?}", attempt + 1, delay);
                tokio::time::sleep(delay).await;
            }

            let send_result = self
                .http
                .post("https://api.openai.com/v1/embeddings")
                .bearer_auth(&self.api_key)
                .json(&body)
                .send()
                .await;

            match send_result {
                Err(e) => {
                    // Network / connection-level failure → retry
                    last_err = Some(anyhow::anyhow!("Embeddings request failed: {e}"));
                    continue;
                }
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        let json: serde_json::Value = resp
                            .json()
                            .await
                            .context("Failed to parse embeddings response")?;
                        let data = json["data"]
                            .as_array()
                            .context("No data array in embeddings response")?;
                        let mut indexed: Vec<(usize, Vec<f32>)> = data
                            .iter()
                            .filter_map(|item| {
                                let idx = item["index"].as_u64()? as usize;
                                let emb: Vec<f32> = item["embedding"]
                                    .as_array()?
                                    .iter()
                                    .map(|v| v.as_f64().unwrap_or(0.0) as f32)
                                    .collect();
                                Some((idx, emb))
                            })
                            .collect();
                        indexed.sort_by_key(|(i, _)| *i);
                        return Ok(indexed.into_iter().map(|(_, emb)| emb).collect());
                    }

                    // 429 (rate limit) and 5xx are transient → retry. 4xx (other) is fatal.
                    let body_text = resp.text().await.unwrap_or_default();
                    let is_transient = status.as_u16() == 429 || status.is_server_error();
                    let err = anyhow::anyhow!("Embeddings API returned {status}: {body_text}");
                    if !is_transient {
                        return Err(err);
                    }
                    last_err = Some(err);
                }
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("Embeddings API: exhausted retries")))
    }

    pub async fn remember(
        &self,
        fact: &str,
        user_id: &str,
        category: Option<&str>,
        metadata: Option<HashMap<String, serde_json::Value>>,
    ) -> Result<String> {
        let embedding = self.embed(fact).await?;
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let emb_bytes = embedding_to_bytes(&embedding);
        let meta_json = metadata.map(|m| serde_json::to_string(&m).unwrap_or_default());

        let db = self
            .db
            .lock()
            .map_err(|e| anyhow::anyhow!("DB lock: {e}"))?;
        db.execute(
            "INSERT INTO memories (id, user_id, content, embedding, category, metadata, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![id, user_id, fact, emb_bytes, category, meta_json, now, now],
        )
        .context("Failed to insert memory")?;

        Ok(id)
    }

    pub async fn recall(
        &self,
        query: &str,
        user_id: &str,
        limit: usize,
    ) -> Result<Vec<MemoryEntry>> {
        let query_emb = self.embed(query).await?;

        let rows = {
            let db = self
                .db
                .lock()
                .map_err(|e| anyhow::anyhow!("DB lock: {e}"))?;
            let mut stmt = db.prepare(
                "SELECT id, content, user_id, category, metadata, embedding, created_at, updated_at
                 FROM memories WHERE user_id = ?1 AND embedding IS NOT NULL",
            )?;

            let rows: Vec<(
                String,
                String,
                String,
                Option<String>,
                Option<String>,
                Vec<u8>,
                String,
                String,
            )> = stmt
                .query_map(params![user_id], |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                        row.get(7)?,
                    ))
                })?
                .filter_map(|r| r.ok())
                .collect();
            rows
        };

        let mut scored: Vec<MemoryEntry> = rows
            .into_iter()
            .filter_map(
                |(id, content, uid, category, meta_str, emb_bytes, created, updated)| {
                    let emb = bytes_to_embedding(&emb_bytes);
                    let score = cosine_similarity(&query_emb, &emb);
                    if score >= self.config.similarity_threshold {
                        let metadata = meta_str.and_then(|s| serde_json::from_str(&s).ok());
                        Some(MemoryEntry {
                            id,
                            content,
                            user_id: uid,
                            category,
                            metadata,
                            score,
                            created_at: created,
                            updated_at: updated,
                        })
                    } else {
                        None
                    }
                },
            )
            .collect();

        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scored.truncate(limit);
        Ok(scored)
    }

    pub async fn get_all_memories(&self, user_id: &str) -> Result<Vec<MemoryEntry>> {
        let db = self
            .db
            .lock()
            .map_err(|e| anyhow::anyhow!("DB lock: {e}"))?;
        let mut stmt = db.prepare(
            "SELECT id, content, user_id, category, metadata, created_at, updated_at
             FROM memories WHERE user_id = ?1 ORDER BY updated_at DESC",
        )?;

        let rows = stmt
            .query_map(params![user_id], |row| {
                let meta_str: Option<String> = row.get(4)?;
                Ok(MemoryEntry {
                    id: row.get(0)?,
                    content: row.get(1)?,
                    user_id: row.get(2)?,
                    category: row.get(3)?,
                    metadata: meta_str.and_then(|s| serde_json::from_str(&s).ok()),
                    score: 1.0,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(rows)
    }

    pub async fn get_memory(&self, id: &str) -> Result<Option<MemoryEntry>> {
        let db = self
            .db
            .lock()
            .map_err(|e| anyhow::anyhow!("DB lock: {e}"))?;
        let mut stmt = db.prepare(
            "SELECT id, content, user_id, category, metadata, created_at, updated_at
             FROM memories WHERE id = ?1",
        )?;

        let result = stmt
            .query_row(params![id], |row| {
                let meta_str: Option<String> = row.get(4)?;
                Ok(MemoryEntry {
                    id: row.get(0)?,
                    content: row.get(1)?,
                    user_id: row.get(2)?,
                    category: row.get(3)?,
                    metadata: meta_str.and_then(|s| serde_json::from_str(&s).ok()),
                    score: 1.0,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                })
            })
            .ok();

        Ok(result)
    }

    pub async fn update_memory(&self, id: &str, content: &str) -> Result<()> {
        let embedding = self.embed(content).await?;
        let emb_bytes = embedding_to_bytes(&embedding);
        let now = chrono::Utc::now().to_rfc3339();

        let db = self
            .db
            .lock()
            .map_err(|e| anyhow::anyhow!("DB lock: {e}"))?;
        let affected = db.execute(
            "UPDATE memories SET content = ?1, embedding = ?2, updated_at = ?3 WHERE id = ?4",
            params![content, emb_bytes, now, id],
        )?;

        if affected == 0 {
            anyhow::bail!("Memory {id} not found");
        }
        Ok(())
    }

    pub async fn forget(&self, id: &str) -> Result<()> {
        let db = self
            .db
            .lock()
            .map_err(|e| anyhow::anyhow!("DB lock: {e}"))?;
        db.execute("DELETE FROM memories WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub async fn reset_memories(&self, user_id: &str) -> Result<()> {
        let db = self
            .db
            .lock()
            .map_err(|e| anyhow::anyhow!("DB lock: {e}"))?;
        db.execute("DELETE FROM memories WHERE user_id = ?1", params![user_id])?;
        Ok(())
    }

    /// Delete all knowledge chunks matching a given title and source.
    pub async fn delete_knowledge_by_document(&self, title: &str, source: &str) -> Result<usize> {
        let db = self
            .db
            .lock()
            .map_err(|e| anyhow::anyhow!("DB lock: {e}"))?;
        // Cascade FTS rows by chunk_id first
        db.execute(
            "DELETE FROM knowledge_chunks_fts WHERE chunk_id IN
             (SELECT id FROM knowledge_chunks WHERE title = ?1 AND source = ?2)",
            params![title, source],
        )
        .ok();
        let deleted = db.execute(
            "DELETE FROM knowledge_chunks WHERE title = ?1 AND source = ?2",
            params![title, source],
        )?;
        Ok(deleted)
    }

    /// Delete all knowledge chunks belonging to a collection.
    pub async fn delete_knowledge_by_collection(&self, collection_id: &str) -> Result<usize> {
        let db = self
            .db
            .lock()
            .map_err(|e| anyhow::anyhow!("DB lock: {e}"))?;
        db.execute(
            "DELETE FROM knowledge_chunks_fts WHERE chunk_id IN
             (SELECT id FROM knowledge_chunks WHERE collection_id = ?1)",
            params![collection_id],
        )
        .ok();
        let deleted = db.execute(
            "DELETE FROM knowledge_chunks WHERE collection_id = ?1",
            params![collection_id],
        )?;
        Ok(deleted)
    }

    pub async fn index_document_chunk(
        &self,
        content: &str,
        metadata: HashMap<String, serde_json::Value>,
    ) -> Result<String> {
        let embedding = self.embed(content).await?;
        let id = Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let emb_bytes = embedding_to_bytes(&embedding);
        let source = metadata
            .get("source")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let title = metadata
            .get("title")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let chunk_index = metadata
            .get("chunk_index")
            .and_then(|v| v.as_i64())
            .map(|i| i as i32);
        let collection_id = metadata
            .get("collection_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let meta_json = serde_json::to_string(&metadata).unwrap_or_default();

        let db = self
            .db
            .lock()
            .map_err(|e| anyhow::anyhow!("DB lock: {e}"))?;
        db.execute(
            "INSERT INTO knowledge_chunks (id, collection_id, content, embedding, source, title, chunk_index, metadata, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![id, collection_id, content, emb_bytes, source, title, chunk_index, meta_json, now],
        )
        .context("Failed to insert knowledge chunk")?;
        // Mirror into FTS5 for hybrid search
        db.execute(
            "INSERT INTO knowledge_chunks_fts (chunk_id, content) VALUES (?1, ?2)",
            params![id, content],
        )
        .ok();

        Ok(id)
    }

    /// Index multiple chunks in batches, calling `on_progress(completed, total)` after each batch.
    /// Uses batch embedding API calls for much faster indexing of large documents.
    pub async fn index_document_chunks_batched<F>(
        &self,
        chunks: &[crate::document_chunker::Chunk],
        batch_size: usize,
        mut on_progress: F,
    ) -> Result<usize>
    where
        F: FnMut(usize, usize),
    {
        let total = chunks.len();
        if total == 0 {
            return Ok(0);
        }

        let mut indexed = 0;

        for batch in chunks.chunks(batch_size) {
            // Collect texts for batch embedding
            let texts: Vec<String> = batch.iter().map(|c| c.content.clone()).collect();
            let embeddings = self.embed_batch(&texts).await?;

            // Insert all chunks in this batch into the DB
            let db = self
                .db
                .lock()
                .map_err(|e| anyhow::anyhow!("DB lock: {e}"))?;
            for (chunk, embedding) in batch.iter().zip(embeddings.iter()) {
                let id = Uuid::new_v4().to_string();
                let now = chrono::Utc::now().to_rfc3339();
                let emb_bytes = embedding_to_bytes(embedding);
                let source = chunk
                    .metadata
                    .get("source")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let title = chunk
                    .metadata
                    .get("title")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let chunk_index = chunk
                    .metadata
                    .get("chunk_index")
                    .and_then(|v| v.as_i64())
                    .map(|i| i as i32);
                let collection_id = chunk
                    .metadata
                    .get("collection_id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let meta_json = serde_json::to_string(&chunk.metadata).unwrap_or_default();

                db.execute(
                    "INSERT INTO knowledge_chunks (id, collection_id, content, embedding, source, title, chunk_index, metadata, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![id, collection_id, chunk.content, emb_bytes, source, title, chunk_index, meta_json, now],
                )
                .context("Failed to insert knowledge chunk")?;
                // Mirror into FTS5 for hybrid search
                db.execute(
                    "INSERT INTO knowledge_chunks_fts (chunk_id, content) VALUES (?1, ?2)",
                    params![id, chunk.content],
                )
                .ok();
            }
            drop(db);

            indexed += batch.len();
            on_progress(indexed, total);
        }

        Ok(indexed)
    }

    /// Hybrid search: dense (cosine) + BM25 (FTS5) fused via Reciprocal Rank Fusion,
    /// then diversified with Maximal Marginal Relevance (MMR), then expanded with neighbor chunks.
    ///
    /// This is the high-quality RAG retrieval path used by `build_context()`.
    pub async fn search_knowledge(&self, query: &str, limit: usize) -> Result<Vec<KnowledgeEntry>> {
        tracing::debug!(
            "Hybrid knowledge search: query='{}', limit={}, threshold={}",
            query,
            limit,
            self.config.similarity_threshold
        );

        // Pull more candidates than `limit` so MMR has room to diversify (4× headroom).
        let candidate_pool = (limit * 4).max(20);

        // ── Stage 1a: dense retrieval (semantic) ──────────────────────────
        let dense_hits: Vec<(KnowledgeEntry, Option<String>, f32)> =
            self.dense_candidates(query, candidate_pool).await?;

        // ── Stage 1b: BM25 retrieval (lexical / keyword) ─────────────────
        let bm25_hits: Vec<(KnowledgeEntry, Option<String>, f32)> = self
            .bm25_candidates(query, candidate_pool)
            .unwrap_or_else(|e| {
                tracing::warn!("BM25 search failed (continuing with dense only): {e}");
                Vec::new()
            });

        tracing::info!(
            "Retrieval candidates: {} dense, {} BM25",
            dense_hits.len(),
            bm25_hits.len()
        );

        // ── Stage 2: Reciprocal Rank Fusion ──────────────────────────────
        // RRF score = Σ over each ranker of  1 / (k + rank)   where k=60 (standard).
        // This is rank-based and unitless, so it cleanly mixes cosine-similarity
        // and BM25 even though they live on different scales.
        const RRF_K: f32 = 60.0;
        let mut fused: HashMap<String, (KnowledgeEntry, Option<String>, f32)> = HashMap::new();

        for (rank, (entry, coll, _score)) in dense_hits.iter().enumerate() {
            let contribution = 1.0 / (RRF_K + (rank + 1) as f32);
            fused
                .entry(entry.id.clone())
                .and_modify(|e| e.2 += contribution)
                .or_insert_with(|| (entry.clone(), coll.clone(), contribution));
        }
        for (rank, (entry, coll, _score)) in bm25_hits.iter().enumerate() {
            let contribution = 1.0 / (RRF_K + (rank + 1) as f32);
            fused
                .entry(entry.id.clone())
                .and_modify(|e| e.2 += contribution)
                .or_insert_with(|| (entry.clone(), coll.clone(), contribution));
        }

        let mut fused_list: Vec<(KnowledgeEntry, Option<String>, f32)> =
            fused.into_values().collect();
        fused_list.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

        if fused_list.is_empty() {
            tracing::info!("Hybrid search: no candidates above any signal");
            return Ok(Vec::new());
        }

        // ── Stage 3: MMR for diversity ───────────────────────────────────
        // Pick `limit` chunks that are both high-relevance AND non-redundant.
        // Need embeddings for similarity-between-chunks comparison.
        let query_emb = self.embed(query).await?;
        let mut selected = self.mmr_select(&fused_list, &query_emb, limit, 0.7).await?;

        // ── Stage 4: neighbor expansion ──────────────────────────────────
        // For each selected chunk, also pull chunk_index ± 1 from the same collection
        // so the LLM sees coherent surrounding context (capped at score=0).
        let neighbors = self.fetch_neighbors(&selected)?;
        selected.extend(neighbors);

        // Final ordering: by source, then chunk_index for coherent reading order.
        let mut results: Vec<KnowledgeEntry> = selected.into_iter().map(|(e, _)| e).collect();
        results.sort_by(|a, b| {
            let s = a.source.cmp(&b.source);
            if s != std::cmp::Ordering::Equal {
                return s;
            }
            a.chunk_index.cmp(&b.chunk_index)
        });

        tracing::info!(
            "Hybrid search returned {} chunks (incl. neighbors)",
            results.len()
        );
        Ok(results)
    }

    /// Dense (cosine-similarity) candidates over all knowledge_chunks.
    /// Returns the top-N by score (no threshold filter — fusion will handle that).
    async fn dense_candidates(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<(KnowledgeEntry, Option<String>, f32)>> {
        let query_emb = self.embed(query).await?;

        let rows = {
            let db = self
                .db
                .lock()
                .map_err(|e| anyhow::anyhow!("DB lock: {e}"))?;
            let mut stmt = db.prepare(
                "SELECT id, content, source, title, chunk_index, metadata, embedding, created_at, collection_id
                 FROM knowledge_chunks WHERE embedding IS NOT NULL",
            )?;
            let rows: Vec<(
                String,
                String,
                Option<String>,
                Option<String>,
                Option<i32>,
                Option<String>,
                Vec<u8>,
                String,
                Option<String>,
            )> = stmt
                .query_map([], |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                        row.get(7)?,
                        row.get(8)?,
                    ))
                })?
                .filter_map(|r| r.ok())
                .collect();
            rows
        };

        let mut scored: Vec<(KnowledgeEntry, Option<String>, f32)> = rows
            .into_iter()
            .map(
                |(
                    id,
                    content,
                    source,
                    title,
                    chunk_index,
                    meta_str,
                    emb_bytes,
                    created,
                    coll_id,
                )| {
                    let emb = bytes_to_embedding(&emb_bytes);
                    let score = cosine_similarity(&query_emb, &emb);
                    let metadata = meta_str.and_then(|s| serde_json::from_str(&s).ok());
                    let entry = KnowledgeEntry {
                        id,
                        content,
                        source,
                        title,
                        chunk_index,
                        metadata,
                        score,
                        created_at: created,
                    };
                    (entry, coll_id, score)
                },
            )
            // Pre-filter: drop very weak signals so they don't pollute fusion.
            // Use a permissive floor (threshold − 0.1, min 0.0) since BM25 might still rescue them.
            .filter(|(_, _, s)| *s >= (self.config.similarity_threshold - 0.1).max(0.0))
            .collect();

        scored.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit);
        Ok(scored)
    }

    /// BM25 (FTS5) candidates. Returns top-N by BM25 rank (lower bm25 = better match in FTS5,
    /// so we negate to get a "higher = better" score for fusion convenience).
    fn bm25_candidates(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<(KnowledgeEntry, Option<String>, f32)>> {
        // Sanitize the query for FTS5 MATCH: strip operator chars, then quote each term.
        let cleaned: String = query
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c.is_whitespace() {
                    c
                } else {
                    ' '
                }
            })
            .collect();
        let terms: Vec<String> = cleaned
            .split_whitespace()
            .filter(|t| t.len() >= 2) // skip single chars
            .map(|t| format!("\"{}\"", t))
            .collect();
        if terms.is_empty() {
            return Ok(Vec::new());
        }
        // OR semantics — any term matches contributes to BM25.
        let match_expr = terms.join(" OR ");

        let db = self
            .db
            .lock()
            .map_err(|e| anyhow::anyhow!("DB lock: {e}"))?;
        let mut stmt = db.prepare(
            "SELECT k.id, k.content, k.source, k.title, k.chunk_index, k.metadata,
                    k.created_at, k.collection_id, bm25(knowledge_chunks_fts) AS rank
             FROM knowledge_chunks_fts f
             JOIN knowledge_chunks k ON k.id = f.chunk_id
             WHERE knowledge_chunks_fts MATCH ?1
             ORDER BY rank ASC
             LIMIT ?2",
        )?;
        let hits: Vec<(KnowledgeEntry, Option<String>, f32)> = stmt
            .query_map(params![match_expr, limit as i64], |row| {
                let meta_str: Option<String> = row.get(5)?;
                let bm25: f64 = row.get(8)?;
                let entry = KnowledgeEntry {
                    id: row.get(0)?,
                    content: row.get(1)?,
                    source: row.get(2)?,
                    title: row.get(3)?,
                    chunk_index: row.get(4)?,
                    metadata: meta_str.and_then(|s| serde_json::from_str(&s).ok()),
                    score: (-bm25) as f32,
                    created_at: row.get(6)?,
                };
                let coll: Option<String> = row.get(7)?;
                Ok((entry, coll, (-bm25) as f32))
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(hits)
    }

    /// Maximal Marginal Relevance: pick `k` items that maximize
    /// `λ * relevance(query, doc) − (1 − λ) * max_sim(doc, already_selected)`.
    /// Requires re-embedding selected docs on the fly (cheap from cache) — but
    /// since we already have embeddings stored as BLOBs, we use those directly.
    async fn mmr_select(
        &self,
        candidates: &[(KnowledgeEntry, Option<String>, f32)],
        query_emb: &[f32],
        k: usize,
        lambda: f32,
    ) -> Result<Vec<(KnowledgeEntry, Option<String>)>> {
        if candidates.is_empty() {
            return Ok(Vec::new());
        }
        let target = k.min(candidates.len());

        // Fetch embeddings for all candidates in one DB call.
        let ids: Vec<String> = candidates.iter().map(|(e, _, _)| e.id.clone()).collect();
        let embs: HashMap<String, Vec<f32>> = {
            let db = self
                .db
                .lock()
                .map_err(|e| anyhow::anyhow!("DB lock: {e}"))?;
            let placeholders = std::iter::repeat("?")
                .take(ids.len())
                .collect::<Vec<_>>()
                .join(",");
            let sql = format!(
                "SELECT id, embedding FROM knowledge_chunks WHERE id IN ({})",
                placeholders
            );
            let mut stmt = db.prepare(&sql)?;
            let params_dyn: Vec<&dyn rusqlite::ToSql> =
                ids.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
            let rows: Vec<(String, Vec<f32>)> = stmt
                .query_map(params_dyn.as_slice(), |row| {
                    let id: String = row.get(0)?;
                    let bytes: Vec<u8> = row.get(1)?;
                    Ok((id, bytes_to_embedding(&bytes)))
                })?
                .filter_map(|r| r.ok())
                .collect();
            rows.into_iter().collect()
        };

        // Pre-compute relevance to query for each candidate
        let rels: Vec<f32> = candidates
            .iter()
            .map(|(e, _, _)| {
                embs.get(&e.id)
                    .map(|emb| cosine_similarity(query_emb, emb))
                    .unwrap_or(0.0)
            })
            .collect();

        let mut selected_idx: Vec<usize> = Vec::with_capacity(target);
        let mut remaining: Vec<usize> = (0..candidates.len()).collect();

        // Greedy MMR
        while selected_idx.len() < target && !remaining.is_empty() {
            let mut best_i = 0usize;
            let mut best_score = f32::MIN;
            for (idx_pos, &cand_i) in remaining.iter().enumerate() {
                let rel = rels[cand_i];
                let max_sim_to_selected = selected_idx
                    .iter()
                    .map(|&s| {
                        let a = embs.get(&candidates[cand_i].0.id);
                        let b = embs.get(&candidates[s].0.id);
                        match (a, b) {
                            (Some(av), Some(bv)) => cosine_similarity(av, bv),
                            _ => 0.0,
                        }
                    })
                    .fold(0.0_f32, f32::max);
                let mmr = lambda * rel - (1.0 - lambda) * max_sim_to_selected;
                if mmr > best_score {
                    best_score = mmr;
                    best_i = idx_pos;
                }
            }
            let chosen = remaining.remove(best_i);
            selected_idx.push(chosen);
        }

        Ok(selected_idx
            .into_iter()
            .map(|i| {
                let (e, c, _) = &candidates[i];
                (e.clone(), c.clone())
            })
            .collect())
    }

    /// Fetch chunk_index ± 1 neighbors for each selected chunk (deduped against selection).
    fn fetch_neighbors(
        &self,
        selected: &[(KnowledgeEntry, Option<String>)],
    ) -> Result<Vec<(KnowledgeEntry, Option<String>)>> {
        let mut seen: std::collections::HashSet<(String, i32)> = std::collections::HashSet::new();
        for (e, c) in selected {
            if let (Some(cid), Some(ci)) = (c.as_ref(), e.chunk_index) {
                seen.insert((cid.clone(), ci));
            }
        }
        let mut wanted: Vec<(String, i32)> = Vec::new();
        for (e, c) in selected {
            if let (Some(cid), Some(ci)) = (c.as_ref(), e.chunk_index) {
                for off in [-1i32, 1] {
                    let ni = ci + off;
                    if ni >= 0 && !seen.contains(&(cid.clone(), ni)) {
                        wanted.push((cid.clone(), ni));
                        seen.insert((cid.clone(), ni));
                    }
                }
            }
        }
        if wanted.is_empty() {
            return Ok(Vec::new());
        }
        let db = self
            .db
            .lock()
            .map_err(|e| anyhow::anyhow!("DB lock: {e}"))?;
        let mut out: Vec<(KnowledgeEntry, Option<String>)> = Vec::new();
        for (cid, ci) in &wanted {
            let mut stmt = db.prepare(
                "SELECT id, content, source, title, chunk_index, metadata, created_at
                 FROM knowledge_chunks WHERE collection_id = ?1 AND chunk_index = ?2",
            )?;
            if let Ok(entry) = stmt.query_row(params![cid, ci], |row| {
                let meta_str: Option<String> = row.get(5)?;
                Ok(KnowledgeEntry {
                    id: row.get(0)?,
                    content: row.get(1)?,
                    source: row.get(2)?,
                    title: row.get(3)?,
                    chunk_index: row.get(4)?,
                    metadata: meta_str.and_then(|s| serde_json::from_str(&s).ok()),
                    score: 0.0,
                    created_at: row.get(6)?,
                })
            }) {
                out.push((entry, Some(cid.clone())));
            }
        }
        Ok(out)
    }

    pub async fn build_context(&self, query: &str, user_id: &str) -> Result<String> {
        tracing::info!(
            "Building context for query: '{}' (threshold={}, max_knowledge={})",
            query,
            self.config.similarity_threshold,
            self.config.max_knowledge_context
        );
        let mut context_parts = Vec::new();

        // ── Relevance gate ──────────────────────────────────────────────
        // Skip the (expensive) retrieval pipeline entirely for queries that
        // can't plausibly benefit from KB context: very short, greetings,
        // pure acknowledgements. Saves an embedding call + full-table scan
        // and avoids the "anchor effect" where irrelevant context distorts
        // the LLM's answer.
        let trimmed = query.trim().to_lowercase();
        let token_count = trimmed.split_whitespace().count();
        let is_trivial = token_count < 3
            || matches!(
                trimmed.as_str(),
                "hi" | "hey"
                    | "hello"
                    | "thanks"
                    | "thank you"
                    | "ok"
                    | "okay"
                    | "yes"
                    | "no"
                    | "yep"
                    | "nope"
            );
        let skip_kb = is_trivial;
        if skip_kb {
            tracing::info!("Relevance gate: skipping KB retrieval for trivial query");
        }

        let memories = match self
            .recall(query, user_id, self.config.max_memory_context)
            .await
        {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!("Memory recall failed: {e}");
                Vec::new()
            }
        };
        if !memories.is_empty() {
            tracing::info!("Found {} relevant memories for context", memories.len());
            context_parts.push("## What You Know About the User".to_string());
            for mem in &memories {
                context_parts.push(format!("- {}", mem.content));
            }
        }

        let knowledge = if skip_kb {
            Vec::new()
        } else {
            match self
                .search_knowledge(query, self.config.max_knowledge_context)
                .await
            {
                Ok(k) => k,
                Err(e) => {
                    tracing::warn!("Knowledge search failed: {e}");
                    Vec::new()
                }
            }
        };
        if !knowledge.is_empty() {
            tracing::info!(
                "Found {} relevant knowledge chunks for context",
                knowledge.len()
            );
            context_parts.push("\n## Relevant Knowledge from User's Documents".to_string());
            context_parts.push(
                "Use the following document excerpts to answer. \
                Each chunk is tagged with a citation marker like `[doc:Title#3]`. \
                When you use information from a chunk, cite it inline using that exact marker. \
                If the answer is not contained in these excerpts, say so honestly rather than guessing.\n"
                    .to_string()
            );

            // Group chunks by source document for coherent reading
            let mut by_source: std::collections::BTreeMap<String, Vec<&KnowledgeEntry>> =
                std::collections::BTreeMap::new();
            for doc in &knowledge {
                let key = format!(
                    "{} ({})",
                    doc.title.as_deref().unwrap_or("Unknown"),
                    doc.source.as_deref().unwrap_or("unknown")
                );
                by_source.entry(key).or_default().push(doc);
            }

            // Deduplicate: if two chunks share >60% of their words, keep the higher-scored one
            for (source_label, chunks) in &by_source {
                context_parts.push(format!("### {}", source_label));

                let mut used_contents: Vec<&str> = Vec::new();
                for chunk in chunks {
                    // Simple overlap check: skip if >60% words overlap with any already-used chunk
                    let words: std::collections::HashSet<&str> =
                        chunk.content.split_whitespace().collect();
                    let is_dup = used_contents.iter().any(|prev| {
                        let prev_words: std::collections::HashSet<&str> =
                            prev.split_whitespace().collect();
                        if prev_words.is_empty() || words.is_empty() {
                            return false;
                        }
                        let overlap = words.intersection(&prev_words).count();
                        let min_len = words.len().min(prev_words.len());
                        min_len > 0 && (overlap as f32 / min_len as f32) > 0.6
                    });

                    if is_dup {
                        tracing::debug!(
                            "Skipping duplicate chunk {} from {}",
                            chunk.chunk_index.unwrap_or(-1),
                            source_label
                        );
                        continue;
                    }

                    // Include section metadata if available
                    let section = chunk
                        .metadata
                        .as_ref()
                        .and_then(|m| m.get("section"))
                        .and_then(|v| v.as_str());
                    if let Some(sec) = section {
                        context_parts.push(format!("**Section: {}**", sec));
                    }

                    // Citation marker: [doc:Title#chunk_index] — short and stable.
                    let cite = format!(
                        "[doc:{}#{}]",
                        chunk.title.as_deref().unwrap_or("Unknown"),
                        chunk.chunk_index.unwrap_or(0)
                    );
                    context_parts.push(format!("{} {}", cite, chunk.content));
                    context_parts.push(String::new()); // blank line between chunks
                    used_contents.push(&chunk.content);
                }
            }
        } else {
            tracing::info!(
                "No knowledge chunks found above threshold for query: '{}'",
                query
            );
        }

        let result = context_parts.join("\n");
        tracing::info!("Built context: {} chars", result.len());
        Ok(result)
    }

    pub async fn summarize_conversation(
        &self,
        messages: &[crate::llm::Message],
        user_id: &str,
        conversation_id: &str,
    ) -> Result<()> {
        let formatted: Vec<String> = messages
            .iter()
            .map(|m| format!("{}: {}", m.role, m.content))
            .collect();
        let content = format!(
            "Conversation summary (id: {}):\n{}",
            conversation_id,
            formatted.join("\n")
        );

        let mut metadata = HashMap::new();
        metadata.insert(
            "type".to_string(),
            serde_json::json!("conversation_summary"),
        );
        metadata.insert(
            "conversation_id".to_string(),
            serde_json::json!(conversation_id),
        );

        self.remember(
            &content,
            user_id,
            Some("conversation_summary"),
            Some(metadata),
        )
        .await?;
        Ok(())
    }

    /// Extract key facts, preferences, and decisions from recent conversation messages
    /// using the LLM, then store each fact as a separate memory with deduplication.
    pub async fn extract_and_store_memories(
        &self,
        messages: &[crate::llm::Message],
        user_id: &str,
    ) -> Result<usize> {
        if messages.is_empty() {
            return Ok(0);
        }

        // Format messages for the extraction prompt
        let formatted: Vec<String> = messages
            .iter()
            .filter(|m| m.role == crate::llm::Role::User || m.role == crate::llm::Role::Assistant)
            .map(|m| format!("{}: {}", m.role, m.content))
            .collect();

        if formatted.is_empty() {
            return Ok(0);
        }

        let conversation_text = formatted.join("\n");

        // Use OpenAI chat API (we already have the API key) to extract facts
        let extraction_prompt = format!(
            "Extract key facts, user preferences, decisions, and important information from this conversation.\n\
            Return ONLY a JSON array of strings, where each string is one distinct fact.\n\
            Focus on:\n\
            - User preferences and personal details\n\
            - Decisions made or actions taken\n\
            - Important information the user shared\n\
            - Technical details or configurations discussed\n\
            Skip generic/trivial exchanges. If nothing notable, return [].\n\n\
            Conversation:\n{}\n\n\
            JSON array of facts:",
            conversation_text
        );

        let body = serde_json::json!({
            "model": "gpt-4o-mini",
            "messages": [
                {"role": "system", "content": "You extract structured facts from conversations. Always return valid JSON."},
                {"role": "user", "content": extraction_prompt}
            ],
            "temperature": 0.1,
            "max_tokens": 1024
        });

        let resp = self
            .http
            .post("https://api.openai.com/v1/chat/completions")
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .context("Memory extraction API call failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Memory extraction API returned {status}: {text}");
        }

        let json: serde_json::Value = resp
            .json()
            .await
            .context("Failed to parse extraction response")?;
        let content = json["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("[]");

        // Parse the JSON array of facts - handle potential markdown code blocks
        let clean_content = content
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();

        let facts: Vec<String> = match serde_json::from_str(clean_content) {
            Ok(f) => f,
            Err(e) => {
                tracing::warn!("Failed to parse extracted facts as JSON: {e}. Raw: {content}");
                return Ok(0);
            }
        };

        if facts.is_empty() {
            tracing::debug!("No notable facts extracted from conversation");
            return Ok(0);
        }

        // Deduplication: check each fact against existing memories
        let mut stored = 0;
        for fact in &facts {
            if fact.trim().is_empty() {
                continue;
            }

            // Check similarity against existing memories to avoid duplicates
            let existing = self.recall(fact, user_id, 3).await.unwrap_or_default();
            let is_duplicate = existing.iter().any(|m| m.score > 0.85);

            if is_duplicate {
                tracing::debug!(
                    "Skipping duplicate memory: '{}'",
                    &fact[..fact.len().min(80)]
                );
                continue;
            }

            let mut metadata = HashMap::new();
            metadata.insert("type".to_string(), serde_json::json!("extracted_fact"));

            if let Err(e) = self
                .remember(fact, user_id, Some("extracted"), Some(metadata))
                .await
            {
                tracing::warn!("Failed to store extracted fact: {e}");
            } else {
                stored += 1;
            }
        }

        tracing::info!(
            "Extracted {} facts from conversation, stored {} (after dedup)",
            facts.len(),
            stored
        );
        Ok(stored)
    }

    pub fn auto_extract_enabled(&self) -> bool {
        self.config.auto_extract
    }

    pub fn extraction_interval(&self) -> usize {
        self.config.extraction_interval
    }

    async fn migrate_legacy_memory(&self, data_dir: &Path) {
        let flag_path = data_dir.join("memory_migrated.flag");
        if flag_path.exists() {
            return;
        }

        let store = match MemoryStore::load(&data_dir.to_path_buf()) {
            Ok(s) => s,
            Err(_) => return,
        };

        if store.facts.is_empty() && store.summaries.is_empty() {
            let _ = std::fs::write(&flag_path, "done");
            return;
        }

        tracing::info!(
            "Migrating {} legacy facts + {} summaries to smart memory...",
            store.facts.len(),
            store.summaries.len()
        );

        for fact in &store.facts {
            let content = format!("{}: {}", fact.key, fact.value);
            if let Err(e) = self.remember(&content, "default", None, None).await {
                tracing::warn!("Failed to migrate fact '{}': {}", fact.key, e);
            }
        }

        for summary in &store.summaries {
            if let Err(e) = self
                .remember(&summary.summary, "default", Some("summary"), None)
                .await
            {
                tracing::warn!("Failed to migrate summary: {}", e);
            }
        }

        let _ = std::fs::write(&flag_path, "done");
        tracing::info!("Legacy memory migration complete");
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot / (norm_a * norm_b)
    }
}

fn embedding_to_bytes(embedding: &[f32]) -> Vec<u8> {
    embedding.iter().flat_map(|f| f.to_le_bytes()).collect()
}

// ---------------------------------------------------------------------------
// Trait implementation — allows SmartMemory to be used as a MemoryProvider
// ---------------------------------------------------------------------------

use crate::traits::MemoryProvider;

#[async_trait::async_trait]
impl MemoryProvider for SmartMemory {
    async fn remember(
        &self,
        content: &str,
        user_id: &str,
        category: Option<&str>,
        metadata: Option<HashMap<String, serde_json::Value>>,
    ) -> Result<String> {
        self.remember(content, user_id, category, metadata).await
    }

    async fn recall(&self, query: &str, user_id: &str, limit: usize) -> Result<Vec<MemoryEntry>> {
        self.recall(query, user_id, limit).await
    }

    async fn forget(&self, id: &str) -> Result<()> {
        self.forget(id).await
    }

    async fn build_context(&self, query: &str, user_id: &str) -> Result<String> {
        self.build_context(query, user_id).await
    }

    async fn extract_and_store(
        &self,
        conversation: &[crate::llm::Message],
        user_id: &str,
    ) -> Result<usize> {
        self.extract_and_store_memories(conversation, user_id).await
    }

    async fn search_knowledge(&self, query: &str, limit: usize) -> Result<Vec<KnowledgeEntry>> {
        self.search_knowledge(query, limit).await
    }

    fn auto_extract_enabled(&self) -> bool {
        self.auto_extract_enabled()
    }

    fn extraction_interval(&self) -> usize {
        self.extraction_interval()
    }
}

fn bytes_to_embedding(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}
