use std::{
    collections::HashMap,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    thread,
    time::{Duration, UNIX_EPOCH},
};

use af_httpclient::{HttpClientConfig, HttpClientPool, HttpTimeouts, ProxyConfig, RemoteDnsPolicy};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
use serde::Deserialize;

use super::*;
use crate::AdaptorSendExt as _;

// 该公私钥对仅用于本地签名回归，随源码公开且不具备任何外部权限。
const TEST_PRIVATE_KEY: &str = include_str!("fixtures/service_account_private.pem");
const TEST_PUBLIC_KEY: &[u8] = include_bytes!("fixtures/service_account_public.pem");
const TEST_EMAIL: &str = "vertex-test@vertex-project.iam.gserviceaccount.com";
const TEST_KEY_ID: &str = "test-private-key-id";
const SERVER_TIMEOUT: Duration = Duration::from_secs(5);

fn context() -> RelayContext {
    RelayContext::new(
        HttpClientPool::default()
            .get(&HttpClientConfig::default())
            .unwrap(),
    )
}

fn service_account() -> Credential {
    Credential::service_account(TEST_EMAIL, Some(TEST_KEY_ID.to_owned()), TEST_PRIVATE_KEY).unwrap()
}

fn target(model: &str, operation: Operation, response_mode: ResponseMode) -> AdaptorTarget<'_> {
    AdaptorTarget::new(model, operation, response_mode)
}

#[test]
fn vertex_metadata_models_and_debug_are_stable() {
    let regional = VertexAdaptor::with_supported_models(
        "vertex-project",
        "us-central1",
        ["gemini-flash-test", "gemini-pro-test"],
    )
    .unwrap();
    assert_eq!(regional.channel_type(), ChannelType::Vertex);
    assert_eq!(regional.default_protocol(), Protocol::Gemini);
    assert_eq!(
        regional.default_base_url(),
        "https://us-central1-aiplatform.googleapis.com"
    );
    assert_eq!(
        regional.supported_models(),
        ["gemini-flash-test", "gemini-pro-test"]
    );
    let global = VertexAdaptor::new("vertex-project", "global").unwrap();
    assert_eq!(
        global.default_base_url(),
        "https://aiplatform.googleapis.com"
    );

    let debug = format!("{regional:?}");
    assert!(debug.contains("supported_model_count: 2"));
    for private in [
        "vertex-project",
        "us-central1",
        "gemini-flash-test",
        "gemini-pro-test",
        VertexAdaptor::TOKEN_ENDPOINT,
    ] {
        assert!(!debug.contains(private));
    }
}

#[test]
fn build_url_supports_regional_global_stream_and_proxy_prefixes() {
    let regional = VertexAdaptor::new("vertex-project", "us-central1").unwrap();
    assert_eq!(
        regional
            .build_url(
                &context(),
                target("gemini-2.5-flash", Operation::Chat, ResponseMode::Full),
            )
            .unwrap(),
        "https://us-central1-aiplatform.googleapis.com/v1/projects/vertex-project/locations/us-central1/publishers/google/models/gemini-2.5-flash:generateContent"
    );
    assert_eq!(
        regional
            .build_url(
                &context(),
                target("gemini-2.5-flash", Operation::Chat, ResponseMode::Stream,),
            )
            .unwrap(),
        "https://us-central1-aiplatform.googleapis.com/v1/projects/vertex-project/locations/us-central1/publishers/google/models/gemini-2.5-flash:streamGenerateContent?alt=sse"
    );

    let global = VertexAdaptor::new("123456789012", "global").unwrap();
    assert!(
        global
            .build_url(
                &context(),
                target("gemini-test", Operation::Chat, ResponseMode::Full),
            )
            .unwrap()
            .starts_with(
                "https://aiplatform.googleapis.com/v1/projects/123456789012/locations/global/"
            )
    );

    for base_url in [
        "https://gateway.example/proxy/vertex",
        "https://gateway.example/proxy/vertex/v1",
        "https://gateway.example/proxy/vertex/v1/",
    ] {
        let built = regional
            .build_url(
                &context().with_base_url(base_url).unwrap(),
                target("gemini-test", Operation::Chat, ResponseMode::Full),
            )
            .unwrap();
        assert!(built.ends_with(
            "/proxy/vertex/v1/projects/vertex-project/locations/us-central1/publishers/google/models/gemini-test:generateContent"
        ));
    }

    let encoded = regional
        .build_url(
            &context(),
            target(
                "gemini?mode=test#fragment",
                Operation::Chat,
                ResponseMode::Full,
            ),
        )
        .unwrap();
    assert!(encoded.contains("/gemini%3Fmode=test%23fragment:generateContent"));
    assert!(!encoded.contains("?mode="));
    assert!(!encoded.contains("#fragment"));
}

