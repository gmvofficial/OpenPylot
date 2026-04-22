use std::collections::HashSet;

use anyhow::Result;

use super::store::MemoryStore;
use super::types::{ConsolidationReport, MemoryType};

/// Consolidates memories: dedup, merge near-duplicates, decay old memories.
pub struct MemoryConsolidator<'a> {
    store: &'a MemoryStore,
}

impl<'a> MemoryConsolidator<'a> {
    pub fn new(store: &'a MemoryStore) -> Self {
        Self { store }
    }

    /// Run all consolidation passes.
    pub fn consolidate(&self) -> Result<ConsolidationReport> {
        let mut report = ConsolidationReport::default();
        report.exact_dupes_removed = self.dedup_exact()?;
        report.near_dupes_merged = self.merge_near_duplicates()?;
        report.decayed_count = self.apply_importance_decay()?;
        report.stale_summaries_pruned = self.prune_stale_summaries()?;
        tracing::info!(
            "Memory consolidation: {} dupes removed, {} merged, {} decayed, {} summaries pruned",
            report.exact_dupes_removed, report.near_dupes_merged,
            report.decayed_count, report.stale_summaries_pruned,
        );
        Ok(report)
    }

    /// Remove exact content duplicates (keep the one with highest importance).
    fn dedup_exact(&self) -> Result<usize> {
        let units = self.store.all_units()?;
        let mut seen: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        let mut to_delete = Vec::new();

        for unit in &units {
            let key = format!("{}:{}", unit.user_id, unit.content.trim().to_lowercase());
            if let Some(existing_id) = seen.get(&key) {
                // Keep the one with higher importance
                if let Some(existing) = self.store.get(existing_id)? {
                    if unit.importance > existing.importance {
                        to_delete.push(existing_id.clone());
                        seen.insert(key, unit.id.clone());
                    } else {
                        to_delete.push(unit.id.clone());
                    }
                }
            } else {
                seen.insert(key, unit.id.clone());
            }
        }

        for id in &to_delete {
            self.store.delete(id)?;
        }
        Ok(to_delete.len())
    }

    /// Merge near-duplicates with Jaccard similarity > 0.80.
    fn merge_near_duplicates(&self) -> Result<usize> {
        let units = self.store.all_units()?;
        let mut merged_ids: HashSet<String> = HashSet::new();
        let mut merge_count = 0;

        for i in 0..units.len() {
            if merged_ids.contains(&units[i].id) {
                continue;
            }
            for j in (i + 1)..units.len() {
                if merged_ids.contains(&units[j].id) {
                    continue;
                }
                if units[i].user_id != units[j].user_id {
                    continue;
                }
                if units[i].memory_type != units[j].memory_type {
                    continue;
                }

                let sim = jaccard_similarity(&units[i].content, &units[j].content);
                if sim > 0.80 {
                    // Keep the longer/higher-importance one
                    let (keep, remove) = if units[i].importance >= units[j].importance {
                        (&units[i], &units[j])
                    } else {
                        (&units[j], &units[i])
                    };

                    // Mark the kept one as superseding the removed one
                    if let Some(mut keeper) = self.store.get(&keep.id)? {
                        keeper.supersedes.push(remove.id.clone());
                        let _ = self.store.update(&keeper);
                    }

                    self.store.delete(&remove.id)?;
                    merged_ids.insert(remove.id.clone());
                    merge_count += 1;
                }
            }
        }

        Ok(merge_count)
    }

    /// Decay importance of old, rarely-accessed memories.
    /// importance *= 0.95 for memories older than 30 days with access_count < 3.
    fn apply_importance_decay(&self) -> Result<usize> {
        let units = self.store.all_units()?;
        let cutoff = chrono::Utc::now() - chrono::Duration::days(30);
        let cutoff_str = cutoff.to_rfc3339();
        let mut decayed = 0;

        for unit in &units {
            if unit.access_count < 3 && unit.created_at < cutoff_str {
                let new_importance = (unit.importance * 0.95).max(0.01);
                if (new_importance - unit.importance).abs() > 0.001 {
                    let mut updated = unit.clone();
                    updated.importance = new_importance;
                    self.store.update(&updated)?;
                    decayed += 1;
                }
            }
        }

        Ok(decayed)
    }

