use af_config::{CorsOrigin, ServerConfig};
use af_domain::{
    AfError, NetworkFailureKind, PublicErrorCode, QuotaWindowRetryAfter, RateLimitScope,
    UpstreamError, UpstreamServerStatus,
};
use af_telemetry::RequestId;
use axum::{
    Router,
    body::{Body, to_bytes},
    response::IntoResponse,
    routing::get,
};
use http::{
    Request, StatusCode,
    header::{
        ACCEPT_LANGUAGE, ACCESS_CONTROL_ALLOW_ORIGIN, ACCESS_CONTROL_EXPOSE_HEADERS, CONTENT_TYPE,
        ORIGIN, RETRY_AFTER,
    },
};
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::{
    ApiKeyExtractionError, REQUEST_ID_HEADER_NAME,
    error_response::{
        AnthropicHttpError, GeminiHttpError, MappedError, OpenAiHttpError, OpenAiResponsesHttpError,
    },
    router::build_router_with_routes,
};

struct ErrorCase {
    error: AfError,
    status: StatusCode,
    code: PublicErrorCode,
    kind: &'static str,
    message: &'static str,
}

fn error_cases() -> Vec<ErrorCase> {
    vec![
        ErrorCase {
            error: AfError::InvalidRequest,
            status: StatusCode::BAD_REQUEST,
            code: PublicErrorCode::InvalidRequest,
            kind: "invalid_request_error",
            message: "Invalid request.",
        },
        ErrorCase {
            error: AfError::InvalidApiKey,
            status: StatusCode::UNAUTHORIZED,
            code: PublicErrorCode::InvalidApiKey,
            kind: "invalid_request_error",
            message: "Invalid API key.",
        },
        ErrorCase {
            error: AfError::ModelNotAllowed,
            status: StatusCode::NOT_FOUND,
            code: PublicErrorCode::ModelNotFound,
            kind: "invalid_request_error",
            message: "The requested model was not found or is unavailable.",
        },
        ErrorCase {
            error: AfError::TaskNotFound,
            status: StatusCode::NOT_FOUND,
            code: PublicErrorCode::TaskNotFound,
            kind: "invalid_request_error",
            message: "The requested task was not found.",
        },
        ErrorCase {
            error: AfError::IdempotencyConflict,
            status: StatusCode::CONFLICT,
            code: PublicErrorCode::IdempotencyConflict,
            kind: "invalid_request_error",
            message: "The idempotency key is already associated with a different request.",
        },
        ErrorCase {
            error: AfError::RequestOutcomeUnknown,
            status: StatusCode::SERVICE_UNAVAILABLE,
            code: PublicErrorCode::RequestOutcomeUnknown,
            kind: "server_error",
            message: "The request outcome is unknown. Retry with the same idempotency key.",
        },
        ErrorCase {
            error: AfError::InsufficientQuota,
            status: StatusCode::TOO_MANY_REQUESTS,
            code: PublicErrorCode::InsufficientQuota,
            kind: "insufficient_quota",
            message: "Insufficient quota.",
        },
        ErrorCase {
            error: AfError::QuotaWindowLimited {
                retry_after: QuotaWindowRetryAfter::from_seconds(37).unwrap(),
            },
            status: StatusCode::TOO_MANY_REQUESTS,
            code: PublicErrorCode::RateLimited,
            kind: "rate_limit_error",
            message: "Rate limit exceeded. Please retry later.",
        },
        ErrorCase {
            error: AfError::ConcurrencyLimited,
            status: StatusCode::TOO_MANY_REQUESTS,
            code: PublicErrorCode::RateLimited,
            kind: "rate_limit_error",
            message: "Rate limit exceeded. Please retry later.",
        },
        upstream_case(
            UpstreamError::rate_limited(RateLimitScope::Window),
            StatusCode::TOO_MANY_REQUESTS,
            PublicErrorCode::RateLimited,
            "rate_limit_error",
            "Rate limit exceeded. Please retry later.",
        ),
        upstream_unavailable_case(UpstreamError::overloaded()),
        upstream_unavailable_case(UpstreamError::AuthExpired),
        upstream_unavailable_case(UpstreamError::AuthRevoked),
        upstream_unavailable_case(UpstreamError::AccountDisabled),
        upstream_unavailable_case(UpstreamError::QuotaExhausted),
        ErrorCase {
            error: AfError::from(UpstreamError::ModelUnsupported),
            status: StatusCode::NOT_FOUND,
            code: PublicErrorCode::ModelNotFound,
            kind: "invalid_request_error",
            message: "The requested model was not found or is unavailable.",
        },
        upstream_unavailable_case(UpstreamError::ProtocolError),
        upstream_unavailable_case(UpstreamError::ServerError {
            status: UpstreamServerStatus::new(503).unwrap(),
        }),
        upstream_case(
            UpstreamError::BadRequest,
            StatusCode::BAD_REQUEST,
            PublicErrorCode::InvalidRequest,
            "invalid_request_error",
            "Invalid request.",
        ),
        upstream_unavailable_case(UpstreamError::network(NetworkFailureKind::Connect)),
        ErrorCase {
            error: AfError::Internal,
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: PublicErrorCode::InternalError,
            kind: "server_error",
            message: "An internal server error occurred.",
        },
    ]
}

