use std::str::FromStr;

use super::*;

#[test]
fn async_task_identifiers_round_trip_without_debug_disclosure() {
    let task_id = AsyncTaskId::new([0x12; 16]).unwrap();
    let request_id = AsyncTaskRequestId::new([0x34; 16]).unwrap();

    for (key, rendered) in [
        (task_id.persistence_key(), format!("{task_id:?}")),
        (request_id.persistence_key(), format!("{request_id:?}")),
    ] {
        assert_eq!(key.len(), 32);
        assert!(
            key.bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        );
        assert!(rendered.contains("<脱敏>"));
        assert!(!rendered.contains(&key));
    }
    assert_eq!(
        AsyncTaskId::from_persistence_key(&task_id.persistence_key()).unwrap(),
        task_id
    );
    assert_eq!(
        AsyncTaskRequestId::from_persistence_key(&request_id.persistence_key()).unwrap(),
        request_id
    );
}

#[test]
fn async_task_identifiers_reject_zero_and_noncanonical_keys() {
    assert_eq!(
        AsyncTaskId::new([0; 16]),
        Err(AsyncTaskIdentifierError::AllZero)
    );
    for invalid in [
        "0".repeat(32),
        "A".repeat(32),
        "g".repeat(32),
        "a".repeat(31),
        "a".repeat(33),
    ] {
        assert!(AsyncTaskRequestId::from_persistence_key(&invalid).is_err());
    }
}

#[test]
fn async_task_attempt_and_fingerprint_keys_round_trip_without_debug_disclosure() {
    let attempt = AsyncTaskAttemptId::new([0x34; 16]).unwrap();
    let request_fingerprint = AsyncTaskRequestFingerprint::new([0x56; 32]);
    let binding_fingerprint = AsyncTaskBindingFingerprint::new([0x78; 32]);

    assert_eq!(
        AsyncTaskAttemptId::from_persistence_key(&attempt.persistence_key()).unwrap(),
        attempt
    );
    assert_eq!(
        AsyncTaskRequestFingerprint::from_persistence_key(&request_fingerprint.persistence_key())
            .unwrap(),
        request_fingerprint
    );
    assert_eq!(
        AsyncTaskBindingFingerprint::from_persistence_key(&binding_fingerprint.persistence_key())
            .unwrap(),
        binding_fingerprint
    );
    assert!(!format!("{attempt:?}").contains(&attempt.persistence_key()));
    assert!(!format!("{request_fingerprint:?}").contains(&"56".repeat(32)));
    assert!(!format!("{binding_fingerprint:?}").contains(&"78".repeat(32)));
}

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
