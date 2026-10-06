use af_domain::{TaskFailureKind, TaskState};
use serde_json::json;

use super::*;

#[test]
fn parses_and_rebuilds_full_text_to_video_request() {
    let body = br#"{"model":"grok-imagine-video-1.5","prompt":"private prompt","duration":10,"aspect_ratio":"16:9","resolution":"1080p"}"#;
    let request = parse_request(body).unwrap();
    assert_eq!(request.duration().unwrap().seconds(), 10);
    assert_eq!(request.aspect_ratio(), Some(VideoAspectRatio::Landscape));
    assert_eq!(request.resolution(), Some(VideoResolution::P1080));
    assert_eq!(
        build_request(&request).unwrap(),
        json!({
            "model": "grok-imagine-video-1.5",
            "prompt": "private prompt",
            "duration": 10,
            "aspect_ratio": "16:9",
            "resolution": "1080p"
        })
    );
    let debug = format!("{request:?}");
    assert!(!debug.contains("private prompt"));
    assert!(!debug.contains("grok-imagine-video"));
}

#[test]
fn request_rejects_unknown_duplicate_null_and_unsupported_values() {
    for body in [
        br#"{"model":"grok-imagine-video-1.5","prompt":"ok","unknown":1}"#.as_slice(),
        br#"{"model":"grok-imagine-video-1.5","model":"duplicate","prompt":"ok"}"#,
        br#"{"model":"grok-imagine-video-1.5","prompt":"ok","duration":null}"#,
        br#"{"model":"grok-imagine-video-1.5","prompt":"ok","duration":16}"#,
        br#"{"model":"grok-imagine-video","prompt":"ok","resolution":"1080p"}"#,
        br#"{"model":"grok-imagine-video-1.5","prompt":"ok","aspect_ratio":"10:7"}"#,
    ] {
        assert!(parse_request(body).is_err());
    }
}

#[test]
fn submission_and_pending_poll_preserve_closed_states() {
    let submission = parse_submission(br#"{"request_id":"video-task-123"}"#).unwrap();
    assert_eq!(submission.task_id().as_str(), "video-task-123");
    assert_eq!(submission.status().state(), TaskState::Submitted);

    let pending = parse_poll(br#"{"status":"pending"}"#).unwrap();
    assert_eq!(pending.status().state(), TaskState::Running);
    assert!(pending.output().is_none());
}

#[test]
fn done_poll_returns_redacted_video_output() {
    let poll = parse_poll(
        br#"{"status":"done","video":{"url":"https://vidgen.x.ai/out.mp4?token=private","duration":8,"respect_moderation":true},"model":"grok-imagine-video-1.5"}"#,
    )
    .unwrap();
    assert_eq!(poll.status().state(), TaskState::Succeeded);
    let video = poll.output().unwrap().as_video().unwrap();
    assert_eq!(video.duration().seconds(), 8);
    assert_eq!(video.model().as_str(), "grok-imagine-video-1.5");
    assert!(video.url().as_str().starts_with("https://vidgen.x.ai/"));
    let debug = format!("{poll:?}");
    for secret in ["vidgen.x.ai", "token=private", "grok-imagine-video"] {
        assert!(!debug.contains(secret));
    }
}

#[test]
fn moderation_expiry_and_failures_map_without_retaining_messages() {
    let moderated = parse_poll(
        br#"{"status":"done","video":{"duration":5,"respect_moderation":false},"model":"grok-imagine-video-1.5"}"#,
    )
    .unwrap();
    assert_eq!(
        moderated.status().failure().unwrap().kind(),
        TaskFailureKind::Rejected
    );

    let expired = parse_poll(br#"{"status":"expired"}"#).unwrap();
    assert_eq!(
        expired.status().failure().unwrap().kind(),
        TaskFailureKind::TimedOut
    );

    let failed = parse_poll(
        br#"{"status":"failed","error":{"code":"service_unavailable","message":"private upstream detail"}}"#,
    )
    .unwrap();
    assert_eq!(
        failed.status().failure().unwrap().kind(),
        TaskFailureKind::Upstream
    );
    assert!(!format!("{failed:?}").contains("private upstream detail"));
}

#[test]
fn poll_rejects_cross_state_fields_and_unknown_error_codes() {
    for body in [
        br#"{"status":"pending","model":"private"}"#.as_slice(),
        br#"{"status":"done","model":"grok-imagine-video-1.5"}"#,
        br#"{"status":"done","video":{"url":"http://unsafe.example/out.mp4","duration":8,"respect_moderation":true},"model":"grok-imagine-video-1.5"}"#,
        br#"{"status":"failed","error":{"code":"unknown","message":"no"}}"#,
        br#"{"status":"expired","error":{"code":"internal_error","message":"no"}}"#,
    ] {
        assert!(parse_poll(body).is_err());
    }
}
