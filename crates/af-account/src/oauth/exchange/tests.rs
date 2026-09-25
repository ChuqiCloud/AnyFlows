use std::{
    collections::HashMap,
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    thread::{self, JoinHandle},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use af_domain::{ChannelId, CredentialId, UserId};
use af_httpclient::{
    HttpClientConfig, HttpClientPool, HttpTimeouts, PooledClient, ProxyConfig, RemoteDnsPolicy,
};
use base64::{
    Engine as _,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use url::{Url, form_urlencoded};

use super::*;
use crate::oauth::authorization::{OAuthAuthorizationContext, OAuthAuthorizationGrant};

const CALLBACK_URI: &str = "http://127.0.0.1:43123/oauth/callback";
const AUTHORIZATION_CODE: &str = "authorization-code-marker";
const CODE_VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
const REFRESH_TOKEN: &str = "refresh-token-marker";
const REFRESH_SCOPE: &str = "openid profile";
const SERVER_TIMEOUT: Duration = Duration::from_secs(5);

fn context() -> OAuthAuthorizationContext {
    OAuthAuthorizationContext::new(
        UserId::new(11).unwrap(),
        Some(ChannelId::new(22).unwrap()),
        Some(CredentialId::new(33).unwrap()),
    )
    .unwrap()
}

fn grant(provider: UpstreamOAuthProvider) -> OAuthAuthorizationGrant {
    OAuthAuthorizationGrant::new(
        provider,
        context(),
        Url::parse(CALLBACK_URI).unwrap(),
        "state-marker".to_owned(),
        AUTHORIZATION_CODE.to_owned(),
        CODE_VERIFIER.to_owned(),
    )
}

fn refresh_request(provider: UpstreamOAuthProvider) -> OAuthTokenRefreshRequest {
    OAuthTokenRefreshRequest::new(
        provider,
        REFRESH_TOKEN.to_owned(),
        Some(REFRESH_SCOPE.to_owned()),
    )
    .unwrap()
}

fn exchange(
    client_id: &str,
    method: OAuthClientAuthenticationMethod,
    client_secret: Option<&str>,
) -> OAuthTokenExchange {
    let mut exchange = OAuthTokenExchange::new(
        UpstreamOAuthProvider::ClaudeCode,
        client_id.to_owned(),
        "https://token.example/oauth/token".to_owned(),
        method,
        client_secret.map(str::to_owned),
    )
    .unwrap();
    // 测试只替换传输目标，生产构造器仍强制 HTTPS。
    exchange.token_endpoint = Url::parse("http://token.example/oauth/token").unwrap();
    exchange
}

fn proxied_client(address: SocketAddr) -> PooledClient {
    let config = HttpClientConfig::new(
        ProxyConfig::parse(format!("http://{address}")).unwrap(),
        HttpTimeouts::default(),
    )
    .with_remote_dns_policy(RemoteDnsPolicy::TrustProxy);
    HttpClientPool::default().get(&config).unwrap()
}

fn spawn_json_server<F>(
    status: u16,
    body: &'static [u8],
    inspect: F,
) -> (SocketAddr, JoinHandle<()>)
where
    F: FnOnce(&str) + Send + 'static,
{
    spawn_raw_server(
        move |stream| {
            let reason = if status == 200 { "OK" } else { "Bad Request" };
            let headers = format!(
                "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(headers.as_bytes()).unwrap();
            stream.write_all(body).unwrap();
        },
        inspect,
    )
}

fn spawn_raw_server<R, F>(respond: R, inspect: F) -> (SocketAddr, JoinHandle<()>)
where
    R: FnOnce(&mut TcpStream) + Send + 'static,
    F: FnOnce(&str) + Send + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream.set_read_timeout(Some(SERVER_TIMEOUT)).unwrap();
        let request = read_request(&mut stream);
        inspect(&request);
        respond(&mut stream);
    });
    (address, handle)
}

