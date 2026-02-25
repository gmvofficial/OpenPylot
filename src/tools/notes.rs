use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;
use uuid::Uuid;

use crate::tools::{Tool, ToolDefinition, ToolResult};

// ── Note data model ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    pub id: String,
    pub title: String,
    pub content: String,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct NotesStore {
    notes: Vec<Note>,
}

// ── File I/O helpers ─────────────────────────────────────────────────

fn notes_path(data_dir: &PathBuf) -> PathBuf {
    data_dir.join("notes.json")
}

fn load_notes(data_dir: &PathBuf) -> Result<NotesStore> {
    let path = notes_path(data_dir);
    if !path.exists() {
        return Ok(NotesStore::default());
    }
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read notes from {}", path.display()))?;
    let store: NotesStore =
        serde_json::from_str(&content).with_context(|| "Failed to parse notes file")?;
    Ok(store)
}

fn save_notes(data_dir: &PathBuf, store: &NotesStore) -> Result<()> {
    let path = notes_path(data_dir);
    let content = serde_json::to_string_pretty(store)?;
    std::fs::write(&path, content)
        .with_context(|| format!("Failed to write notes to {}", path.display()))?;
    Ok(())
}

// ════════════════════════════════════════════════════════════════════
//  CreateNote
// ════════════════════════════════════════════════════════════════════

pub struct CreateNote {
    data_dir: PathBuf,
}

impl CreateNote {
    pub fn new(data_dir: PathBuf) -> Self {
        Self { data_dir }
    }
}

#[async_trait]
impl Tool for CreateNote {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "create_note".into(),
            description: "Create a new note with a title, content, and optional tags.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "title": {
                        "type": "string",
                        "description": "Title of the note"
                    },
                    "content": {
                        "type": "string",
                        "description": "Content/body of the note"
                    },
                    "tags": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional tags for categorizing the note"
                    }
                },
                "required": ["title", "content"]
            }),
        }
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let title = params["title"]
            .as_str()
            .context("Missing 'title' parameter")?
            .to_string();
        let content = params["content"]
            .as_str()
            .context("Missing 'content' parameter")?
            .to_string();
        let tags: Vec<String> = params["tags"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let now = Utc::now();
        let note = Note {
            id: Uuid::new_v4().to_string(),
            title: title.clone(),
            content,
            tags,
            created_at: now,
            updated_at: now,
        };

        let mut store = load_notes(&self.data_dir)?;
        store.notes.push(note.clone());
        save_notes(&self.data_dir, &store)?;

        Ok(ToolResult::ok(format!(
            "Note created successfully.\nID: {}\nTitle: {}",
            note.id, title
        )))
    }
}

// ════════════════════════════════════════════════════════════════════
//  ListNotes
// ════════════════════════════════════════════════════════════════════

pub struct ListNotes {
    data_dir: PathBuf,
}

impl ListNotes {
    pub fn new(data_dir: PathBuf) -> Self {
        Self { data_dir }
    }
}

#[async_trait]
impl Tool for ListNotes {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "list_notes".into(),
            description: "List all saved notes. Optionally filter by tag or limit results.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "tag": {
                        "type": "string",
                        "description": "Filter notes by this tag"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of notes to return (default: 20)"
                    }
                }
            }),
        }
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let store = load_notes(&self.data_dir)?;
        let tag_filter = params["tag"].as_str();
        let limit = params["limit"].as_u64().unwrap_or(20) as usize;

        let notes: Vec<&Note> = store
            .notes
            .iter()
            .filter(|n| {
                if let Some(tag) = tag_filter {
                    n.tags.iter().any(|t| t.eq_ignore_ascii_case(tag))
                } else {
                    true
                }
            })
            .rev() // newest first
            .take(limit)
            .collect();

        if notes.is_empty() {
            return Ok(ToolResult::ok("No notes found."));
        }

        let mut output = format!("Found {} note(s):\n\n", notes.len());
        for note in &notes {
            output.push_str(&format!(
                "- [{}] {} ({})\n  Tags: {}\n  Created: {}\n\n",
                &note.id[..8],
                note.title,
                if note.content.len() > 80 {
                    format!("{}...", &note.content[..80])
                } else {
                    note.content.clone()
                },
                if note.tags.is_empty() {
                    "none".to_string()
                } else {
                    note.tags.join(", ")
                },
                note.created_at.format("%Y-%m-%d %H:%M"),
            ));
        }

        Ok(ToolResult::ok(output))
    }
}

// ════════════════════════════════════════════════════════════════════
//  SearchNotes
// ════════════════════════════════════════════════════════════════════

pub struct SearchNotes {
    data_dir: PathBuf,
}

impl SearchNotes {
    pub fn new(data_dir: PathBuf) -> Self {
        Self { data_dir }
    }
}

