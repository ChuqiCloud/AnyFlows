use axum::response::{IntoResponse, Response};
use http::{
    HeaderValue, StatusCode,
    header::{CACHE_CONTROL, RETRY_AFTER},
};
use serde::Serialize;
use utoipa::ToSchema;

/// 管理 API 对外稳定的错误码和英文文案。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ManagementError {
    /// JSON 正文或字段结构无效。
    InvalidRequest,
    /// 用户名或密码不正确。
    InvalidCredentials,
    /// 用户名密码登录已被运营设置关闭。
    LoginDisabled,
    /// JWT 缺失、无效、过期或对应用户已失效。
    InvalidSession,
    /// 当前会话没有访问该管理端点的角色权限。
    Forbidden,
    /// 系统已经完成首次安装，或并发请求先完成了安装。
    SetupConflict,
    /// 用户不存在或已经被软删除。
    UserNotFound,
    /// 用户名或邮箱与当前有效用户冲突。
    UserConflict,
    /// 当前实例尚未启用公开注册。
    RegistrationDisabled,
    /// 当前客户端已耗尽公开注册固定窗口尝试次数。
    RegistrationRateLimited { retry_after_seconds: u64 },
    /// 验证码无效或带邮箱身份无法创建；不得细分具体原因。
    RegistrationRejected,
    /// SMTP 设置尚未达到可投递状态。
    EmailNotConfigured,
    /// SMTP 连接、认证或投递失败，底层响应不会向外透传。
    EmailDeliveryFailed,
    /// 分组不存在或已经被软删除。
    GroupNotFound,
    /// 分组名与当前有效分组冲突。
    GroupConflict,
    /// 分组仍被有效用户或有效令牌引用。
    GroupInUse,
    /// 令牌不存在或已经被软删除。
    TokenNotFound,
    /// 当前用户的未软删除 API Key 已达到容量上限。
    TokenLimitReached,
    /// 渠道不存在或已经被软删除。
    ChannelNotFound,
    /// 凭据不存在、已经软删除或不属于指定渠道。
    CredentialNotFound,
    /// 渠道测活服务未在当前实例注入。
    ProbeUnavailable,
    /// 分享令牌未知、已失效、已撤销或不属于当前用户。
    PlaygroundShareNotFound,
    /// 当前用户的有效分享数量已经达到上限。
    PlaygroundShareLimitReached,
    /// 私有会话标识未知或不属于当前用户。
    PlaygroundConversationNotFound,
    /// 当前用户的私有会话数量已经达到上限。
    PlaygroundConversationLimitReached,
    /// 私有会话 revision 已被其他请求推进。
    PlaygroundConversationConflict,
    /// 管理认证依赖发生内部故障。
    Internal,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = ManagementError)]
pub(crate) struct ManagementErrorBody {
    #[schema(
        value_type = crate::openapi::schema::ManagementErrorCodeSchema,
        inline
    )]
    code: &'static str,
    message: &'static str,
}

