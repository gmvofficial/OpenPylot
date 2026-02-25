use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;
use uuid::Uuid;

use crate::tools::{Tool, ToolDefinition, ToolResult};

// ── Reminder data model ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reminder {
    pub id: String,
    pub title: String,
    pub description: String,
    pub remind_at: DateTime<Utc>,
    pub completed: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct ReminderStore {
    reminders: Vec<Reminder>,
}

// ── File I/O helpers ─────────────────────────────────────────────────

fn reminders_path(data_dir: &PathBuf) -> PathBuf {
    data_dir.join("reminders.json")
}

fn load_reminders(data_dir: &PathBuf) -> Result<ReminderStore> {
    let path = reminders_path(data_dir);
    if !path.exists() {
        return Ok(ReminderStore::default());
    }
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read reminders from {}", path.display()))?;
    let store: ReminderStore =
        serde_json::from_str(&content).with_context(|| "Failed to parse reminders file")?;
    Ok(store)
}

fn save_reminders(data_dir: &PathBuf, store: &ReminderStore) -> Result<()> {
    let path = reminders_path(data_dir);
    let content = serde_json::to_string_pretty(store)?;
    std::fs::write(&path, content)
        .with_context(|| format!("Failed to write reminders to {}", path.display()))?;
    Ok(())
}

// ════════════════════════════════════════════════════════════════════
//  SetReminder
// ════════════════════════════════════════════════════════════════════

pub struct SetReminder {
    data_dir: PathBuf,
}

impl SetReminder {
    pub fn new(data_dir: PathBuf) -> Self {
        Self { data_dir }
    }
}

#[async_trait]
impl Tool for SetReminder {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "set_reminder".into(),
            description: "Set a reminder for a specific date and time.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "title": {
                        "type": "string",
                        "description": "What to be reminded about"
                    },
                    "remind_at": {
                        "type": "string",
                        "description": "When to remind, in ISO 8601 format (e.g., 2026-02-26T15:00:00Z)"
                    },
                    "description": {
                        "type": "string",
                        "description": "Additional details for the reminder (optional)"
                    }
                },
                "required": ["title", "remind_at"]
            }),
        }
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let title = params["title"]
            .as_str()
            .context("Missing 'title' parameter")?
            .to_string();
        let remind_at_str = params["remind_at"]
            .as_str()
            .context("Missing 'remind_at' parameter")?;
        let description = params["description"]
            .as_str()
            .unwrap_or("")
            .to_string();

        let remind_at: DateTime<Utc> = remind_at_str
            .parse()
            .or_else(|_| {
                // Try parsing without timezone and assume UTC
                chrono::NaiveDateTime::parse_from_str(remind_at_str, "%Y-%m-%dT%H:%M:%S")
                    .map(|ndt| ndt.and_utc())
            })
            .with_context(|| {
                format!(
                    "Invalid datetime format: '{}'. Use ISO 8601 format.",
                    remind_at_str
                )
            })?;

        let now = Utc::now();
        let reminder = Reminder {
            id: Uuid::new_v4().to_string(),
            title: title.clone(),
            description,
            remind_at,
            completed: false,
            created_at: now,
        };

        let mut store = load_reminders(&self.data_dir)?;
        store.reminders.push(reminder.clone());
        save_reminders(&self.data_dir, &store)?;

        let time_until = remind_at.signed_duration_since(now);
        let hours = time_until.num_hours();
        let minutes = time_until.num_minutes() % 60;

        Ok(ToolResult::ok(format!(
            "Reminder set successfully!\nTitle: {}\nRemind at: {}\nTime until: {}h {}m\nID: {}",
            title,
            remind_at.format("%Y-%m-%d %H:%M UTC"),
            hours,
            minutes,
            &reminder.id[..8]
        )))
    }
}

// ════════════════════════════════════════════════════════════════════
//  ListReminders
// ════════════════════════════════════════════════════════════════════

pub struct ListReminders {
    data_dir: PathBuf,
}

