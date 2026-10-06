//! Playground 只读分享 OpenAPI 契约。

#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    playground_shares::{
        PlaygroundShareCreateRequest, PlaygroundShareCreateResponse, PlaygroundShareMessageDto,
        PlaygroundShareMessageRoleDto, PlaygroundShareReadResponse, PlaygroundShareSessionDto,
    },
};

#[utoipa::path(
    post,
    path = "/api/playground/shares",
    operation_id = "createPlaygroundShare",
    tag = "Playground 分享",
    summary = "创建只读 Playground 分享",
    description = "登录用户为一至四个已结束模型会话创建不可变快照。API Key、系统提示词、生成参数、用量和错误不会进入快照，分享令牌只返回一次。",
    request_body = PlaygroundShareCreateRequest,
    responses(
        (status = 200, description = "分享创建成功，令牌只在本响应返回", body = PlaygroundShareCreateResponse),
        (status = 400, description = "TTL、模型、消息往返或容量无效", body = ManagementErrorBody),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 409, description = "当前用户的有效分享达到上限", body = ManagementErrorBody),
        (status = 500, description = "随机源或持久化服务不可用", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn create_playground_share() {}

#[utoipa::path(
    get,
    path = "/api/playground/shares/{token}",
    operation_id = "getPlaygroundShare",
    tag = "Playground 分享",
    summary = "公开读取只读 Playground 分享",
    description = "凭高熵分享令牌读取仍有效且未撤销的可见模型与用户/助手消息。未知、过期、撤销和格式错误统一返回 404。",
    params(
        ("token" = String, Path, min_length = 49, max_length = 49, description = "一次性返回的 sh-af- 分享令牌")
    ),
    responses(
        (status = 200, description = "有效只读快照", body = PlaygroundShareReadResponse),
        (status = 404, description = "分享不存在或已失效", body = ManagementErrorBody),
        (status = 500, description = "持久化服务不可用", body = ManagementErrorBody)
    ),
    security(())
)]
fn get_playground_share() {}

#[utoipa::path(
    delete,
    path = "/api/playground/shares/{token}",
    operation_id = "revokePlaygroundShare",
    tag = "Playground 分享",
    summary = "撤销自己的 Playground 分享",
    description = "仅创建者可以撤销仍有效的分享；未知、失效、已撤销和非所有者令牌统一返回 404。",
    params(
        ("token" = String, Path, min_length = 49, max_length = 49, description = "待撤销的 sh-af- 分享令牌")
    ),
    responses(
        (status = 204, description = "撤销成功"),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 404, description = "分享不存在、已失效或不属于当前用户", body = ManagementErrorBody),
        (status = 500, description = "持久化服务不可用", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn revoke_playground_share() {}

#[derive(OpenApi)]
#[openapi(
    paths(create_playground_share, get_playground_share, revoke_playground_share),
    components(schemas(
        PlaygroundShareMessageRoleDto,
        PlaygroundShareMessageDto,
        PlaygroundShareSessionDto,
        PlaygroundShareCreateRequest,
        PlaygroundShareCreateResponse,
        PlaygroundShareReadResponse,
        ManagementErrorBody
    ))
)]
struct PlaygroundSharesApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    PlaygroundSharesApi::openapi()
}
