use af_admin::TokenAuthentication;
use af_domain::{AfError, AsyncTaskId, AsyncTaskRequestId, TaskFailureKind, TaskStatus};
use af_protocol::xai_video::parse_request;
use af_telemetry::RequestId;
use axum::{
    Json,
    body::Bytes,
    extract::{Extension, Path, RawQuery, State},
    response::{IntoResponse, Response},
};
use http::{HeaderMap, StatusCode, header::CACHE_CONTROL};
use serde::Serialize;

use crate::{
    OpenAiHttpError, VideoTaskListCursor, VideoTaskListItem, VideoTaskPage, VideoTaskSnapshot,
    chat_completions::HttpState,
};

const IDEMPOTENCY_KEY_HEADER: &str = "idempotency-key";
const DEFAULT_HISTORY_LIMIT: usize = 20;
const MAX_HISTORY_LIMIT: usize = 100;
const MAX_HISTORY_QUERY_BYTES: usize = 1_024;

/// 解析 xAI 视频请求并使用客户端幂等键提交 owner-scoped 任务。
pub(crate) async fn submit_video_task(
    State(state): State<HttpState>,
    Extension(request_id): Extension<RequestId>,
    Extension(authentication): Extension<TokenAuthentication>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, OpenAiHttpError> {
    let request = parse_request(&body).map_err(|_| AfError::InvalidRequest)?;
    if !authentication
        .model_policy()
        .allows(request.model().as_str())
    {
        return Err(OpenAiHttpError::from(AfError::ModelNotAllowed));
    }
    let idempotency_key = parse_idempotency_key(&headers)?;
    let service = state
        .video_task_service()
        .ok_or_else(|| OpenAiHttpError::from(AfError::Internal))?;
    let snapshot = service
        .submit(
            authentication.principal(),
            idempotency_key,
            request,
            request_id.as_str(),
        )
        .await
        .map_err(OpenAiHttpError::from)?;
    let response = VideoSubmissionResponse {
        request_id: snapshot.task_id().persistence_key(),
    };
    Ok((
        StatusCode::OK,
        [(CACHE_CONTROL, "no-store")],
        Json(response),
    )
        .into_response())
}

/// 在认证用户范围内查询任务；成功终态只重取原任务的短期结果地址。
pub(crate) async fn poll_video_task(
    State(state): State<HttpState>,
    Extension(request_id): Extension<RequestId>,
    Extension(authentication): Extension<TokenAuthentication>,
    Path(task_id): Path<String>,
) -> Result<Response, OpenAiHttpError> {
    let task_id = AsyncTaskId::from_persistence_key(&task_id).map_err(|_| AfError::TaskNotFound)?;
    let service = state
        .video_task_service()
        .ok_or_else(|| OpenAiHttpError::from(AfError::Internal))?;
    let snapshot = service
        .poll(authentication.principal(), task_id, request_id.as_str())
        .await
        .map_err(OpenAiHttpError::from)?;
    let response = VideoPollResponse::from_snapshot(&snapshot)?;
    Ok((
        StatusCode::OK,
        [(CACHE_CONTROL, "no-store")],
        Json(response),
    )
        .into_response())
}

/// 返回当前认证用户的持久化视频任务历史；列表读取不会访问上游。
pub(crate) async fn list_video_tasks(
    State(state): State<HttpState>,
    Extension(authentication): Extension<TokenAuthentication>,
    RawQuery(raw_query): RawQuery,
) -> Result<Response, OpenAiHttpError> {
    let (before, limit) = parse_history_query(raw_query.as_deref())?;
    let service = state
        .video_task_service()
        .ok_or_else(|| OpenAiHttpError::from(AfError::Internal))?;
    let page = service
        .list(authentication.principal(), before, limit)
        .await
        .map_err(OpenAiHttpError::from)?;
    Ok((
        StatusCode::OK,
        [(CACHE_CONTROL, "no-store")],
        Json(VideoTaskListResponse::from_page(&page)),
    )
        .into_response())
}

fn parse_history_query(
    raw_query: Option<&str>,
) -> Result<(Option<VideoTaskListCursor>, usize), OpenAiHttpError> {
    let raw_query = raw_query.unwrap_or_default();
    if raw_query.len() > MAX_HISTORY_QUERY_BYTES {
        return Err(OpenAiHttpError::from(AfError::InvalidRequest));
    }
    validate_percent_encoding(raw_query)?;
    let mut before = None;
    let mut limit = None;
    for (key, value) in url::form_urlencoded::parse(raw_query.as_bytes()) {
        if key.contains('\u{fffd}') || value.contains('\u{fffd}') {
            return Err(OpenAiHttpError::from(AfError::InvalidRequest));
        }
        match key.as_ref() {
            "before" if before.is_none() => before = Some(decode_history_cursor(&value)?),
            "limit" if limit.is_none() => limit = Some(parse_history_limit(&value)?),
            _ => return Err(OpenAiHttpError::from(AfError::InvalidRequest)),
        }
    }
    Ok((before, limit.unwrap_or(DEFAULT_HISTORY_LIMIT)))
}

fn parse_history_limit(value: &str) -> Result<usize, OpenAiHttpError> {
    if value.is_empty() || value.bytes().any(|byte| !byte.is_ascii_digit()) {
        return Err(OpenAiHttpError::from(AfError::InvalidRequest));
    }
    let limit = value
        .parse::<usize>()
        .map_err(|_| OpenAiHttpError::from(AfError::InvalidRequest))?;
    if !(1..=MAX_HISTORY_LIMIT).contains(&limit) {
        return Err(OpenAiHttpError::from(AfError::InvalidRequest));
    }
    Ok(limit)
}

fn encode_history_cursor(cursor: VideoTaskListCursor) -> String {
    cursor.task_id().persistence_key()
}

fn decode_history_cursor(value: &str) -> Result<VideoTaskListCursor, OpenAiHttpError> {
    let task_id = AsyncTaskId::from_persistence_key(value)
        .map_err(|_| OpenAiHttpError::from(AfError::InvalidRequest))?;
    Ok(VideoTaskListCursor::new(task_id))
}

fn validate_percent_encoding(raw_query: &str) -> Result<(), OpenAiHttpError> {
    let bytes = raw_query.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len()
                || !bytes[index + 1].is_ascii_hexdigit()
                || !bytes[index + 2].is_ascii_hexdigit()
            {
                return Err(OpenAiHttpError::from(AfError::InvalidRequest));
            }
            index += 3;
        } else {
            index += 1;
        }
    }
    Ok(())
}