fn read_request(stream: &mut TcpStream) -> String {
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 4_096];
    loop {
        let count = stream.read(&mut buffer).unwrap();
        assert!(count > 0, "请求在正文完整前结束");
        bytes.extend_from_slice(&buffer[..count]);
        let Some(header_end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") else {
            continue;
        };
        let header_end = header_end + 4;
        let headers = String::from_utf8_lossy(&bytes[..header_end]);
        let content_length = headers
            .lines()
            .find_map(|line| {
                line.split_once(':').and_then(|(name, value)| {
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().unwrap())
                })
            })
            .unwrap_or(0);
        if bytes.len() >= header_end + content_length {
            return String::from_utf8(bytes).unwrap();
        }
    }
}

fn form_values(request: &str) -> HashMap<String, String> {
    let (_, body) = request.split_once("\r\n\r\n").unwrap();
    form_urlencoded::parse(body.as_bytes())
        .into_owned()
        .collect()
}

fn header_value<'a>(request: &'a str, expected_name: &str) -> Option<&'a str> {
    request.lines().find_map(|line| {
        line.split_once(':').and_then(|(name, value)| {
            name.eq_ignore_ascii_case(expected_name)
                .then(|| value.trim())
        })
    })
}

#[test]
fn configuration_rejects_unsafe_endpoints_and_authentication_mismatches() {
    for endpoint in [
        "http://token.example/oauth/token",
        "https://user:secret@token.example/oauth/token",
        "https://token.example/oauth/token#fragment",
        "https://token.example:0/oauth/token",
    ] {
        assert_eq!(
            OAuthTokenExchange::new(
                UpstreamOAuthProvider::ClaudeCode,
                "client".to_owned(),
                endpoint.to_owned(),
                OAuthClientAuthenticationMethod::Public,
                None,
            )
            .unwrap_err(),
            OAuthTokenExchangeError::InvalidTokenEndpoint
        );
    }
    assert_eq!(
        OAuthTokenExchange::new(
            UpstreamOAuthProvider::ClaudeCode,
            "client id".to_owned(),
            "https://token.example/oauth/token".to_owned(),
            OAuthClientAuthenticationMethod::Public,
            None,
        )
        .unwrap_err(),
        OAuthTokenExchangeError::InvalidClientId
    );
    for (method, secret) in [
        (OAuthClientAuthenticationMethod::Public, Some("unexpected")),
        (OAuthClientAuthenticationMethod::ClientSecretBasic, None),
        (
            OAuthClientAuthenticationMethod::ClientSecretPost,
            Some("bad\nsecret"),
        ),
    ] {
        assert_eq!(
            OAuthTokenExchange::new(
                UpstreamOAuthProvider::ClaudeCode,
                "client".to_owned(),
                "https://token.example/oauth/token".to_owned(),
                method,
                secret.map(str::to_owned),
            )
            .unwrap_err(),
            OAuthTokenExchangeError::InvalidClientAuthentication
        );
    }
    assert!(
        OAuthTokenExchange::new(
            UpstreamOAuthProvider::ClaudeCode,
            "client".to_owned(),
            "https://token.example/oauth/token".to_owned(),
            OAuthClientAuthenticationMethod::ClientSecretPost,
            Some("secret with visible spaces".to_owned()),
        )
        .is_ok()
    );

    let exchange = OAuthTokenExchange::new(
        UpstreamOAuthProvider::ClaudeCode,
        "private-client-marker".to_owned(),
        "https://token-private.example/oauth/token?tenant=private".to_owned(),
        OAuthClientAuthenticationMethod::ClientSecretBasic,
        Some("client-secret-marker".to_owned()),
    )
    .unwrap();
    let debug = format!("{exchange:?}");
    for private in [
        "private-client-marker",
        "token-private.example",
        "tenant=private",
        "client-secret-marker",
    ] {
        assert!(!debug.contains(private));
    }
}

