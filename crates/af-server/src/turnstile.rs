use std::{fmt, time::Duration};

use af_config::SecretString;
use af_domain::TrustedClientIp;
use af_http::{TurnstileVerification, TurnstileVerificationFuture, TurnstileVerifier};
use af_httpclient::{Body, HeaderMap, HeaderValue, HttpClientProvider, Method, StatusCode};
use serde::Deserialize;
use url::form_urlencoded;
use zeroize::Zeroize;

const TURNSTILE_SITEVERIFY_URL: &str = "https://challenges.cloudflare.com/turnstile/v0/siteverify";
const MAX_TURNSTILE_RESPONSE_BYTES: usize = 64 * 1024;

/// 使用共享受控 HTTP Client 调用 Cloudflare Turnstile siteverify 端点。
pub(crate) struct CloudflareTurnstileVerifier {
    secret_key: Vec<u8>,
    clients: HttpClientProvider,
    request_timeout: Duration,
}

impl CloudflareTurnstileVerifier {
    pub(crate) fn new(
        secret_key: &SecretString,
        clients: HttpClientProvider,
        request_timeout: Duration,
    ) -> Self {
        Self {
            secret_key: secret_key.expose().as_bytes().to_vec(),
            clients,
            request_timeout,
        }
    }

    async fn verify_inner(&self, token: &str, client_ip: TrustedClientIp) -> TurnstileVerification {
        let body = {
            // Serializer 不是 Send，必须在进入网络等待前显式销毁。
            let mut serializer = form_urlencoded::Serializer::new(String::new());
            serializer.append_pair(
                "secret",
                std::str::from_utf8(&self.secret_key).unwrap_or_default(),
            );
            serializer.append_pair("response", token);
            serializer.append_pair("remoteip", &client_ip.as_ip().to_string());
            serializer.finish()
        };

        let client = match self.clients.get(Some(self.request_timeout)) {
            Ok(client) => client,
            Err(_) => return TurnstileVerification::Unavailable,
        };
        let mut headers = HeaderMap::new();
        headers.insert(
            "content-type",
            HeaderValue::from_static("application/x-www-form-urlencoded"),
        );
        headers.insert("accept", HeaderValue::from_static("application/json"));
        let response = match client
            .execute(
                Method::POST,
                TURNSTILE_SITEVERIFY_URL,
                headers,
                Some(Body::from(body)),
            )
            .await
        {
            Ok(response) => response,
            Err(_) => return TurnstileVerification::Unavailable,
        };
        let status = response.status();
        if !status.is_success()
            || response
                .content_length()
                .is_some_and(|length| length > MAX_TURNSTILE_RESPONSE_BYTES as u64)
        {
            return TurnstileVerification::Unavailable;
        }
        let mut stream = response.into_bytes_stream();
        let mut body = Vec::new();
        while let Some(chunk) = stream.next_chunk().await {
            let chunk = match chunk {
                Ok(chunk) => chunk,
                Err(_) => return TurnstileVerification::Unavailable,
            };
            let next_len = match body.len().checked_add(chunk.len()) {
                Some(length) => length,
                None => return TurnstileVerification::Unavailable,
            };
            if next_len > MAX_TURNSTILE_RESPONSE_BYTES {
                return TurnstileVerification::Unavailable;
            }
            body.extend_from_slice(&chunk);
        }
        decode_response(status, &body)
    }
}

impl TurnstileVerifier for CloudflareTurnstileVerifier {
    fn verify<'a>(
        &'a self,
        token: &'a str,
        client_ip: TrustedClientIp,
    ) -> TurnstileVerificationFuture<'a> {
        Box::pin(async move { self.verify_inner(token, client_ip).await })
    }
}

impl Drop for CloudflareTurnstileVerifier {
    fn drop(&mut self) {
        self.secret_key.zeroize();
    }
}

impl fmt::Debug for CloudflareTurnstileVerifier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CloudflareTurnstileVerifier")
            .field("request_timeout", &self.request_timeout)
            .field("endpoint", &"<固定 Cloudflare 端点>")
            .finish_non_exhaustive()
    }
}

#[derive(Deserialize)]
struct SiteVerifyResponse {
    success: bool,
}

fn decode_response(status: StatusCode, body: &[u8]) -> TurnstileVerification {
    if !status.is_success() {
        return TurnstileVerification::Unavailable;
    }
    match serde_json::from_slice::<SiteVerifyResponse>(body) {
        Ok(response) if response.success => TurnstileVerification::Passed,
        Ok(_) => TurnstileVerification::Rejected,
        Err(_) => TurnstileVerification::Unavailable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_decoder_keeps_upstream_body_private_and_fail_closed() {
        assert_eq!(
            decode_response(StatusCode::OK, br#"{"success":true}"#),
            TurnstileVerification::Passed
        );
        assert_eq!(
            decode_response(
                StatusCode::OK,
                br#"{"success":false,"error-codes":["bad-request"]}"#
            ),
            TurnstileVerification::Rejected
        );
        assert_eq!(
            decode_response(StatusCode::OK, b"not-json"),
            TurnstileVerification::Unavailable
        );
        assert_eq!(
            decode_response(StatusCode::BAD_GATEWAY, br#"{"success":true}"#),
            TurnstileVerification::Unavailable
        );
    }
}