fn parse_idempotency_key(headers: &HeaderMap) -> Result<AsyncTaskRequestId, OpenAiHttpError> {
    let mut values = headers.get_all(IDEMPOTENCY_KEY_HEADER).iter();
    let value = values
        .next()
        .ok_or_else(|| OpenAiHttpError::from(AfError::InvalidRequest))?;
    if values.next().is_some() {
        return Err(OpenAiHttpError::from(AfError::InvalidRequest));
    }
    let value = value
        .to_str()
        .map_err(|_| OpenAiHttpError::from(AfError::InvalidRequest))?;
    AsyncTaskRequestId::from_persistence_key(value)
        .map_err(|_| OpenAiHttpError::from(AfError::InvalidRequest))
}

#[derive(Serialize)]
struct VideoSubmissionResponse {
    request_id: String,
}

#[derive(Serialize)]
struct VideoTaskListResponse {
    data: Vec<VideoTaskListItemResponse>,
    next_cursor: Option<String>,
}

impl VideoTaskListResponse {
    fn from_page(page: &VideoTaskPage) -> Self {
        Self {
            data: page
                .items()
                .iter()
                .map(VideoTaskListItemResponse::from_item)
                .collect(),
            next_cursor: page.next_cursor().map(encode_history_cursor),
        }
    }
}

#[derive(Serialize)]
struct VideoTaskListItemResponse {
    id: String,
    model: String,
    status: &'static str,
    progress_basis_points: u16,
    created_at: u64,
    updated_at: u64,
}

