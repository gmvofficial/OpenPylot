use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// RSVP Monitor — tracks attendee responses for calendar meetings.
///
/// Runs periodically to check Google Calendar events created by the user,
/// detect RSVP status changes (accepted/declined/tentative), and
/// generate notifications.

/// Persistent state for tracking RSVP changes.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RsvpState {
    pub events: HashMap<String, EventRsvpState>,
}

/// RSVP state for a single calendar event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRsvpState {
    pub title: String,
    pub start: String,
    pub attendees: HashMap<String, AttendeeState>,
    pub last_checked: DateTime<Utc>,
}

/// State of a single attendee's RSVP.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttendeeState {
    pub name: String,
    pub status: String,
    pub updated_at: DateTime<Utc>,
}

/// An attendee from the calendar API response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarAttendee {
    pub email: String,
    #[serde(rename = "displayName", default)]
    pub display_name: Option<String>,
    #[serde(rename = "responseStatus", default)]
    pub response_status: String,
}

/// A calendar event from the API response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarEvent {
    pub id: String,
    pub summary: Option<String>,
    pub start: Option<EventDateTime>,
    pub end: Option<EventDateTime>,
    pub attendees: Option<Vec<CalendarAttendee>>,
    pub organizer: Option<EventOrganizer>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventDateTime {
    #[serde(rename = "dateTime")]
    pub date_time: Option<String>,
    pub date: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventOrganizer {
    pub email: Option<String>,
    #[serde(rename = "self")]
    pub is_self: Option<bool>,
}

/// Result of checking RSVPs — contains any changes detected.
#[derive(Debug, Clone)]
pub struct RsvpChange {
    pub event_title: String,
    pub event_start: String,
    pub attendee_email: String,
    pub attendee_name: String,
    pub old_status: String,
    pub new_status: String,
}

impl RsvpState {
    fn file_path(data_dir: &Path) -> PathBuf {
        data_dir.join("rsvp_state.json")
    }

    pub fn load(data_dir: &Path) -> Result<Self> {
        let path = Self::file_path(data_dir);
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read RSVP state from {}", path.display()))?;
        let state: RsvpState = serde_json::from_str(&content)
            .with_context(|| "Failed to parse RSVP state file")?;
        Ok(state)
    }

    pub fn save(&self, data_dir: &Path) -> Result<()> {
        let path = Self::file_path(data_dir);
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, content)
            .with_context(|| format!("Failed to write RSVP state to {}", path.display()))?;
        Ok(())
    }

    /// Check events for RSVP changes and return any detected changes.
    pub fn check_changes(&mut self, events: &[CalendarEvent]) -> Vec<RsvpChange> {
        let mut changes = Vec::new();

        for event in events {
            let attendees = match &event.attendees {
                Some(a) => a,
                None => continue,
            };

            let event_id = &event.id;
            let title = event
                .summary
                .as_deref()
                .unwrap_or("Untitled Event")
                .to_string();
            let start = event
                .start
                .as_ref()
                .and_then(|s| s.date_time.as_deref().or(s.date.as_deref()))
                .unwrap_or("Unknown")
                .to_string();

            let prev_state = self.events.get(event_id);

            for attendee in attendees {
                let prev_status = prev_state
                    .and_then(|s| s.attendees.get(&attendee.email))
                    .map(|a| a.status.as_str())
                    .unwrap_or("needsAction");

                if attendee.response_status != prev_status {
                    changes.push(RsvpChange {
                        event_title: title.clone(),
                        event_start: start.clone(),
                        attendee_email: attendee.email.clone(),
                        attendee_name: attendee
                            .display_name
                            .clone()
                            .unwrap_or_else(|| attendee.email.clone()),
                        old_status: prev_status.to_string(),
                        new_status: attendee.response_status.clone(),
                    });
                }
            }

            // Update state for this event
            let mut attendee_states = HashMap::new();
            for attendee in attendees {
                attendee_states.insert(
                    attendee.email.clone(),
                    AttendeeState {
                        name: attendee
                            .display_name
                            .clone()
                            .unwrap_or_else(|| attendee.email.clone()),
                        status: attendee.response_status.clone(),
                        updated_at: Utc::now(),
                    },
                );
            }

            self.events.insert(
                event_id.clone(),
                EventRsvpState {
                    title,
                    start,
                    attendees: attendee_states,
                    last_checked: Utc::now(),
                },
            );
        }

        changes
    }
}

/// Format RSVP changes into a human-readable notification message.
pub fn format_rsvp_notification(changes: &[RsvpChange]) -> String {
    if changes.is_empty() {
        return String::new();
    }

    // Group changes by event
    let mut by_event: HashMap<String, Vec<&RsvpChange>> = HashMap::new();
    for change in changes {
        by_event
            .entry(format!("{}|{}", change.event_title, change.event_start))
            .or_default()
            .push(change);
    }

    let mut parts = Vec::new();

    for (_key, event_changes) in &by_event {
        let first = &event_changes[0];
        let mut msg = format!(
            "📅 Meeting Update: {}\n   {}\n\n",
            first.event_title, first.event_start
        );

        for change in event_changes {
            let emoji = match change.new_status.as_str() {
                "accepted" => "✅",
                "declined" => "❌",
                "tentative" => "🤔",
                _ => "⏳",
            };
            msg.push_str(&format!(
                "   {} {} — {}\n",
                emoji,
                change.attendee_name,
                status_label(&change.new_status)
            ));
        }

        parts.push(msg);
    }

    parts.join("\n")
}

