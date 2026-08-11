//! xAI Grok Imagine Video 文本生成视频协议。
//!
//! 本模块严格实现官方 `POST /v1/videos/generations` 与
//! `GET /v1/videos/{request_id}` 的文本生成交集。图片、参考视频、编辑和扩展字段均失败关闭。

use std::{error::Error, fmt, str::FromStr as _};

use af_domain::{
    TaskFailure, TaskFailureKind, TaskProgress, TaskStatus, TaskSubmission, UpstreamTaskId,
};
use serde_json::{Map, Value};

use crate::bounded_json::{self, BoundedJsonError, JsonLimits};
use crate::{
    CanonicalTaskOutput, CanonicalTaskPoll, CanonicalVideoGenerationRequest, CanonicalVideoOutput,
    VideoAspectRatio, VideoDuration, VideoModel, VideoOutputUrl, VideoPrompt, VideoResolution,
};

mod wire;

use wire::{
    Field, XaiVideoErrorCodeWire, XaiVideoGenerationRequestWire, XaiVideoPollWire,
    XaiVideoStatusWire, XaiVideoSubmissionWire,
};

/// xAI 视频请求或响应正文上限。
pub const MAX_BODY_BYTES: usize = 64 * 1_024;
/// xAI 失败消息的最大 UTF-8 字节数；消息仅校验后丢弃，不进入领域对象。
pub const MAX_ERROR_MESSAGE_BYTES: usize = 2_048;

const JSON_LIMITS: JsonLimits = JsonLimits {
    max_depth: 5,
    max_nodes: 64,
    max_object_entries: 8,
    max_array_items: 1,
    max_string_bytes: 48 * 1_024,
    max_key_bytes: 64,
};

/// xAI 视频协议的稳定错误类别。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum XaiVideoProtocolError {
    /// 正文超过固定字节上限。
    BodyTooLarge,
    /// JSON 语法无效。
    InvalidJson,
    /// JSON 对象包含重复键。
    DuplicateKey,
    /// JSON 深度、节点或字符串预算超限。
    LimitExceeded,
    /// 字段缺失、显式空值或值域无效。
    InvalidValue,
    /// 请求使用当前切片尚未建模的能力组合。
    UnsupportedFeature,
}

impl fmt::Display for XaiVideoProtocolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::BodyTooLarge => "xAI 视频正文超过大小限制",
            Self::InvalidJson => "xAI 视频 JSON 无效",
            Self::DuplicateKey => "xAI 视频 JSON 包含重复键",
            Self::LimitExceeded => "xAI 视频 JSON 超过结构预算",
            Self::InvalidValue => "xAI 视频字段值无效",
            Self::UnsupportedFeature => "xAI 视频请求包含未建模能力",
        })
    }
}

impl Error for XaiVideoProtocolError {}

/// 解析严格的 xAI 文本生成视频请求。
pub fn parse_request(
    body: &[u8],
) -> Result<CanonicalVideoGenerationRequest, XaiVideoProtocolError> {
    let wire: XaiVideoGenerationRequestWire = parse_wire(body)?;
    let request = CanonicalVideoGenerationRequest::new(
        VideoModel::new(wire.model).map_err(|_| XaiVideoProtocolError::InvalidValue)?,
        VideoPrompt::new(wire.prompt).map_err(|_| XaiVideoProtocolError::InvalidValue)?,
        optional_field(wire.duration)
            .map(VideoDuration::new)
            .transpose()
            .map_err(|_| XaiVideoProtocolError::InvalidValue)?,
        optional_field(wire.aspect_ratio)
            .map(|value| VideoAspectRatio::from_str(&value))
            .transpose()
            .map_err(|_| XaiVideoProtocolError::InvalidValue)?,
        optional_field(wire.resolution)
            .map(|value| VideoResolution::from_str(&value))
            .transpose()
            .map_err(|_| XaiVideoProtocolError::InvalidValue)?,
    );
    validate_request(&request)?;
    Ok(request)
}

/// 构造 xAI 文本生成视频请求；Canonical 边界之外的组合失败关闭。
pub fn build_request(
    request: &CanonicalVideoGenerationRequest,
) -> Result<Value, XaiVideoProtocolError> {
    validate_request(request)?;
    let mut root = Map::from_iter([
        (
            "model".to_owned(),
            Value::String(request.model().as_str().to_owned()),
        ),
        (
            "prompt".to_owned(),
            Value::String(request.prompt().as_str().to_owned()),
        ),
    ]);
    if let Some(duration) = request.duration() {
        root.insert(
            "duration".to_owned(),
            Value::Number(duration.seconds().into()),
        );
    }
    if let Some(aspect_ratio) = request.aspect_ratio() {
        root.insert(
            "aspect_ratio".to_owned(),
            Value::String(aspect_ratio.as_str().to_owned()),
        );
    }
    if let Some(resolution) = request.resolution() {
        root.insert(
            "resolution".to_owned(),
            Value::String(resolution.as_str().to_owned()),
        );
    }
    Ok(Value::Object(root))
}

/// 解析提交响应并创建后续轮询必须复用的脱敏句柄。
pub fn parse_submission(body: &[u8]) -> Result<TaskSubmission, XaiVideoProtocolError> {
    let wire: XaiVideoSubmissionWire = parse_wire(body)?;
    let task_id =
        UpstreamTaskId::new(wire.request_id).map_err(|_| XaiVideoProtocolError::InvalidValue)?;
    Ok(TaskSubmission::new(
        task_id,
        TaskStatus::Submitted {
            progress: TaskProgress::ZERO,
        },
    ))
}

