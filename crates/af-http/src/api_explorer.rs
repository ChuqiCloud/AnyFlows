//! 面向公开首页的 API 目录与受控调试元数据。
//!
//! 目录始终从服务端生成的 OpenAPI 文档投影，不能把完整文档交给浏览器后再
//! 隐藏菜单。这样游客、普通用户、企业成员和管理员拿到的内容在响应边界就
//! 已经不同，前端只负责展示和执行明确标记为可调试的请求。

use std::{
    collections::BTreeSet,
    sync::{Arc, OnceLock},
};

use af_admin::SessionAuthenticator;
use axum::{
    Router,
    extract::{Extension, Path, RawQuery},
    response::Response,
    routing::get,
};
use serde::Serialize;
use serde_json::Value;

use crate::{
    chat_completions::HttpState,
    management_auth::{ManagementAuthenticationState, authenticate_optional_management_session},
    management_error::ManagementError,
    management_session::no_store_json,
    openapi::openapi_document,
};

const PAGE_SIZE_DEFAULT: usize = 24;
const PAGE_SIZE_MAX: usize = 100;
const PAGE_MAX: usize = 10_000;

/// API 目录公开的认证方式；不会把内部凭据名称返回给浏览器。
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum ApiExplorerAuthMode {
    None,
    Session,
    ApiKey,
    Special,
}