#[test]
fn vertex_configuration_and_request_target_fail_closed() {
    for project in [
        "",
        "short",
        "Uppercase-project",
        "vertex-project-",
        "vertex/project",
        "12345",
    ] {
        assert_eq!(
            VertexAdaptor::new(project, "us-central1").unwrap_err(),
            AdaptorError::InvalidGoogleProject
        );
    }
    assert_eq!(
        VertexAdaptor::new("p".repeat(MAX_VERTEX_PROJECT_ID_BYTES + 1), "us-central1").unwrap_err(),
        AdaptorError::InvalidGoogleProject
    );
    for location in ["", "US-CENTRAL1", "us/central1", "us-central1-"] {
        assert_eq!(
            VertexAdaptor::new("vertex-project", location).unwrap_err(),
            AdaptorError::InvalidGoogleLocation
        );
    }
    assert_eq!(
        VertexAdaptor::new("vertex-project", "l".repeat(MAX_VERTEX_LOCATION_BYTES + 1),)
            .unwrap_err(),
        AdaptorError::InvalidGoogleLocation
    );

    let adaptor = VertexAdaptor::new("vertex-project", "us-central1").unwrap();
    for model in [
        "",
        " gemini-test",
        "gemini-test ",
        "gemini/test",
        "gemini\nsecret",
    ] {
        assert_eq!(
            adaptor
                .build_url(
                    &context(),
                    target(model, Operation::Chat, ResponseMode::Full),
                )
                .unwrap_err(),
            AdaptorError::InvalidRequestTarget
        );
    }
    assert_eq!(
        adaptor
            .build_url(
                &context(),
                target("gemini-test", Operation::Responses, ResponseMode::Full),
            )
            .unwrap_err(),
        AdaptorError::UnsupportedOperation {
            operation: Operation::Responses
        }
    );
}

#[test]
fn service_account_credential_is_structured_validated_and_redacted() {
    let credential = service_account();
    assert_eq!(credential.kind(), CredentialKind::ServiceAccount);
    assert_eq!(
        credential.service_account_parts(),
        Some((TEST_EMAIL, Some(TEST_KEY_ID), TEST_PRIVATE_KEY))
    );
    let debug = format!("{credential:?}");
    for private in [TEST_EMAIL, TEST_KEY_ID, "MIIEvgIBADAN"] {
        assert!(!debug.contains(private));
    }

    for invalid in [
        Credential::service_account("missing-at.example", None, TEST_PRIVATE_KEY),
        Credential::service_account("fake@example.com", None, TEST_PRIVATE_KEY),
        Credential::service_account("space @example.com", None, TEST_PRIVATE_KEY),
        Credential::service_account(TEST_EMAIL, Some(String::new()), TEST_PRIVATE_KEY),
        Credential::service_account(TEST_EMAIL, None, "not-a-private-key"),
    ] {
        assert_eq!(invalid, Err(AdaptorError::InvalidCredential));
    }
}

#[test]
fn service_account_assertion_uses_fixed_official_claims_and_rs256() {
    let adaptor = VertexAdaptor::new("vertex-project", "us-central1").unwrap();
    let issued_at = 1_700_000_000_u64;
    let assertion = adaptor
        .build_assertion_at(
            &service_account(),
            UNIX_EPOCH + Duration::from_secs(issued_at),
        )
        .unwrap();
    let claims = decode_assertion(&assertion);
    assert_eq!(claims.iss, TEST_EMAIL);
    assert_eq!(claims.scope, VertexAdaptor::CLOUD_PLATFORM_SCOPE);
    assert_eq!(claims.aud, VertexAdaptor::TOKEN_ENDPOINT);
    assert_eq!(claims.iat, issued_at);
    assert_eq!(claims.exp, issued_at + JWT_TTL.as_secs());
}

