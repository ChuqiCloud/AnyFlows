use std::sync::Arc;

use af_admin::{
    ClaimedOrganizationSsoLoginTransaction, ORGANIZATION_SSO_OIDC_CALLBACK_PATH,
    ORGANIZATION_SSO_SAML_CALLBACK_PATH, OrganizationSsoDiscovery,
    OrganizationSsoDiscoveryServiceError, OrganizationSsoLoginTransactionService,
    OrganizationSsoLoginTransactionServiceError, OrganizationSsoOidcCallback,
    OrganizationSsoSamlCallback, OrganizationSsoSessionService, OrganizationSsoSessionServiceError,
};
use af_domain::{OrganizationId, OrganizationSsoProtocol};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, RawQuery, State, rejection::JsonRejection},
    response::Response,
    routing::{get, post},
};
use http::{HeaderValue, StatusCode, header::CACHE_CONTROL};
use serde::Serialize;
use tower_http::limit::RequestBodyLimitLayer;
use utoipa::ToSchema;

use crate::management_session::no_store_json;

const MAX_SAML_CALLBACK_BODY_BYTES: usize = 256 * 1024;
const ORGANIZATION_SSO_SESSION_EXCHANGE_PATH: &str = "/api/auth/organization-sso/session/exchange";
const ORGANIZATION_SSO_DISCOVERY_PATH: &str = "/api/auth/organization-sso/discovery";

#[derive(Clone)]
struct OrganizationSsoLoginHttpState {
    service: Arc<dyn OrganizationSsoLoginTransactionService>,
    session_service: Option<Arc<dyn OrganizationSsoSessionService>>,
}

/// 企业 SSO 回调当前只返回稳定错误投影，不返回 state、授权码或断言原文。
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = OrganizationSsoLoginCallbackErrorResponse)]
pub(crate) struct OrganizationSsoLoginCallbackErrorResponse {
    code: &'static str,
    message: &'static str,
}

/// OIDC 协议验证成功后返回的一次性登录票据；不返回 subject、Token 或 Provider 原文。
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct OrganizationSsoLoginCallbackSuccessResponse {
    ticket: String,
    expires_at: i64,
}

/// 同源交换一次性企业 SSO ticket 的请求；ticket 不会写入响应或日志。
#[derive(serde::Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct OrganizationSsoSessionExchangeRequest {
    #[schema(minimum = 1)]
    organization_id: i64,
    #[schema(min_length = 1, max_length = 4096, write_only)]
    ticket: String,
}

/// 企业 SSO 会话交换失败的稳定响应，不回显 ticket、subject 或依赖错误。
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct OrganizationSsoSessionExchangeErrorResponse {
    code: &'static str,
    message: &'static str,
}

/// 公开发现请求；identifier 可以是企业公开标识或邮箱，不接受额外字段。
#[derive(serde::Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = OrganizationSsoDiscoveryRequest)]
pub(crate) struct OrganizationSsoDiscoveryRequest {
    #[schema(min_length = 1, max_length = 320)]
    identifier: String,
}

/// 公开发现响应只返回通用企业 SSO 动作资格，不泄露企业或 Provider 资料。
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = OrganizationSsoDiscoveryResponse)]
pub(crate) struct OrganizationSsoDiscoveryResponse {
    available: bool,
}

/// 公开发现输入无效或依赖不可用时的稳定错误投影。
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = OrganizationSsoDiscoveryErrorResponse)]
pub(crate) struct OrganizationSsoDiscoveryErrorResponse {
    code: &'static str,
    message: &'static str,
}

/// 构建不需要登录会话的企业 SSO 固定回调路由。
pub(crate) fn build_organization_sso_login_router(
    service: Arc<dyn OrganizationSsoLoginTransactionService>,
    session_service: Option<Arc<dyn OrganizationSsoSessionService>>,
) -> Router {
    let mut router = Router::new()
        .route(
            ORGANIZATION_SSO_DISCOVERY_PATH,
            post(discover_organization_sso),
        )
        .route(
            ORGANIZATION_SSO_OIDC_CALLBACK_PATH,
            get(complete_oidc_callback),
        )
        .route(
            ORGANIZATION_SSO_SAML_CALLBACK_PATH,
            post(complete_saml_callback),
        )
        .layer(DefaultBodyLimit::max(MAX_SAML_CALLBACK_BODY_BYTES))
        .layer(RequestBodyLimitLayer::new(MAX_SAML_CALLBACK_BODY_BYTES));
    if session_service.is_some() {
        router = router.route(
            ORGANIZATION_SSO_SESSION_EXCHANGE_PATH,
            post(exchange_organization_sso_session),
        );
    }
    router.with_state(OrganizationSsoLoginHttpState {
        service,
        session_service,
    })
}