fn upstream_case(
    error: UpstreamError,
    status: StatusCode,
    code: PublicErrorCode,
    kind: &'static str,
    message: &'static str,
) -> ErrorCase {
    ErrorCase {
        error: AfError::from(error),
        status,
        code,
        kind,
        message,
    }
}

fn upstream_unavailable_case(error: UpstreamError) -> ErrorCase {
    upstream_case(
        error,
        StatusCode::SERVICE_UNAVAILABLE,
        PublicErrorCode::UpstreamUnavailable,
        "server_error",
        "The service is temporarily unavailable. Please retry later.",
    )
}

async fn response_json(response: axum::response::Response) -> Value {
    let body = to_bytes(response.into_body(), 16 * 1024).await.unwrap();
    serde_json::from_slice(&body).unwrap()
}

#[test]
fn public_error_metadata_covers_every_stable_code() {
    let cases = [
        (
            PublicErrorCode::InvalidRequest,
            StatusCode::BAD_REQUEST,
            "Invalid request.",
        ),
        (
            PublicErrorCode::InvalidApiKey,
            StatusCode::UNAUTHORIZED,
            "Invalid API key.",
        ),
        (
            PublicErrorCode::InsufficientQuota,
            StatusCode::TOO_MANY_REQUESTS,
            "Insufficient quota.",
        ),
        (
            PublicErrorCode::ModelNotFound,
            StatusCode::NOT_FOUND,
            "The requested model was not found or is unavailable.",
        ),
        (
            PublicErrorCode::TaskNotFound,
            StatusCode::NOT_FOUND,
            "The requested task was not found.",
        ),
        (
            PublicErrorCode::IdempotencyConflict,
            StatusCode::CONFLICT,
            "The idempotency key is already associated with a different request.",
        ),
        (
            PublicErrorCode::RequestOutcomeUnknown,
            StatusCode::SERVICE_UNAVAILABLE,
            "The request outcome is unknown. Retry with the same idempotency key.",
        ),
        (
            PublicErrorCode::UpstreamUnavailable,
            StatusCode::SERVICE_UNAVAILABLE,
            "The service is temporarily unavailable. Please retry later.",
        ),
        (
            PublicErrorCode::RateLimited,
            StatusCode::TOO_MANY_REQUESTS,
            "Rate limit exceeded. Please retry later.",
        ),
        (
            PublicErrorCode::InternalError,
            StatusCode::INTERNAL_SERVER_ERROR,
            "An internal server error occurred.",
        ),
    ];
    assert_eq!(cases.len(), PublicErrorCode::ALL.len());

    for (code, status, message) in cases {
        let mapped = MappedError::new(code, "test_error_kind");
        assert_eq!(mapped.status(), status);
        assert_eq!(mapped.message(), message);
        assert!(message.is_ascii());
    }
}

