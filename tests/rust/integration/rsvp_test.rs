use pylot::jobs::rsvp_monitor::{
    CalendarAttendee, CalendarEvent, EventDateTime, RsvpChange, RsvpState,
};

/// Verify first check detects new attendees as changes from default "needsAction".
#[test]
fn test_rsvp_first_check() {
    let mut state = RsvpState::default();
    let events = vec![CalendarEvent {
        id: "evt1".into(),
        summary: Some("Team Standup".into()),
        start: Some(EventDateTime {
            date_time: Some("2025-01-15T09:00:00Z".into()),
            date: None,
        }),
        end: None,
        attendees: Some(vec![
            CalendarAttendee {
                email: "alice@test.com".into(),
                response_status: "accepted".into(),
                display_name: Some("Alice".into()),
            },
            CalendarAttendee {
                email: "bob@test.com".into(),
                response_status: "needsAction".into(),
                display_name: Some("Bob".into()),
            },
        ]),
        organizer: None,
    }];

    let changes = state.check_changes(&events);
    // First check: alice's "accepted" differs from default "needsAction" = 1 change
    // bob's "needsAction" same as default = no change
    assert_eq!(changes.len(), 1, "Should detect Alice's non-default status");
    assert_eq!(changes[0].attendee_email, "alice@test.com");
}

/// Verify RSVP status change detection.
#[test]
fn test_rsvp_detect_change() {
    let mut state = RsvpState::default();

    // First check: baseline
    let events = vec![CalendarEvent {
        id: "evt1".into(),
        summary: Some("Review Meeting".into()),
        start: Some(EventDateTime {
            date_time: Some("2025-01-15T14:00:00Z".into()),
            date: None,
        }),
        end: None,
        attendees: Some(vec![CalendarAttendee {
            email: "charlie@test.com".into(),
            response_status: "needsAction".into(),
            display_name: Some("Charlie".into()),
        }]),
        organizer: None,
    }];
    state.check_changes(&events);

    // Second check: Charlie accepted
    let events2 = vec![CalendarEvent {
        id: "evt1".into(),
        summary: Some("Review Meeting".into()),
        start: Some(EventDateTime {
            date_time: Some("2025-01-15T14:00:00Z".into()),
            date: None,
        }),
        end: None,
        attendees: Some(vec![CalendarAttendee {
            email: "charlie@test.com".into(),
            response_status: "accepted".into(),
            display_name: Some("Charlie".into()),
        }]),
        organizer: None,
    }];
    let changes = state.check_changes(&events2);

    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].attendee_email, "charlie@test.com");
    assert_eq!(changes[0].old_status, "needsAction");
    assert_eq!(changes[0].new_status, "accepted");
}

/// Verify no duplicate changes on re-check.
#[test]
fn test_rsvp_no_duplicate() {
    let mut state = RsvpState::default();

    let events = vec![CalendarEvent {
        id: "evt1".into(),
        summary: Some("Demo".into()),
        start: Some(EventDateTime {
            date_time: Some("2025-01-15T16:00:00Z".into()),
            date: None,
        }),
        end: None,
        attendees: Some(vec![CalendarAttendee {
            email: "dave@test.com".into(),
            response_status: "accepted".into(),
            display_name: Some("Dave".into()),
        }]),
        organizer: None,
    }];

    // First check
    state.check_changes(&events);
    // Second check with same data
    let changes = state.check_changes(&events);
    assert!(changes.is_empty(), "Same data should produce no changes");
}

/// Verify notification formatting.
#[test]
fn test_rsvp_format_notification() {
    let change = RsvpChange {
        event_title: "Sprint Planning".into(),
        event_start: "2025-01-15T10:00:00Z".into(),
        attendee_email: "eve@test.com".into(),
        attendee_name: "Eve".into(),
        old_status: "needsAction".into(),
        new_status: "declined".into(),
    };

    let msg = pylot::jobs::rsvp_monitor::format_rsvp_notification(&[change]);
    assert!(msg.contains("Eve"));
    assert!(msg.contains("Sprint Planning"));
    assert!(msg.contains("Declined"), "Should contain 'Declined', got: {}", msg);
}
