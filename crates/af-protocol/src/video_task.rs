use std::{error::Error, fmt, str::FromStr};

use af_domain::TaskStatus;
use url::Url;

/// 视频任务模型标识的最大 UTF-8 字节数。
pub const MAX_VIDEO_MODEL_BYTES: usize = 256;
/// 视频生成提示词的最大 UTF-8 字节数。
pub const MAX_VIDEO_PROMPT_BYTES: usize = 32 * 1_024;
/// 视频结果临时地址的最大 UTF-8 字节数。
pub const MAX_VIDEO_OUTPUT_URL_BYTES: usize = 8 * 1_024;
/// 视频任务允许的最小时长。
pub const MIN_VIDEO_DURATION_SECONDS: u8 = 1;
/// 当前视频任务统一允许的最大时长。
pub const MAX_VIDEO_DURATION_SECONDS: u8 = 15;

/// 视频任务值对象或状态关联错误。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VideoTaskError {
    /// 模型标识为空。
    EmptyModel,
    /// 模型标识超过容量上限。
    ModelTooLong,
    /// 模型标识包含空白或控制字符。
    InvalidModel,
    /// 提示词为空或只包含空白。
    EmptyPrompt,
    /// 提示词超过容量上限。
    PromptTooLong,
    /// 提示词包含不受支持的控制字符。
    InvalidPrompt,
    /// 时长不在闭合范围内。
    DurationOutOfRange,
    /// 宽高比不属于闭合集合。
    InvalidAspectRatio,
    /// 分辨率不属于闭合集合。
    InvalidResolution,
    /// 视频临时地址超过容量上限。
    OutputUrlTooLong,
    /// 视频临时地址不满足 HTTPS 安全边界。
    InvalidOutputUrl,
    /// 成功状态缺少输出或非成功状态携带输出。
    StatusOutputMismatch,
}

impl fmt::Display for VideoTaskError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::EmptyModel => "视频模型不能为空",
            Self::ModelTooLong => "视频模型标识过长",
            Self::InvalidModel => "视频模型标识无效",
            Self::EmptyPrompt => "视频提示词不能为空",
            Self::PromptTooLong => "视频提示词过长",
            Self::InvalidPrompt => "视频提示词包含非法字符",
            Self::DurationOutOfRange => "视频时长超出允许范围",
            Self::InvalidAspectRatio => "视频宽高比不受支持",
            Self::InvalidResolution => "视频分辨率不受支持",
            Self::OutputUrlTooLong => "视频结果地址过长",
            Self::InvalidOutputUrl => "视频结果地址无效",
            Self::StatusOutputMismatch => "任务状态与输出结果不一致",
        })
    }
}

impl Error for VideoTaskError {}

/// 已校验且 Debug 脱敏的视频模型标识。
#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct VideoModel(String);

impl VideoModel {
    /// 校验并创建视频模型标识。
    pub fn new(value: impl Into<String>) -> Result<Self, VideoTaskError> {
        let value = value.into();
        if value.is_empty() {
            return Err(VideoTaskError::EmptyModel);
        }
        if value.len() > MAX_VIDEO_MODEL_BYTES {
            return Err(VideoTaskError::ModelTooLong);
        }
        if value.trim() != value
            || value
                .chars()
                .any(|character| character.is_whitespace() || character.is_control())
        {
            return Err(VideoTaskError::InvalidModel);
        }
        Ok(Self(value))
    }

    /// 返回已校验模型；调用方不得直接记录该值。
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for VideoModel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("VideoModel(<脱敏>)")
    }
}

/// 已校验且 Debug 脱敏的视频生成提示词。
#[derive(Clone, Eq, PartialEq)]
pub struct VideoPrompt(String);

impl VideoPrompt {
    /// 校验并创建视频提示词；换行和制表符允许保留，其他控制字符拒绝。
    pub fn new(value: impl Into<String>) -> Result<Self, VideoTaskError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(VideoTaskError::EmptyPrompt);
        }
        if value.len() > MAX_VIDEO_PROMPT_BYTES {
            return Err(VideoTaskError::PromptTooLong);
        }
        if value
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
        {
            return Err(VideoTaskError::InvalidPrompt);
        }
        Ok(Self(value))
    }

    /// 返回已校验提示词；调用方不得直接记录该值。
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for VideoPrompt {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("VideoPrompt(<脱敏>)")
    }
}

/// 视频时长，以整秒表示。
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct VideoDuration(u8);

