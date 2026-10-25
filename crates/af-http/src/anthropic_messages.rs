use af_admin::GatewayModelAuthentication;
use af_domain::{AfError, Protocol};
use af_protocol::{
    CanonicalRequestEnvelope, anthropic::parse_request_envelope, normalize_anthropic_beta_values,
};
use af_relay::RelayDiagnosticInput;
use af_telemetry::RequestId;
use axum::{
    body::Bytes,
    extract::{Extension, State},
    response::Response,
};
use http::HeaderMap;

use crate::{
    chat_completions::{HttpState, execute_chat_response, prepare_model_request},
    error_response::AnthropicHttpError,
};

/// 解析 Anthropic Messages 请求并经 Canonical Chat 执行边界转发。
pub(crate) async fn anthropic_messages(
    State(state): State<HttpState>,
    Extension(request_id): Extension<RequestId>,
    Extension(authentication): Extension<GatewayModelAuthentication>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, AnthropicHttpError> {
    let diagnostic = RelayDiagnosticInput::capture("POST", "/v1/messages", &headers, &body);
    let request = parse_request_with_headers(body, &headers)?;
    let request =
        prepare_model_request(request, &authentication).map_err(AnthropicHttpError::from)?;
    execute_chat_response(
        &state,
        authentication,
        request,
        Protocol::Anthropic,
        request_id,
        diagnostic,
    )
    .await
    .map_err(AnthropicHttpError::from)
}

fn parse_request_with_headers(
    body: Bytes,
    headers: &HeaderMap,
) -> Result<CanonicalRequestEnvelope, AnthropicHttpError> {
    let beta_values = headers
        .get_all("anthropic-beta")
        .iter()
        .map(|value| value.to_str())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| AnthropicHttpError::from(AfError::InvalidRequest))?;
    let beta = normalize_anthropic_beta_values(beta_values)
        .map_err(|_| AnthropicHttpError::from(AfError::InvalidRequest))?;
    parse_request_envelope(body)
        .map_err(|_| AnthropicHttpError::from(AfError::InvalidRequest))?
        .with_anthropic_beta((!beta.is_empty()).then_some(beta.as_str()))
        .map_err(|_| AnthropicHttpError::from(AfError::InvalidRequest))
}

#[cfg(test)]
mod tests {
    use axum::{body::to_bytes, response::IntoResponse};
    use http::{HeaderValue, StatusCode};

    use super::*;

    fn body() -> Bytes {
        Bytes::from_static(br#"{"model":"claude-test","max_tokens":128,"messages":[{"role":"user","content":"hello"}]}"#)
    }

    #[test]
    fn anthropic_ingress_keeps_repeated_beta_headers_and_ignores_unrelated_headers() {
        let mut headers = HeaderMap::new();
        headers.append(
            "anthropic-beta",
            HeaderValue::from_static(" compact-2026-09-04,custom-beta "),
        );
        headers.append(
            "anthropic-beta",
            HeaderValue::from_static("custom-beta,context-management-2025-06-27"),
        );
        headers.insert(
            "authorization",
            HeaderValue::from_static("Bearer must-not-be-forwarded"),
        );
        headers.insert(
            "x-api-key",
            HeaderValue::from_static("must-not-be-forwarded"),
        );
        let request = parse_request_with_headers(body(), &headers).unwrap();
        assert_eq!(
            request.anthropic_beta(),
            Some("compact-2026-09-04,custom-beta,context-management-2025-06-27")
        );
        let debug = format!("{request:?}");
        assert!(!debug.contains("must-not-be-forwarded"));
        assert!(!debug.contains("custom-beta"));
        assert_eq!(
            parse_request_with_headers(body(), &HeaderMap::new())
                .unwrap()
                .anthropic_beta(),
            None
        );
    }

    #[tokio::test]
    async fn invalid_beta_headers_fail_as_safe_anthropic_bad_requests() {
        for beta in [
            HeaderValue::from_static("secret beta"),
            HeaderValue::from_bytes(b"secret\xffbeta").unwrap(),
            HeaderValue::from_bytes(&vec![b'x'; af_protocol::MAX_ANTHROPIC_BETA_BYTES + 1])
                .unwrap(),
        ] {
            let mut headers = HeaderMap::new();
            headers.insert("anthropic-beta", beta);
            let response = parse_request_with_headers(body(), &headers)
                .unwrap_err()
                .into_response();
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            let response_body = to_bytes(response.into_body(), 8192).await.unwrap();
            let value: serde_json::Value = serde_json::from_slice(&response_body).unwrap();
            assert_eq!(value["type"], "error");
            assert_eq!(value["error"]["type"], "invalid_request_error");
            assert!(!String::from_utf8_lossy(&response_body).contains("secret"));
        }
    }
}