impl IntoResponse for ManagementError {
    fn into_response(self) -> Response {
        let retry_after_seconds = match self {
            Self::RegistrationRateLimited {
                retry_after_seconds,
            } => Some(retry_after_seconds),
            _ => None,
        };
        let (status, body) = match self {
            Self::InvalidRequest => (
                StatusCode::BAD_REQUEST,
                ManagementErrorBody {
                    code: "invalid_request",
                    message: "Invalid request",
                },
            ),
            Self::InvalidCredentials => (
                StatusCode::UNAUTHORIZED,
                ManagementErrorBody {
                    code: "invalid_credentials",
                    message: "Invalid credentials",
                },
            ),
            Self::LoginDisabled => (
                StatusCode::FORBIDDEN,
                ManagementErrorBody {
                    code: "password_login_disabled",
                    message: "Password login disabled",
                },
            ),
            Self::InvalidSession => (
                StatusCode::UNAUTHORIZED,
                ManagementErrorBody {
                    code: "invalid_session",
                    message: "Invalid session",
                },
            ),
            Self::Forbidden => (
                StatusCode::FORBIDDEN,
                ManagementErrorBody {
                    code: "forbidden",
                    message: "Forbidden",
                },
            ),
            Self::SetupConflict => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "setup_conflict",
                    message: "Setup already completed",
                },
            ),
            Self::UserNotFound => (
                StatusCode::NOT_FOUND,
                ManagementErrorBody {
                    code: "user_not_found",
                    message: "User not found",
                },
            ),
            Self::UserConflict => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "user_conflict",
                    message: "User conflict",
                },
            ),
            Self::RegistrationDisabled => (
                StatusCode::FORBIDDEN,
                ManagementErrorBody {
                    code: "registration_disabled",
                    message: "Registration disabled",
                },
            ),
            Self::RegistrationRateLimited { .. } => (
                StatusCode::TOO_MANY_REQUESTS,
                ManagementErrorBody {
                    code: "registration_rate_limited",
                    message: "Registration rate limited",
                },
            ),
            Self::RegistrationRejected => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "registration_rejected",
                    message: "Registration rejected",
                },
            ),
            Self::EmailNotConfigured => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "email_not_configured",
                    message: "Email delivery is not configured",
                },
            ),
            Self::EmailDeliveryFailed => (
                StatusCode::BAD_GATEWAY,
                ManagementErrorBody {
                    code: "email_delivery_failed",
                    message: "Email delivery failed",
                },
            ),
            Self::GroupNotFound => (
                StatusCode::NOT_FOUND,
                ManagementErrorBody {
                    code: "group_not_found",
                    message: "Group not found",
                },
            ),
            Self::GroupConflict => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "group_conflict",
                    message: "Group conflict",
                },
            ),
            Self::GroupInUse => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "group_in_use",
                    message: "Group in use",
                },
            ),
            Self::TokenNotFound => (
                StatusCode::NOT_FOUND,
                ManagementErrorBody {
                    code: "token_not_found",
                    message: "Token not found",
                },
            ),
            Self::TokenLimitReached => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "token_limit_reached",
                    message: "Token limit reached",
                },
            ),
            Self::ChannelNotFound => (
                StatusCode::NOT_FOUND,
                ManagementErrorBody {
                    code: "channel_not_found",
                    message: "Channel not found",
                },
            ),
            Self::CredentialNotFound => (
                StatusCode::NOT_FOUND,
                ManagementErrorBody {
                    code: "credential_not_found",
                    message: "Credential not found",
                },
            ),
            Self::ProbeUnavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                ManagementErrorBody {
                    code: "probe_unavailable",
                    message: "Channel probe unavailable",
                },
            ),
            Self::PlaygroundShareNotFound => (
                StatusCode::NOT_FOUND,
                ManagementErrorBody {
                    code: "playground_share_not_found",
                    message: "Playground share not found",
                },
            ),
            Self::PlaygroundShareLimitReached => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "playground_share_limit_reached",
                    message: "Playground share limit reached",
                },
            ),
            Self::PlaygroundConversationNotFound => (
                StatusCode::NOT_FOUND,
                ManagementErrorBody {
                    code: "playground_conversation_not_found",
                    message: "Playground conversation not found",
                },
            ),
            Self::PlaygroundConversationLimitReached => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "playground_conversation_limit_reached",
                    message: "Playground conversation limit reached",
                },
            ),
            Self::PlaygroundConversationConflict => (
                StatusCode::CONFLICT,
                ManagementErrorBody {
                    code: "playground_conversation_conflict",
                    message: "Playground conversation conflict",
                },
            ),
            Self::Internal => (
                StatusCode::INTERNAL_SERVER_ERROR,
                ManagementErrorBody {
                    code: "internal_error",
                    message: "Internal server error",
                },
            ),
        };
        let mut response = (status, axum::Json(body)).into_response();
        response
            .headers_mut()
            .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
        if let Some(retry_after_seconds) = retry_after_seconds {
            response.headers_mut().insert(
                RETRY_AFTER,
                HeaderValue::from_str(&retry_after_seconds.to_string())
                    .expect("非负整数秒数必须始终是有效响应头"),
            );
        }
        response
    }
}
