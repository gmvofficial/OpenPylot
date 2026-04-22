//! # Advanced Memory System (v2)
//!
//! 6-type memory with FTS5 keyword search, embedding-based similarity search,
//! hybrid RRF retrieval, automatic extraction, and consolidation.

pub mod consolidator;
pub mod extractor;
pub mod retriever;
pub mod store;
pub mod types;

pub use consolidator::MemoryConsolidator;
pub use extractor::MemoryExtractor;
pub use retriever::{EmbeddingClient, MemoryRetriever};
pub use store::MemoryStore;
pub use types::{ConsolidationReport, MatchSource, MemorySearchResult, MemoryType, MemoryUnit, RetrievalMode};
