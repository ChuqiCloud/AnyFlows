//! 企业 SSO Provider 管理接口的 OpenAPI 契约。
#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    organization_sso_login::{
        OrganizationSsoLoginCallbackErrorResponse, OrganizationSsoLoginCallbackSuccessResponse,
    },
    organizations::{
        OrganizationSsoDomainCreateRequest, OrganizationSsoDomainCreatedResponse,
        OrganizationSsoDomainListResponse, OrganizationSsoDomainResponse,
        OrganizationSsoDomainVerifyRequest, OrganizationSsoIdentityBindingRequest,
        OrganizationSsoIdentityBindingResponse, OrganizationSsoLoginStartRequest,
        OrganizationSsoLoginStartResponse, OrganizationSsoProviderListResponse,
        OrganizationSsoProviderRequest, OrganizationSsoProviderResponse,
    },
};

#[utoipa::path(
    get,
    path = "/api/organizations/{organization_id}/sso/providers",
    operation_id = "listOrganizationSsoProviders",
    tag = "企业 SSO",
    summary = "列出企业 SSO Provider",
    params(("organization_id" = i64, Path, minimum = 1, description = "企业内部标识")),
    responses(
        (status = 200, description = "企业 SSO Provider 脱敏列表", body = OrganizationSsoProviderListResponse),
        (status = 400, description = "企业标识无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "无权查看企业 SSO Provider", body = ManagementErrorBody),
        (status = 404, description = "企业不存在或无权访问", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_organization_sso_providers() {}

#[utoipa::path(
    post,
    path = "/api/organizations/{organization_id}/sso/providers",
    operation_id = "createOrganizationSsoProvider",
    tag = "企业 SSO",
    summary = "创建企业 SSO Provider",
    params(("organization_id" = i64, Path, minimum = 1, description = "企业内部标识")),
    request_body = OrganizationSsoProviderRequest,
    responses(
        (status = 200, description = "企业 SSO Provider 已创建", body = OrganizationSsoProviderResponse),
        (status = 400, description = "Provider 参数无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "无权管理企业 SSO Provider", body = ManagementErrorBody),
        (status = 404, description = "企业不存在或无权访问", body = ManagementErrorBody),
        (status = 409, description = "Provider 版本或状态冲突", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn create_organization_sso_provider() {}

#[utoipa::path(
    get,
    path = "/api/organizations/{organization_id}/sso/providers/{provider_key}",
    operation_id = "getOrganizationSsoProvider",
    tag = "企业 SSO",
    summary = "读取企业 SSO Provider",
    params(
        ("organization_id" = i64, Path, minimum = 1, description = "企业内部标识"),
        ("provider_key" = String, Path, description = "Provider 稳定标识")
    ),
    responses(
        (status = 200, description = "企业 SSO Provider 脱敏详情", body = OrganizationSsoProviderResponse),
        (status = 400, description = "企业或 Provider 标识无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "无权查看企业 SSO Provider", body = ManagementErrorBody),
        (status = 404, description = "Provider 不存在或无权访问", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_organization_sso_provider() {}

#[utoipa::path(
    put,
    path = "/api/organizations/{organization_id}/sso/providers/{provider_key}",
    operation_id = "updateOrganizationSsoProvider",
    tag = "企业 SSO",
    summary = "按版本 CAS 更新企业 SSO Provider",
    params(
        ("organization_id" = i64, Path, minimum = 1, description = "企业内部标识"),
        ("provider_key" = String, Path, description = "Provider 稳定标识")
    ),
    request_body = OrganizationSsoProviderRequest,
    responses(
        (status = 200, description = "企业 SSO Provider 已更新", body = OrganizationSsoProviderResponse),
        (status = 400, description = "Provider 参数无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "无权管理企业 SSO Provider", body = ManagementErrorBody),
        (status = 404, description = "Provider 不存在或无权访问", body = ManagementErrorBody),
        (status = 409, description = "Provider 版本或状态冲突", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn update_organization_sso_provider() {}

#[utoipa::path(
    get,
    path = "/api/organizations/{organization_id}/sso/domains",
    operation_id = "listOrganizationSsoDomains",
    tag = "企业 SSO",
    summary = "列出企业 SSO 域名",
    params(("organization_id" = i64, Path, minimum = 1, description = "企业内部标识")),
    responses(
        (status = 200, description = "企业 SSO 域名脱敏列表", body = OrganizationSsoDomainListResponse),
        (status = 400, description = "企业标识无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "无权查看企业 SSO 域名", body = ManagementErrorBody),
        (status = 404, description = "企业不存在或无权访问", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn list_organization_sso_domains() {}

#[utoipa::path(
    post,
    path = "/api/organizations/{organization_id}/sso/domains",
    operation_id = "createOrganizationSsoDomain",
    tag = "企业 SSO",
    summary = "创建企业 SSO 域名证明",
    params(("organization_id" = i64, Path, minimum = 1, description = "企业内部标识")),
    request_body = OrganizationSsoDomainCreateRequest,
    responses(
        (status = 200, description = "域名证明已创建；TXT 值仅在本次响应返回", body = OrganizationSsoDomainCreatedResponse),
        (status = 400, description = "域名或证明参数无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "无权管理企业 SSO 域名", body = ManagementErrorBody),
        (status = 404, description = "企业或 Provider 不存在", body = ManagementErrorBody),
        (status = 409, description = "域名已归属其他企业或状态冲突", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn create_organization_sso_domain() {}

#[utoipa::path(
    post,
    path = "/api/organizations/{organization_id}/sso/domains/{domain_id}/verify",
    operation_id = "verifyOrganizationSsoDomain",
    tag = "企业 SSO",
    summary = "服务端验证企业 SSO 域名",
    params(
        ("organization_id" = i64, Path, minimum = 1, description = "企业内部标识"),
        ("domain_id" = i64, Path, minimum = 1, description = "企业域名 ID")
    ),
    request_body = OrganizationSsoDomainVerifyRequest,
    responses(
        (status = 200, description = "域名已通过服务端 DNS 验证", body = OrganizationSsoDomainResponse),
        (status = 400, description = "验证参数无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "无权管理企业 SSO 域名", body = ManagementErrorBody),
        (status = 404, description = "域名不存在或无权访问", body = ManagementErrorBody),
        (status = 409, description = "域名版本或状态冲突", body = ManagementErrorBody),
        (status = 422, description = "服务端 DNS TXT 证明失败", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn verify_organization_sso_domain() {}

#[utoipa::path(
    post,
    path = "/api/organizations/{organization_id}/sso/login/start",
    operation_id = "startOrganizationSsoLogin",
    tag = "企业 SSO",
    summary = "启动企业 SSO 登录事务",
    params(("organization_id" = i64, Path, minimum = 1, description = "企业内部标识")),
    request_body = OrganizationSsoLoginStartRequest,
    responses(
        (status = 200, description = "返回脱敏的登录事务摘要", body = OrganizationSsoLoginStartResponse),
        (status = 400, description = "登录事务参数无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "无权启动企业 SSO 登录", body = ManagementErrorBody),
        (status = 404, description = "Provider 不存在或无权访问", body = ManagementErrorBody),
        (status = 409, description = "Provider 状态或登录事务冲突", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn start_organization_sso_login() {}

#[utoipa::path(
    post,
    path = "/api/organizations/{organization_id}/sso/identities/bind",
    operation_id = "bindOrganizationSsoIdentity",
    tag = "企业 SSO",
    summary = "完成企业 SSO 显式身份绑定",
    description = "要求当前登录会话和一次性 OIDC ticket，并使用密码加二次因素或 Passkey 完成 step-up。响应不包含 subject、ticket 或原始认证材料。",
    params(("organization_id" = i64, Path, minimum = 1, description = "企业内部标识")),
    request_body = OrganizationSsoIdentityBindingRequest,
    responses(
        (status = 200, description = "身份绑定已完成", body = OrganizationSsoIdentityBindingResponse),
        (status = 400, description = "绑定请求或 step-up 参数无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户不是企业活动成员", body = ManagementErrorBody),
        (status = 409, description = "身份绑定已存在或发生并发冲突", body = ManagementErrorBody),
        (status = 422, description = "ticket 或 step-up 未通过校验", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn bind_organization_sso_identity() {}

#[utoipa::path(
    get,
    path = "/api/auth/organization-sso/oidc/callback",
    operation_id = "completeOrganizationSsoOidcLogin",
    tag = "浼佷笟 SSO",
    summary = "鎺ユ敹浼佷笟 OIDC 鍥炶皟",
    params(
        ("code" = String, Query, description = "OIDC 授权码"),
        ("state" = String, Query, description = "一次性登录 state")
    ),
    responses(
        (status = 200, description = "OIDC 回调验证成功", body = OrganizationSsoLoginCallbackSuccessResponse),
        (status = 400, description = "回调参数无效", body = OrganizationSsoLoginCallbackErrorResponse),
        (status = 422, description = "回调被拒绝", body = OrganizationSsoLoginCallbackErrorResponse),
        (status = 503, description = "回调暂不可用", body = OrganizationSsoLoginCallbackErrorResponse)
    )
)]
fn complete_organization_sso_oidc_login() {}

#[utoipa::path(
    post,
    path = "/api/auth/organization-sso/saml/callback",
    operation_id = "completeOrganizationSsoSamlLogin",
    tag = "浼佷笟 SSO",
    summary = "鎺ユ敹浼佷笟 SAML 鍥炶皟",
    request_body = String,
    responses(
        (status = 400, description = "回调表单无效", body = OrganizationSsoLoginCallbackErrorResponse),
        (status = 422, description = "回调被拒绝", body = OrganizationSsoLoginCallbackErrorResponse),
        (status = 503, description = "回调暂不可用", body = OrganizationSsoLoginCallbackErrorResponse)
    )
)]
fn complete_organization_sso_saml_login() {}

#[derive(OpenApi)]
#[openapi(
    paths(
        list_organization_sso_providers,
        create_organization_sso_provider,
        get_organization_sso_provider,
        update_organization_sso_provider,
        list_organization_sso_domains,
        create_organization_sso_domain,
        verify_organization_sso_domain,
        start_organization_sso_login,
        bind_organization_sso_identity,
        complete_organization_sso_oidc_login,
        complete_organization_sso_saml_login
    ),
    components(schemas(
        OrganizationSsoProviderResponse,
        OrganizationSsoProviderListResponse,
        OrganizationSsoProviderRequest,
        OrganizationSsoDomainResponse,
        OrganizationSsoDomainListResponse,
        OrganizationSsoDomainCreatedResponse,
        OrganizationSsoDomainCreateRequest,
        OrganizationSsoDomainVerifyRequest,
        OrganizationSsoLoginStartRequest,
        OrganizationSsoLoginStartResponse,
        OrganizationSsoIdentityBindingRequest,
        OrganizationSsoIdentityBindingResponse,
        OrganizationSsoLoginCallbackErrorResponse,
        OrganizationSsoLoginCallbackSuccessResponse,
        ManagementErrorBody
    ))
)]
struct OrganizationSsoApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    OrganizationSsoApi::openapi()
}
