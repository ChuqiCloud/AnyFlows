//! Playground 私有会话历史 OpenAPI 契约。

#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    playground_conversations::{
        PlaygroundConversationListResponse, PlaygroundConversationResponse,
        PlaygroundConversationSaveRequest, PlaygroundConversationSummaryDto,
    },
    playground_shares::{
        PlaygroundShareMessageDto, PlaygroundShareMessageRoleDto, PlaygroundShareSessionDto,
    },
};

#[utoipa::path(
    get,
    path = "/api/playground/conversations",
    operation_id = "listPlaygroundConversations",
    tag = "Playground 历史",
    summary = "列出自己的 Playground 会话历史",
    description = "按最近更新时间返回当前登录用户最多 50 条无正文摘要，不读取或返回 API Key、系统提示词、生成参数、用量和错误。",
    responses(
        (status = 200, description = "当前用户的有界历史摘要", body = PlaygroundConversationListResponse),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 500, description = "持久化服务不可用", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_playground_conversations() {}

#[utoipa::path(
    put,
    path = "/api/playground/conversations/{conversation_id}",
    operation_id = "savePlaygroundConversation",
    tag = "Playground 历史",
    summary = "保存自己的 Playground 会话",
    description = "首次保存由客户端提供非零 128 位随机标识且 revision 为空；相同标识与相同内容可幂等重放。后续保存必须携带当前 revision，旧版本冲突失败关闭。标题和模型摘要由服务端派生。",
    params(
        ("conversation_id" = String, Path, min_length = 32, max_length = 32, description = "32 位小写十六进制随机会话标识")
    ),
    request_body = PlaygroundConversationSaveRequest,
    responses(
        (status = 200, description = "创建、更新或幂等重放后的完整会话", body = PlaygroundConversationResponse),
        (status = 400, description = "标识、revision、模型、消息往返或容量无效", body = ManagementErrorBody),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 404, description = "带 revision 的更新目标不存在", body = ManagementErrorBody),
        (status = 409, description = "历史容量达到上限或 revision 冲突", body = ManagementErrorBody),
        (status = 500, description = "持久化服务不可用", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn save_playground_conversation() {}

#[utoipa::path(
    get,
    path = "/api/playground/conversations/{conversation_id}",
    operation_id = "getPlaygroundConversation",
    tag = "Playground 历史",
    summary = "恢复自己的 Playground 会话",
    description = "只在会话属于当前登录用户时返回完整成功往返；管理员角色也不能跨用户读取。",
    params(
        ("conversation_id" = String, Path, min_length = 32, max_length = 32, description = "32 位小写十六进制随机会话标识")
    ),
    responses(
        (status = 200, description = "当前用户拥有的完整会话", body = PlaygroundConversationResponse),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 404, description = "会话不存在或不属于当前用户", body = ManagementErrorBody),
        (status = 500, description = "持久化服务不可用", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_playground_conversation() {}

#[utoipa::path(
    delete,
    path = "/api/playground/conversations/{conversation_id}",
    operation_id = "deletePlaygroundConversation",
    tag = "Playground 历史",
    summary = "删除自己的 Playground 会话",
    description = "物理删除当前登录用户拥有的会话，不影响已经签发的独立只读分享。",
    params(
        ("conversation_id" = String, Path, min_length = 32, max_length = 32, description = "32 位小写十六进制随机会话标识")
    ),
    responses(
        (status = 204, description = "删除成功"),
        (status = 401, description = "登录会话无效", body = ManagementErrorBody),
        (status = 404, description = "会话不存在或不属于当前用户", body = ManagementErrorBody),
        (status = 500, description = "持久化服务不可用", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn delete_playground_conversation() {}

#[derive(OpenApi)]
#[openapi(
    paths(
        list_playground_conversations,
        save_playground_conversation,
        get_playground_conversation,
        delete_playground_conversation
    ),
    components(schemas(
        PlaygroundShareMessageRoleDto,
        PlaygroundShareMessageDto,
        PlaygroundShareSessionDto,
        PlaygroundConversationSaveRequest,
        PlaygroundConversationSummaryDto,
        PlaygroundConversationListResponse,
        PlaygroundConversationResponse,
        ManagementErrorBody
    ))
)]
struct PlaygroundConversationsApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    PlaygroundConversationsApi::openapi()
}