#[async_trait]
impl Tool for SearchNotes {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "search_notes".into(),
            description: "Search notes by keyword in title or content.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query to match against note titles and content"
                    }
                },
                "required": ["query"]
            }),
        }
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let query = params["query"]
            .as_str()
            .context("Missing 'query' parameter")?
            .to_lowercase();

        let store = load_notes(&self.data_dir)?;

        let matches: Vec<&Note> = store
            .notes
            .iter()
            .filter(|n| {
                n.title.to_lowercase().contains(&query)
                    || n.content.to_lowercase().contains(&query)
                    || n.tags.iter().any(|t| t.to_lowercase().contains(&query))
            })
            .collect();

        if matches.is_empty() {
            return Ok(ToolResult::ok(format!(
                "No notes found matching '{}'.",
                query
            )));
        }

        let mut output = format!("Found {} note(s) matching '{}':\n\n", matches.len(), query);
        for note in &matches {
            output.push_str(&format!(
                "- [{}] {}\n  {}\n  Tags: {}\n\n",
                &note.id[..8],
                note.title,
                if note.content.len() > 120 {
                    format!("{}...", &note.content[..120])
                } else {
                    note.content.clone()
                },
                if note.tags.is_empty() {
                    "none".to_string()
                } else {
                    note.tags.join(", ")
                },
            ));
        }

        Ok(ToolResult::ok(output))
    }
}

// ════════════════════════════════════════════════════════════════════
//  DeleteNote
// ════════════════════════════════════════════════════════════════════

pub struct DeleteNote {
    data_dir: PathBuf,
}

impl DeleteNote {
    pub fn new(data_dir: PathBuf) -> Self {
        Self { data_dir }
    }
}

#[async_trait]
impl Tool for DeleteNote {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "delete_note".into(),
            description: "Delete a note by its ID (or partial ID).".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "note_id": {
                        "type": "string",
                        "description": "The ID (or first 8 characters) of the note to delete"
                    }
                },
                "required": ["note_id"]
            }),
        }
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let note_id = params["note_id"]
            .as_str()
            .context("Missing 'note_id' parameter")?;

        let mut store = load_notes(&self.data_dir)?;
        let original_len = store.notes.len();
        store.notes.retain(|n| !n.id.starts_with(note_id));

        if store.notes.len() == original_len {
            return Ok(ToolResult::err(format!(
                "No note found with ID starting with '{}'.",
                note_id
            )));
        }

        save_notes(&self.data_dir, &store)?;
        Ok(ToolResult::ok(format!(
            "Note deleted successfully (ID: {}).",
            note_id
        )))
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn temp_data_dir() -> (TempDir, PathBuf) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().to_path_buf();
        (dir, path)
    }

    #[tokio::test]
    async fn test_create_and_list_notes() {
        let (_dir, data_dir) = temp_data_dir();

        let create = CreateNote::new(data_dir.clone());
        let result = create
            .execute(json!({
                "title": "Test Note",
                "content": "Hello world",
                "tags": ["test", "demo"]
            }))
            .await
            .unwrap();
        assert!(result.success);

        let list = ListNotes::new(data_dir.clone());
        let result = list.execute(json!({})).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("Test Note"));
    }

    #[tokio::test]
    async fn test_search_notes() {
        let (_dir, data_dir) = temp_data_dir();

        let create = CreateNote::new(data_dir.clone());
        create
            .execute(json!({
                "title": "Grocery List",
                "content": "Buy milk and eggs"
            }))
            .await
            .unwrap();
        create
            .execute(json!({
                "title": "Work Tasks",
                "content": "Finish the report"
            }))
            .await
            .unwrap();

        let search = SearchNotes::new(data_dir.clone());
        let result = search.execute(json!({"query": "milk"})).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("Grocery List"));
        assert!(!result.output.contains("Work Tasks"));
    }

    #[tokio::test]
    async fn test_delete_note() {
        let (_dir, data_dir) = temp_data_dir();

        let create = CreateNote::new(data_dir.clone());
        let result = create
            .execute(json!({
                "title": "To Delete",
                "content": "This will be deleted"
            }))
            .await
            .unwrap();

        // Extract ID from output
        let id_line = result.output.lines().find(|l| l.starts_with("ID:")).unwrap();
        let id = id_line.trim_start_matches("ID: ").trim();

        let delete = DeleteNote::new(data_dir.clone());
        let result = delete
            .execute(json!({"note_id": &id[..8]}))
            .await
            .unwrap();
        assert!(result.success);

        let list = ListNotes::new(data_dir.clone());
        let result = list.execute(json!({})).await.unwrap();
        assert!(result.output.contains("No notes found"));
    }
}
