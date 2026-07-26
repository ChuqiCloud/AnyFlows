use af_admin::{SessionAuthentication, SessionRole};
use axum::{
    extract::Request,
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::management_error::ManagementError;

/// 管理员 API 的第二层角色边界，避免只依赖路由装配时的鉴权顺序。
pub(crate) async fn authorize_management_admin(request: Request, next: Next) -> Response {
    let Some(authentication) = request.extensions().get::<SessionAuthentication>() else {
        return ManagementError::Internal.into_response();
    };
    if authentication.principal().role() != SessionRole::Admin {
        return ManagementError::Forbidden.into_response();
    }
    next.run(request).await
}