#[tokio::test]
async fn every_presentation_failure_uses_one_invalid_api_key_response() {
    let mut expected = None;
    for error in [
        ApiKeyExtractionError::Missing,
        ApiKeyExtractionError::Malformed,
        ApiKeyExtractionError::Ambiguous,
        ApiKeyExtractionError::QueryDenied,
    ] {
        let response = OpenAiHttpError::from(error).into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let body = response_json(response).await;
        assert_eq!(
            body,
            json!({
                "error": {
                    "message": "Invalid API key.",
                    "type": "invalid_request_error",
                    "param": null,
                    "code": "invalid_api_key",
                }
            })
        );
        if let Some(expected) = &expected {
            assert_eq!(&body, expected);
        } else {
            expected = Some(body);
        }
    }
}

#[tokio::test]
async fn maps_every_current_domain_error_without_exposing_diagnostics() {
    for case in error_cases() {
        let diagnostic = case.error.to_string();
        let response = OpenAiHttpError::from(case.error).into_response();
        assert_eq!(response.status(), case.status);
        assert_eq!(
            response.headers().get(CONTENT_TYPE).unwrap(),
            "application/json"
        );
        let body = response_json(response).await;
        assert_eq!(
            body,
            json!({
                "error": {
                    "message": case.message,
                    "type": case.kind,
                    "param": null,
                    "code": case.code.as_str(),
                }
            })
        );
        let rendered = body.to_string();
        assert!(!rendered.contains(&diagnostic));
        assert!(rendered.is_ascii());
    }
}

#[tokio::test]
async fn maps_every_current_domain_error_to_anthropic_wire() {
    for case in error_cases() {
        let diagnostic = case.error.to_string();
        let response = AnthropicHttpError::from(case.error).into_response();
        assert_eq!(response.status(), case.status);
        let body = response_json(response).await;
        let expected_kind = match case.code {
            PublicErrorCode::InvalidRequest => "invalid_request_error",
            PublicErrorCode::InvalidApiKey => "authentication_error",
            PublicErrorCode::InsufficientQuota => "billing_error",
            PublicErrorCode::ModelNotFound | PublicErrorCode::TaskNotFound => "not_found_error",
            PublicErrorCode::IdempotencyConflict => "invalid_request_error",
            PublicErrorCode::RequestOutcomeUnknown | PublicErrorCode::UpstreamUnavailable => {
                "overloaded_error"
            }
            PublicErrorCode::RateLimited => "rate_limit_error",
            PublicErrorCode::InternalError => "api_error",
        };
        assert_eq!(
            body,
            json!({
                "type": "error",
                "error": {
                    "type": expected_kind,
                    "message": case.message,
                }
            })
        );
        assert!(!body.to_string().contains(&diagnostic));
    }
}

#[tokio::test]
async fn maps_every_current_domain_error_to_google_json_api_wire() {
    for case in error_cases() {
        let diagnostic = case.error.to_string();
        let response = GeminiHttpError::from(case.error).into_response();
        assert_eq!(response.status(), case.status);
        let body = response_json(response).await;
        let expected_status = match case.code {
            PublicErrorCode::InvalidRequest => "INVALID_ARGUMENT",
            PublicErrorCode::InvalidApiKey => "UNAUTHENTICATED",
            PublicErrorCode::InsufficientQuota | PublicErrorCode::RateLimited => {
                "RESOURCE_EXHAUSTED"
            }
            PublicErrorCode::ModelNotFound | PublicErrorCode::TaskNotFound => "NOT_FOUND",
            PublicErrorCode::IdempotencyConflict => "ALREADY_EXISTS",
            PublicErrorCode::RequestOutcomeUnknown | PublicErrorCode::UpstreamUnavailable => {
                "UNAVAILABLE"
            }
            PublicErrorCode::InternalError => "INTERNAL",
        };
        assert_eq!(
            body,
            json!({
                "error": {
                    "code": case.status.as_u16(),
                    "message": case.message,
                    "status": expected_status,
                }
            })
        );
        assert!(!body.to_string().contains(&diagnostic));
    }
}