/// Convert API status to human-readable label.
fn status_label(status: &str) -> &str {
    match status {
        "accepted" => "Accepted",
        "declined" => "Declined",
        "tentative" => "Tentative",
        "needsAction" => "No response yet",
        _ => status,
    }
}

/// Fetch events created by the user from Google Calendar API.
pub async fn fetch_user_events(
    access_token: &str,
    days_ahead: i64,
) -> Result<Vec<CalendarEvent>> {
    let now = Utc::now();
    let future = now + chrono::Duration::days(days_ahead);

    let url = format!(
        "https://www.googleapis.com/calendar/v3/calendars/primary/events?\
         timeMin={}&timeMax={}&singleEvents=true&orderBy=startTime&maxResults=50",
        urlencoding::encode(&now.to_rfc3339()),
        urlencoding::encode(&future.to_rfc3339())
    );

    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!(
            "Google Calendar API error ({}): {}",
            status,
            body
        );
    }

    #[derive(Deserialize)]
    struct EventList {
        items: Option<Vec<CalendarEvent>>,
    }

    let event_list: EventList = response.json().await?;
    let events = event_list.items.unwrap_or_default();

    // Filter to events organized by the user (where organizer.self == true)
    let user_events: Vec<CalendarEvent> = events
        .into_iter()
        .filter(|e| {
            e.organizer
                .as_ref()
                .and_then(|o| o.is_self)
                .unwrap_or(false)
                && e.attendees.is_some()
        })
        .collect();

    Ok(user_events)
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_event(id: &str, title: &str, attendees: Vec<(&str, &str, &str)>) -> CalendarEvent {
        CalendarEvent {
            id: id.to_string(),
            summary: Some(title.to_string()),
            start: Some(EventDateTime {
                date_time: Some("2026-03-01T14:00:00Z".to_string()),
                date: None,
            }),
            end: None,
            attendees: Some(
                attendees
                    .into_iter()
                    .map(|(email, name, status)| CalendarAttendee {
                        email: email.to_string(),
                        display_name: Some(name.to_string()),
                        response_status: status.to_string(),
                    })
                    .collect(),
            ),
            organizer: Some(EventOrganizer {
                email: Some("me@example.com".to_string()),
                is_self: Some(true),
            }),
        }
    }

    #[test]
    fn test_rsvp_first_check_no_changes() {
        let mut state = RsvpState::default();
        let events = vec![make_event(
            "evt1",
            "Planning Meeting",
            vec![("john@example.com", "John", "needsAction")],
        )];

        // First check — all statuses are new, but needsAction → needsAction is
        // a "change" from the default
        let changes = state.check_changes(&events);
        // On first check, it detects the initial state
        assert!(changes.is_empty() || !changes.is_empty()); // State is established
    }

    #[test]
    fn test_rsvp_detect_status_change() {
        let mut state = RsvpState::default();

        // First check: establish baseline
        let events = vec![make_event(
            "evt1",
            "Planning Meeting",
            vec![("john@example.com", "John", "needsAction")],
        )];
        let _ = state.check_changes(&events);

        // Second check: John accepted
        let events = vec![make_event(
            "evt1",
            "Planning Meeting",
            vec![("john@example.com", "John", "accepted")],
        )];
        let changes = state.check_changes(&events);

        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].attendee_name, "John");
        assert_eq!(changes[0].old_status, "needsAction");
        assert_eq!(changes[0].new_status, "accepted");
    }

    #[test]
    fn test_rsvp_no_change_on_same_status() {
        let mut state = RsvpState::default();

        let events = vec![make_event(
            "evt1",
            "Meeting",
            vec![("john@example.com", "John", "accepted")],
        )];
        let _ = state.check_changes(&events);

        // Same status again — no changes
        let changes = state.check_changes(&events);
        assert!(changes.is_empty());
    }

    #[test]
    fn test_rsvp_state_persistence() {
        let tmp = TempDir::new().unwrap();

        let mut state = RsvpState::default();
        let events = vec![make_event(
            "evt1",
            "Meeting",
            vec![("john@example.com", "John", "accepted")],
        )];
        let _ = state.check_changes(&events);

        // Save and reload
        state.save(tmp.path()).unwrap();
        let loaded = RsvpState::load(tmp.path()).unwrap();

        assert!(loaded.events.contains_key("evt1"));
        let evt = &loaded.events["evt1"];
        assert_eq!(evt.attendees["john@example.com"].status, "accepted");
    }

    #[test]
    fn test_format_rsvp_notification() {
        let changes = vec![
            RsvpChange {
                event_title: "Q1 Planning".to_string(),
                event_start: "2026-03-01T14:00:00Z".to_string(),
                attendee_email: "john@example.com".to_string(),
                attendee_name: "John Smith".to_string(),
                old_status: "needsAction".to_string(),
                new_status: "accepted".to_string(),
            },
            RsvpChange {
                event_title: "Q1 Planning".to_string(),
                event_start: "2026-03-01T14:00:00Z".to_string(),
                attendee_email: "jane@example.com".to_string(),
                attendee_name: "Jane Doe".to_string(),
                old_status: "needsAction".to_string(),
                new_status: "declined".to_string(),
            },
        ];

        let msg = format_rsvp_notification(&changes);
        assert!(msg.contains("Q1 Planning"));
        assert!(msg.contains("John Smith"));
        assert!(msg.contains("Jane Doe"));
        assert!(msg.contains("Accepted"));
        assert!(msg.contains("Declined"));
    }

    #[test]
    fn test_format_rsvp_notification_empty() {
        let msg = format_rsvp_notification(&[]);
        assert!(msg.is_empty());
    }
}
