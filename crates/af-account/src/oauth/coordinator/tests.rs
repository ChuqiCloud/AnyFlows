use std::{
    collections::HashMap,
    error::Error,
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    thread::{self, JoinHandle},
    time::Duration,
};

use af_config::{CREDENTIAL_ENCRYPTION_KEY_BYTES, CredentialEncryptionSettings};
use af_db::{
    AdminChannelRepository, AdminChannelWriteRecord, AdminCredentialCreateOutcome,
    AdminCredentialLookupOutcome, AdminCredentialWriteRecord, DatabaseOptions, DatabasePool,
    EncryptedCredentialEnvelope, MigrationOptions, OAuthCredentialRepository,
};
use af_domain::{
    ChannelId, ChannelTimeout, ChannelType, CredentialId, CredentialKind, CredentialQuotaDimension,
    Protocol, Status, UserId,
};
use af_httpclient::{
    HttpClientConfig, HttpClientProvider, HttpTimeouts, ProxyConfig, RemoteDnsPolicy,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use url::{Url, form_urlencoded};

use super::*;
use crate::CredentialEncryptor;

const AUTHORIZATION_CODE: &str = "authorization-code-marker";
const SERVER_TIMEOUT: Duration = Duration::from_secs(5);

#[tokio::test]
async fn coordinator_closes_profile_lookup_one_time_exchange_and_persistence_mapping()
-> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let channel_id = create_channel(&fixture).await?;
    let credential_id = create_credential(&fixture, channel_id, None).await?;
    let mismatched_credential_id =
        create_credential(&fixture, channel_id, Some("claude_code")).await?;

    let duplicate = OAuthConnectionCoordinator::new(
        [codex_profile(), codex_profile()],
        OAuthAuthorizationSessionStore::with_defaults(),
        direct_clients(),
        persistence(&fixture.pool),
    )
    .unwrap_err();
    assert_eq!(
        duplicate,
        OAuthConnectionError::DuplicateProviderProfile {
            provider: UpstreamOAuthProvider::Codex
        }
    );

    let (address, server) = spawn_token_server(200, successful_token_response(), |request| {
        assert!(request.starts_with("POST http://token.example/oauth/token HTTP/1.1"));
        assert_eq!(
            header_value(request, "content-type"),
            Some("application/x-www-form-urlencoded")
        );
        let values = form_values(request);
        assert_eq!(
            values.get("code").map(String::as_str),
            Some(AUTHORIZATION_CODE)
        );
        assert_eq!(
            values.get("client_id").map(String::as_str),
            Some("codex-client-marker")
        );
        assert!(!values.contains_key("state"));
        assert!(!values.contains_key("client_secret"));
    });
    let coordinator = coordinator(&fixture.pool, address);
    let start = coordinator.begin(
        UpstreamOAuthProvider::Codex,
        context(channel_id, credential_id),
    )?;
    let (state, callback_url) = callback_url(&start);
    assert_eq!(coordinator.pending_count()?, 1);

    let wrong_provider = OAuthAuthorizationCallback::from_redirect_url(
        UpstreamOAuthProvider::Gemini,
        callback_url.clone(),
    )?;
    assert_eq!(
        coordinator.complete(wrong_provider).await.unwrap_err(),
        OAuthConnectionError::ProviderProfileNotConfigured {
            provider: UpstreamOAuthProvider::Gemini
        }
    );
    assert_eq!(coordinator.pending_count()?, 1);

    let callback = OAuthAuthorizationCallback::from_redirect_url(
        UpstreamOAuthProvider::Codex,
        callback_url.clone(),
    )?;
    assert_eq!(
        coordinator
            .complete_for(UserId::new(12)?, callback)
            .await
            .unwrap_err(),
        OAuthConnectionError::Authorization(OAuthAuthorizationError::PrincipalMismatch)
    );
    assert_eq!(coordinator.pending_count()?, 1);
    let callback = OAuthAuthorizationCallback::from_redirect_url(
        UpstreamOAuthProvider::Codex,
        callback_url.clone(),
    )?;
    assert_eq!(
        coordinator.complete_for(UserId::new(11)?, callback).await?,
        OAuthConnectionOutcome::Connected
    );
    server.join().unwrap();
    assert_eq!(coordinator.pending_count()?, 0);
    let AdminCredentialLookupOutcome::Found(connected) = fixture
        .admin_repository
        .get_credential(channel_id, credential_id)
        .await?
    else {
        panic!("完成授权后必须仍能读取目标凭据");
    };
    assert_eq!(connected.oauth_account_key(), Some("org-codex-test"));
    assert_eq!(connected.oauth_project_id(), Some("project-id"));

    let replay =
        OAuthAuthorizationCallback::from_redirect_url(UpstreamOAuthProvider::Codex, callback_url)?;
    assert_eq!(
        coordinator.complete(replay).await.unwrap_err(),
        OAuthConnectionError::Authorization(OAuthAuthorizationError::SessionNotFound)
    );
    let rendered = format!("{coordinator:?}{duplicate:?}");
    for private in [
        state.as_str(),
        AUTHORIZATION_CODE,
        "codex-client-marker",
        "access-token-marker",
        "refresh-token-marker",
        "openid profile",
    ] {
        assert!(!rendered.contains(private));
    }

    assert_eq!(
        complete_once(
            &fixture.pool,
            context(channel_id, CredentialId::new(i64::MAX)?),
        )
        .await?,
        OAuthConnectionOutcome::TargetNotFound
    );
    assert_eq!(
        complete_once(&fixture.pool, context(channel_id, mismatched_credential_id),).await?,
        OAuthConnectionOutcome::CredentialProviderMismatch
    );

    fixture.pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn exchange_failure_still_consumes_the_authorization_session() -> Result<(), Box<dyn Error>> {
    let fixture = fixture().await?;
    let channel_id = create_channel(&fixture).await?;
    let credential_id = create_credential(&fixture, channel_id, None).await?;
    let (address, server) = spawn_token_server(
        400,
        br#"{"error":"invalid_grant","error_description":"private-upstream-marker"}"#,
        |_| {},
    );
    let coordinator = coordinator(&fixture.pool, address);
    let start = coordinator.begin(
        UpstreamOAuthProvider::Codex,
        context(channel_id, credential_id),
    )?;
    let (_, callback_url) = callback_url(&start);
    let callback = OAuthAuthorizationCallback::from_redirect_url(
        UpstreamOAuthProvider::Codex,
        callback_url.clone(),
    )?;
    let error = coordinator.complete(callback).await.unwrap_err();
    server.join().unwrap();
    assert_eq!(
        error,
        OAuthConnectionError::TokenExchange(OAuthTokenExchangeError::EndpointRejected {
            status: 400,
            code: Some(super::super::OAuthEndpointErrorCode::InvalidGrant),
        })
    );
    assert!(!format!("{error:?}{error}").contains("private-upstream-marker"));

    let replay =
        OAuthAuthorizationCallback::from_redirect_url(UpstreamOAuthProvider::Codex, callback_url)?;
    assert_eq!(
        coordinator.complete(replay).await.unwrap_err(),
        OAuthConnectionError::Authorization(OAuthAuthorizationError::SessionNotFound)
    );

    fixture.pool.close().await?;
    Ok(())
}

async fn complete_once(
    pool: &DatabasePool,
    context: OAuthAuthorizationContext,
) -> Result<OAuthConnectionOutcome, Box<dyn Error>> {
    let (address, server) = spawn_token_server(200, successful_token_response(), |_| {});
    let coordinator = coordinator(pool, address);
    let start = coordinator.begin(UpstreamOAuthProvider::Codex, context)?;
    let (_, callback_url) = callback_url(&start);
    let callback =
        OAuthAuthorizationCallback::from_redirect_url(UpstreamOAuthProvider::Codex, callback_url)?;
    let outcome = coordinator.complete(callback).await?;
    server.join().unwrap();
    Ok(outcome)
}

fn coordinator(pool: &DatabasePool, address: SocketAddr) -> OAuthConnectionCoordinator {
    let mut profile = codex_profile();
    profile.set_token_endpoint_for_test(Url::parse("http://token.example/oauth/token").unwrap());
    OAuthConnectionCoordinator::new(
        [profile],
        OAuthAuthorizationSessionStore::with_defaults(),
        proxied_clients(address),
        persistence(pool),
    )
    .unwrap()
}

fn codex_profile() -> OAuthProviderProfile {
    OAuthProviderProfile::new(
        UpstreamOAuthProvider::Codex,
        "codex-client-marker".to_owned(),
        None,
    )
    .unwrap()
}

fn persistence(pool: &DatabasePool) -> OAuthTokenPersistenceService {
    OAuthTokenPersistenceService::new(
        CredentialEncryptor::new(&encryption_settings()).unwrap(),
        OAuthCredentialRepository::new(pool.clone()),
    )
}

fn proxied_clients(address: SocketAddr) -> HttpClientProvider {
    let config = HttpClientConfig::new(
        ProxyConfig::parse(format!("http://{address}")).unwrap(),
        HttpTimeouts::default(),
    )
    .with_remote_dns_policy(RemoteDnsPolicy::TrustProxy);
    HttpClientProvider::new(config, 8).unwrap()
}

fn direct_clients() -> HttpClientProvider {
    HttpClientProvider::new(
        HttpClientConfig::new(ProxyConfig::direct(), HttpTimeouts::default()),
        8,
    )
    .unwrap()
}

fn context(channel_id: ChannelId, credential_id: CredentialId) -> OAuthAuthorizationContext {
    OAuthAuthorizationContext::new(
        UserId::new(11).unwrap(),
        Some(channel_id),
        Some(credential_id),
    )
    .unwrap()
}

fn callback_url(start: &OAuthAuthorizationStart) -> (String, Url) {
    let state = start
        .authorization_url()
        .query_pairs()
        .find_map(|(name, value)| (name == "state").then(|| value.into_owned()))
        .unwrap();
    let mut callback = start.redirect_uri().clone();
    callback
        .query_pairs_mut()
        .append_pair("state", &state)
        .append_pair("code", AUTHORIZATION_CODE);
    (state, callback)
}

fn successful_token_response() -> &'static [u8] {
    br#"{"access_token":"access-token-marker","token_type":"Bearer","expires_in":3600,"refresh_token":"refresh-token-marker","scope":"openid profile","id_token":"eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9.eyJodHRwczovL2FwaS5vcGVuYWkuY29tL2F1dGguY2hhdGdwdF9hY2NvdW50X2lkIjoib3JnLWNvZGV4LXRlc3QifQ.c2lnbmF0dXJl"}"#
}