#[test]
fn setup_headers_accepts_service_account_or_oauth_and_rejects_api_key() {
    let adaptor = VertexAdaptor::new("vertex-project", "us-central1").unwrap();
    let context = context().with_request_id("request-vertex-1").unwrap();
    let mut headers = HeaderMap::new();
    headers.insert(
        HeaderName::from_static("authorization"),
        HeaderValue::from_static("Bearer stale-token"),
    );
    headers.insert(
        HeaderName::from_static("x-goog-api-key"),
        HeaderValue::from_static("stale-api-key"),
    );
    adaptor
        .setup_headers(&mut headers, &service_account(), &context)
        .unwrap();
    assert!(headers.get("authorization").is_none());
    assert!(headers.get("x-goog-api-key").is_none());
    assert_eq!(headers["content-type"], JSON_CONTENT_TYPE);
    assert_eq!(headers["accept"], JSON_CONTENT_TYPE);
    assert_eq!(headers["x-request-id"], "request-vertex-1");

    adaptor
        .setup_headers(
            &mut headers,
            &Credential::oauth("vertex-oauth-token").unwrap(),
            &context,
        )
        .unwrap();
    assert_eq!(headers["authorization"], "Bearer vertex-oauth-token");
    assert!(!format!("{headers:?}").contains("vertex-oauth-token"));
    assert_eq!(
        adaptor
            .setup_headers(
                &mut headers,
                &Credential::api_key("vertex-api-key").unwrap(),
                &context,
            )
            .unwrap_err(),
        AdaptorError::UnsupportedCredential {
            kind: CredentialKind::ApiKey
        }
    );
}