#[test]
fn quota_window_limit_emits_month_scale_retry_after_for_every_protocol() {
    let retry_after = QuotaWindowRetryAfter::from_seconds(8 * 24 * 60 * 60).unwrap();
    let responses = [
        OpenAiHttpError::from(AfError::QuotaWindowLimited { retry_after }).into_response(),
        OpenAiResponsesHttpError::from(AfError::QuotaWindowLimited { retry_after }).into_response(),
        AnthropicHttpError::from(AfError::QuotaWindowLimited { retry_after }).into_response(),
        GeminiHttpError::from(AfError::QuotaWindowLimited { retry_after }).into_response(),
    ];

    for response in responses {
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(response.headers()[RETRY_AFTER], "691200");
    }
}

#[tokio::test]
async fn folds_every_upstream_server_status_into_one_public_response() {
    let expected = response_json(
        OpenAiHttpError::from(AfError::from(UpstreamError::ServerError {
            status: UpstreamServerStatus::new(500).unwrap(),
        }))
        .into_response(),
    )
    .await;

    for status in UpstreamServerStatus::MIN..=UpstreamServerStatus::MAX {
        let response = OpenAiHttpError::from(AfError::from(UpstreamError::ServerError {
            status: UpstreamServerStatus::new(status).unwrap(),
        }))
        .into_response();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(response_json(response).await, expected);
    }
}

async fn invalid_request_handler() -> Result<StatusCode, OpenAiHttpError> {
    Err(AfError::InvalidRequest.into())
}

#[tokio::test]
async fn router_error_keeps_cors_and_server_request_id_with_english_body() {
    const ALLOWED_ORIGIN: &str = "https://console.example";
    let config = ServerConfig::default()
        .with_cors_allowed_origins([ALLOWED_ORIGIN.parse::<CorsOrigin>().unwrap()]);
    let router = build_router_with_routes(
        Router::new().route("/error", get(invalid_request_handler)),
        Router::new(),
        &config,
        64,
    );
    let response = router
        .oneshot(
            Request::builder()
                .uri("/error?secret=query-canary")
                .header(ORIGIN, ALLOWED_ORIGIN)
                .header(ACCEPT_LANGUAGE, "zh-CN")
                .header(REQUEST_ID_HEADER_NAME, "client-request-id-canary")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        response.headers().get(ACCESS_CONTROL_ALLOW_ORIGIN).unwrap(),
        ALLOWED_ORIGIN
    );
    assert!(
        response.headers()[ACCESS_CONTROL_EXPOSE_HEADERS]
            .to_str()
            .unwrap()
            .contains(REQUEST_ID_HEADER_NAME)
    );
    let request_ids = response.headers().get_all(REQUEST_ID_HEADER_NAME);
    assert_eq!(request_ids.iter().count(), 1);
    let request_id = request_ids
        .iter()
        .next()
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    RequestId::new(request_id.clone()).unwrap();
    assert_ne!(request_id, "client-request-id-canary");

    let body = response_json(response).await;
    assert_eq!(
        body,
        json!({
            "error": {
                "message": "Invalid request.",
                "type": "invalid_request_error",
                "param": null,
                "code": "invalid_request",
            }
        })
    );
    let rendered = body.to_string();
    for private_value in [
        request_id.as_str(),
        "client-request-id-canary",
        "query-canary",
        "请求无效",
        "zh-CN",
    ] {
        assert!(!rendered.contains(private_value));
    }
}
