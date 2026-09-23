//! 企业 SSO Provider 管理接口的 OpenAPI 契约。
#![allow(dead_code, reason = "文档端点仅由 utoipa 过程宏读取")]

use utoipa::OpenApi;

use crate::{
    management_error::ManagementErrorBody,
    management_session::LoginResponse,
    organization_sso_login::{
        OrganizationSsoDiscoveryErrorResponse, OrganizationSsoDiscoveryRequest,
        OrganizationSsoDiscoveryResponse, OrganizationSsoLoginCallbackErrorResponse,
        OrganizationSsoLoginCallbackSuccessResponse, OrganizationSsoPublicLoginStartErrorResponse,
        OrganizationSsoPublicLoginStartRequest, OrganizationSsoPublicLoginStartResponse,
        OrganizationSsoSessionExchangeErrorResponse, OrganizationSsoSessionExchangeRequest,
    },
    organizations::{
        OrganizationSsoDomainCreateRequest, OrganizationSsoDomainCreatedResponse,
        OrganizationSsoDomainListResponse, OrganizationSsoDomainResponse,
        OrganizationSsoDomainVerifyRequest, OrganizationSsoIdentityBindingRequest,
        OrganizationSsoIdentityBindingResponse, OrganizationSsoLoginStartRequest,
        OrganizationSsoLoginStartResponse, OrganizationSsoPolicyRequest,
        OrganizationSsoPolicyResponse, OrganizationSsoProviderListResponse,
        OrganizationSsoProviderRequest, OrganizationSsoProviderResponse,
        OrganizationSsoRecoveryRequest, OrganizationSsoRecoveryResponse,
        OrganizationSsoSamlAuthnRequestFieldResponse, OrganizationSsoSamlAuthnRequestResponse,
    },
};

#[utoipa::path(
    post,
    path = "/api/auth/organization-sso/discovery",
    operation_id = "discoverOrganizationSso",
    tag = "企业 SSO",
    summary = "公开发现企业 SSO",
    description = "仅接受企业公开标识或邮箱并返回通用可用性。未知、停用、过期、冲突或损坏事实均统一为 available=false，不返回企业、域名或 Provider 资料。",
    request_body = OrganizationSsoDiscoveryRequest,
    responses(
        (status = 200, description = "返回通用企业 SSO 可用性", body = OrganizationSsoDiscoveryResponse),
        (status = 400, description = "发现请求无效", body = OrganizationSsoDiscoveryErrorResponse),
        (status = 503, description = "发现服务暂不可用", body = OrganizationSsoDiscoveryErrorResponse)
    )
)]
fn discover_organization_sso() {}

