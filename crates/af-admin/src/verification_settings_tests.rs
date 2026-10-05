use super::*;
use af_db::{DatabaseOptions, MigrationOptions, SiteSettingsWriteRecord};
use af_httpclient::{HttpClientConfig, HttpTimeouts, ProxyConfig, RemoteDnsPolicy};
use base64::{
    Engine as _,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use rsa::{
    RsaPrivateKey, RsaPublicKey,
    pkcs1v15::{Signature, SigningKey, VerifyingKey},
    pkcs8::{DecodePrivateKey, EncodePublicKey, LineEnding},
    signature::{SignatureEncoding, Signer, Verifier},
};
use serde_json::{Value, json};
use sha2::Sha256;
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

const PRIVATE_KEY: &str = include_str!("fixtures/alipay_verification_test_private.pem");
const TIMEOUT: Duration = Duration::from_secs(10);

async fn read_form(stream: &mut TcpStream) -> BTreeMap<String, String> {
    let mut bytes = Vec::new();
    let mut buffer = [0; 4096];
    loop {
        let count = stream.read(&mut buffer).await.unwrap();
        assert!(count > 0, "incomplete gateway request");
        bytes.extend_from_slice(&buffer[..count]);
        assert!(bytes.len() < 65_536);
        let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") else {
            continue;
        };
        let headers = String::from_utf8_lossy(&bytes[..end]);
        let length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().unwrap())
            })
            .unwrap();
        if bytes.len() >= end + 4 + length {
            return url::form_urlencoded::parse(&bytes[end + 4..end + 4 + length])
                .into_owned()
                .collect();
        }
    }
}