impl ListReminders {
    pub fn new(data_dir: PathBuf) -> Self {
        Self { data_dir }
    }
}

#[async_trait]
impl Tool for ListReminders {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "list_reminders".into(),
            description: "List all reminders, optionally including completed ones.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "include_completed": {
                        "type": "boolean",
                        "description": "Whether to include completed reminders (default: false)"
                    }
                }
            }),
        }
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let include_completed = params["include_completed"].as_bool().unwrap_or(false);
        let store = load_reminders(&self.data_dir)?;

        let now = Utc::now();
        let reminders: Vec<&Reminder> = store
            .reminders
            .iter()
            .filter(|r| include_completed || !r.completed)
            .collect();

        if reminders.is_empty() {
            return Ok(ToolResult::ok("No reminders found."));
        }

        let mut output = format!("Found {} reminder(s):\n\n", reminders.len());
        for r in &reminders {
            let status = if r.completed {
                "✅ Completed"
            } else if r.remind_at <= now {
                "🔔 DUE"
            } else {
                "⏰ Pending"
            };

            output.push_str(&format!(
                "- [{}] {} ({})\n  When: {}\n  {}\n\n",
                &r.id[..8],
                r.title,
                status,
                r.remind_at.format("%Y-%m-%d %H:%M UTC"),
                if r.description.is_empty() {
                    String::new()
                } else {
                    format!("  Note: {}", r.description)
                }
            ));
        }

        Ok(ToolResult::ok(output))
    }
}

// ════════════════════════════════════════════════════════════════════
//  CompleteReminder
// ════════════════════════════════════════════════════════════════════

pub struct CompleteReminder {
    data_dir: PathBuf,
}

impl CompleteReminder {
    pub fn new(data_dir: PathBuf) -> Self {
        Self { data_dir }
    }
}

#[async_trait]
impl Tool for CompleteReminder {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "complete_reminder".into(),
            description: "Mark a reminder as completed by its ID.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "reminder_id": {
                        "type": "string",
                        "description": "The ID (or first 8 characters) of the reminder to complete"
                    }
                },
                "required": ["reminder_id"]
            }),
        }
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let reminder_id = params["reminder_id"]
            .as_str()
            .context("Missing 'reminder_id' parameter")?;

        let mut store = load_reminders(&self.data_dir)?;
        let mut found = false;

        for r in &mut store.reminders {
            if r.id.starts_with(reminder_id) {
                r.completed = true;
                found = true;
                break;
            }
        }

        if !found {
            return Ok(ToolResult::err(format!(
                "No reminder found with ID starting with '{}'.",
                reminder_id
            )));
        }

        save_reminders(&self.data_dir, &store)?;
        Ok(ToolResult::ok(format!(
            "Reminder marked as completed (ID: {}).",
            reminder_id
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
    async fn test_set_and_list_reminders() {
        let (_dir, data_dir) = temp_data_dir();

        let set = SetReminder::new(data_dir.clone());
        let result = set
            .execute(json!({
                "title": "Team standup",
                "remind_at": "2026-03-01T10:00:00Z",
                "description": "Daily team standup"
            }))
            .await
            .unwrap();
        assert!(result.success);

        let list = ListReminders::new(data_dir.clone());
        let result = list.execute(json!({})).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("Team standup"));
    }

    #[tokio::test]
    async fn test_complete_reminder() {
        let (_dir, data_dir) = temp_data_dir();

        let set = SetReminder::new(data_dir.clone());
        let result = set
            .execute(json!({
                "title": "Test reminder",
                "remind_at": "2026-03-01T10:00:00Z"
            }))
            .await
            .unwrap();

        let id_line = result.output.lines().find(|l| l.starts_with("ID:")).unwrap();
        let id = id_line.trim_start_matches("ID: ").trim();

        let complete = CompleteReminder::new(data_dir.clone());
        let result = complete
            .execute(json!({"reminder_id": id}))
            .await
            .unwrap();
        assert!(result.success);
    }
}
