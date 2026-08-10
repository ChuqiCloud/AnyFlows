use std::str::FromStr;

use super::*;

#[test]
fn task_id_is_bounded_and_redacted() {
    let id = UpstreamTaskId::new("provider-task-123").unwrap();
    assert_eq!(id.as_str(), "provider-task-123");
    assert!(format!("{id:?}").contains("<脱敏>"));
    assert!(!format!("{id:?}").contains("provider-task-123"));

    for invalid in ["", "task id", "task\nsecret", " task"] {
        assert!(matches!(
            UpstreamTaskId::new(invalid),
            Err(TaskIdentifierError::Empty | TaskIdentifierError::InvalidCharacter)
        ));
    }
    assert_eq!(
        UpstreamTaskId::new("x".repeat(MAX_UPSTREAM_TASK_ID_BYTES + 1)),
        Err(TaskIdentifierError::TooLong)
    );
}

#[test]
fn progress_rejects_values_above_one_hundred_percent() {
    assert_eq!(TaskProgress::new(0).unwrap(), TaskProgress::ZERO);
    assert_eq!(TaskProgress::new(10_000).unwrap(), TaskProgress::COMPLETE);
    assert_eq!(
        TaskProgress::new(10_001),
        Err(TaskProgressError::OutOfRange)
    );
}

#[test]
fn task_state_is_closed_and_unknown_values_fail() {
    assert_eq!(
        TaskState::from_str("submitted").unwrap(),
        TaskState::Submitted
    );
    assert_eq!(TaskState::Succeeded.as_str(), "succeeded");
    assert!(TaskState::from_str("completed").is_err());
    assert!(TaskState::Succeeded.is_terminal());
    assert!(!TaskState::Running.is_terminal());
}

#[test]
fn status_helpers_preserve_terminal_invariants() {
    let running = TaskStatus::Running {
        progress: TaskProgress::new(2_500).unwrap(),
    };
    assert_eq!(running.state(), TaskState::Running);
    assert_eq!(running.progress().basis_points(), 2_500);
    assert!(!running.is_terminal());

    let success = TaskStatus::Succeeded;
    assert_eq!(success.progress(), TaskProgress::COMPLETE);
    assert!(success.is_terminal());
    assert!(success.failure().is_none());

    let failure =
        TaskFailure::with_reason(TaskFailureKind::Upstream, "provider rejected task").unwrap();
    let failed = TaskStatus::Failed { failure };
    assert_eq!(failed.state(), TaskState::Failed);
    assert_eq!(failed.progress(), TaskProgress::ZERO);
    assert_eq!(failed.failure().unwrap().kind(), TaskFailureKind::Upstream);
    let debug = format!("{failed:?}");
    assert!(!debug.contains("provider rejected task"));
}

#[test]
fn failure_reason_rejects_control_characters_and_is_bounded() {
    assert_eq!(
        TaskFailure::with_reason(TaskFailureKind::Rejected, ""),
        Err(TaskFailureError::Empty)
    );
    assert_eq!(
        TaskFailure::with_reason(TaskFailureKind::Rejected, "bad\nreason"),
        Err(TaskFailureError::InvalidCharacter)
    );
    assert_eq!(
        TaskFailure::with_reason(
            TaskFailureKind::Rejected,
            "x".repeat(MAX_TASK_FAILURE_REASON_BYTES + 1)
        ),
        Err(TaskFailureError::TooLong)
    );
}

#[test]
fn submission_debug_hides_task_identifier_and_failure_reason() {
    let task_id = UpstreamTaskId::new("provider-task-secret").unwrap();
    let failure =
        TaskFailure::with_reason(TaskFailureKind::Rejected, "secret failure reason").unwrap();
    let submission = TaskSubmission::new(task_id, TaskStatus::Failed { failure });
    let debug = format!("{submission:?}");
    assert!(!debug.contains("provider-task-secret"));
    assert!(!debug.contains("secret failure reason"));
}