/// API 目录公开的访问范围。
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum ApiExplorerAccessScope {
    Public,
    Gateway,
    Session,
    Organization,
    Admin,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
struct ApiExplorerOperation {
    operation_id: String,
    method: String,
    path: String,
    tag: String,
    summary: String,
    description: Option<String>,
    auth_mode: ApiExplorerAuthMode,
    access_scope: ApiExplorerAccessScope,
    debuggable: bool,
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct ApiExplorerCatalogResponse {
    operations: Vec<ApiExplorerOperation>,
    tags: Vec<String>,
    total: usize,
    page: usize,
    page_size: usize,
    has_more: bool,
    viewer: &'static str,
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct ApiExplorerDetailResponse {
    operation: ApiExplorerOperation,
    /// 当前接口的 OpenAPI 细节，仅对已通过目录权限投影的 operation 返回。
    openapi: Value,
}

#[derive(Clone, Copy)]
struct Viewer {
    authenticated: bool,
    admin: bool,
}

#[derive(Clone)]
struct ProjectedOperation {
    metadata: ApiExplorerOperation,
    openapi: Value,
}

#[derive(Debug, Default)]
struct CatalogQuery {
    q: Option<String>,
    tag: Option<String>,
    page: Option<usize>,
    page_size: Option<usize>,
}

/// 将路由挂到公开路由树；中间件允许游客访问，但带来的 Bearer JWT 仍会被严格校验。
pub(crate) fn build_api_explorer_router(
    session_authenticator: Arc<dyn SessionAuthenticator>,
    state: HttpState,
) -> Router {
    Router::new()
        .route("/api/explorer/catalog", get(list_catalog))
        .route("/api/explorer/catalog/{operation_id}", get(get_operation))
        .layer(axum::middleware::from_fn_with_state(
            ManagementAuthenticationState::new(session_authenticator),
            authenticate_optional_management_session,
        ))
        .with_state(state)
}

async fn list_catalog(
    RawQuery(raw_query): RawQuery,
    authentication: Option<Extension<af_admin::SessionAuthentication>>,
) -> Result<Response, ManagementError> {
    let query = parse_catalog_query(raw_query.as_deref())?;
    let viewer = viewer(authentication.as_ref());
    let page = query.page.unwrap_or(1);
    let page_size = query.page_size.unwrap_or(PAGE_SIZE_DEFAULT);
    if page == 0 || page > PAGE_MAX || page_size == 0 || page_size > PAGE_SIZE_MAX {
        return Err(ManagementError::InvalidRequest);
    }
    let query_text = normalize_filter(query.q.as_deref())?;
    let tag_filter = normalize_filter(query.tag.as_deref())?;
    let mut operations = projected_operations(viewer);
    operations.retain(|item| {
        let matches_query = query_text.as_deref().is_none_or(|query| {
            let haystack = format!(
                "{} {} {} {}",
                item.metadata.operation_id,
                item.metadata.path,
                item.metadata.summary,
                item.metadata.description.as_deref().unwrap_or_default(),
            )
            .to_ascii_lowercase();
            haystack.contains(query)
        });
        let matches_tag = tag_filter
            .as_deref()
            .is_none_or(|tag| item.metadata.tag.eq_ignore_ascii_case(tag));
        matches_query && matches_tag
    });

    let total = operations.len();
    let tags = operations
        .iter()
        .map(|item| item.metadata.tag.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let start = page.saturating_sub(1).saturating_mul(page_size);
    let has_more = start.saturating_add(page_size) < total;
    let operations = operations
        .into_iter()
        .skip(start)
        .take(page_size)
        .map(|item| item.metadata)
        .collect();

    Ok(no_store_json(ApiExplorerCatalogResponse {
        operations,
        tags,
        total,
        page,
        page_size,
        has_more,
        viewer: viewer_label(viewer),
    }))
}

fn parse_catalog_query(raw_query: Option<&str>) -> Result<CatalogQuery, ManagementError> {
    let Some(raw_query) = raw_query else {
        return Ok(CatalogQuery::default());
    };
    if raw_query.is_empty() {
        return Ok(CatalogQuery::default());
    }
    if raw_query.len() > 1_024 {
        return Err(ManagementError::InvalidRequest);
    }
    validate_percent_encoding(raw_query)?;
    let mut query = CatalogQuery::default();
    for (key, value) in url::form_urlencoded::parse(raw_query.as_bytes()) {
        if key.contains('\u{fffd}') || value.contains('\u{fffd}') {
            return Err(ManagementError::InvalidRequest);
        }
        match key.as_ref() {
            "q" if query.q.is_none() => query.q = Some(value.into_owned()),
            "tag" if query.tag.is_none() => query.tag = Some(value.into_owned()),
            "page" if query.page.is_none() => query.page = Some(parse_catalog_number(&value)?),
            "page_size" if query.page_size.is_none() => {
                query.page_size = Some(parse_catalog_number(&value)?)
            }
            _ => return Err(ManagementError::InvalidRequest),
        }
    }
    Ok(query)
}

fn parse_catalog_number(value: &str) -> Result<usize, ManagementError> {
    if value.is_empty()
        || value.starts_with(['+', '-'])
        || value.chars().any(|character| !character.is_ascii_digit())
    {
        return Err(ManagementError::InvalidRequest);
    }
    value
        .parse::<usize>()
        .map_err(|_| ManagementError::InvalidRequest)
}

fn validate_percent_encoding(raw_query: &str) -> Result<(), ManagementError> {
    let bytes = raw_query.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len()
                || !bytes[index + 1].is_ascii_hexdigit()
                || !bytes[index + 2].is_ascii_hexdigit()
            {
                return Err(ManagementError::InvalidRequest);
            }
            index += 3;
        } else {
            index += 1;
        }
    }
    Ok(())
}

async fn get_operation(
    Path(operation_id): Path<String>,
    authentication: Option<Extension<af_admin::SessionAuthentication>>,
) -> Result<Response, ManagementError> {
    if operation_id.is_empty() || operation_id.len() > 128 {
        return Err(ManagementError::InvalidRequest);
    }
    let viewer = viewer(authentication.as_ref());
    let Some(operation) = projected_operations(viewer)
        .into_iter()
        .find(|item| item.metadata.operation_id == operation_id)
    else {
        // 对不存在和无权访问的 operation 使用同一个结果，避免目录枚举泄漏。
        return Err(ManagementError::InvalidRequest);
    };
    Ok(no_store_json(ApiExplorerDetailResponse {
        operation: operation.metadata,
        openapi: operation.openapi,
    }))
}

fn normalize_filter(value: Option<&str>) -> Result<Option<String>, ManagementError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    if value.chars().count() > 128 {
        return Err(ManagementError::InvalidRequest);
    }
    Ok(Some(value.to_ascii_lowercase()))
}

fn viewer(authentication: Option<&Extension<af_admin::SessionAuthentication>>) -> Viewer {
    let Some(Extension(authentication)) = authentication else {
        return Viewer {
            authenticated: false,
            admin: false,
        };
    };
    Viewer {
        authenticated: true,
        admin: authentication.principal().role() == af_admin::SessionRole::Admin,
    }
}

const fn viewer_label(viewer: Viewer) -> &'static str {
    if viewer.admin {
        "admin"
    } else if viewer.authenticated {
        "user"
    } else {
        "guest"
    }
}