fn spawn_token_server<F>(
    status: u16,
    body: &'static [u8],
    inspect: F,
) -> (SocketAddr, JoinHandle<()>)
where
    F: FnOnce(&str) + Send + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream.set_read_timeout(Some(SERVER_TIMEOUT)).unwrap();
        let request = read_request(&mut stream);
        inspect(&request);
        let reason = if status == 200 { "OK" } else { "Bad Request" };
        let headers = format!(
            "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        stream.write_all(headers.as_bytes()).unwrap();
        stream.write_all(body).unwrap();
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

struct Fixture {
    pool: DatabasePool,
    admin_repository: AdminChannelRepository,
}

async fn fixture() -> Result<Fixture, Box<dyn Error>> {
    let pool = af_db::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    Ok(Fixture {
        admin_repository: AdminChannelRepository::new(pool.clone(), Duration::from_secs(2))?,
        pool,
    })
}

async fn create_channel(fixture: &Fixture) -> Result<ChannelId, Box<dyn Error>> {
    Ok(fixture
        .admin_repository
        .create_channel(AdminChannelWriteRecord::new(
            "oauth-coordinator-test".to_owned(),
            ChannelType::OpenAi,
            Protocol::OpenAiChat,
            Some("https://api.example.com/v1".to_owned()),
            Some(ChannelTimeout::new(60)?),
            Status::Enabled,
            10,
            20,
            true,
            Vec::new(),
            Vec::new(),
            serde_json::json!({}),
            serde_json::json!({}),
            serde_json::json!({}),
            serde_json::json!({}),
            None,
        ))
        .await?
        .channel_id())
}

async fn create_credential(
    fixture: &Fixture,
    channel_id: ChannelId,
    oauth_provider: Option<&str>,
) -> Result<CredentialId, Box<dyn Error>> {
    let AdminCredentialCreateOutcome::Created(record) = fixture
        .admin_repository
        .create_credential(
            channel_id,
            AdminCredentialWriteRecord::new(
                CredentialKind::Oauth,
                Status::Enabled,
                Some(2),
                30,
                40,
                Some(5),
                Some(1_250_000),
                Some(875_000),
                true,
                None,
                CredentialQuotaDimension::Global,
                None,
                oauth_provider.map(str::to_owned),
                Some("account-key".to_owned()),
                Some("project-id".to_owned()),
            ),
            |_| Ok(envelope(0x33)),
        )
        .await?
    else {
        panic!("测试渠道必须存在");
    };
    Ok(record.credential_id())
}

fn envelope(marker: u8) -> EncryptedCredentialEnvelope {
    EncryptedCredentialEnvelope::new("oauth-coordinator-test", [marker; 24], vec![marker; 32])
        .unwrap()
}

fn encryption_settings() -> CredentialEncryptionSettings {
    serde_json::from_value(serde_json::json!({
        "key_id": "oauth-coordinator-test",
        "key": URL_SAFE_NO_PAD.encode([0x5a; CREDENTIAL_ENCRYPTION_KEY_BYTES])
    }))
    .unwrap()
}
