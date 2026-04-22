use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;

use super::store::{self, MemoryStore};
use super::types::{MatchSource, MemorySearchResult, MemoryType, RetrievalMode};

/// Embedding generation client (wraps OpenAI API).
pub struct EmbeddingClient {
    api_key: String,
    model: String,
    http: reqwest::Client,
}

impl EmbeddingClient {
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            api_key,
            model,
            http: reqwest::Client::new(),
        }
    }

    pub async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let body = serde_json::json!({
            "input": [text],
            "model": self.model,
        });

        let resp = self
            .http
            .post("https://api.openai.com/v1/embeddings")
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Embeddings API {status}: {text}");
        }

        let json: serde_json::Value = resp.json().await?;
        let emb: Vec<f32> = json["data"][0]["embedding"]
            .as_array()
            .map(|arr| arr.iter().map(|v| v.as_f64().unwrap_or(0.0) as f32).collect())
            .unwrap_or_default();
        Ok(emb)
    }
}

/// Memory retriever supporting 4 modes: keyword, embedding, hybrid (RRF), auto.
pub struct MemoryRetriever {
    store: Arc<MemoryStore>,
    embeddings: Option<Arc<EmbeddingClient>>,
    default_mode: RetrievalMode,
    rrf_k: f64,
    similarity_threshold: f32,
}

impl MemoryRetriever {
    pub fn new(
        store: Arc<MemoryStore>,
        embeddings: Option<Arc<EmbeddingClient>>,
        default_mode: RetrievalMode,
    ) -> Self {
        Self {
            store,
            embeddings,
            default_mode,
            rrf_k: 60.0,
            similarity_threshold: 0.35,
        }
    }

    pub fn set_threshold(&mut self, threshold: f32) {
        self.similarity_threshold = threshold;
    }

    /// Get a reference to the embeddings client (if configured).
    pub fn embeddings(&self) -> Option<&Arc<EmbeddingClient>> {
        self.embeddings.as_ref()
    }

    /// Run a search using the configured retrieval mode.
    pub async fn search(&self, query: &str, user_id: &str, limit: usize) -> Result<Vec<MemorySearchResult>> {
        let mode = match self.default_mode {
            RetrievalMode::Auto => self.auto_select_mode(query),
            ref m => m.clone(),
        };

        match mode {
            RetrievalMode::Keyword => self.search_keyword(query, user_id, limit),
            RetrievalMode::Embedding => self.search_embedding(query, user_id, limit).await,
            RetrievalMode::Hybrid => self.search_hybrid(query, user_id, limit).await,
            RetrievalMode::Auto => unreachable!(),
        }
    }

    /// Auto-select: short queries (< 4 words) → keyword, longer → hybrid.
    fn auto_select_mode(&self, query: &str) -> RetrievalMode {
        let word_count = query.split_whitespace().count();
        if word_count < 4 || self.embeddings.is_none() {
            RetrievalMode::Keyword
        } else {
            RetrievalMode::Hybrid
        }
    }

    /// BM25-ranked keyword search via FTS5.
    fn search_keyword(&self, query: &str, user_id: &str, limit: usize) -> Result<Vec<MemorySearchResult>> {
        let results = self.store.search_keyword(query, user_id, limit)?;
        Ok(results
            .into_iter()
            .map(|(unit, score)| MemorySearchResult {
                unit,
                score,
                match_source: MatchSource::Keyword,
            })
            .collect())
    }

    /// Cosine-similarity embedding search.
    async fn search_embedding(&self, query: &str, user_id: &str, limit: usize) -> Result<Vec<MemorySearchResult>> {
        let emb_client = self.embeddings.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Embedding client not configured"))?;

        let query_emb = emb_client.embed(query).await?;
        let results = self.store.search_embedding(&query_emb, user_id, limit, self.similarity_threshold)?;

        Ok(results
            .into_iter()
            .map(|(unit, score)| MemorySearchResult {
                unit,
                score,
                match_source: MatchSource::Embedding,
            })
            .collect())
    }