#[test]
fn encoded_token_request_has_a_hard_length_limit() {
    let exchange = OAuthTokenExchange::new(
        UpstreamOAuthProvider::ClaudeCode,
        "client".to_owned(),
        "https://token.example/oauth/token".to_owned(),
        OAuthClientAuthenticationMethod::ClientSecretPost,
        Some("s".repeat(MAX_OAUTH_CLIENT_SECRET_BYTES)),
    )
    .unwrap();
    let redirect_uri = Url::parse(&format!(
        "http://127.0.0.1:43123/{}",
        "r".repeat(16 * 1_024)
    ))
    .unwrap();
    let grant = OAuthAuthorizationGrant::new(
        UpstreamOAuthProvider::ClaudeCode,
        context(),
        redirect_uri,
        "state-marker".to_owned(),
        "c".repeat(4 * 1_024),
        CODE_VERIFIER.to_owned(),
    );

    assert_eq!(
        exchange.build_form(&grant).unwrap_err(),
        OAuthTokenExchangeError::RequestTooLarge
    );

    let refresh_request = OAuthTokenRefreshRequest::new(
        UpstreamOAuthProvider::ClaudeCode,
        "r".repeat(MAX_DECRYPTED_CREDENTIAL_SECRET_BYTES),
        None,
    )
    .unwrap();
    assert_eq!(
        exchange.build_refresh_form(&refresh_request).unwrap_err(),
        OAuthTokenExchangeError::RequestTooLarge
    );
}

#[test]
fn refresh_request_validates_and_redacts_renewable_material() {
    for (refresh_token, scope) in [
        ("bad token", None),
        ("refresh-token", Some("openid  profile")),
    ] {
        assert_eq!(
            OAuthTokenRefreshRequest::new(
                UpstreamOAuthProvider::Codex,
                refresh_token.to_owned(),
                scope.map(str::to_owned),
            )
            .unwrap_err(),
            OAuthTokenExchangeError::InvalidRefreshRequest
        );
    }

    let request = refresh_request(UpstreamOAuthProvider::Codex);
    assert_eq!(request.provider(), UpstreamOAuthProvider::Codex);
    assert_eq!(request.refresh_token(), REFRESH_TOKEN);
    assert_eq!(request.scope(), Some(REFRESH_SCOPE));
    let debug = format!("{request:?}");
    assert!(!debug.contains(REFRESH_TOKEN));
    assert!(!debug.contains(REFRESH_SCOPE));
}

#[tokio::test]
async fn public_client_exchange_sends_exact_pkce_form_and_returns_typed_tokens() {
    let (address, server) = spawn_json_server(
        200,
        br#"{"access_token":"access-token-marker","token_type":"Bearer","expires_in":3600,"refresh_token":"refresh-token-marker","scope":"openid profile","account":{"uuid":"550E8400-E29B-41D4-A716-446655440000","email_address":"private@example.com"},"provider_extension":"ignored"}"#,
        |request| {
            assert!(request.starts_with("POST http://token.example/oauth/token HTTP/1.1"));
            assert_eq!(
                header_value(request, "content-type"),
                Some(FORM_CONTENT_TYPE)
            );
            assert_eq!(header_value(request, "accept"), Some(JSON_CONTENT_TYPE));
            assert!(header_value(request, "authorization").is_none());
            let values = form_values(request);
            assert_eq!(
                values.get("grant_type").map(String::as_str),
                Some(AUTHORIZATION_CODE_GRANT_TYPE)
            );
            assert_eq!(
                values.get("code").map(String::as_str),
                Some(AUTHORIZATION_CODE)
            );
            assert_eq!(
                values.get("redirect_uri").map(String::as_str),
                Some(CALLBACK_URI)
            );
            assert_eq!(
                values.get("code_verifier").map(String::as_str),
                Some(CODE_VERIFIER)
            );
            assert_eq!(
                values.get("client_id").map(String::as_str),
                Some("public-client")
            );
            assert!(!values.contains_key("client_secret"));
        },
    );
    let result = exchange(
        "public-client",
        OAuthClientAuthenticationMethod::Public,
        None,
    )
    .exchange(
        &proxied_client(address),
        grant(UpstreamOAuthProvider::ClaudeCode),
    )
    .await
    .unwrap();
    server.join().unwrap();

    assert_eq!(result.provider(), UpstreamOAuthProvider::ClaudeCode);
    assert_eq!(result.context(), context());
    assert_eq!(result.access_token(), "access-token-marker");
    assert_eq!(result.refresh_token(), Some("refresh-token-marker"));
    assert_eq!(result.expires_in(), Some(Duration::from_secs(3_600)));
    assert_eq!(result.scope(), Some("openid profile"));
    assert_eq!(
        result.oauth_account_key(),
        Some("550e8400-e29b-41d4-a716-446655440000")
    );
    assert_eq!(result.oauth_project_id(), None);
    let debug = format!("{result:?}");
    for private in [
        "access-token-marker",
        "refresh-token-marker",
        "openid profile",
        "550e8400-e29b-41d4-a716-446655440000",
        "private@example.com",
    ] {
        assert!(!debug.contains(private));
    }
}