/// 解析一次 xAI 视频轮询响应。
pub fn parse_poll(body: &[u8]) -> Result<CanonicalTaskPoll, XaiVideoProtocolError> {
    let wire: XaiVideoPollWire = parse_wire(body)?;
    match wire.status {
        XaiVideoStatusWire::Pending => {
            require_absent(&wire.video)?;
            require_absent(&wire.model)?;
            require_absent(&wire.error)?;
            CanonicalTaskPoll::new(
                TaskStatus::Running {
                    progress: TaskProgress::ZERO,
                },
                None,
            )
            .map_err(|_| XaiVideoProtocolError::InvalidValue)
        }
        XaiVideoStatusWire::Done => parse_done(wire.video, wire.model, wire.error),
        XaiVideoStatusWire::Expired => {
            require_absent(&wire.video)?;
            require_absent(&wire.model)?;
            require_absent(&wire.error)?;
            failed_poll(TaskFailureKind::TimedOut)
        }
        XaiVideoStatusWire::Failed => parse_failed(wire.video, wire.model, wire.error),
    }
}

fn parse_done(
    video: Field<wire::XaiVideoOutputWire>,
    model: Field<String>,
    error: Field<wire::XaiVideoErrorWire>,
) -> Result<CanonicalTaskPoll, XaiVideoProtocolError> {
    require_absent(&error)?;
    let video = required_field(video)?;
    let model =
        VideoModel::new(required_field(model)?).map_err(|_| XaiVideoProtocolError::InvalidValue)?;
    let duration =
        VideoDuration::new(video.duration).map_err(|_| XaiVideoProtocolError::InvalidValue)?;
    if !video.respect_moderation {
        return failed_poll(TaskFailureKind::Rejected);
    }
    let url = VideoOutputUrl::new(required_field(video.url)?)
        .map_err(|_| XaiVideoProtocolError::InvalidValue)?;
    CanonicalTaskPoll::new(
        TaskStatus::Succeeded,
        Some(CanonicalTaskOutput::Video(CanonicalVideoOutput::new(
            url, duration, model,
        ))),
    )
    .map_err(|_| XaiVideoProtocolError::InvalidValue)
}

fn parse_failed(
    video: Field<wire::XaiVideoOutputWire>,
    model: Field<String>,
    error: Field<wire::XaiVideoErrorWire>,
) -> Result<CanonicalTaskPoll, XaiVideoProtocolError> {
    require_absent(&video)?;
    require_absent(&model)?;
    let error = required_field(error)?;
    validate_error_message(&error.message)?;
    let kind = match error.code {
        XaiVideoErrorCodeWire::InvalidArgument
        | XaiVideoErrorCodeWire::PermissionDenied
        | XaiVideoErrorCodeWire::FailedPrecondition => TaskFailureKind::Rejected,
        XaiVideoErrorCodeWire::ServiceUnavailable | XaiVideoErrorCodeWire::InternalError => {
            TaskFailureKind::Upstream
        }
    };
    failed_poll(kind)
}

fn failed_poll(kind: TaskFailureKind) -> Result<CanonicalTaskPoll, XaiVideoProtocolError> {
    CanonicalTaskPoll::new(
        TaskStatus::Failed {
            failure: TaskFailure::without_reason(kind),
        },
        None,
    )
    .map_err(|_| XaiVideoProtocolError::InvalidValue)
}

fn validate_request(
    request: &CanonicalVideoGenerationRequest,
) -> Result<(), XaiVideoProtocolError> {
    if request.resolution() == Some(VideoResolution::P1080)
        && !request
            .model()
            .as_str()
            .starts_with("grok-imagine-video-1.5")
    {
        return Err(XaiVideoProtocolError::UnsupportedFeature);
    }
    Ok(())
}

fn validate_error_message(message: &str) -> Result<(), XaiVideoProtocolError> {
    if message.is_empty()
        || message.len() > MAX_ERROR_MESSAGE_BYTES
        || message.chars().any(char::is_control)
    {
        Err(XaiVideoProtocolError::InvalidValue)
    } else {
        Ok(())
    }
}

fn parse_wire<T>(body: &[u8]) -> Result<T, XaiVideoProtocolError>
where
    T: serde::de::DeserializeOwned,
{
    if body.len() > MAX_BODY_BYTES {
        return Err(XaiVideoProtocolError::BodyTooLarge);
    }
    let value = bounded_json::parse_value(body, JSON_LIMITS).map_err(map_json_error)?;
    serde_json::from_value(value).map_err(|_| XaiVideoProtocolError::InvalidValue)
}

fn map_json_error(error: BoundedJsonError) -> XaiVideoProtocolError {
    match error {
        BoundedJsonError::InvalidJson => XaiVideoProtocolError::InvalidJson,
        BoundedJsonError::DuplicateKey => XaiVideoProtocolError::DuplicateKey,
        BoundedJsonError::LimitExceeded => XaiVideoProtocolError::LimitExceeded,
    }
}

fn optional_field<T>(field: Field<T>) -> Option<T> {
    match field {
        Field::Missing => None,
        Field::Value(value) => Some(value),
    }
}

fn required_field<T>(field: Field<T>) -> Result<T, XaiVideoProtocolError> {
    optional_field(field).ok_or(XaiVideoProtocolError::InvalidValue)
}

fn require_absent<T>(field: &Field<T>) -> Result<(), XaiVideoProtocolError> {
    if matches!(field, Field::Missing) {
        Ok(())
    } else {
        Err(XaiVideoProtocolError::InvalidValue)
    }
}

#[cfg(test)]
mod tests;
