//! xAI 兼容视频异步任务公开契约。

#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, ToSchema};

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = VideoGenerationRequest)]
struct VideoGenerationRequestSchema {
    #[schema(min_length = 1, max_length = 256, example = "grok-imagine-video-1.5")]
    model: String,
    #[schema(min_length = 1, max_length = 32768)]
    prompt: String,
    #[schema(minimum = 1, maximum = 15, required = false)]
    duration: Option<u8>,
    #[schema(required = false, example = "16:9")]
    aspect_ratio: Option<VideoAspectRatioSchema>,
    #[schema(required = false, example = "720p")]
    resolution: Option<VideoResolutionSchema>,
}

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
#[schema(as = VideoAspectRatio)]
enum VideoAspectRatioSchema {
    #[serde(rename = "1:1")]
    Square,
    #[serde(rename = "16:9")]
    Landscape,
    #[serde(rename = "9:16")]
    Portrait,
    #[serde(rename = "4:3")]
    StandardLandscape,
    #[serde(rename = "3:4")]
    StandardPortrait,
    #[serde(rename = "3:2")]
    PhotoLandscape,
    #[serde(rename = "2:3")]
    PhotoPortrait,
}

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
#[schema(as = VideoResolution)]
enum VideoResolutionSchema {
    #[serde(rename = "480p")]
    P480,
    #[serde(rename = "720p")]
    P720,
    #[serde(rename = "1080p")]
    P1080,
}

#[derive(Serialize, ToSchema)]
#[schema(as = VideoSubmissionResponse)]
struct VideoSubmissionResponseSchema {
    #[schema(
        min_length = 32,
        max_length = 32,
        pattern = "^[0-9a-f]{32}$",
        example = "4f42f8a5d17445a397955d22f1b47eb8"
    )]
    request_id: String,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = VideoTaskStatus)]
enum VideoTaskStatusSchema {
    Pending,
    Done,
    Expired,
    Failed,
}

#[derive(Serialize, ToSchema)]
#[schema(as = VideoOutput)]
struct VideoOutputSchema {
    #[schema(max_length = 8192)]
    url: String,
    #[schema(minimum = 1, maximum = 15)]
    duration: u8,
    respect_moderation: bool,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = VideoFailureCode)]
enum VideoFailureCodeSchema {
    InvalidArgument,
    FailedPrecondition,
    ServiceUnavailable,
}

#[derive(Serialize, ToSchema)]
#[schema(as = VideoFailure)]
struct VideoFailureSchema {
    code: VideoFailureCodeSchema,
    message: String,
}

#[derive(Serialize, ToSchema)]
#[schema(as = VideoPollResponse)]
struct VideoPollResponseSchema {
    status: VideoTaskStatusSchema,
    #[schema(required = false)]
    video: Option<VideoOutputSchema>,
    #[schema(required = false, min_length = 1, max_length = 256)]
    model: Option<String>,
    #[schema(required = false)]
    error: Option<VideoFailureSchema>,
}

#[derive(Serialize, ToSchema)]
#[schema(as = VideoTaskListItem)]
struct VideoTaskListItemSchema {
    #[schema(
        min_length = 32,
        max_length = 32,
        pattern = "^[0-9a-f]{32}$",
        example = "4f42f8a5d17445a397955d22f1b47eb8"
    )]
    id: String,
    #[schema(min_length = 1, max_length = 256)]
    model: String,
    status: VideoTaskStatusSchema,
    #[schema(minimum = 0, maximum = 10000)]
    progress_basis_points: u16,
    #[schema(minimum = 0)]
    created_at: u64,
    #[schema(minimum = 0)]
    updated_at: u64,
}

#[derive(Serialize, ToSchema)]
#[schema(as = VideoTaskListResponse)]
struct VideoTaskListResponseSchema {
    #[schema(max_items = 100)]
    data: Vec<VideoTaskListItemSchema>,
    #[schema(
        required = true,
        min_length = 32,
        max_length = 32,
        pattern = "^[0-9a-f]{32}$"
    )]
    next_cursor: Option<String>,
}

#[derive(Serialize, ToSchema)]
#[schema(as = VideoTaskErrorBody)]
struct VideoTaskErrorBodySchema {
    code: String,
    message: String,
    #[schema(required = true)]
    param: Option<String>,
    #[serde(rename = "type")]
    error_type: String,
}

#[derive(Serialize, ToSchema)]
#[schema(as = VideoTaskError)]
struct VideoTaskErrorSchema {
    error: VideoTaskErrorBodySchema,
}