fn projected_operations(viewer: Viewer) -> Vec<ProjectedOperation> {
    let document = openapi_value();
    let Some(paths) = document.get("paths").and_then(Value::as_object) else {
        return Vec::new();
    };
    let mut operations = Vec::new();
    for (path, path_item) in paths {
        let Some(path_item) = path_item.as_object() else {
            continue;
        };
        for method in ["get", "post", "put", "patch", "delete", "head", "options"] {
            let Some(operation) = path_item.get(method).filter(|value| value.is_object()) else {
                continue;
            };
            let Some(operation_id) = operation.get("operationId").and_then(Value::as_str) else {
                continue;
            };
            let (auth_mode, scope) = classify_operation(path, operation);
            if !is_visible(viewer, scope, auth_mode) {
                continue;
            }
            let metadata = ApiExplorerOperation {
                operation_id: operation_id.to_owned(),
                method: method.to_ascii_uppercase(),
                path: path.to_owned(),
                tag: operation
                    .get("tags")
                    .and_then(Value::as_array)
                    .and_then(|tags| tags.first())
                    .and_then(Value::as_str)
                    .unwrap_or("其他")
                    .to_owned(),
                summary: operation
                    .get("summary")
                    .and_then(Value::as_str)
                    .unwrap_or(operation_id)
                    .to_owned(),
                description: operation
                    .get("description")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                auth_mode,
                access_scope: scope,
                debuggable: is_debuggable(path, method, scope, auth_mode),
            };
            operations.push(ProjectedOperation {
                metadata,
                openapi: operation.clone(),
            });
        }
    }
    // OpenAPI 当前主要描述管理端与部分网关契约；这里补入实际注册但尚未由
    // utoipa 输出的兼容网关入口，保证首页目录与可调用路由保持一致。
    for operation in gateway_operations() {
        if is_visible(
            viewer,
            operation.metadata.access_scope,
            operation.metadata.auth_mode,
        ) {
            operations.push(operation);
        }
    }
    operations.sort_by(|left, right| {
        left.metadata
            .tag
            .cmp(&right.metadata.tag)
            .then_with(|| left.metadata.path.cmp(&right.metadata.path))
            .then_with(|| left.metadata.method.cmp(&right.metadata.method))
            .then_with(|| left.metadata.operation_id.cmp(&right.metadata.operation_id))
    });
    operations
}

fn gateway_operations() -> Vec<ProjectedOperation> {
    [
        (
            "chatCompletions",
            "POST",
            "/v1/chat/completions",
            "OpenAI Chat",
            "OpenAI Chat Completions",
        ),
        (
            "createEmbedding",
            "POST",
            "/v1/embeddings",
            "OpenAI Embeddings",
            "创建文本向量",
        ),
        (
            "createImage",
            "POST",
            "/v1/images/generations",
            "OpenAI Images",
            "生成图片",
        ),
        (
            "createTranscription",
            "POST",
            "/v1/audio/transcriptions",
            "OpenAI Audio",
            "转录音频",
        ),
        (
            "createResponse",
            "POST",
            "/v1/responses",
            "OpenAI Responses",
            "创建 Responses 响应",
        ),
        (
            "createMessage",
            "POST",
            "/v1/messages",
            "Anthropic",
            "创建 Anthropic 消息",
        ),
        (
            "generateGeminiContent",
            "POST",
            "/v1beta/models/{model_action}",
            "Gemini",
            "生成 Gemini 内容",
        ),
    ]
    .into_iter()
    .map(
        |(operation_id, method, path, tag, summary)| ProjectedOperation {
            metadata: ApiExplorerOperation {
                operation_id: operation_id.to_owned(),
                method: method.to_owned(),
                path: path.to_owned(),
                tag: tag.to_owned(),
                summary: summary.to_owned(),
                description: Some(
                    "兼容模型网关入口，使用 API Key 鉴权。请求正文按对应协议传入 JSON。".to_owned(),
                ),
                auth_mode: ApiExplorerAuthMode::ApiKey,
                access_scope: ApiExplorerAccessScope::Gateway,
                debuggable: true,
            },
            openapi: serde_json::json!({
                "operationId": operation_id,
                "summary": summary,
                "tags": [tag],
                "security": [{"apiKeyAuth": []}],
                "parameters": if path.contains("{model_action}") {
                    serde_json::json!([{
                        "name": "model_action",
                        "in": "path",
                        "required": true,
                        "schema": {"type": "string"}
                    }])
                } else {
                    serde_json::json!([])
                },
                "requestBody": {
                    "required": true,
                    "content": {"application/json": {"schema": {"type": "object"}}}
                },
                "responses": {
                    "200": {"description": "网关响应"},
                    "400": {"description": "请求无效"},
                    "401": {"description": "API Key 无效"},
                    "429": {"description": "请求受限"},
                    "500": {"description": "网关处理失败"}
                }
            }),
        },
    )
    .collect()
}

