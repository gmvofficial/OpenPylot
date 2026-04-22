use pylot::jobs::reminders::{ReminderState, UpcomingMeeting};

/// Verify meetings within the 15-min window are flagged.
#[test]
fn test_reminder_within_window() {
    let mut state = ReminderState::default();
    let now = chrono::Utc::now();
    let in_10_min = now + chrono::Duration::minutes(10);

    let meetings = vec![UpcomingMeeting {
        event_id: "evt1".into(),
        title: "Standup".into(),
        start_time: in_10_min,
        location: Some("Room A".into()),
        meet_link: None,
        attendee_count: 3,
    }];

    let upcoming = state.check_upcoming(&meetings, 15);
    assert_eq!(upcoming.len(), 1);
    assert_eq!(upcoming[0].title, "Standup");
}

/// Verify already-sent reminders are not duplicated.
#[test]
fn test_reminder_no_duplicate() {
    let mut state = ReminderState::default();
    let now = chrono::Utc::now();
    let in_10_min = now + chrono::Duration::minutes(10);

    let meetings = vec![UpcomingMeeting {
        event_id: "evt1".into(),
        title: "Standup".into(),
        start_time: in_10_min,
        location: None,
        meet_link: None,
        attendee_count: 2,
    }];

    let first = state.check_upcoming(&meetings, 15);
    assert_eq!(first.len(), 1);

    // Mark sent
    for m in &first {
        state.mark_sent(&m.event_id);
    }

    // Second check should be empty
    let second = state.check_upcoming(&meetings, 15);
    assert!(
        second.is_empty(),
        "Already reminded meeting should not appear again"
    );
}

/// Verify reminder formatting.
#[test]
fn test_reminder_format() {
    let now = chrono::Utc::now();
    let meeting = UpcomingMeeting {
        event_id: "evt1".into(),
        title: "Design Review".into(),
        start_time: now + chrono::Duration::minutes(5),
        location: Some("Conference Room B".into()),
        meet_link: Some("https://meet.google.com/abc-def-ghi".into()),
        attendee_count: 5,
    };

    let msg = pylot::jobs::reminders::format_meeting_reminder(&meeting);
    assert!(msg.contains("Design Review"));
    assert!(msg.contains("Conference Room B"));
    assert!(msg.contains("meet.google.com"));
}

/// Verify meetings outside the window are excluded.
#[test]
fn test_reminder_outside_window() {
    let mut state = ReminderState::default();
    let now = chrono::Utc::now();
    let in_30_min = now + chrono::Duration::minutes(30);

    let meetings = vec![UpcomingMeeting {
        event_id: "evt2".into(),
        title: "Later Meeting".into(),
        start_time: in_30_min,
        location: None,
        meet_link: None,
        attendee_count: 1,
    }];

    let upcoming = state.check_upcoming(&meetings, 15);
    assert!(
        upcoming.is_empty(),
        "Meeting 30 min away should not trigger 15-min reminder"
    );
}
