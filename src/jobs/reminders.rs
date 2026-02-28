use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Local, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Meeting reminder job — checks for upcoming meetings and sends
/// notifications N minutes before they start.

/// State tracking which reminders have already been sent.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReminderState {
    /// Event IDs for which reminders have been sent (cleared after event passes).
    pub sent_reminders: HashSet<String>,
    pub last_checked: Option<DateTime<Utc>>,
}

/// An upcoming meeting extracted from calendar events.
#[derive(Debug, Clone)]
pub struct UpcomingMeeting {
    pub event_id: String,
    pub title: String,
    pub start_time: DateTime<Utc>,
    pub location: Option<String>,
    pub meet_link: Option<String>,
    pub attendee_count: usize,
}

impl ReminderState {
    fn file_path(data_dir: &Path) -> PathBuf {
        data_dir.join("meeting_reminder_state.json")
    }

    pub fn load(data_dir: &Path) -> Result<Self> {
        let path = Self::file_path(data_dir);
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read reminder state from {}", path.display()))?;
        let state: ReminderState = serde_json::from_str(&content)
            .with_context(|| "Failed to parse reminder state file")?;
        Ok(state)
    }

    pub fn save(&self, data_dir: &Path) -> Result<()> {
        let path = Self::file_path(data_dir);
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, content)
            .with_context(|| format!("Failed to write reminder state to {}", path.display()))?;
        Ok(())
    }

    /// Check for meetings starting within `minutes_before` and return
    /// any that haven't had reminders sent yet.
    pub fn check_upcoming<'a>(
        &mut self,
        meetings: &'a [UpcomingMeeting],
        minutes_before: i64,
    ) -> Vec<&'a UpcomingMeeting> {
        let now = Utc::now();
        let window = Duration::minutes(minutes_before);

        // Clean up old entries (events that have already passed)
        self.sent_reminders.retain(|id| {
            meetings
                .iter()
                .any(|m| &m.event_id == id && m.start_time > now)
        });

        self.last_checked = Some(now);

        meetings
            .iter()
            .filter(|m| {
                let time_until = m.start_time - now;
                time_until > Duration::zero()
                    && time_until <= window
                    && !self.sent_reminders.contains(&m.event_id)
            })
            .collect()
    }

    /// Mark a meeting as having been reminded.
    pub fn mark_sent(&mut self, event_id: &str) {
        self.sent_reminders.insert(event_id.to_string());
    }
}

/// Format a meeting reminder notification.
pub fn format_meeting_reminder(meeting: &UpcomingMeeting) -> String {
    let local_start: DateTime<Local> = meeting.start_time.into();
    let minutes_until = (meeting.start_time - Utc::now()).num_minutes();

    let mut msg = format!(
        "⏰ Meeting in {} minutes!\n\n📅 {}\n🕐 {}",
        minutes_until,
        meeting.title,
        local_start.format("%I:%M %p"),
    );

    if let Some(ref link) = meeting.meet_link {
        msg.push_str(&format!("\n🔗 {}", link));
    }

    if let Some(ref location) = meeting.location {
        msg.push_str(&format!("\n📍 {}", location));
    }

    if meeting.attendee_count > 0 {
        msg.push_str(&format!("\n👥 {} attendees", meeting.attendee_count));
    }

    msg
}