    /// Hybrid search: run both keyword + embedding, fuse with RRF.
    async fn search_hybrid(&self, query: &str, user_id: &str, limit: usize) -> Result<Vec<MemorySearchResult>> {
        let fetch_limit = limit * 3; // over-fetch for better fusion

        let keyword_results = self.search_keyword(query, user_id, fetch_limit)?;
        let embedding_results = self.search_embedding(query, user_id, fetch_limit).await
            .unwrap_or_default(); // graceful degradation if embeddings fail

        let fused = reciprocal_rank_fusion(&keyword_results, &embedding_results, self.rrf_k, limit);
        Ok(fused)
    }

    /// Build context string from relevant memories for injection into LLM prompt.
    pub async fn build_context(&self, query: &str, user_id: &str, max_units: usize, max_chars: usize) -> Result<String> {
        let results = self.search(query, user_id, max_units).await?;
        if results.is_empty() {
            return Ok(String::new());
        }

        // Record access for retrieved memories
        for result in &results {
            let _ = self.store.record_access(&result.unit.id);
        }

        let mut context = String::from("\n[Relevant memories]\n");
        let mut chars = 0;
        for result in &results {
            let entry = format!(
                "- [{}] {}\n",
                result.unit.memory_type, result.unit.content
            );
            if chars + entry.len() > max_chars {
                break;
            }
            context.push_str(&entry);
            chars += entry.len();
        }

        Ok(context)
    }
}

/// Reciprocal Rank Fusion (RRF) — combines keyword and embedding results.
/// Formula: score(d) = Σ 1/(k + rank_i) for each retrieval method.
fn reciprocal_rank_fusion(
    keyword_results: &[MemorySearchResult],
    embedding_results: &[MemorySearchResult],
    k: f64,
    limit: usize,
) -> Vec<MemorySearchResult> {
    let mut scores: HashMap<String, f64> = HashMap::new();
    let mut units: HashMap<String, &MemorySearchResult> = HashMap::new();

    for (rank, result) in keyword_results.iter().enumerate() {
        let id = &result.unit.id;
        *scores.entry(id.clone()).or_default() += 1.0 / (k + rank as f64 + 1.0);
        units.entry(id.clone()).or_insert(result);
    }

    for (rank, result) in embedding_results.iter().enumerate() {
        let id = &result.unit.id;
        *scores.entry(id.clone()).or_default() += 1.0 / (k + rank as f64 + 1.0);
        units.entry(id.clone()).or_insert(result);
    }

    // Apply type boosts
    for (id, score) in scores.iter_mut() {
        if let Some(result) = units.get(id) {
            match result.unit.memory_type {
                MemoryType::WorkingSummary => *score *= 1.2,
                MemoryType::Preference => *score *= 1.1,
                MemoryType::ProjectState => *score *= 1.15,
                _ => {}
            }
            // Importance boost
            *score *= 0.8 + result.unit.importance * 0.4;
        }
    }

    let mut sorted: Vec<(String, f64)> = scores.into_iter().collect();
    sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    sorted.truncate(limit);

    sorted
        .into_iter()
        .filter_map(|(id, score)| {
            units.get(&id).map(|r| MemorySearchResult {
                unit: r.unit.clone(),
                score,
                match_source: MatchSource::Hybrid,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_v2::types::MemoryUnit;

    #[test]
    fn test_rrf_fusion() {
        let make = |id: &str, content: &str| MemorySearchResult {
            unit: {
                let mut u = MemoryUnit::new(MemoryType::Semantic, content.into(), "u1".into());
                u.id = id.into();
                u
            },
            score: 1.0,
            match_source: MatchSource::Keyword,
        };

        let keyword = vec![make("a", "result a"), make("b", "result b")];
        let embedding = vec![make("b", "result b"), make("c", "result c")];

        let fused = reciprocal_rank_fusion(&keyword, &embedding, 60.0, 10);
        // "b" should rank highest (appears in both)
        assert!(!fused.is_empty());
        assert_eq!(fused[0].unit.id, "b");
    }

    #[test]
    fn test_auto_mode_short_query() {
        let store = Arc::new(MemoryStore::open_in_memory().unwrap());
        let retriever = MemoryRetriever::new(store, None, RetrievalMode::Auto);
        assert_eq!(retriever.auto_select_mode("rust"), RetrievalMode::Keyword);
        assert_eq!(retriever.auto_select_mode("a b c"), RetrievalMode::Keyword); // 3 words → keyword
    }
}