impl VideoDuration {
    /// 校验并创建 1 到 15 秒的视频时长。
    pub const fn new(seconds: u8) -> Result<Self, VideoTaskError> {
        if seconds < MIN_VIDEO_DURATION_SECONDS || seconds > MAX_VIDEO_DURATION_SECONDS {
            Err(VideoTaskError::DurationOutOfRange)
        } else {
            Ok(Self(seconds))
        }
    }

    /// 返回整秒时长。
    #[must_use]
    pub const fn seconds(self) -> u8 {
        self.0
    }
}

/// 文本生成视频允许的宽高比集合。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum VideoAspectRatio {
    /// 1:1。
    Square,
    /// 16:9。
    Landscape,
    /// 9:16。
    Portrait,
    /// 4:3。
    StandardLandscape,
    /// 3:4。
    StandardPortrait,
    /// 3:2。
    PhotoLandscape,
    /// 2:3。
    PhotoPortrait,
}

impl VideoAspectRatio {
    /// 返回供应商无关的稳定原文。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Square => "1:1",
            Self::Landscape => "16:9",
            Self::Portrait => "9:16",
            Self::StandardLandscape => "4:3",
            Self::StandardPortrait => "3:4",
            Self::PhotoLandscape => "3:2",
            Self::PhotoPortrait => "2:3",
        }
    }
}

impl FromStr for VideoAspectRatio {
    type Err = VideoTaskError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "1:1" => Ok(Self::Square),
            "16:9" => Ok(Self::Landscape),
            "9:16" => Ok(Self::Portrait),
            "4:3" => Ok(Self::StandardLandscape),
            "3:4" => Ok(Self::StandardPortrait),
            "3:2" => Ok(Self::PhotoLandscape),
            "2:3" => Ok(Self::PhotoPortrait),
            _ => Err(VideoTaskError::InvalidAspectRatio),
        }
    }
}

/// 视频输出分辨率集合。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum VideoResolution {
    /// 480p。
    P480,
    /// 720p。
    P720,
    /// 1080p。
    P1080,
}

impl VideoResolution {
    /// 返回供应商无关的稳定原文。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::P480 => "480p",
            Self::P720 => "720p",
            Self::P1080 => "1080p",
        }
    }
}

impl FromStr for VideoResolution {
    type Err = VideoTaskError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "480p" => Ok(Self::P480),
            "720p" => Ok(Self::P720),
            "1080p" => Ok(Self::P1080),
            _ => Err(VideoTaskError::InvalidResolution),
        }
    }
}

/// 供应商无关的文本生成视频请求。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CanonicalVideoGenerationRequest {
    model: VideoModel,
    prompt: VideoPrompt,
    duration: Option<VideoDuration>,
    aspect_ratio: Option<VideoAspectRatio>,
    resolution: Option<VideoResolution>,
}

impl CanonicalVideoGenerationRequest {
    /// 创建已完成字段边界校验的视频生成请求。
    #[must_use]
    pub const fn new(
        model: VideoModel,
        prompt: VideoPrompt,
        duration: Option<VideoDuration>,
        aspect_ratio: Option<VideoAspectRatio>,
        resolution: Option<VideoResolution>,
    ) -> Self {
        Self {
            model,
            prompt,
            duration,
            aspect_ratio,
            resolution,
        }
    }

    #[must_use]
    /// 返回已校验模型。
    pub const fn model(&self) -> &VideoModel {
        &self.model
    }

    #[must_use]
    /// 返回已校验提示词。
    pub const fn prompt(&self) -> &VideoPrompt {
        &self.prompt
    }

    #[must_use]
    /// 返回显式时长；`None` 表示保留上游默认值。
    pub const fn duration(&self) -> Option<VideoDuration> {
        self.duration
    }

    #[must_use]
    /// 返回显式宽高比；`None` 表示保留上游默认值。
    pub const fn aspect_ratio(&self) -> Option<VideoAspectRatio> {
        self.aspect_ratio
    }

    #[must_use]
    /// 返回显式分辨率；`None` 表示保留上游默认值。
    pub const fn resolution(&self) -> Option<VideoResolution> {
        self.resolution
    }
}

/// 已校验且 Debug 脱敏的视频临时结果地址。
#[derive(Clone, Eq, PartialEq)]
pub struct VideoOutputUrl(String);

impl VideoOutputUrl {
    /// 只接受无 userinfo、无片段且具有主机的 HTTPS 地址；签名查询串允许保留。
    pub fn new(value: impl Into<String>) -> Result<Self, VideoTaskError> {
        let value = value.into();
        if value.len() > MAX_VIDEO_OUTPUT_URL_BYTES {
            return Err(VideoTaskError::OutputUrlTooLong);
        }
        if value.trim() != value {
            return Err(VideoTaskError::InvalidOutputUrl);
        }
        let parsed = Url::parse(&value).map_err(|_| VideoTaskError::InvalidOutputUrl)?;
        if parsed.scheme() != "https"
            || !parsed.has_host()
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.fragment().is_some()
        {
            return Err(VideoTaskError::InvalidOutputUrl);
        }
        Ok(Self(parsed.to_string()))
    }