#[utoipa::path(
    post,
    path = "/v1/videos/generations",
    operation_id = "submitVideoTask",
    tag = "视频任务",
    summary = "提交视频生成任务",
    description = "客户端必须生成并在结果未知重试时复用同一个 128 位小写十六进制幂等键；同用户同载荷返回同一本地任务标识，不同载荷返回冲突。",
    params((
        "Idempotency-Key" = String,
        Header,
        min_length = 32,
        max_length = 32,
        pattern = "^[0-9a-f]{32}$",
        description = "客户端生成的非零 128 位幂等键"
    )),
    request_body = VideoGenerationRequestSchema,
    responses(
        (status = 200, description = "任务已提交或按相同幂等载荷安全重放", body = VideoSubmissionResponseSchema),
        (status = 400, description = "请求或幂等键格式无效", body = VideoTaskErrorSchema),
        (status = 401, description = "API Key 无效", body = VideoTaskErrorSchema),
        (status = 404, description = "模型不存在或不在令牌允许范围内", body = VideoTaskErrorSchema),
        (status = 409, description = "幂等键已绑定到不同请求载荷", body = VideoTaskErrorSchema),
        (status = 429, description = "额度、并发或请求频率受限", body = VideoTaskErrorSchema),
        (status = 500, description = "任务持久化或计费状态异常", body = VideoTaskErrorSchema),
        (status = 503, description = "上游或提交结果暂不可确认；必须复用原幂等键重试", body = VideoTaskErrorSchema)
    ),
    security(("apiKeyAuth" = []))
)]
fn submit_video_task() {}

#[utoipa::path(
    get,
    path = "/v1/videos",
    operation_id = "listVideoTasks",
    tag = "视频任务",
    summary = "列出视频生成任务",
    description = "只读取当前 API Key 所属用户已经持久化的视频任务摘要。列表不会访问上游，也不返回幂等键、渠道、凭据、上游任务标识或短期结果地址。",
    params(
        (
            "before" = Option<String>,
            Query,
            min_length = 32,
            max_length = 32,
            pattern = "^[0-9a-f]{32}$",
            description = "上一页返回的不透明倒序游标"
        ),
        (
            "limit" = Option<usize>,
            Query,
            minimum = 1,
            maximum = 100,
            description = "页面容量，默认 20"
        )
    ),
    responses(
        (status = 200, description = "当前用户的视频任务历史", body = VideoTaskListResponseSchema),
        (status = 400, description = "分页参数无效", body = VideoTaskErrorSchema),
        (status = 401, description = "API Key 无效", body = VideoTaskErrorSchema),
        (status = 500, description = "任务持久化状态异常", body = VideoTaskErrorSchema)
    ),
    security(("apiKeyAuth" = []))
)]
fn list_video_tasks() {}

#[utoipa::path(
    get,
    path = "/v1/videos/{task_id}",
    operation_id = "pollVideoTask",
    tag = "视频任务",
    summary = "查询视频生成任务",
    description = "只允许当前 API Key 所属用户查询本人任务。成功终态会轮询原上游任务重新取得短期结果地址，但不会重新提交、冻结或结算。",
    params((
        "task_id" = String,
        Path,
        min_length = 32,
        max_length = 32,
        pattern = "^[0-9a-f]{32}$",
        description = "提交接口返回的本地任务标识"
    )),
    responses(
        (status = 200, description = "xAI 兼容的 pending、done、expired 或 failed 状态", body = VideoPollResponseSchema),
        (status = 401, description = "API Key 无效", body = VideoTaskErrorSchema),
        (status = 404, description = "当前用户范围内不存在该任务", body = VideoTaskErrorSchema),
        (status = 500, description = "任务持久化或计费状态异常", body = VideoTaskErrorSchema),
        (status = 503, description = "任务结果未知或短期结果当前无法重新取得", body = VideoTaskErrorSchema)
    ),
    security(("apiKeyAuth" = []))
)]
fn poll_video_task() {}

#[derive(OpenApi)]
#[openapi(
    paths(list_video_tasks, submit_video_task, poll_video_task),
    components(schemas(
        VideoGenerationRequestSchema,
        VideoAspectRatioSchema,
        VideoResolutionSchema,
        VideoSubmissionResponseSchema,
        VideoTaskStatusSchema,
        VideoOutputSchema,
        VideoFailureCodeSchema,
        VideoFailureSchema,
        VideoPollResponseSchema,
        VideoTaskListItemSchema,
        VideoTaskListResponseSchema,
        VideoTaskErrorBodySchema,
        VideoTaskErrorSchema
    ))
)]
struct VideoTaskApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    VideoTaskApi::openapi()
}