#[tokio::test]
async fn codex_exchange_requires_and_extracts_the_account_claim() {
    let (address, server) = spawn_json_server(
        200,
        br#"{"access_token":"access-token-marker","token_type":"Bearer","expires_in":3600,"refresh_token":"refresh-token-marker","id_token":"eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.eyJodHRwczovL2FwaS5vcGVuYWkuY29tL2F1dGguY2hhdGdwdF9hY2NvdW50X2lkIjoib3JnLWNvZGV4LXRlc3QifQ.c2lnbmF0dXJl"}"#,
        |_| {},
    );
    let mut exchange = OAuthTokenExchange::new(
        UpstreamOAuthProvider::Codex,
        "public-client".to_owned(),
        "https://token.example/oauth/token".to_owned(),
        OAuthClientAuthenticationMethod::Public,
        None,
    )
    .unwrap();
    exchange.token_endpoint = Url::parse("http://token.example/oauth/token").unwrap();
    let result = exchange
        .exchange(
            &proxied_client(address),
            grant(UpstreamOAuthProvider::Codex),
        )
        .await
        .unwrap();
    server.join().unwrap();

    assert_eq!(result.oauth_account_key(), Some("org-codex-test"));
    assert_eq!(result.oauth_project_id(), None);
}

#[test]
fn codex_response_accepts_current_token_shape_and_derives_expiration() {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let access_token = test_jwt(serde_json::json!({ "exp": now + 3_600 }));
    let id_token = test_jwt(serde_json::json!({
        "https://api.openai.com/auth": { "chatgpt_account_id": "org-codex-test" }
    }));
    let wire: TokenResponseWire = serde_json::from_value(serde_json::json!({
        "access_token": access_token,
        "refresh_token": "refresh-token-marker",
        "id_token": id_token,
    }))
    .unwrap();
    let initial = OAuthTokenSet::from_wire(UpstreamOAuthProvider::Codex, context(), wire).unwrap();
    assert_eq!(initial.oauth_account_key(), Some("org-codex-test"));
    assert!((3_599..=3_600).contains(&initial.expires_in().unwrap().as_secs()));

    let wire: TokenResponseWire = serde_json::from_value(serde_json::json!({
        "access_token": access_token,
    }))
    .unwrap();
    let refreshed = OAuthRefreshedTokenSet::from_wire(
        UpstreamOAuthProvider::Codex,
        refresh_request(UpstreamOAuthProvider::Codex),
        wire,
    )
    .unwrap();
    assert_eq!(refreshed.refresh_token(), REFRESH_TOKEN);
    assert!((3_599..=3_600).contains(&refreshed.expires_in().unwrap().as_secs()));

    let wire: TokenResponseWire = serde_json::from_value(serde_json::json!({
        "access_token": "opaque-access-token",
        "refresh_token": "refresh-token-marker",
        "id_token": id_token,
    }))
    .unwrap();
    assert!(matches!(
        OAuthTokenSet::from_wire(UpstreamOAuthProvider::Codex, context(), wire),
        Err(OAuthTokenExchangeError::InvalidTokenResponse)
    ));
    let wire: TokenResponseWire = serde_json::from_value(serde_json::json!({
        "access_token": access_token,
        "token_type": "mac",
        "refresh_token": "refresh-token-marker",
        "id_token": id_token,
    }))
    .unwrap();
    assert!(matches!(
        OAuthTokenSet::from_wire(UpstreamOAuthProvider::Codex, context(), wire),
        Err(OAuthTokenExchangeError::InvalidTokenResponse)
    ));
}