impl VideoTaskListItemResponse {
    fn from_item(item: &VideoTaskListItem) -> Self {
        Self {
            id: item.task_id().persistence_key(),
            model: item.model().to_owned(),
            status: public_task_status(item.status()),
            progress_basis_points: item.status().progress().basis_points(),
            created_at: item.created_at(),
            updated_at: item.updated_at(),
        }
    }
}

fn public_task_status(status: &TaskStatus) -> &'static str {
    match status {
        TaskStatus::Submitted { .. } | TaskStatus::Queued { .. } | TaskStatus::Running { .. } => {
            "pending"
        }
        TaskStatus::Succeeded => "done",
        TaskStatus::Failed { failure } if failure.kind() == TaskFailureKind::TimedOut => "expired",
        TaskStatus::Failed { .. } => "failed",
    }
}

#[derive(Serialize)]
struct VideoPollResponse {
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    video: Option<VideoOutputResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<VideoFailureResponse>,
}

impl VideoPollResponse {
    fn from_snapshot(snapshot: &VideoTaskSnapshot) -> Result<Self, OpenAiHttpError> {
        match snapshot.status() {
            TaskStatus::Submitted { .. }
            | TaskStatus::Queued { .. }
            | TaskStatus::Running { .. } => Ok(Self {
                status: "pending",
                video: None,
                model: None,
                error: None,
            }),
            TaskStatus::Succeeded => {
                let output = snapshot
                    .output()
                    .and_then(|output| output.as_video())
                    .ok_or_else(|| OpenAiHttpError::from(AfError::RequestOutcomeUnknown))?;
                Ok(Self {
                    status: "done",
                    video: Some(VideoOutputResponse {
                        url: output.url().as_str().to_owned(),
                        duration: output.duration().seconds(),
                        respect_moderation: true,
                    }),
                    model: Some(output.model().as_str().to_owned()),
                    error: None,
                })
            }
            TaskStatus::Failed { failure } if failure.kind() == TaskFailureKind::TimedOut => {
                Ok(Self {
                    status: "expired",
                    video: None,
                    model: None,
                    error: None,
                })
            }
            TaskStatus::Failed { failure } => Ok(Self {
                status: "failed",
                video: None,
                model: None,
                error: Some(VideoFailureResponse::from_kind(failure.kind())),
            }),
        }
    }
}

#[derive(Serialize)]
struct VideoOutputResponse {
    url: String,
    duration: u8,
    respect_moderation: bool,
}

#[derive(Serialize)]
struct VideoFailureResponse {
    code: &'static str,
    message: &'static str,
}