#[utoipa::path(
    post,
    path = "/api/auth/organization-sso/login/start",
    operation_id = "startPublicOrganizationSsoLogin",
    tag = "企业 SSO",
    summary = "公开启动企业 SSO 登录",
    description = "服务端重新解析企业公开标识或邮箱，复验企业、授权、Provider 和域名事实后创建一次性事务。OIDC 仅返回受控授权地址，SAML 仅返回受控 POST 材料；响应不包含企业、Provider、配置版本或摘要。",
    request_body = OrganizationSsoPublicLoginStartRequest,
    responses(
        (status = 200, description = "返回受控浏览器登录动作", body = OrganizationSsoPublicLoginStartResponse),
        (status = 400, description = "启动请求无效", body = OrganizationSsoPublicLoginStartErrorResponse),
        (status = 422, description = "企业 SSO 当前不可用", body = OrganizationSsoPublicLoginStartErrorResponse),
        (status = 503, description = "登录启动服务暂不可用", body = OrganizationSsoPublicLoginStartErrorResponse)
    )
)]
fn start_public_organization_sso_login() {}

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
        (status = 200, description = "返回登录事务摘要和受控 OIDC 或 SAML 浏览器动作", body = OrganizationSsoLoginStartResponse),
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
    path = "/api/organizations/{organization_id}/sso/policy",
    operation_id = "getOrganizationSsoPolicy",
    tag = "企业 SSO",
    summary = "读取企业 SSO 强制策略",
    params(("organization_id" = i64, Path, minimum = 1, description = "企业内部标识")),
    responses(
        (status = 200, description = "企业 SSO 策略与恢复就绪状态", body = OrganizationSsoPolicyResponse),
        (status = 400, description = "企业标识无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "无权查看企业 SSO 策略", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody),
        (status = 503, description = "企业 SSO 策略暂不可用", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn get_organization_sso_policy() {}

#[utoipa::path(
    put,
    path = "/api/organizations/{organization_id}/sso/policy",
    operation_id = "updateOrganizationSsoPolicy",
    tag = "企业 SSO",
    summary = "按版本 CAS 更新企业 SSO 强制策略",
    params(("organization_id" = i64, Path, minimum = 1, description = "企业内部标识")),
    request_body = OrganizationSsoPolicyRequest,
    responses(
        (status = 200, description = "企业 SSO 策略已更新", body = OrganizationSsoPolicyResponse),
        (status = 400, description = "策略参数无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "无权更新企业 SSO 策略", body = ManagementErrorBody),
        (status = 409, description = "策略版本冲突", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody),
        (status = 503, description = "企业 SSO 策略暂不可用", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn update_organization_sso_policy() {}

#[utoipa::path(
    post,
    path = "/api/organizations/{organization_id}/sso/recovery/establish",
    operation_id = "establishOrganizationSsoRecovery",
    tag = "企业 SSO",
    summary = "完成企业 SSO Owner break-glass 恢复登记",
    description = "要求当前企业 Owner 使用当前密码与 TOTP/备用码完成 step-up。响应仅返回脱敏策略快照，不包含密码、TOTP 或恢复材料。",
    params(("organization_id" = i64, Path, minimum = 1, description = "企业内部标识")),
    request_body = OrganizationSsoRecoveryRequest,
    responses(
        (status = 200, description = "恢复登记已完成", body = OrganizationSsoRecoveryResponse),
        (status = 400, description = "恢复请求参数无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户不是企业 Owner 或缺少恢复权限", body = ManagementErrorBody),
        (status = 409, description = "企业授权或策略版本不允许本次操作", body = ManagementErrorBody),
        (status = 422, description = "当前密码或二次因子未通过校验", body = ManagementErrorBody),
        (status = 503, description = "恢复事实暂不可用", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn establish_organization_sso_recovery() {}

#[utoipa::path(
    post,
    path = "/api/organizations/{organization_id}/sso/recovery/suspend-required",
    operation_id = "suspendOrganizationSsoRequired",
    tag = "企业 SSO",
    summary = "暂停企业 SSO Required 策略",
    description = "要求当前企业 Owner 使用当前密码与 TOTP/备用码完成 step-up。仅将 Required 暂停为 Optional，不删除恢复事实。",
    params(("organization_id" = i64, Path, minimum = 1, description = "企业内部标识")),
    request_body = OrganizationSsoRecoveryRequest,
    responses(
        (status = 200, description = "Required 策略已暂停", body = OrganizationSsoRecoveryResponse),
        (status = 400, description = "恢复请求参数无效", body = ManagementErrorBody),
        (status = 401, description = "会话无效", body = ManagementErrorBody),
        (status = 403, description = "当前用户不是企业 Owner 或缺少恢复权限", body = ManagementErrorBody),
        (status = 409, description = "企业授权或策略版本不允许本次操作", body = ManagementErrorBody),
        (status = 422, description = "当前密码或二次因子未通过校验", body = ManagementErrorBody),
        (status = 503, description = "恢复事实暂不可用", body = ManagementErrorBody),
        (status = 500, description = "服务内部错误", body = ManagementErrorBody)
    ),
    security(("bearerAuth" = []))
)]
fn suspend_organization_sso_required() {}

#[utoipa::path(
    get,
    path = "/api/auth/organization-sso/oidc/callback",
    operation_id = "completeOrganizationSsoOidcLogin",
    tag = "企业 SSO",
    summary = "接收企业 OIDC 回调",
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
    tag = "企业 SSO",
    summary = "接收企业 SAML 回调",
    request_body = String,
    responses(
        (status = 400, description = "回调表单无效", body = OrganizationSsoLoginCallbackErrorResponse),
        (status = 422, description = "回调被拒绝", body = OrganizationSsoLoginCallbackErrorResponse),
        (status = 503, description = "回调暂不可用", body = OrganizationSsoLoginCallbackErrorResponse)
    )
)]
fn complete_organization_sso_saml_login() {}

#[utoipa::path(
    post,
    path = "/api/auth/organization-sso/session/exchange",
    operation_id = "exchangeOrganizationSsoSession",
    tag = "企业 SSO",
    summary = "交换企业 SSO ticket 为同源站点会话",
    description = "仅消费一次性有效 OIDC ticket，重新校验企业授权、组织边界、身份绑定和域名状态后签发既有 bearer 会话。请求和响应均不记录 ticket、subject 或 Provider 原文，此路径不支持 SAML、强制策略和恢复入口。",
    request_body = OrganizationSsoSessionExchangeRequest,
    responses(
        (status = 200, description = "已成功换取站点会话", body = LoginResponse),
        (status = 400, description = "请求无效", body = OrganizationSsoSessionExchangeErrorResponse),
        (status = 422, description = "ticket 或 SSO 身份已被拒绝", body = OrganizationSsoSessionExchangeErrorResponse),
        (status = 503, description = "SSO 服务暂不可用", body = OrganizationSsoSessionExchangeErrorResponse)
    )
)]
fn exchange_organization_sso_session() {}

#[derive(OpenApi)]
#[openapi(
    paths(
        discover_organization_sso,
        start_public_organization_sso_login,
        list_organization_sso_providers,
        create_organization_sso_provider,
        get_organization_sso_provider,
        update_organization_sso_provider,
        list_organization_sso_domains,
        create_organization_sso_domain,
        verify_organization_sso_domain,
        start_organization_sso_login,
        bind_organization_sso_identity,
        get_organization_sso_policy,
        update_organization_sso_policy,
        establish_organization_sso_recovery,
        suspend_organization_sso_required,
        complete_organization_sso_oidc_login,
        complete_organization_sso_saml_login,
        exchange_organization_sso_session
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
        OrganizationSsoSamlAuthnRequestResponse,
        OrganizationSsoSamlAuthnRequestFieldResponse,
        OrganizationSsoIdentityBindingRequest,
        OrganizationSsoIdentityBindingResponse,
        OrganizationSsoPolicyRequest,
        OrganizationSsoPolicyResponse,
        OrganizationSsoRecoveryRequest,
        OrganizationSsoRecoveryResponse,
        OrganizationSsoDiscoveryRequest,
        OrganizationSsoDiscoveryResponse,
        OrganizationSsoDiscoveryErrorResponse,
        OrganizationSsoPublicLoginStartRequest,
        OrganizationSsoPublicLoginStartResponse,
        OrganizationSsoPublicLoginStartErrorResponse,
        OrganizationSsoLoginCallbackErrorResponse,
        OrganizationSsoLoginCallbackSuccessResponse,
        OrganizationSsoSessionExchangeRequest,
        OrganizationSsoSessionExchangeErrorResponse,
        LoginResponse,
        ManagementErrorBody
    ))
)]
struct OrganizationSsoApi;

pub(super) fn document() -> utoipa::openapi::OpenApi {
    OrganizationSsoApi::openapi()
}