// Exercise the database-backed provider used by bootstrap, including decryption,
// preconsult, token exchange without code/msg, and signed consult responses.
#[tokio::test]
async fn database_settings_drive_the_complete_certdoc_flow() {
    let pool = af_db::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:").unwrap(),
        MigrationOptions::default(),
    )
    .await
    .unwrap();
    let repository = AccountVerificationSettingsRepository::new(pool.clone(), TIMEOUT);
    let site = Arc::new(SiteSettingsRepository::new(pool.clone(), TIMEOUT).unwrap());
    let encryption = serde_json::from_value(json!({
        "key_id": "verification-test", "key": URL_SAFE_NO_PAD.encode([0x42; 32])
    }))
    .unwrap();
    let cipher = SystemSecretCipher::new(&encryption).unwrap();
    let private = RsaPrivateKey::from_pkcs8_pem(PRIVATE_KEY).unwrap();
    let public = RsaPublicKey::from(&private);
    let credentials = PlainSystemSecret::new(
        serde_json::to_string(&AlipayCredentials {
            private_key: PRIVATE_KEY.to_owned(),
            public_key: public.to_public_key_pem(LineEnding::LF).unwrap(),
        })
        .unwrap(),
    )
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let clients = HttpClientProvider::new(
        HttpClientConfig::new(
            ProxyConfig::parse(format!("http://{}", listener.local_addr().unwrap())).unwrap(),
            HttpTimeouts::default(),
        )
        .with_remote_dns_policy(RemoteDnsPolicy::TrustProxy),
        1,
    )
    .unwrap();
    let mut record = repository.settings().await.unwrap();
    record.enabled = true;
    record.app_id = Some("test-app".to_owned());
    record.credentials = Some(
        cipher
            .encrypt(
                SystemSecretKind::AlipayVerificationCredentials,
                &credentials,
            )
            .unwrap(),
    );
    // Only the test repository uses HTTP through a local fake proxy. The admin
    // settings endpoint continues to require HTTPS for production gateways.
    record.gateway_url = "http://alipay.example/gateway.do".to_owned();
    repository
        .update(record.clone(), record.version)
        .await
        .unwrap();
    let settings = DatabaseVerificationSettingsService::new_with_site_settings(
        repository.clone(),
        cipher,
        AlipayVerificationSettings::default(),
        clients,
        Some(site.clone()),
    );
    let provider: &dyn AccountVerificationProvider = &settings;
    let request = AccountVerificationProviderRequest {
        kind: "individual".to_owned(),
        document_country: "CN".to_owned(),
        document_type: "national_id".to_owned(),
        document_number: "test-document".to_owned(),
        subject_name: "Test Person".to_owned(),
    };
    assert!(provider.available().await);
    assert!(provider.initialize(&request).await.is_err());
    // A missing public URL must fail before any identity request is transmitted.
    assert!(
        tokio::time::timeout(Duration::from_millis(50), listener.accept())
            .await
            .is_err()
    );
    let current_site = site.settings().await.unwrap();
    site.update(SiteSettingsWriteRecord::new(
        "Test".to_owned(),
        Some("https://example.com".to_owned()),
        None,
        None,
        None,
        current_site.balance_display().clone(),
        current_site.version(),
    ))
    .await
    .unwrap();

    let gateway = tokio::spawn(async move {
        tokio::time::timeout(TIMEOUT, async {
            for (method, response) in [
                ("alipay.user.certdoc.certverify.preconsult", json!({"code":"10000","verify_id":"verify-test"})),
                ("alipay.system.oauth.token", json!({"access_token":"test-access-token","expires_in":3600})),
                ("alipay.user.certdoc.certverify.consult", json!({"code":"10000","passed":"T"})),
            ] {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut form = read_form(&mut stream).await;
                assert_eq!(form["method"], method);
                assert_eq!(form["format"], "JSON");
                assert_eq!(form["charset"], "UTF-8");
                let signature = STANDARD.decode(form.remove("sign").unwrap()).unwrap();
                let content = form.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("&");
                VerifyingKey::<Sha256>::new(public.clone()).verify(content.as_bytes(), &Signature::try_from(signature.as_slice()).unwrap()).unwrap();
                match method {
                    "alipay.user.certdoc.certverify.preconsult" => assert_eq!(
                        serde_json::from_str::<Value>(&form["biz_content"]).unwrap(),
                        json!({"user_name":"Test Person","cert_no":"test-document","cert_type":"IDENTITY_CARD"}),
                    ),
                    "alipay.system.oauth.token" => {
                        assert_eq!(form["grant_type"], "authorization_code");
                        assert_eq!(form["code"], "test-auth-code");
                        assert!(!form.contains_key("biz_content"));
                    }
                    _ => {
                        assert_eq!(form["auth_token"], "test-access-token");
                        assert_eq!(serde_json::from_str::<Value>(&form["biz_content"]).unwrap(), json!({"verify_id":"verify-test"}));
                    }
                }
                let response = response.to_string();
                let signature = SigningKey::<Sha256>::new(private.clone()).sign(response.as_bytes());
                let body = format!(r#"{{"{}_response":{response},"sign":"{}"}}"#, method.replace('.', "_"), STANDARD.encode(signature.to_bytes()));
                stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
            }
        }).await.expect("CertDoc gateway timed out");
    });
    let start = provider.initialize(&request).await.unwrap();
    let link = Url::parse(&start.action_url).unwrap();
    let query: BTreeMap<_, _> = link.query_pairs().into_owned().collect();
    assert_eq!(query["scope"], "id_verify");
    assert_eq!(
        query["redirect_uri"],
        "https://example.com/api/account/verifications/alipay/callback"
    );
    assert_eq!(start.reference, format!("verify-test.{}", query["state"]));
    let result = provider
        .complete(&start.reference, "test-auth-code")
        .await
        .unwrap();
    assert_eq!(result.status, "approved");
    assert_eq!(result.terminal_status, Some(4));
    gateway.await.unwrap();

    // Runtime changes must also apply to callbacks; a cached enabled provider
    // must not bypass a subsequent administrator switch to disabled.
    let mut disabled = repository.settings().await.unwrap();
    disabled.enabled = false;
    repository
        .update(disabled.clone(), disabled.version)
        .await
        .unwrap();
    assert!(!provider.available().await);
    assert!(
        provider
            .complete(&start.reference, "test-auth-code")
            .await
            .is_err()
    );
    pool.close().await.unwrap();
}