fn test_jwt(payload: serde_json::Value) -> String {
    format!(
        "{}.{}.{}",
        URL_SAFE_NO_PAD.encode(br#"{"alg":"RS256","typ":"JWT"}"#),
        URL_SAFE_NO_PAD.encode(payload.to_string()),
        URL_SAFE_NO_PAD.encode("signature")
    )
}

#[tokio::test]
async fn public_client_refresh_sends_closed_form_and_rotates_tokens() {
    let (address, server) = spawn_json_server(
        200,
        br#"{"access_token":"new-access-token","token_type":"Bearer","expires_in":1800,"refresh_token":"rotated-refresh-token","scope":"openid email"}"#,
        |request| {
            assert!(request.starts_with("POST http://token.example/oauth/token HTTP/1.1"));
            assert_eq!(
                header_value(request, "content-type"),
                Some(FORM_CONTENT_TYPE)
            );
            assert_eq!(header_value(request, "accept"), Some(JSON_CONTENT_TYPE));
            assert!(header_value(request, "authorization").is_none());
            let values = form_values(request);
            assert_eq!(values.len(), 3, "刷新表单不得出现协议白名单之外的字段");
            assert_eq!(
                values.get("grant_type").map(String::as_str),
                Some(REFRESH_TOKEN_GRANT_TYPE)
            );
            assert_eq!(
                values.get("refresh_token").map(String::as_str),
                Some(REFRESH_TOKEN)
            );
            assert_eq!(
                values.get("client_id").map(String::as_str),
                Some("public-client")
            );
            for forbidden in [
                "client_secret",
                "scope",
                "state",
                "code",
                "redirect_uri",
                "code_verifier",
            ] {
                assert!(!values.contains_key(forbidden));
            }
        },
    );
    let result = exchange(
        "public-client",
        OAuthClientAuthenticationMethod::Public,
        None,
    )
    .refresh(
        &proxied_client(address),
        refresh_request(UpstreamOAuthProvider::ClaudeCode),
    )
    .await
    .unwrap();
    server.join().unwrap();

    assert_eq!(result.provider(), UpstreamOAuthProvider::ClaudeCode);
    assert_eq!(result.access_token(), "new-access-token");
    assert_eq!(result.refresh_token(), "rotated-refresh-token");
    assert_eq!(result.expires_in(), Some(Duration::from_secs(1_800)));
    assert_eq!(result.scope(), Some("openid email"));
    let debug = format!("{result:?}");
    for private in ["new-access-token", "rotated-refresh-token", "openid email"] {
        assert!(!debug.contains(private));
    }
}

#[tokio::test]
async fn json_exchange_preserves_state_and_rejects_field_injection() {
    let mut exchange = OAuthTokenExchange::new_with_request_encoding(
        UpstreamOAuthProvider::ClaudeCode,
        "public-client".to_owned(),
        "https://token.example/oauth/token".to_owned(),
        OAuthClientAuthenticationMethod::Public,
        None,
        OAuthTokenRequestEncoding::Json,
    )
    .unwrap();
    exchange.token_endpoint = Url::parse("http://token.example/oauth/token").unwrap();
    let (address, server) = spawn_json_server(
        200,
        br#"{"access_token":"access-token-marker","token_type":"Bearer","refresh_token":"refresh-token-marker"}"#,
        |request| {
            assert_eq!(header_value(request, "content-type"), Some(JSON_CONTENT_TYPE));
            assert_eq!(header_value(request, "accept"), Some(JSON_CONTENT_TYPE));
            assert!(header_value(request, "authorization").is_none());
            let (_, body) = request.split_once("\r\n\r\n").unwrap();
            let values = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(body)
                .unwrap();
            assert_eq!(values.len(), 6, "JSON 请求不得出现 profile 之外的字段");
            assert_eq!(values.get("grant_type").and_then(|value| value.as_str()), Some(AUTHORIZATION_CODE_GRANT_TYPE));
            assert_eq!(values.get("code").and_then(|value| value.as_str()), Some(AUTHORIZATION_CODE));
            assert_eq!(values.get("state").and_then(|value| value.as_str()), Some("state-marker"));
            assert_eq!(values.get("client_id").and_then(|value| value.as_str()), Some("public-client"));
            assert_eq!(values.get("redirect_uri").and_then(|value| value.as_str()), Some(CALLBACK_URI));
            assert_eq!(values.get("code_verifier").and_then(|value| value.as_str()), Some(CODE_VERIFIER));
            assert!(!values.contains_key("client_secret"));
        },
    );

    let result = exchange
        .exchange(
            &proxied_client(address),
            grant(UpstreamOAuthProvider::ClaudeCode),
        )
        .await
        .unwrap();
    server.join().unwrap();
    assert_eq!(result.provider(), UpstreamOAuthProvider::ClaudeCode);
    assert_eq!(result.access_token(), "access-token-marker");
    assert_eq!(result.refresh_token(), Some("refresh-token-marker"));
    assert_eq!(exchange.request_encoding(), OAuthTokenRequestEncoding::Json);
    let debug = format!("{exchange:?}{result:?}");
    for private in [
        "state-marker",
        AUTHORIZATION_CODE,
        CODE_VERIFIER,
        "access-token-marker",
        "refresh-token-marker",
    ] {
        assert!(!debug.contains(private));
    }
}

#[tokio::test]
async fn json_refresh_preserves_omitted_refresh_token_and_scope() {
    let mut exchange = OAuthTokenExchange::new_with_request_encoding(
        UpstreamOAuthProvider::ClaudeCode,
        "public-client".to_owned(),
        "https://token.example/oauth/token".to_owned(),
        OAuthClientAuthenticationMethod::Public,
        None,
        OAuthTokenRequestEncoding::Json,
    )
    .unwrap();
    exchange.token_endpoint = Url::parse("http://token.example/oauth/token").unwrap();
    let (address, server) = spawn_json_server(
        200,
        br#"{"access_token":"new-access-token","token_type":"bearer","expires_in":900}"#,
        |request| {
            assert_eq!(
                header_value(request, "content-type"),
                Some(JSON_CONTENT_TYPE)
            );
            assert_eq!(header_value(request, "accept"), Some(JSON_CONTENT_TYPE));
            assert!(header_value(request, "authorization").is_none());
            let (_, body) = request.split_once("\r\n\r\n").unwrap();
            let values =
                serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(body).unwrap();
            assert_eq!(values.len(), 3, "刷新 JSON 不得出现协议白名单之外的字段");
            assert_eq!(
                values.get("grant_type").and_then(|value| value.as_str()),
                Some(REFRESH_TOKEN_GRANT_TYPE)
            );
            assert_eq!(
                values.get("refresh_token").and_then(|value| value.as_str()),
                Some(REFRESH_TOKEN)
            );
            assert_eq!(
                values.get("client_id").and_then(|value| value.as_str()),
                Some("public-client")
            );
            for forbidden in ["client_secret", "scope", "state", "code", "redirect_uri"] {
                assert!(!values.contains_key(forbidden));
            }
        },
    );

    let result = exchange
        .refresh(
            &proxied_client(address),
            refresh_request(UpstreamOAuthProvider::ClaudeCode),
        )
        .await
        .unwrap();
    server.join().unwrap();
    assert_eq!(result.access_token(), "new-access-token");
    assert_eq!(result.refresh_token(), REFRESH_TOKEN);
    assert_eq!(result.expires_in(), Some(Duration::from_secs(900)));
    assert_eq!(result.scope(), Some(REFRESH_SCOPE));
}

#[tokio::test]
async fn confidential_client_authentication_is_closed_and_unambiguous() {
    for method in [
        OAuthClientAuthenticationMethod::ClientSecretBasic,
        OAuthClientAuthenticationMethod::ClientSecretPost,
    ] {
        let (address, server) = spawn_json_server(
            200,
            br#"{"access_token":"access-token","token_type":"bearer"}"#,
            move |request| {
                let values = form_values(request);
                match method {
                    OAuthClientAuthenticationMethod::ClientSecretBasic => {
                        assert!(!values.contains_key("client_id"));
                        assert!(!values.contains_key("client_secret"));
                        let authorization = header_value(request, "authorization").unwrap();
                        let encoded = authorization.strip_prefix("Basic ").unwrap();
                        let decoded = STANDARD.decode(encoded).unwrap();
                        assert_eq!(decoded, b"client%3Aid:secret%2Fvalue");
                    }
                    OAuthClientAuthenticationMethod::ClientSecretPost => {
                        assert!(header_value(request, "authorization").is_none());
                        assert_eq!(
                            values.get("client_id").map(String::as_str),
                            Some("client:id")
                        );
                        assert_eq!(
                            values.get("client_secret").map(String::as_str),
                            Some("secret/value")
                        );
                    }
                    OAuthClientAuthenticationMethod::Public => unreachable!(),
                }
            },
        );
        let result = exchange("client:id", method, Some("secret/value"))
            .exchange(
                &proxied_client(address),
                grant(UpstreamOAuthProvider::ClaudeCode),
            )
            .await
            .unwrap();
        server.join().unwrap();
        assert_eq!(result.access_token(), "access-token");
        assert_eq!(result.refresh_token(), None);
        assert_eq!(result.expires_in(), None);
    }
}

#[tokio::test]
async fn confidential_refresh_authentication_is_closed_and_unambiguous() {
    for method in [
        OAuthClientAuthenticationMethod::ClientSecretBasic,
        OAuthClientAuthenticationMethod::ClientSecretPost,
    ] {
        let (address, server) = spawn_json_server(
            200,
            br#"{"access_token":"new-access-token","token_type":"bearer","expires_in":3600}"#,
            move |request| {
                let values = form_values(request);
                assert_eq!(
                    values.get("grant_type").map(String::as_str),
                    Some(REFRESH_TOKEN_GRANT_TYPE)
                );
                assert_eq!(
                    values.get("refresh_token").map(String::as_str),
                    Some(REFRESH_TOKEN)
                );
                match method {
                    OAuthClientAuthenticationMethod::ClientSecretBasic => {
                        assert_eq!(values.len(), 2);
                        assert!(!values.contains_key("client_id"));
                        assert!(!values.contains_key("client_secret"));
                        let authorization = header_value(request, "authorization").unwrap();
                        let encoded = authorization.strip_prefix("Basic ").unwrap();
                        let decoded = STANDARD.decode(encoded).unwrap();
                        assert_eq!(decoded, b"client%3Aid:secret%2Fvalue");
                    }
                    OAuthClientAuthenticationMethod::ClientSecretPost => {
                        assert_eq!(values.len(), 4);
                        assert!(header_value(request, "authorization").is_none());
                        assert_eq!(
                            values.get("client_id").map(String::as_str),
                            Some("client:id")
                        );
                        assert_eq!(
                            values.get("client_secret").map(String::as_str),
                            Some("secret/value")
                        );
                    }
                    OAuthClientAuthenticationMethod::Public => unreachable!(),
                }
                assert!(!values.contains_key("scope"));
            },
        );
        let result = exchange("client:id", method, Some("secret/value"))
            .refresh(
                &proxied_client(address),
                refresh_request(UpstreamOAuthProvider::ClaudeCode),
            )
            .await
            .unwrap();
        server.join().unwrap();
        assert_eq!(result.access_token(), "new-access-token");
        assert_eq!(result.refresh_token(), REFRESH_TOKEN);
        assert_eq!(result.scope(), Some(REFRESH_SCOPE));
    }
}

#[tokio::test]
async fn provider_mismatch_fails_before_transport() {
    let client = HttpClientPool::default()
        .get(&HttpClientConfig::default())
        .unwrap();
    let error = exchange(
        "public-client",
        OAuthClientAuthenticationMethod::Public,
        None,
    )
    .exchange(&client, grant(UpstreamOAuthProvider::Codex))
    .await
    .unwrap_err();

    assert_eq!(error, OAuthTokenExchangeError::ProviderMismatch);

    let error = exchange(
        "public-client",
        OAuthClientAuthenticationMethod::Public,
        None,
    )
    .refresh(&client, refresh_request(UpstreamOAuthProvider::Codex))
    .await
    .unwrap_err();
    assert_eq!(error, OAuthTokenExchangeError::ProviderMismatch);
}

#[tokio::test]
async fn endpoint_error_maps_only_status_and_stable_code() {
    let (address, server) = spawn_json_server(
        400,
        br#"{"error":"invalid_grant","error_description":"authorization-code-marker was rejected"}"#,
        |_| {},
    );
    let error = exchange(
        "public-client",
        OAuthClientAuthenticationMethod::Public,
        None,
    )
    .exchange(
        &proxied_client(address),
        grant(UpstreamOAuthProvider::ClaudeCode),
    )
    .await
    .unwrap_err();
    server.join().unwrap();

    assert_eq!(
        error,
        OAuthTokenExchangeError::EndpointRejected {
            status: 400,
            code: Some(OAuthEndpointErrorCode::InvalidGrant),
        }
    );
    let rendered = format!("{error}\n{error:?}");
    assert!(!rendered.contains(AUTHORIZATION_CODE));
    assert!(!rendered.contains("was rejected"));
}

#[tokio::test]
async fn successful_response_rejects_malformed_or_unsafe_token_semantics() {
    let cases: [(&[u8], OAuthTokenExchangeError); 6] = [
        (b"not-json", OAuthTokenExchangeError::MalformedResponse),
        (
            br#"{"access_token":"access-token"}"#,
            OAuthTokenExchangeError::InvalidTokenResponse,
        ),
        (
            br#"{"access_token":"access-token","token_type":"mac"}"#,
            OAuthTokenExchangeError::InvalidTokenResponse,
        ),
        (
            br#"{"access_token":"bad token","token_type":"bearer"}"#,
            OAuthTokenExchangeError::InvalidTokenResponse,
        ),
        (
            br#"{"access_token":"access-token","token_type":"bearer","expires_in":0}"#,
            OAuthTokenExchangeError::InvalidTokenResponse,
        ),
        (
            br#"{"access_token":"access-token","token_type":"bearer","scope":"openid  profile"}"#,
            OAuthTokenExchangeError::InvalidTokenResponse,
        ),
    ];
    for (body, expected) in cases {
        let (address, server) = spawn_json_server(200, body, |_| {});
        let error = exchange(
            "public-client",
            OAuthClientAuthenticationMethod::Public,
            None,
        )
        .exchange(
            &proxied_client(address),
            grant(UpstreamOAuthProvider::ClaudeCode),
        )
        .await
        .unwrap_err();
        server.join().unwrap();
        assert_eq!(error, expected);
    }
}

#[tokio::test]
async fn response_size_limit_handles_declared_and_streamed_bodies() {
    let (address, server) = spawn_raw_server(
        |stream| {
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                MAX_OAUTH_TOKEN_RESPONSE_BYTES + 1
            );
            stream.write_all(response.as_bytes()).unwrap();
        },
        |_| {},
    );
    let error = exchange(
        "public-client",
        OAuthClientAuthenticationMethod::Public,
        None,
    )
    .exchange(
        &proxied_client(address),
        grant(UpstreamOAuthProvider::ClaudeCode),
    )
    .await
    .unwrap_err();
    server.join().unwrap();
    assert_eq!(error, OAuthTokenExchangeError::ResponseTooLarge);

    let (address, server) = spawn_raw_server(
        |stream| {
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n",
                )
                .unwrap();
            let body = vec![b'x'; MAX_OAUTH_TOKEN_RESPONSE_BYTES + 1];
            write!(stream, "{:x}\r\n", body.len()).unwrap();
            stream.write_all(&body).unwrap();
            stream.write_all(b"\r\n0\r\n\r\n").unwrap();
        },
        |_| {},
    );
    let error = exchange(
        "public-client",
        OAuthClientAuthenticationMethod::Public,
        None,
    )
    .exchange(
        &proxied_client(address),
        grant(UpstreamOAuthProvider::ClaudeCode),
    )
    .await
    .unwrap_err();
    server.join().unwrap();
    assert_eq!(error, OAuthTokenExchangeError::ResponseTooLarge);
}