    /// Keep only the newest WorkingSummary per user per session.
    fn prune_stale_summaries(&self) -> Result<usize> {
        let units = self.store.all_units()?;
        let summaries: Vec<_> = units
            .iter()
            .filter(|u| u.memory_type == MemoryType::WorkingSummary)
            .collect();

        // Group by (user_id, source_session)
        let mut groups: std::collections::HashMap<(String, String), Vec<&super::types::MemoryUnit>> =
            std::collections::HashMap::new();
        for s in &summaries {
            let session = s.source_session.clone().unwrap_or_default();
            groups
                .entry((s.user_id.clone(), session))
                .or_default()
                .push(s);
        }

        let mut pruned = 0;
        for (_, mut group) in groups {
            if group.len() <= 1 {
                continue;
            }
            // Sort by created_at descending, keep newest
            group.sort_by(|a, b| b.created_at.cmp(&a.created_at));
            for stale in &group[1..] {
                self.store.delete(&stale.id)?;
                pruned += 1;
            }
        }

        Ok(pruned)
    }
}

/// Jaccard similarity between two strings (word-level).
fn jaccard_similarity(a: &str, b: &str) -> f64 {
    let set_a: HashSet<&str> = a.split_whitespace().collect();
    let set_b: HashSet<&str> = b.split_whitespace().collect();
    let intersection = set_a.intersection(&set_b).count() as f64;
    let union = set_a.union(&set_b).count() as f64;
    if union == 0.0 {
        0.0
    } else {
        intersection / union
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_v2::types::MemoryUnit;

    #[test]
    fn test_jaccard_identical() {
        assert!((jaccard_similarity("hello world", "hello world") - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_jaccard_disjoint() {
        assert!((jaccard_similarity("hello world", "foo bar")).abs() < 1e-6);
    }

    #[test]
    fn test_jaccard_partial() {
        let sim = jaccard_similarity("a b c d", "a b c e");
        assert!(sim > 0.5 && sim < 1.0); // 3/5 = 0.6
    }

    #[test]
    fn test_dedup_exact() {
        let store = MemoryStore::open_in_memory().unwrap();
        let u1 = MemoryUnit::new(MemoryType::Semantic, "duplicate content".into(), "u1".into());
        let u2 = MemoryUnit::new(MemoryType::Semantic, "duplicate content".into(), "u1".into());
        store.insert(&u1).unwrap();
        store.insert(&u2).unwrap();
        assert_eq!(store.count("u1").unwrap(), 2);

        let consolidator = MemoryConsolidator::new(&store);
        let removed = consolidator.dedup_exact().unwrap();
        assert_eq!(removed, 1);
        assert_eq!(store.count("u1").unwrap(), 1);
    }

    #[test]
    fn test_prune_stale_summaries() {
        let store = MemoryStore::open_in_memory().unwrap();
        let mut s1 = MemoryUnit::new(MemoryType::WorkingSummary, "old summary".into(), "u1".into());
        s1.source_session = Some("session1".into());
        s1.created_at = "2025-01-01T00:00:00Z".into();
        store.insert(&s1).unwrap();

        let mut s2 = MemoryUnit::new(MemoryType::WorkingSummary, "new summary".into(), "u1".into());
        s2.source_session = Some("session1".into());
        s2.created_at = "2025-06-01T00:00:00Z".into();
        store.insert(&s2).unwrap();

        let consolidator = MemoryConsolidator::new(&store);
        let pruned = consolidator.prune_stale_summaries().unwrap();
        assert_eq!(pruned, 1);

        let remaining = store.list("u1", Some(&MemoryType::WorkingSummary), 10).unwrap();
        assert_eq!(remaining.len(), 1);
        assert!(remaining[0].content.contains("new"));
    }
}
