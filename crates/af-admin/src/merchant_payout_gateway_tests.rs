use super::*;
use af_account::PlainSystemSecret;
use af_config::{CREDENTIAL_ENCRYPTION_KEY_BYTES, CredentialEncryptionSettings};
use af_db::{MerchantPayout, encode_payout_secret};
use af_httpclient::{HttpClientConfig, HttpTimeouts, ProxyConfig, RemoteDnsPolicy};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

const KEY: &str = "test-payout-signing-key-32-characters";
fn cipher() -> SystemSecretCipher {
    let settings:CredentialEncryptionSettings=serde_json::from_value(serde_json::json!({"key_id":"payout-test","key":URL_SAFE_NO_PAD.encode([0x62;CREDENTIAL_ENCRYPTION_KEY_BYTES])})).unwrap();
    SystemSecretCipher::new(&settings).unwrap()
}
fn claim(cipher: &SystemSecretCipher, submit: bool) -> MerchantPayoutClaim {
    let gateway = cipher
        .encrypt(
            SystemSecretKind::MerchantPayoutGateway,
            &PlainSystemSecret::new(KEY.to_owned()).unwrap(),
        )
        .unwrap();
    let account = cipher
        .encrypt(
            SystemSecretKind::MerchantPayoutAccount(77),
            &PlainSystemSecret::new("private-bank-token".to_owned()).unwrap(),
        )
        .unwrap();
    MerchantPayoutClaim {
        payout: MerchantPayout {
            withdrawal_id: 1,
            merchant_id: 77,
            event_key: "a".repeat(32),
            status: "processing".to_owned(),
            amount_minor: 100,
            currency: "USD".to_owned(),
            reference: String::new(),
            reason: None,
            created_at: 0,
            updated_at: 0,
        },
        submit,
        gateway_url: "http://upstream.example".to_owned(),
        gateway_secret: encode_payout_secret(&gateway).unwrap(),
        account_secret: encode_payout_secret(&account).unwrap(),
        lease_owner: "test".to_owned(),
    }
}
async fn gateway(
    status: &str,
    signed: bool,
    submit: bool,
    currency: &str,
) -> (HttpClientProvider, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let status = status.to_owned();
    let currency = currency.to_owned();
    let task = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut bytes = Vec::new();
        let (mut expected, mut split) = (None, 0);
        loop {
            let mut chunk = [0_u8; 4096];
            let length = stream.read(&mut chunk).await.unwrap();
            assert_ne!(length, 0);
            bytes.extend_from_slice(&chunk[..length]);
            assert!(bytes.len() < 16 * 1024);
            if expected.is_none()
                && let Some(index) = bytes.windows(4).position(|value| value == b"\r\n\r\n")
            {
                split = index + 4;
                let headers = String::from_utf8_lossy(&bytes[..index]);
                expected = Some(
                    headers
                        .lines()
                        .find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            name.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse::<usize>().unwrap())
                        })
                        .unwrap(),
                );
            }
            if expected.is_some_and(|length| bytes.len() >= split + length) {
                break;
            }
        }
        let header = String::from_utf8(bytes[..split].to_vec()).unwrap();
        assert!(header.lines().next().unwrap().contains(if submit {
            "/payouts HTTP/1.1"
        } else {
            "/payouts/query HTTP/1.1"
        }));
        let field = |name: &str| {
            header
                .lines()
                .find_map(|line| {
                    let (key, value) = line.split_once(':')?;
                    key.eq_ignore_ascii_case(name)
                        .then(|| value.trim().to_owned())
                })
                .unwrap()
        };
        assert_eq!(field("idempotency-key"), "a".repeat(32));
        verify(
            KEY.as_bytes(),
            &field("x-anyflows-timestamp"),
            &bytes[split..],
            &field("x-anyflows-signature"),
        )
        .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes[split..]).unwrap();
        assert_eq!(
            body.get("account").and_then(serde_json::Value::as_str),
            submit.then_some("private-bank-token")
        );
        let response=serde_json::to_vec(&serde_json::json!({"event_key":"a".repeat(32),"status":status,"amount_minor":100,"currency":currency,"reference":"gateway-id-1"})).unwrap();
        let stamp = unix_time().unwrap().to_string();
        let signature = signature(KEY.as_bytes(), &stamp, &response).unwrap();
        let headers = if signed {
            format!("X-AnyFlows-Timestamp: {stamp}\r\nX-AnyFlows-Signature: {signature}\r\n")
        } else {
            String::new()
        };
        stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{headers}Connection: close\r\n\r\n",response.len()).as_bytes()).await.unwrap();
        stream.write_all(&response).await.unwrap();
    });
    let config = HttpClientConfig::new(
        ProxyConfig::parse(format!("http://{address}")).unwrap(),
        HttpTimeouts::default(),
    )
    .with_remote_dns_policy(RemoteDnsPolicy::TrustProxy);
    (HttpClientProvider::new(config, 4).unwrap(), task)
}
#[tokio::test]
async fn merchant_payout_gateway_mock_requires_signed_matching_confirmation() {
    let cipher = cipher();
    for (status, signed, submit, currency, expected) in [
        ("paid", true, true, "USD", "paid"),
        ("paid", true, false, "USD", "paid"),
        ("rejected", true, true, "USD", "failed"),
        ("paid", false, true, "USD", "unknown"),
        ("paid", true, true, "CNY", "unknown"),
        ("not_found", true, false, "USD", "unknown"),
    ] {
        let (clients, server) = gateway(status, signed, submit, currency).await;
        let result = execute(&clients, &cipher, &claim(&cipher, submit)).await;
        server.await.unwrap();
        assert_eq!(result.status, expected);
    }
}
#[test]
fn merchant_payout_account_ciphertext_cannot_cross_merchants_or_secret_domains() {
    let cipher = cipher();
    let claim = claim(&cipher, true);
    let envelope = decode_payout_secret(&claim.account_secret).unwrap();
    assert!(
        cipher
            .decrypt(SystemSecretKind::MerchantPayoutAccount(78), &envelope)
            .is_err()
    );
    assert!(
        cipher
            .decrypt(SystemSecretKind::MerchantPayoutGateway, &envelope)
            .is_err()
    );
    let stamp = unix_time().unwrap().to_string();
    assert!(verify(KEY.as_bytes(), &stamp, b"{}", &"é".repeat(32)).is_err());
}