    /// 返回临时结果地址；调用方不得记录或由服务端主动抓取该值。
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for VideoOutputUrl {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("VideoOutputUrl(<脱敏>)")
    }
}

/// 已完成视频任务的规范输出。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CanonicalVideoOutput {
    url: VideoOutputUrl,
    duration: VideoDuration,
    model: VideoModel,
}

impl CanonicalVideoOutput {
    /// 创建已经分别完成 URL、时长和模型校验的视频输出。
    #[must_use]
    pub const fn new(url: VideoOutputUrl, duration: VideoDuration, model: VideoModel) -> Self {
        Self {
            url,
            duration,
            model,
        }
    }

    #[must_use]
    /// 返回临时视频结果地址。
    pub const fn url(&self) -> &VideoOutputUrl {
        &self.url
    }

    #[must_use]
    /// 返回上游报告的真实视频时长。
    pub const fn duration(&self) -> VideoDuration {
        self.duration
    }

    #[must_use]
    /// 返回上游报告的实际模型。
    pub const fn model(&self) -> &VideoModel {
        &self.model
    }
}

/// 异步任务完成后的供应商无关输出集合。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CanonicalTaskOutput {
    Video(CanonicalVideoOutput),
}

impl CanonicalTaskOutput {
    /// 在输出为视频时返回对应值。
    #[must_use]
    pub const fn as_video(&self) -> Option<&CanonicalVideoOutput> {
        match self {
            Self::Video(output) => Some(output),
        }
    }
}

/// 一次任务轮询的闭合状态与可选终态输出。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CanonicalTaskPoll {
    status: TaskStatus,
    output: Option<CanonicalTaskOutput>,
}

impl CanonicalTaskPoll {
    /// 成功状态必须携带输出，其他状态禁止携带输出。
    pub fn new(
        status: TaskStatus,
        output: Option<CanonicalTaskOutput>,
    ) -> Result<Self, VideoTaskError> {
        if matches!(status, TaskStatus::Succeeded) != output.is_some() {
            return Err(VideoTaskError::StatusOutputMismatch);
        }
        Ok(Self { status, output })
    }

    #[must_use]
    /// 返回闭合任务状态。
    pub const fn status(&self) -> &TaskStatus {
        &self.status
    }

    #[must_use]
    /// 返回仅在成功终态存在的规范输出。
    pub const fn output(&self) -> Option<&CanonicalTaskOutput> {
        self.output.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use af_domain::{TaskProgress, TaskStatus};

    use super::*;

    #[test]
    fn values_enforce_bounds_and_redact_external_text() {
        let model = VideoModel::new("grok-imagine-video-1.5").unwrap();
        let prompt = VideoPrompt::new("private prompt").unwrap();
        let url = VideoOutputUrl::new("https://video.example/out.mp4?signature=private").unwrap();
        let debug = format!("{model:?}\n{prompt:?}\n{url:?}");
        for secret in ["grok-imagine", "private prompt", "signature"] {
            assert!(!debug.contains(secret));
        }
        assert!(VideoModel::new("invalid model").is_err());
        assert!(VideoPrompt::new(" \n ").is_err());
        assert!(VideoDuration::new(0).is_err());
        assert!(VideoDuration::new(16).is_err());
        assert!(VideoOutputUrl::new("http://video.example/out.mp4").is_err());
    }

    #[test]
    fn task_poll_requires_output_only_for_success() {
        let running = CanonicalTaskPoll::new(
            TaskStatus::Running {
                progress: TaskProgress::ZERO,
            },
            None,
        )
        .unwrap();
        assert!(running.output().is_none());

        let output = CanonicalTaskOutput::Video(CanonicalVideoOutput::new(
            VideoOutputUrl::new("https://video.example/out.mp4").unwrap(),
            VideoDuration::new(8).unwrap(),
            VideoModel::new("grok-imagine-video-1.5").unwrap(),
        ));
        assert!(CanonicalTaskPoll::new(TaskStatus::Succeeded, Some(output)).is_ok());
        assert_eq!(
            CanonicalTaskPoll::new(TaskStatus::Succeeded, None),
            Err(VideoTaskError::StatusOutputMismatch)
        );
    }
}
