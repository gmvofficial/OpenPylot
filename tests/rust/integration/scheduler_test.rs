use openpylot::scheduler::normalize_cron;

/// Verify 5-field cron expressions are normalized to 7-field.
#[test]
fn test_normalize_cron_5_field() {
    let result = normalize_cron("*/15 * * * *");
    assert_eq!(result, "0 */15 * * * * *");
}

/// Verify 6-field cron expressions are normalized to 7-field.
#[test]
fn test_normalize_cron_6_field() {
    let result = normalize_cron("0 30 9 * * MON-FRI");
    assert_eq!(result, "0 0 30 9 * * MON-FRI");
}

/// Verify 7-field cron expressions pass through unchanged.
#[test]
fn test_normalize_cron_7_field() {
    let result = normalize_cron("0 0 12 * * MON-FRI 2024");
    assert_eq!(result, "0 0 12 * * MON-FRI 2024");
}

/// Verify SchedulerState can be serialized/deserialized.
#[test]
fn test_scheduler_state_serde() {
    use openpylot::scheduler::{JobState, SchedulerState};

    let state = SchedulerState {
        jobs: vec![
            JobState {
                name: "rsvp_monitor".into(),
                last_run: None,
                last_result: None,
                run_count: 0,
                error_count: 0,
            },
            JobState {
                name: "reminders".into(),
                last_run: Some(chrono::Utc::now()),
                last_result: Some("ok".into()),
                run_count: 5,
                error_count: 1,
            },
        ],
    };

    let json = serde_json::to_string_pretty(&state).unwrap();
    let loaded: SchedulerState = serde_json::from_str(&json).unwrap();
    assert_eq!(loaded.jobs.len(), 2);
    assert_eq!(loaded.jobs[0].name, "rsvp_monitor");
    assert_eq!(loaded.jobs[1].run_count, 5);
}
