//! Embedded smart memory using SQLite + OpenAI embeddings.
//!
//! Zero external dependencies - no Docker, no Qdrant, no servers.
//! Works in terminal mode, pip install, brew install, frontend mode - everywhere.

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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

        let conn = Connection::open(&db_path).with_context(|| {
            format!("Failed to open smart memory DB: {}", db_path.display())
        })?;

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
            CREATE INDEX IF NOT EXISTS idx_knowledge_collection ON knowledge_chunks(collection_id);",
        )
        .context("Failed to initialize smart memory tables")?;

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
        };

        smart.migrate_legacy_memory(&app_config.data_dir).await;
        Ok(smart)
    }

    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let results = self.embed_batch(&[text.to_string()]).await?;
        results
            .into_iter()
            .next()
            .context("Empty batch embedding response")
    }

    /// Embed multiple texts in a single API call (up to ~2048 inputs per call).
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let body = serde_json::json!({
            "input": texts,
            "model": self.embedding_model,
        });

        let resp = self
            .http
            .post("https://api.openai.com/v1/embeddings")
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .context("Embeddings API request failed")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Embeddings API returned {status}: {text}");
        }

        let json: serde_json::Value =
            resp.json().await.context("Failed to parse embeddings response")?;

        let data = json["data"]
            .as_array()
            .context("No data array in embeddings response")?;

        // OpenAI returns results ordered by index, but let's sort just in case
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

        Ok(indexed.into_iter().map(|(_, emb)| emb).collect())
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

        let db = self.db.lock().map_err(|e| anyhow::anyhow!("DB lock: {e}"))?;
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
            let db = self.db.lock().map_err(|e| anyhow::anyhow!("DB lock: {e}"))?;
            let mut stmt = db.prepare(
                "SELECT id, content, user_id, category, metadata, embedding, created_at, updated_at
                 FROM memories WHERE user_id = ?1 AND embedding IS NOT NULL",
            )?;

            let rows: Vec<(String, String, String, Option<String>, Option<String>, Vec<u8>, String, String)> = stmt
                .query_map(params![user_id], |row| {
                    Ok((
                        row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?,
                        row.get(4)?, row.get(5)?, row.get(6)?, row.get(7)?,
                    ))
                })?
                .filter_map(|r| r.ok())
                .collect();
            rows
        };

        let mut scored: Vec<MemoryEntry> = rows
            .into_iter()
            .filter_map(|(id, content, uid, category, meta_str, emb_bytes, created, updated)| {
                let emb = bytes_to_embedding(&emb_bytes);
                let score = cosine_similarity(&query_emb, &emb);
                if score >= self.config.similarity_threshold {
                    let metadata = meta_str.and_then(|s| serde_json::from_str(&s).ok());
                    Some(MemoryEntry {
                        id, content, user_id: uid, category, metadata, score,
                        created_at: created, updated_at: updated,
                    })
                } else {
                    None
                }
            })
            .collect();

        scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit);
        Ok(scored)
    }

    pub async fn get_all_memories(&self, user_id: &str) -> Result<Vec<MemoryEntry>> {
        let db = self.db.lock().map_err(|e| anyhow::anyhow!("DB lock: {e}"))?;
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
        let db = self.db.lock().map_err(|e| anyhow::anyhow!("DB lock: {e}"))?;
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

        let db = self.db.lock().map_err(|e| anyhow::anyhow!("DB lock: {e}"))?;
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
        let db = self.db.lock().map_err(|e| anyhow::anyhow!("DB lock: {e}"))?;
        db.execute("DELETE FROM memories WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub async fn reset_memories(&self, user_id: &str) -> Result<()> {
        let db = self.db.lock().map_err(|e| anyhow::anyhow!("DB lock: {e}"))?;
        db.execute("DELETE FROM memories WHERE user_id = ?1", params![user_id])?;
        Ok(())
    }

    /// Delete all knowledge chunks matching a given title and source.
    pub async fn delete_knowledge_by_document(&self, title: &str, source: &str) -> Result<usize> {
        let db = self.db.lock().map_err(|e| anyhow::anyhow!("DB lock: {e}"))?;
        let deleted = db.execute(
            "DELETE FROM knowledge_chunks WHERE title = ?1 AND source = ?2",
            params![title, source],
        )?;
        Ok(deleted)
    }

    /// Delete all knowledge chunks belonging to a collection.
    pub async fn delete_knowledge_by_collection(&self, collection_id: &str) -> Result<usize> {
        let db = self.db.lock().map_err(|e| anyhow::anyhow!("DB lock: {e}"))?;
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
        let source = metadata.get("source").and_then(|v| v.as_str()).map(|s| s.to_string());
        let title = metadata.get("title").and_then(|v| v.as_str()).map(|s| s.to_string());
        let chunk_index = metadata.get("chunk_index").and_then(|v| v.as_i64()).map(|i| i as i32);
        let collection_id = metadata.get("collection_id").and_then(|v| v.as_str()).map(|s| s.to_string());
        let meta_json = serde_json::to_string(&metadata).unwrap_or_default();

        let db = self.db.lock().map_err(|e| anyhow::anyhow!("DB lock: {e}"))?;
        db.execute(
            "INSERT INTO knowledge_chunks (id, collection_id, content, embedding, source, title, chunk_index, metadata, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![id, collection_id, content, emb_bytes, source, title, chunk_index, meta_json, now],
        )
        .context("Failed to insert knowledge chunk")?;

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
            let db = self.db.lock().map_err(|e| anyhow::anyhow!("DB lock: {e}"))?;
            for (chunk, embedding) in batch.iter().zip(embeddings.iter()) {
                let id = Uuid::new_v4().to_string();
                let now = chrono::Utc::now().to_rfc3339();
                let emb_bytes = embedding_to_bytes(embedding);
                let source = chunk.metadata.get("source").and_then(|v| v.as_str()).map(|s| s.to_string());
                let title = chunk.metadata.get("title").and_then(|v| v.as_str()).map(|s| s.to_string());
                let chunk_index = chunk.metadata.get("chunk_index").and_then(|v| v.as_i64()).map(|i| i as i32);
                let collection_id = chunk.metadata.get("collection_id").and_then(|v| v.as_str()).map(|s| s.to_string());
                let meta_json = serde_json::to_string(&chunk.metadata).unwrap_or_default();

                db.execute(
                    "INSERT INTO knowledge_chunks (id, collection_id, content, embedding, source, title, chunk_index, metadata, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![id, collection_id, chunk.content, emb_bytes, source, title, chunk_index, meta_json, now],
                )
                .context("Failed to insert knowledge chunk")?;
            }
            drop(db);

            indexed += batch.len();
            on_progress(indexed, total);
        }

        Ok(indexed)
    }

    pub async fn search_knowledge(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<KnowledgeEntry>> {
        tracing::debug!("Knowledge search: query='{}', limit={}, threshold={}", query, limit, self.config.similarity_threshold);
        let query_emb = self.embed(query).await?;

        let rows = {
            let db = self.db.lock().map_err(|e| anyhow::anyhow!("DB lock: {e}"))?;
            let mut stmt = db.prepare(
                "SELECT id, content, source, title, chunk_index, metadata, embedding, created_at, collection_id
                 FROM knowledge_chunks WHERE embedding IS NOT NULL",
            )?;

            let rows: Vec<(String, String, Option<String>, Option<String>, Option<i32>, Option<String>, Vec<u8>, String, Option<String>)> = stmt
                .query_map([], |row| {
                    Ok((
                        row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?,
                        row.get(4)?, row.get(5)?, row.get(6)?, row.get(7)?,
                        row.get(8)?,
                    ))
                })?
                .filter_map(|r| r.ok())
                .collect();
            rows
        };

        let mut scored: Vec<(KnowledgeEntry, Option<String>)> = rows
            .into_iter()
            .filter_map(|(id, content, source, title, chunk_index, meta_str, emb_bytes, created, coll_id)| {
                let emb = bytes_to_embedding(&emb_bytes);
                let score = cosine_similarity(&query_emb, &emb);
                if score >= self.config.similarity_threshold {
                    let metadata = meta_str.and_then(|s| serde_json::from_str(&s).ok());
                    Some((KnowledgeEntry {
                        id, content, source, title, chunk_index, metadata, score,
                        created_at: created,
                    }, coll_id))
                } else {
                    None
                }
            })
            .collect();

        scored.sort_by(|a, b| b.0.score.partial_cmp(&a.0.score).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit);

        // Neighbor expansion: for each matched chunk, also fetch chunk_index ± 1 from same collection
        let mut neighbor_keys: Vec<(String, i32)> = Vec::new(); // (collection_id, chunk_index)
        let mut seen_keys: std::collections::HashSet<(String, i32)> = std::collections::HashSet::new();

        // Track which chunks we already have
        for (entry, coll_id) in &scored {
            if let (Some(cid), Some(cidx)) = (coll_id.as_ref(), entry.chunk_index) {
                seen_keys.insert((cid.clone(), cidx));
            }
        }

        // Collect neighbor indices we need
        for (entry, coll_id) in &scored {
            if let (Some(cid), Some(cidx)) = (coll_id.as_ref(), entry.chunk_index) {
                for offset in [-1i32, 1i32] {
                    let neighbor_idx = cidx + offset;
                    if neighbor_idx >= 0 && !seen_keys.contains(&(cid.clone(), neighbor_idx)) {
                        neighbor_keys.push((cid.clone(), neighbor_idx));
                        seen_keys.insert((cid.clone(), neighbor_idx));
                    }
                }
            }
        }

        // Fetch neighbor chunks from DB
        if !neighbor_keys.is_empty() {
            let db = self.db.lock().map_err(|e| anyhow::anyhow!("DB lock: {e}"))?;
            for (coll_id, cidx) in &neighbor_keys {
                let mut stmt = db.prepare(
                    "SELECT id, content, source, title, chunk_index, metadata, created_at
                     FROM knowledge_chunks WHERE collection_id = ?1 AND chunk_index = ?2",
                )?;
                if let Ok(entry) = stmt.query_row(params![coll_id, cidx], |row| {
                    let meta_str: Option<String> = row.get(5)?;
                    Ok(KnowledgeEntry {
                        id: row.get(0)?,
                        content: row.get(1)?,
                        source: row.get(2)?,
                        title: row.get(3)?,
                        chunk_index: row.get(4)?,
                        metadata: meta_str.and_then(|s| serde_json::from_str(&s).ok()),
                        score: 0.0, // neighbor chunks get score 0 (context-only)
                        created_at: row.get(6)?,
                    })
                }) {
                    scored.push((entry, Some(coll_id.clone())));
                }
            }
            tracing::debug!("Expanded with {} neighbor chunks", neighbor_keys.len());
        }

        // Extract just the entries, sorted by collection+chunk_index for coherent reading
        let mut results: Vec<KnowledgeEntry> = scored.into_iter().map(|(e, _)| e).collect();
        results.sort_by(|a, b| {
            let source_cmp = a.source.cmp(&b.source);
            if source_cmp != std::cmp::Ordering::Equal {
                return source_cmp;
            }
            a.chunk_index.cmp(&b.chunk_index)
        });

        tracing::info!("Knowledge search returned {} results (including neighbors)",
            results.len(),
        );
        Ok(results)
    }

    pub async fn build_context(&self, query: &str, user_id: &str) -> Result<String> {
        tracing::info!("Building context for query: '{}' (threshold={}, max_knowledge={})",
            query, self.config.similarity_threshold, self.config.max_knowledge_context);
        let mut context_parts = Vec::new();

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

        let knowledge = match self
            .search_knowledge(query, self.config.max_knowledge_context)
            .await
        {
            Ok(k) => k,
            Err(e) => {
                tracing::warn!("Knowledge search failed: {e}");
                Vec::new()
            }
        };
        if !knowledge.is_empty() {
            tracing::info!("Found {} relevant knowledge chunks for context",
                knowledge.len());
            context_parts.push("\n## Relevant Knowledge from User's Documents".to_string());
            context_parts.push("Use the following document excerpts to answer. Cite the source when possible.\n".to_string());

            // Group chunks by source document for coherent reading
            let mut by_source: std::collections::BTreeMap<String, Vec<&KnowledgeEntry>> = std::collections::BTreeMap::new();
            for doc in &knowledge {
                let key = format!("{} ({})",
                    doc.title.as_deref().unwrap_or("Unknown"),
                    doc.source.as_deref().unwrap_or("unknown"));
                by_source.entry(key).or_default().push(doc);
            }

            // Deduplicate: if two chunks share >60% of their words, keep the higher-scored one
            for (source_label, chunks) in &by_source {
                context_parts.push(format!("### {}", source_label));

                let mut used_contents: Vec<&str> = Vec::new();
                for chunk in chunks {
                    // Simple overlap check: skip if >60% words overlap with any already-used chunk
                    let words: std::collections::HashSet<&str> = chunk.content.split_whitespace().collect();
                    let is_dup = used_contents.iter().any(|prev| {
                        let prev_words: std::collections::HashSet<&str> = prev.split_whitespace().collect();
                        if prev_words.is_empty() || words.is_empty() {
                            return false;
                        }
                        let overlap = words.intersection(&prev_words).count();
                        let min_len = words.len().min(prev_words.len());
                        min_len > 0 && (overlap as f32 / min_len as f32) > 0.6
                    });

                    if is_dup {
                        tracing::debug!("Skipping duplicate chunk {} from {}", chunk.chunk_index.unwrap_or(-1), source_label);
                        continue;
                    }

                    // Include section metadata if available
                    let section = chunk.metadata.as_ref()
                        .and_then(|m| m.get("section"))
                        .and_then(|v| v.as_str());
                    if let Some(sec) = section {
                        context_parts.push(format!("**Section: {}**", sec));
                    }

                    context_parts.push(chunk.content.clone());
                    context_parts.push(String::new()); // blank line between chunks
                    used_contents.push(&chunk.content);
                }
            }
        } else {
            tracing::info!("No knowledge chunks found above threshold for query: '{}'", query);
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
        metadata.insert("type".to_string(), serde_json::json!("conversation_summary"));
        metadata.insert("conversation_id".to_string(), serde_json::json!(conversation_id));

        self.remember(&content, user_id, Some("conversation_summary"), Some(metadata))
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

        let json: serde_json::Value = resp.json().await.context("Failed to parse extraction response")?;
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
                tracing::debug!("Skipping duplicate memory: '{}'", &fact[..fact.len().min(80)]);
                continue;
            }

            let mut metadata = HashMap::new();
            metadata.insert("type".to_string(), serde_json::json!("extracted_fact"));

            if let Err(e) = self.remember(fact, user_id, Some("extracted"), Some(metadata)).await {
                tracing::warn!("Failed to store extracted fact: {e}");
            } else {
                stored += 1;
            }
        }

        tracing::info!("Extracted {} facts from conversation, stored {} (after dedup)", facts.len(), stored);
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
