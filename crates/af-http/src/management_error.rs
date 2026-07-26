use axum::response::{IntoResponse, Response};
use http::{HeaderValue, StatusCode, header::CACHE_CONTROL};
use serde::Serialize;

/// 管理 API 对外稳定的错误码和英文文案。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ManagementError {
    /// JSON 正文或字段结构无效。
    InvalidRequest,
    /// 用户名或密码不正确。
    InvalidCredentials,
    /// JWT 缺失、无效、过期或对应用户已失效。
    InvalidSession,
    /// 当前会话没有访问该管理端点的角色权限。
    Forbidden,
    /// 用户不存在或已经被软删除。
    UserNotFound,
    /// 用户名或邮箱与当前有效用户冲突。
    UserConflict,
    /// 分组不存在或已经被软删除。
    GroupNotFound,
    /// 管理认证依赖发生内部故障。
    Internal,
}

#[derive(Serialize)]
struct ManagementErrorBody {
    code: &'static str,
    message: &'static str,
}

impl IntoResponse for ManagementError {
    fn into_response(self) -> Response {
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
            Self::GroupNotFound => (
                StatusCode::NOT_FOUND,
                ManagementErrorBody {
                    code: "group_not_found",
                    message: "Group not found",
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
        response
    }
}
