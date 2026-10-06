//! 当前用户邀请中心 OpenAPI 契约。

#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    invitations::{UserInvitationRebateResponse, UserInvitationSummaryResponse},
    management_error::ManagementErrorBody,
};

#[utoipa::path(
    get,
    path = "/api/account/invitations",
    operation_id = "getUserInvitations",
    tag = "邀请中心",
    summary = "读取当前用户邀请汇总",
    responses(
        (status = 200, description = "当前用户的邀请码、统计与脱敏到账记录", body = UserInvitationSummaryResponse),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_user_invitations() {}

#[derive(OpenApi)]
#[openapi(
    paths(get_user_invitations),
    components(schemas(
        UserInvitationSummaryResponse,
        UserInvitationRebateResponse,
        ManagementErrorBody
    ))
)]
struct UserInvitationApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    UserInvitationApi::openapi()
}