impl VideoFailureResponse {
    const fn from_kind(kind: TaskFailureKind) -> Self {
        match kind {
            TaskFailureKind::Rejected => Self {
                code: "invalid_argument",
                message: "Video task was rejected.",
            },
            TaskFailureKind::Cancelled => Self {
                code: "failed_precondition",
                message: "Video task was cancelled.",
            },
            TaskFailureKind::TimedOut => Self {
                code: "service_unavailable",
                message: "Video task expired.",
            },
            TaskFailureKind::Upstream => Self {
                code: "service_unavailable",
                message: "Video task failed upstream.",
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use af_domain::{TaskFailure, TaskProgress, TaskState};
    use af_protocol::{
        CanonicalTaskOutput, CanonicalVideoOutput, VideoDuration, VideoModel, VideoOutputUrl,
    };
    use http::HeaderValue;

    use super::*;

    #[test]
    fn idempotency_key_requires_one_lowercase_128_bit_value() {
        let mut headers = HeaderMap::new();
        headers.insert(
            IDEMPOTENCY_KEY_HEADER,
            HeaderValue::from_static("11111111111111111111111111111111"),
        );
        assert!(parse_idempotency_key(&headers).is_ok());

        for invalid in [
            "",
            "1111111111111111111111111111111",
            "1111111111111111111111111111111A",
            "00000000000000000000000000000000",
        ] {
            headers.insert(
                IDEMPOTENCY_KEY_HEADER,
                HeaderValue::from_str(invalid).unwrap(),
            );
            assert!(parse_idempotency_key(&headers).is_err());
        }
    }

    #[test]
    fn history_query_uses_strict_opaque_cursor_and_bounded_limit() {
        let cursor = VideoTaskListCursor::new(AsyncTaskId::new([0x42; 16]).unwrap());
        let encoded = encode_history_cursor(cursor);
        assert_eq!(decode_history_cursor(&encoded).unwrap(), cursor);
        assert_eq!(
            parse_history_query(Some(&format!("before={encoded}&limit=50"))).unwrap(),
            (Some(cursor), 50)
        );
        assert_eq!(parse_history_query(None).unwrap(), (None, 20));

        for invalid in [
            "before=bad",
            "before=00000000000000000000000000000000",
            "limit=0",
            "limit=101",
            "limit=+1",
            "limit=1&limit=2",
            "unknown=1",
            "before=%",
        ] {
            assert!(parse_history_query(Some(invalid)).is_err(), "{invalid}");
        }
    }

    #[test]
    fn history_response_does_not_include_temporary_output_or_private_binding() {
        let item = VideoTaskListItem::new(
            AsyncTaskId::new([0x33; 16]).unwrap(),
            "gpt-video".to_owned(),
            TaskStatus::Running {
                progress: TaskProgress::new(2_500).unwrap(),
            },
            1_800_000_000,
            1_800_000_030,
        );
        let response = VideoTaskListResponse::from_page(&VideoTaskPage::new(vec![item], None));
        let value = serde_json::to_value(response).unwrap();
        assert_eq!(value["data"][0]["status"], "pending");
        assert_eq!(value["data"][0]["progress_basis_points"], 2_500);
        assert!(value["data"][0].get("video").is_none());
        assert!(value["data"][0].get("channel_id").is_none());
        assert!(value["data"][0].get("credential_id").is_none());
        assert!(value["data"][0].get("idempotency_key").is_none());
    }

    #[test]
    fn poll_response_preserves_xai_pending_success_and_failure_shapes() {
        let task_id = AsyncTaskId::new([0x11; 16]).unwrap();
        let pending = VideoTaskSnapshot::new(
            task_id,
            TaskStatus::Running {
                progress: TaskProgress::ZERO,
            },
            None,
        );
        assert_eq!(
            serde_json::to_value(VideoPollResponse::from_snapshot(&pending).unwrap()).unwrap(),
            serde_json::json!({"status": "pending"})
        );

        let output = CanonicalVideoOutput::new(
            VideoOutputUrl::new("https://video.example/result.mp4?signature=private").unwrap(),
            VideoDuration::new(8).unwrap(),
            VideoModel::new("grok-imagine-video-1.5").unwrap(),
        );
        let succeeded = VideoTaskSnapshot::new(
            task_id,
            TaskStatus::Succeeded,
            Some(CanonicalTaskOutput::Video(output)),
        );
        assert_eq!(
            serde_json::to_value(VideoPollResponse::from_snapshot(&succeeded).unwrap()).unwrap(),
            serde_json::json!({
                "status": "done",
                "video": {
                    "url": "https://video.example/result.mp4?signature=private",
                    "duration": 8,
                    "respect_moderation": true
                },
                "model": "grok-imagine-video-1.5"
            })
        );

        let failed = VideoTaskSnapshot::new(
            task_id,
            TaskStatus::Failed {
                failure: TaskFailure::without_reason(TaskFailureKind::Upstream),
            },
            None,
        );
        assert_eq!(
            serde_json::to_value(VideoPollResponse::from_snapshot(&failed).unwrap()).unwrap(),
            serde_json::json!({
                "status": "failed",
                "error": {
                    "code": "service_unavailable",
                    "message": "Video task failed upstream."
                }
            })
        );
    }

    #[test]
    fn succeeded_snapshot_without_refreshed_output_fails_closed() {
        let snapshot = VideoTaskSnapshot::new(
            AsyncTaskId::new([0x22; 16]).unwrap(),
            TaskStatus::Succeeded,
            None,
        );
        assert!(VideoPollResponse::from_snapshot(&snapshot).is_err());
    }

    #[test]
    fn task_state_mapping_remains_closed() {
        assert_eq!(TaskState::Submitted.as_str(), "submitted");
    }
}