/// Check user reminders (from reminders.json) that are due and
/// haven't been alerted yet. Returns formatted notification strings.
pub fn check_due_reminders(data_dir: &Path) -> Result<Vec<String>> {
    let reminders_path = data_dir.join("reminders.json");
    if !reminders_path.exists() {
        return Ok(Vec::new());
    }

    let content = std::fs::read_to_string(&reminders_path)?;

    #[derive(Deserialize)]
    struct Reminder {
        id: String,
        title: String,
        due_at: String,
        #[serde(default)]
        description: Option<String>,
        #[serde(default)]
        completed: bool,
        #[serde(default)]
        notified: bool,
    }

    #[derive(Serialize, Deserialize)]
    struct RemindersFile {
        reminders: Vec<serde_json::Value>,
    }

    let file: RemindersFile = serde_json::from_str(&content)?;
    let now = Utc::now();
    let mut notifications = Vec::new();
    let mut updated = false;

    let mut reminders_raw = file.reminders.clone();

    for reminder_val in &mut reminders_raw {
        let obj = match reminder_val.as_object_mut() {
            Some(o) => o,
            None => continue,
        };

        let completed = obj
            .get("completed")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let notified = obj
            .get("notified")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if completed || notified {
            continue;
        }

        let due_str = match obj.get("due_at").and_then(|v| v.as_str()) {
            Some(s) => s,
            None => continue,
        };

        if let Ok(due) = due_str.parse::<DateTime<Utc>>() {
            if due <= now {
                let title = obj
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Reminder");
                let desc = obj
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                let mut msg = format!("🔔 Reminder: {}", title);
                if !desc.is_empty() {
                    msg.push_str(&format!("\n   {}", desc));
                }
                notifications.push(msg);

                // Mark as notified
                obj.insert("notified".to_string(), serde_json::Value::Bool(true));
                updated = true;
            }
        }
    }

    if updated {
        let updated_file = serde_json::json!({ "reminders": reminders_raw });
        let content = serde_json::to_string_pretty(&updated_file)?;
        std::fs::write(&reminders_path, content)?;
    }

    Ok(notifications)
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_meeting(id: &str, title: &str, minutes_from_now: i64) -> UpcomingMeeting {
        UpcomingMeeting {
            event_id: id.to_string(),
            title: title.to_string(),
            start_time: Utc::now() + Duration::minutes(minutes_from_now),
            location: None,
            meet_link: Some("https://meet.google.com/abc-defg-hij".to_string()),
            attendee_count: 3,
        }
    }

    #[test]
    fn test_check_upcoming_within_window() {
        let mut state = ReminderState::default();
        let meetings = vec![
            make_meeting("m1", "Standup", 10),     // 10 min from now
            make_meeting("m2", "Planning", 60),     // 60 min from now
            make_meeting("m3", "Retro", 5),         // 5 min from now
        ];

        let upcoming = state.check_upcoming(&meetings, 15);
        assert_eq!(upcoming.len(), 2); // m1 (10 min) and m3 (5 min)
        assert!(upcoming.iter().any(|m| m.event_id == "m1"));
        assert!(upcoming.iter().any(|m| m.event_id == "m3"));
    }

    #[test]
    fn test_check_upcoming_no_duplicates() {
        let mut state = ReminderState::default();
        let meetings = vec![make_meeting("m1", "Standup", 10)];

        let upcoming = state.check_upcoming(&meetings, 15);
        assert_eq!(upcoming.len(), 1);

        state.mark_sent("m1");

        let upcoming = state.check_upcoming(&meetings, 15);
        assert_eq!(upcoming.len(), 0); // Already sent
    }

    #[test]
    fn test_format_meeting_reminder() {
        let meeting = make_meeting("m1", "Team Standup", 10);
        let msg = format_meeting_reminder(&meeting);

        assert!(msg.contains("Team Standup"));
        assert!(msg.contains("meet.google.com"));
        assert!(msg.contains("3 attendees"));
    }

    #[test]
    fn test_reminder_state_persistence() {
        let tmp = tempfile::TempDir::new().unwrap();

        let mut state = ReminderState::default();
        state.mark_sent("event-123");
        state.save(tmp.path()).unwrap();

        let loaded = ReminderState::load(tmp.path()).unwrap();
        assert!(loaded.sent_reminders.contains("event-123"));
    }

    #[test]
    fn test_check_due_reminders_empty() {
        let tmp = tempfile::TempDir::new().unwrap();
        let result = check_due_reminders(tmp.path()).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_check_due_reminders_with_due_item() {
        let tmp = tempfile::TempDir::new().unwrap();
        let past = (Utc::now() - Duration::minutes(5)).to_rfc3339();

        let reminders = serde_json::json!({
            "reminders": [
                {
                    "id": "r1",
                    "title": "Buy groceries",
                    "due_at": past,
                    "completed": false,
                    "notified": false
                }
            ]
        });

        std::fs::write(
            tmp.path().join("reminders.json"),
            serde_json::to_string_pretty(&reminders).unwrap(),
        )
        .unwrap();

        let result = check_due_reminders(tmp.path()).unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].contains("Buy groceries"));
    }
}