/// 执行无会话企业 SSO 发现；未知和不可用企业统一返回 available=false。
async fn discover_organization_sso(
    State(state): State<OrganizationSsoLoginHttpState>,
    request: Result<Json<OrganizationSsoDiscoveryRequest>, JsonRejection>,
) -> Response {
    let Json(request) = match request {
        Ok(request) => request,
        Err(_) => return discovery_error(StatusCode::BAD_REQUEST, "invalid_request"),
    };
    match state.service.discover(request.identifier).await {
        Ok(discovery) => discovery_response(discovery),
        Err(OrganizationSsoDiscoveryServiceError::InvalidInput) => {
            discovery_error(StatusCode::BAD_REQUEST, "invalid_request")
        }
        Err(OrganizationSsoDiscoveryServiceError::Internal) => {
            discovery_error(StatusCode::SERVICE_UNAVAILABLE, "unavailable")
        }
    }
}

fn discovery_response(discovery: OrganizationSsoDiscovery) -> Response {
    let mut response = no_store_json(OrganizationSsoDiscoveryResponse {
        available: discovery.available(),
    });
    *response.status_mut() = StatusCode::OK;
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

fn discovery_error(status: StatusCode, code: &'static str) -> Response {
    let message = match code {
        "invalid_request" => "企业 SSO 发现请求无效",
        _ => "企业 SSO 发现暂不可用",
    };
    let mut response = no_store_json(OrganizationSsoDiscoveryErrorResponse { code, message });
    *response.status_mut() = status;
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// 在同源 POST 边界消费一次性 ticket，并返回既有站点会话。
async fn exchange_organization_sso_session(
    State(state): State<OrganizationSsoLoginHttpState>,
    request: Result<Json<OrganizationSsoSessionExchangeRequest>, JsonRejection>,
) -> Response {
    let Json(request) = match request {
        Ok(request) => request,
        Err(_) => return session_exchange_error(StatusCode::BAD_REQUEST, "invalid_request"),
    };
    let organization_id = match OrganizationId::new(request.organization_id) {
        Ok(organization_id) => organization_id,
        Err(_) => return session_exchange_error(StatusCode::BAD_REQUEST, "invalid_request"),
    };
    if request.ticket.is_empty() || request.ticket.len() > 4_096 {
        return session_exchange_error(StatusCode::BAD_REQUEST, "invalid_request");
    }
    let session_service = match state.session_service.as_ref() {
        Some(service) => service,
        None => return session_exchange_error(StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
    };
    match session_service
        .exchange_ticket(organization_id, request.ticket)
        .await
    {
        Ok(session) => crate::management_session::issued_session_response(session),
        Err(error) => map_session_exchange_error(error),
    }
}

async fn complete_oidc_callback(
    State(state): State<OrganizationSsoLoginHttpState>,
    RawQuery(raw_query): RawQuery,
) -> Response {
    let callback = match OrganizationSsoOidcCallback::parse(raw_query.as_deref().unwrap_or("")) {
        Ok(callback) => callback,
        Err(_) => return callback_error(StatusCode::BAD_REQUEST, "invalid_request"),
    };
    let claimed = match state
        .service
        .claim_callback(callback.state().to_owned())
        .await
    {
        Ok(claimed) => claimed,
        Err(error) => return map_claim_error(error),
    };
    if claimed.protocol() != OrganizationSsoProtocol::Oidc {
        return reject_claimed_callback(&state, claimed).await;
    }
    match state
        .service
        .complete_oidc_callback(claimed, callback.code().to_owned())
        .await
    {
        Ok(ticket) => callback_success(ticket),
        Err(error) => map_claim_error(error),
    }
}

async fn complete_saml_callback(
    State(state): State<OrganizationSsoLoginHttpState>,
    body: String,
) -> Response {
    let callback = match OrganizationSsoSamlCallback::parse(&body) {
        Ok(callback) => callback,
        Err(_) => return callback_error(StatusCode::BAD_REQUEST, "invalid_request"),
    };
    let (saml_response, relay_state) = callback.into_parts();
    process_claimed_callback(
        &state,
        &relay_state,
        OrganizationSsoProtocol::Saml,
        Some(saml_response),
    )
    .await
}

async fn process_claimed_callback(
    state: &OrganizationSsoLoginHttpState,
    state_material: &str,
    expected_protocol: OrganizationSsoProtocol,
    saml_response: Option<String>,
) -> Response {
    let claimed = match state
        .service
        .claim_callback(state_material.to_owned())
        .await
    {
        Ok(claimed) => claimed,
        Err(error) => return map_claim_error(error),
    };
    if claimed.protocol() != expected_protocol {
        return reject_claimed_callback(state, claimed).await;
    }

    match expected_protocol {
        OrganizationSsoProtocol::Saml => {
            let Some(saml_response) = saml_response else {
                return reject_claimed_callback(state, claimed).await;
            };
            match state
                .service
                .complete_saml_callback(claimed, saml_response)
                .await
            {
                Ok(ticket) => callback_success(ticket),
                Err(error) => map_claim_error(error),
            }
        }
        OrganizationSsoProtocol::Oidc => reject_claimed_callback(state, claimed).await,
    }
}

async fn reject_claimed_callback(
    state: &OrganizationSsoLoginHttpState,
    claimed: ClaimedOrganizationSsoLoginTransaction,
) -> Response {
    match state
        .service
        .complete_rejected(claimed.organization_id(), claimed.transaction_id())
        .await
    {
        Ok(()) => callback_error(StatusCode::UNPROCESSABLE_ENTITY, "rejected"),
        Err(_) => callback_error(StatusCode::SERVICE_UNAVAILABLE, "unavailable"),
    }
}

fn map_claim_error(error: OrganizationSsoLoginTransactionServiceError) -> Response {
    let (status, code) = match error {
        OrganizationSsoLoginTransactionServiceError::InvalidInput => {
            (StatusCode::BAD_REQUEST, "invalid_request")
        }
        OrganizationSsoLoginTransactionServiceError::Rejected
        | OrganizationSsoLoginTransactionServiceError::Expired
        | OrganizationSsoLoginTransactionServiceError::AlreadyProcessed
        | OrganizationSsoLoginTransactionServiceError::InvalidTransition => {
            (StatusCode::UNPROCESSABLE_ENTITY, "rejected")
        }
        OrganizationSsoLoginTransactionServiceError::Unavailable
        | OrganizationSsoLoginTransactionServiceError::Internal => {
            (StatusCode::SERVICE_UNAVAILABLE, "unavailable")
        }
    };
    callback_error(status, code)
}

fn callback_error(status: StatusCode, code: &'static str) -> Response {
    let message = match code {
        "invalid_request" => "企业 SSO 回调请求无效",
        "rejected" => "企业 SSO 回调已拒绝",
        _ => "企业 SSO 回调暂不可用",
    };
    let mut response = no_store_json(OrganizationSsoLoginCallbackErrorResponse { code, message });
    *response.status_mut() = status;
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

fn callback_success(ticket: af_admin::IssuedOrganizationSsoLoginTicket) -> Response {
    callback_success_response(ticket.ticket(), ticket.expires_at())
}

/// 以固定的最小投影返回已签发票据，避免回调边界意外扩展敏感字段。
fn callback_success_response(ticket: &str, expires_at: i64) -> Response {
    let mut response = no_store_json(OrganizationSsoLoginCallbackSuccessResponse {
        ticket: ticket.to_owned(),
        expires_at,
    });
    *response.status_mut() = StatusCode::OK;
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

fn map_session_exchange_error(error: OrganizationSsoSessionServiceError) -> Response {
    let (status, code) = match error {
        OrganizationSsoSessionServiceError::InvalidInput => {
            (StatusCode::BAD_REQUEST, "invalid_request")
        }
        OrganizationSsoSessionServiceError::Rejected => {
            (StatusCode::UNPROCESSABLE_ENTITY, "rejected")
        }
        OrganizationSsoSessionServiceError::Unavailable => {
            (StatusCode::SERVICE_UNAVAILABLE, "unavailable")
        }
        OrganizationSsoSessionServiceError::Internal => {
            (StatusCode::SERVICE_UNAVAILABLE, "unavailable")
        }
    };
    session_exchange_error(status, code)
}

fn session_exchange_error(status: StatusCode, code: &'static str) -> Response {
    let message = match code {
        "invalid_request" => "企业 SSO 会话交换请求无效",
        "rejected" => "企业 SSO 会话交换已拒绝",
        _ => "企业 SSO 会话交换暂不可用",
    };
    let mut response = no_store_json(OrganizationSsoSessionExchangeErrorResponse { code, message });
    *response.status_mut() = status;
    response
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use af_admin::{
        ExchangedOrganizationSsoLoginTicket, IssuedOrganizationSsoLoginTicket, IssuedSession,
        OrganizationSsoDiscoveryFuture, OrganizationSsoLoginTransactionFuture,
        OrganizationSsoLoginTransactionStart, OrganizationSsoSessionFuture,
        OrganizationSsoSessionService, OrganizationSsoSessionServiceError, SessionPrincipal,
        SessionRole, SessionToken,
    };
    use af_domain::{OrganizationId, UserId};
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use serde_json::Value;
    use tower::ServiceExt as _;

    use super::*;

    struct TestService;

    #[derive(Clone, Copy)]
    enum TestSessionOutcome {
        Error(OrganizationSsoSessionServiceError),
        Success,
    }

    struct TestSessionService(TestSessionOutcome);

    impl OrganizationSsoSessionService for TestSessionService {
        fn exchange_ticket<'a>(
            &'a self,
            _organization_id: OrganizationId,
            _ticket: String,
        ) -> OrganizationSsoSessionFuture<'a> {
            let outcome = self.0;
            Box::pin(async move {
                match outcome {
                    TestSessionOutcome::Error(error) => Err(error),
                    TestSessionOutcome::Success => Ok(IssuedSession::from_parts(
                        SessionToken::from_string("access-token".to_owned()),
                        SessionPrincipal::new(
                            UserId::new(42).expect("测试用户标识"),
                            SessionRole::User,
                        ),
                        1_800_000_000,
                    )),
                }
            })
        }
    }

    impl OrganizationSsoLoginTransactionService for TestService {
        fn discover<'a>(&'a self, identifier: String) -> OrganizationSsoDiscoveryFuture<'a> {
            Box::pin(async move {
                Ok(OrganizationSsoDiscovery::new(
                    identifier == "available@example.com",
                ))
            })
        }

        fn begin<'a>(
            &'a self,
            _organization_id: OrganizationId,
            _provider_key: String,
            _request_digest: String,
        ) -> OrganizationSsoLoginTransactionFuture<'a, OrganizationSsoLoginTransactionStart>
        {
            panic!("测试服务不应执行 begin")
        }

        fn claim<'a>(
            &'a self,
            _organization_id: OrganizationId,
            _state: String,
        ) -> OrganizationSsoLoginTransactionFuture<'a, ClaimedOrganizationSsoLoginTransaction>
        {
            panic!("测试服务不应执行按企业 claim")
        }

        fn claim_callback<'a>(
            &'a self,
            _state: String,
        ) -> OrganizationSsoLoginTransactionFuture<'a, ClaimedOrganizationSsoLoginTransaction>
        {
            Box::pin(async { Err(OrganizationSsoLoginTransactionServiceError::Rejected) })
        }

        fn complete_verified<'a>(
            &'a self,
            _organization_id: OrganizationId,
            _transaction_id: i64,
            _assertion_digest: String,
        ) -> OrganizationSsoLoginTransactionFuture<'a, IssuedOrganizationSsoLoginTicket> {
            panic!("测试服务不应执行 complete_verified")
        }

        fn complete_rejected<'a>(
            &'a self,
            _organization_id: OrganizationId,
            _transaction_id: i64,
        ) -> OrganizationSsoLoginTransactionFuture<'a, ()> {
            Box::pin(async { Ok(()) })
        }

        fn exchange_ticket<'a>(
            &'a self,
            _organization_id: OrganizationId,
            _ticket: String,
        ) -> OrganizationSsoLoginTransactionFuture<'a, ExchangedOrganizationSsoLoginTicket>
        {
            panic!("测试服务不应执行 exchange_ticket")
        }
    }

    #[tokio::test]
    async fn invalid_oidc_callback_does_not_leak_material() {
        let router = build_organization_sso_login_router(Arc::new(TestService), None);
        let response = router
            .oneshot(
                Request::builder()
                    .uri(format!(
                        "{}?code=secret-code&state=secret-state",
                        ORGANIZATION_SSO_OIDC_CALLBACK_PATH
                    ))
                    .body(Body::empty())
                    .expect("请求构造成功"),
            )
            .await
            .expect("路由响应成功");
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = to_bytes(response.into_body(), 4096)
            .await
            .expect("读取错误响应");
        let body = String::from_utf8_lossy(&body);
        assert!(!body.contains("secret-code"));
        assert!(!body.contains("secret-state"));
    }

    #[tokio::test]
    async fn successful_oidc_callback_response_only_contains_ticket_and_expiry() {
        let response = callback_success_response("ticket-material", 1_800_000_000);
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(CACHE_CONTROL),
            Some(&HeaderValue::from_static("no-store"))
        );

        let body = to_bytes(response.into_body(), 4096)
            .await
            .expect("璇诲彇鎴愬姛鍥炶皟鍝嶅簲");
        let body: Value = serde_json::from_slice(&body).expect("瑙ｆ瀽鎴愬姛鍥炶皟 JSON");
        let object = body.as_object().expect("鎴愬姛鍥炶皟搴旇鏄璞?");
        assert_eq!(object.len(), 2);
        assert_eq!(
            object.get("ticket").and_then(Value::as_str),
            Some("ticket-material")
        );
        assert_eq!(
            object.get("expires_at").and_then(Value::as_i64),
            Some(1_800_000_000)
        );
        for sensitive in ["code", "state", "nonce", "token", "subject"] {
            assert!(!body.to_string().contains(sensitive));
        }
    }

    #[tokio::test]
    async fn session_exchange_rejects_invalid_input_without_echoing_ticket() {
        let router = build_organization_sso_login_router(
            Arc::new(TestService),
            Some(Arc::new(TestSessionService(TestSessionOutcome::Error(
                OrganizationSsoSessionServiceError::Rejected,
            )))),
        );
        let response = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(ORGANIZATION_SSO_SESSION_EXCHANGE_PATH)
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"organization_id":1,"ticket":"ticket-material"}"#,
                    ))
                    .expect("请求构造成功"),
            )
            .await
            .expect("路由响应成功");
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            response.headers().get(CACHE_CONTROL),
            Some(&HeaderValue::from_static("no-store"))
        );
        let body = to_bytes(response.into_body(), 4096)
            .await
            .expect("读取错误响应");
        let body = String::from_utf8_lossy(&body);
        assert!(!body.contains("ticket-material"));
        assert!(!body.contains("subject"));
    }

    #[tokio::test]
    async fn session_exchange_rejects_invalid_json_and_empty_ticket() {
        let router = build_organization_sso_login_router(
            Arc::new(TestService),
            Some(Arc::new(TestSessionService(TestSessionOutcome::Error(
                OrganizationSsoSessionServiceError::Rejected,
            )))),
        );
        let invalid_json = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(ORGANIZATION_SSO_SESSION_EXCHANGE_PATH)
                    .header("content-type", "application/json")
                    .body(Body::from("{"))
                    .expect("请求构造成功"),
            )
            .await
            .expect("路由响应成功");
        assert_eq!(invalid_json.status(), StatusCode::BAD_REQUEST);

        let empty_ticket = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(ORGANIZATION_SSO_SESSION_EXCHANGE_PATH)
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"organization_id":1,"ticket":""}"#))
                    .expect("请求构造成功"),
            )
            .await
            .expect("路由响应成功");
        assert_eq!(empty_ticket.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn session_exchange_maps_unavailable_without_leaking_material() {
        let router = build_organization_sso_login_router(
            Arc::new(TestService),
            Some(Arc::new(TestSessionService(TestSessionOutcome::Error(
                OrganizationSsoSessionServiceError::Unavailable,
            )))),
        );
        let response = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(ORGANIZATION_SSO_SESSION_EXCHANGE_PATH)
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"organization_id":1,"ticket":"ticket-material"}"#,
                    ))
                    .expect("请求构造成功"),
            )
            .await
            .expect("路由响应成功");
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            response.headers().get(CACHE_CONTROL),
            Some(&HeaderValue::from_static("no-store"))
        );
        let body = to_bytes(response.into_body(), 4096)
            .await
            .expect("读取错误响应");
        let body = String::from_utf8_lossy(&body);
        assert!(!body.contains("ticket-material"));
        assert!(!body.contains("subject"));
    }

    #[tokio::test]
    async fn successful_session_exchange_returns_only_existing_login_response() {
        let router = build_organization_sso_login_router(
            Arc::new(TestService),
            Some(Arc::new(TestSessionService(TestSessionOutcome::Success))),
        );
        let response = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(ORGANIZATION_SSO_SESSION_EXCHANGE_PATH)
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"organization_id":1,"ticket":"ticket-material"}"#,
                    ))
                    .expect("请求构造成功"),
            )
            .await
            .expect("路由响应成功");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(CACHE_CONTROL),
            Some(&HeaderValue::from_static("no-store"))
        );
        let body = to_bytes(response.into_body(), 4096)
            .await
            .expect("读取成功响应");
        let body: Value = serde_json::from_slice(&body).expect("解析成功响应 JSON");
        assert_eq!(body["access_token"], "access-token");
        assert_eq!(body["token_type"], "Bearer");
        assert_eq!(body["user"]["id"], 42);
        for sensitive in ["ticket-material", "subject", "provider"] {
            assert!(!body.to_string().contains(sensitive));
        }
    }

    #[tokio::test]
    async fn discovery_returns_only_low_information_availability() {
        let router = build_organization_sso_login_router(Arc::new(TestService), None);
        let response = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(ORGANIZATION_SSO_DISCOVERY_PATH)
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"identifier":"available@example.com"}"#))
                    .expect("请求构造成功"),
            )
            .await
            .expect("路由响应成功");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(CACHE_CONTROL),
            Some(&HeaderValue::from_static("no-store"))
        );
        let body = to_bytes(response.into_body(), 4096)
            .await
            .expect("读取发现响应");
        let body: Value = serde_json::from_slice(&body).expect("解析发现响应");
        assert_eq!(body, serde_json::json!({"available": true}));
        for sensitive in [
            "available@example.com",
            "organization",
            "provider",
            "domain",
        ] {
            assert!(!body.to_string().contains(sensitive));
        }
    }

    #[tokio::test]
    async fn discovery_unknown_identifier_keeps_same_response_shape() {
        let router = build_organization_sso_login_router(Arc::new(TestService), None);
        let response = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(ORGANIZATION_SSO_DISCOVERY_PATH)
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"identifier":"unknown@example.com"}"#))
                    .expect("请求构造成功"),
            )
            .await
            .expect("路由响应成功");
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 4096)
            .await
            .expect("读取发现响应");
        let body: Value = serde_json::from_slice(&body).expect("解析发现响应");
        assert_eq!(body, serde_json::json!({"available": false}));
    }

    #[tokio::test]
    async fn discovery_rejects_unknown_fields_and_invalid_json() {
        let router = build_organization_sso_login_router(Arc::new(TestService), None);
        for body in [r#"{"identifier":"a@example.com","organization_id":1}"#, "{"] {
            let response = router
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(ORGANIZATION_SSO_DISCOVERY_PATH)
                        .header("content-type", "application/json")
                        .body(Body::from(body))
                        .expect("请求构造成功"),
                )
                .await
                .expect("路由响应成功");
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        }
    }
}