#[tokio::test]
async fn service_account_exchange_and_vertex_transport_use_controlled_proxy() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut token_stream, _) = listener.accept().unwrap();
        token_stream.set_read_timeout(Some(SERVER_TIMEOUT)).unwrap();
        let token_request = read_request(&mut token_stream);
        assert!(token_request.starts_with("POST http://oauth2.googleapis.com/token HTTP/1.1"));
        assert!(
            token_request
                .to_ascii_lowercase()
                .contains("content-type: application/x-www-form-urlencoded")
        );
        let form = request_body(&token_request);
        let values = form_urlencoded::parse(form.as_bytes())
            .into_owned()
            .collect::<HashMap<_, _>>();
        assert_eq!(values.get("grant_type").unwrap(), JWT_GRANT_TYPE);
        let claims = decode_assertion(values.get("assertion").unwrap());
        assert_eq!(claims.iss, TEST_EMAIL);
        assert_eq!(claims.aud, VertexAdaptor::TOKEN_ENDPOINT);
        write_response(
            &mut token_stream,
            "application/json",
            br#"{"access_token":"vertex-access-token","token_type":"Bearer","expires_in":3600}"#,
        );

        let (mut model_stream, _) = listener.accept().unwrap();
        model_stream.set_read_timeout(Some(SERVER_TIMEOUT)).unwrap();
        let model_request = read_request(&mut model_stream);
        assert!(model_request.starts_with(
            "POST http://vertex.example/proxy/vertex/v1/projects/vertex-project/locations/us-central1/publishers/google/models/gemini-test:generateContent HTTP/1.1"
        ));
        let lowercase = model_request.to_ascii_lowercase();
        assert!(lowercase.contains("authorization: bearer vertex-access-token"));
        assert!(!lowercase.contains("stale-token"));
        assert!(!lowercase.contains("stale-api-key"));
        assert!(lowercase.contains("content-type: application/json"));
        assert!(model_request.ends_with(r#"{"contents":[]}"#));
        write_response(
            &mut model_stream,
            "application/json",
            br#"{"candidates":[]}"#,
        );
    });

    let config = HttpClientConfig::new(
        ProxyConfig::parse(format!("http://{address}")).unwrap(),
        HttpTimeouts::default(),
    )
    .with_remote_dns_policy(RemoteDnsPolicy::TrustProxy);
    let client = HttpClientPool::default().get(&config).unwrap();
    let context = RelayContext::new(client)
        .with_base_url("http://vertex.example/proxy/vertex")
        .unwrap()
        .with_request_id("request-vertex-loopback")
        .unwrap();
    let mut adaptor =
        VertexAdaptor::with_supported_models("vertex-project", "us-central1", ["gemini-test"])
            .unwrap();
    // 仅测试替换传输目标；JWT 的 aud 仍固定为官方 HTTPS token endpoint。
    adaptor.token_endpoint = "http://oauth2.googleapis.com/token".to_owned();
    let credential = service_account();
    let mut headers = HeaderMap::new();
    adaptor
        .setup_headers(&mut headers, &credential, &context)
        .unwrap();
    headers.insert(
        HeaderName::from_static("authorization"),
        HeaderValue::from_static("Bearer stale-token"),
    );
    headers.insert(
        HeaderName::from_static("x-goog-api-key"),
        HeaderValue::from_static("stale-api-key"),
    );
    headers.insert(
        HeaderName::from_static("content-type"),
        HeaderValue::from_static("text/plain"),
    );
    let request = UpstreamRequest::new(
        Method::POST,
        adaptor
            .build_url(
                &context,
                target("gemini-test", Operation::Chat, ResponseMode::Full),
            )
            .unwrap(),
        headers,
        Some(af_httpclient::Bytes::from_static(br#"{"contents":[]}"#)),
    )
    .unwrap();
    let request = adaptor
        .finalize_request(request, &credential, &context)
        .await
        .unwrap();
    assert_eq!(
        request.headers()["authorization"],
        "Bearer vertex-access-token"
    );
    assert_eq!(request.headers()["content-type"], JSON_CONTENT_TYPE);
    assert!(!format!("{:?}", request.headers()).contains("vertex-access-token"));

    let response = adaptor.send(request, &context).await.unwrap();
    assert_eq!(
        response.into_body().into_bytes().await.unwrap(),
        af_httpclient::Bytes::from_static(br#"{"candidates":[]}"#)
    );
    server.join().unwrap();
}

#[derive(Debug, Deserialize)]
struct TestClaims {
    iss: String,
    scope: String,
    aud: String,
    iat: u64,
    exp: u64,
}

fn decode_assertion(assertion: &str) -> TestClaims {
    let mut validation = Validation::new(Algorithm::RS256);
    validation.validate_exp = false;
    validation.validate_aud = false;
    let decoded = decode::<TestClaims>(
        assertion,
        &DecodingKey::from_rsa_pem(TEST_PUBLIC_KEY).unwrap(),
        &validation,
    )
    .unwrap();
    assert_eq!(decoded.header.alg, Algorithm::RS256);
    assert_eq!(decoded.header.kid.as_deref(), Some(TEST_KEY_ID));
    decoded.claims
}

fn read_request(stream: &mut TcpStream) -> String {
    let mut request = Vec::with_capacity(2_048);
    let mut buffer = [0_u8; 1_024];
    let header_end = loop {
        let read = stream.read(&mut buffer).unwrap();
        assert!(read > 0, "客户端在测试请求头结束前关闭连接");
        request.extend_from_slice(&buffer[..read]);
        assert!(request.len() <= 64 * 1_024, "测试请求头超过 64 KiB");
        if let Some(index) = request.windows(4).position(|window| window == b"\r\n\r\n") {
            break index + 4;
        }
    };
    let headers = String::from_utf8(request[..header_end].to_vec()).unwrap();
    let content_length = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().unwrap())
        })
        .unwrap_or(0);
    while request.len() - header_end < content_length {
        let read = stream.read(&mut buffer).unwrap();
        assert!(read > 0, "客户端在测试请求体结束前关闭连接");
        request.extend_from_slice(&buffer[..read]);
    }
    String::from_utf8(request).unwrap()
}

fn request_body(request: &str) -> &str {
    request
        .split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .unwrap()
}

fn write_response(stream: &mut TcpStream, content_type: &str, body: &[u8]) {
    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes()).unwrap();
    stream.write_all(body).unwrap();
    stream.flush().unwrap();
}
