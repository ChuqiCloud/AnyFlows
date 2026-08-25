use std::sync::Arc;

use af_admin::{
    ClaimedOrganizationSsoLoginTransaction, ORGANIZATION_SSO_OIDC_CALLBACK_PATH,
    ORGANIZATION_SSO_SAML_CALLBACK_PATH, OrganizationSsoLoginTransactionService,
    OrganizationSsoLoginTransactionServiceError, OrganizationSsoOidcCallback,
    OrganizationSsoSamlCallback,
};
use af_domain::OrganizationSsoProtocol;
use axum::{
    Router,
    extract::{DefaultBodyLimit, RawQuery, State},
    response::Response,
    routing::{get, post},
};
use http::{HeaderValue, StatusCode, header::CACHE_CONTROL};
use serde::Serialize;
use tower_http::limit::RequestBodyLimitLayer;
use utoipa::ToSchema;

use crate::management_session::no_store_json;

const MAX_SAML_CALLBACK_BODY_BYTES: usize = 256 * 1024;

#[derive(Clone)]
struct OrganizationSsoLoginHttpState {
    service: Arc<dyn OrganizationSsoLoginTransactionService>,
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

/// 构建不需要登录会话的企业 SSO 固定回调路由。
pub(crate) fn build_organization_sso_login_router(
    service: Arc<dyn OrganizationSsoLoginTransactionService>,
) -> Router {
    Router::new()
        .route(
            ORGANIZATION_SSO_OIDC_CALLBACK_PATH,
            get(complete_oidc_callback),
        )
        .route(
            ORGANIZATION_SSO_SAML_CALLBACK_PATH,
            post(complete_saml_callback),
        )
        .layer(DefaultBodyLimit::max(MAX_SAML_CALLBACK_BODY_BYTES))
        .layer(RequestBodyLimitLayer::new(MAX_SAML_CALLBACK_BODY_BYTES))
        .with_state(OrganizationSsoLoginHttpState { service })
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
    process_claimed_callback(
        &state,
        callback.relay_state(),
        OrganizationSsoProtocol::Saml,
    )
    .await
}

async fn process_claimed_callback(
    state: &OrganizationSsoLoginHttpState,
    state_material: &str,
    expected_protocol: OrganizationSsoProtocol,
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

    // Provider 的元数据、密钥读取和协议交换尚未接入；在此边界绝不把回调材料伪装成已验证。
    reject_claimed_callback(state, claimed).await
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use af_admin::{
        ExchangedOrganizationSsoLoginTicket, IssuedOrganizationSsoLoginTicket,
        OrganizationSsoLoginTransactionFuture, OrganizationSsoLoginTransactionStart,
    };
    use af_domain::OrganizationId;
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use serde_json::Value;
    use tower::ServiceExt as _;

    use super::*;

    struct TestService;

    impl OrganizationSsoLoginTransactionService for TestService {
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
        let router = build_organization_sso_login_router(Arc::new(TestService));
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
}
