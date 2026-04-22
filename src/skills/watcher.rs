//! # Skill Hot Reload Watcher
//!
//! Watches skill directories for changes and triggers a reload.
//! Mirrors OpenClaw's `skills.load.watch` + `watchDebounceMs` config.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing;

/// Event emitted when skills need reloading.
#[derive(Debug, Clone)]
pub enum SkillWatchEvent {
    /// A skill file was created or modified.
    Changed(String),
    /// A skill file was removed.
    Removed(String),
}

/// Start watching skill directories for changes.
/// Returns a receiver that emits events when skills change.
///
/// Requires the `notify` crate: `notify = "6"`
pub fn watch_skill_dirs(
    dirs: Vec<&Path>,
    debounce_ms: u64,
) -> anyhow::Result<mpsc::UnboundedReceiver<SkillWatchEvent>> {
    use notify::{RecommendedWatcher, RecursiveMode, Watcher, Event, EventKind};

    let (tx, rx) = mpsc::unbounded_channel();

    let debounce = Duration::from_millis(debounce_ms);

    let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
        match res {
            Ok(event) => {
                let is_skill = event.paths.iter().any(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| n.eq_ignore_ascii_case("SKILL.md"))
                        .unwrap_or(false)
                });

                if !is_skill {
                    return;
                }

                for path in &event.paths {
                    let path_str = path.display().to_string();
                    let event = match event.kind {
                        EventKind::Remove(_) => SkillWatchEvent::Removed(path_str),
                        _ => SkillWatchEvent::Changed(path_str),
                    };
                    let _ = tx.send(event);
                }
            }
            Err(e) => {
                tracing::warn!("Skill watcher error: {}", e);
            }
        }
    })?;

    for dir in dirs {
        if dir.exists() {
            watcher.watch(dir, RecursiveMode::Recursive)?;
            tracing::info!("Watching skill directory: {}", dir.display());
        }
    }

    // Leak the watcher so it keeps running (owned by the background thread).
    // In production, store it in the agent state for proper cleanup.
    std::mem::forget(watcher);

    Ok(rx)
}