fn openapi_value() -> &'static Value {
    static DOCUMENT: OnceLock<Value> = OnceLock::new();
    DOCUMENT
        .get_or_init(|| serde_json::to_value(openapi_document()).expect("OpenAPI 文档必须可序列化"))
}

fn classify_operation(
    path: &str,
    operation: &Value,
) -> (ApiExplorerAuthMode, ApiExplorerAccessScope) {
    let auth_mode = security_mode(operation);
    let scope = if path.starts_with("/api/admin/")
        || path.starts_with("/api/platform/")
        || path.starts_with("/scim/")
    {
        ApiExplorerAccessScope::Admin
    } else if path.starts_with("/api/organizations/")
        || path.starts_with("/api/account/organization")
    {
        ApiExplorerAccessScope::Organization
    } else if auth_mode == ApiExplorerAuthMode::ApiKey || path.starts_with("/v1/") {
        ApiExplorerAccessScope::Gateway
    } else if auth_mode == ApiExplorerAuthMode::Session {
        ApiExplorerAccessScope::Session
    } else {
        ApiExplorerAccessScope::Public
    };
    (auth_mode, scope)
}

fn security_mode(operation: &Value) -> ApiExplorerAuthMode {
    let Some(security) = operation.get("security") else {
        return ApiExplorerAuthMode::None;
    };
    let Some(requirements) = security.as_array() else {
        return ApiExplorerAuthMode::Special;
    };
    if requirements
        .iter()
        .any(|item| item.as_object().is_some_and(|item| item.is_empty()))
    {
        return ApiExplorerAuthMode::None;
    }
    if requirements
        .iter()
        .any(|item| item.get("apiKeyAuth").is_some())
    {
        return ApiExplorerAuthMode::ApiKey;
    }
    if requirements
        .iter()
        .any(|item| item.get("bearerAuth").is_some())
    {
        return ApiExplorerAuthMode::Session;
    }
    ApiExplorerAuthMode::Special
}

fn is_visible(
    viewer: Viewer,
    scope: ApiExplorerAccessScope,
    auth_mode: ApiExplorerAuthMode,
) -> bool {
    if viewer.admin {
        return true;
    }
    match scope {
        ApiExplorerAccessScope::Public | ApiExplorerAccessScope::Gateway => true,
        ApiExplorerAccessScope::Session | ApiExplorerAccessScope::Organization => {
            viewer.authenticated && auth_mode == ApiExplorerAuthMode::Session
        }
        ApiExplorerAccessScope::Admin => false,
    }
}

fn is_debuggable(
    path: &str,
    method: &str,
    scope: ApiExplorerAccessScope,
    auth_mode: ApiExplorerAuthMode,
) -> bool {
    if auth_mode == ApiExplorerAuthMode::Special
        || path.contains("/webhook")
        || path.starts_with("/api/auth/oauth/")
        || path.starts_with("/api/auth/organization-sso/")
        || path.starts_with("/api/setup")
    {
        return false;
    }
    if method == "GET" {
        return true;
    }
    // 网关协议的 POST 是 API 页面最核心的调试场景；管理端写入保持文档-only。
    scope == ApiExplorerAccessScope::Gateway && method == "POST" && path.starts_with("/v1/")
}

#[cfg(test)]
mod verification_visibility_tests {
    use super::*;

    #[test]
    fn account_verification_operations_follow_viewer_permissions() {
        let guest = Viewer {
            authenticated: false,
            admin: false,
        };
        let user = Viewer {
            authenticated: true,
            admin: false,
        };
        let admin = Viewer {
            authenticated: true,
            admin: true,
        };
        let contains = |viewer, id: &str| {
            projected_operations(viewer)
                .iter()
                .any(|entry| entry.metadata.operation_id == id)
        };
        for id in [
            "listAccountVerifications",
            "getAccountVerification",
            "downloadAccountVerificationMaterial",
        ] {
            assert!(!contains(guest, id), "{id}");
            assert!(contains(user, id), "{id}");
        }
        for id in [
            "listAdminAccountVerifications",
            "getAdminAccountVerification",
            "decideAccountVerification",
        ] {
            assert!(!contains(guest, id), "{id}");
            assert!(!contains(user, id), "{id}");
            assert!(contains(admin, id), "{id}");
        }
    }
}
