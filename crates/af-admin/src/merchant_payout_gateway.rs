//! Signed gateway protocol. An ambiguous send is never submitted again.
#[cfg(test)]
#[path = "merchant_payout_gateway_tests.rs"]
mod network_tests;
use af_account::{SystemSecretCipher, SystemSecretKind};
use af_db::{MerchantPayoutClaim, decode_payout_secret};
use af_httpclient::{Body, HeaderMap, HeaderValue, HttpClientProvider, Method, StatusCode};
use hmac::{Hmac, Mac};
use serde::Deserialize;
use sha2::Sha256;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use zeroize::Zeroizing;

pub(crate) struct Outcome {
    pub status: &'static str,
    pub reference: String,
    pub reason: Option<&'static str>,
}
impl Outcome {
    fn unknown(reason: &'static str) -> Self {
        Self {
            status: "unknown",
            reference: String::new(),
            reason: Some(reason),
        }
    }
}

pub(crate) async fn execute(
    clients: &HttpClientProvider,
    cipher: &SystemSecretCipher,
    claim: &MerchantPayoutClaim,
) -> Outcome {
    let result =
        tokio::time::timeout(Duration::from_secs(30), exchange(clients, cipher, claim)).await;
    match result {
        Ok(Ok(value)) => value,
        Ok(Err(reason)) => Outcome::unknown(reason),
        Err(_) => Outcome::unknown("gateway_timeout"),
    }
}
async fn exchange(
    clients: &HttpClientProvider,
    cipher: &SystemSecretCipher,
    claim: &MerchantPayoutClaim,
) -> Result<Outcome, &'static str> {
    let envelope = decode_payout_secret(&claim.gateway_secret).map_err(|_| "secret_unavailable")?;
    let key = cipher
        .decrypt(SystemSecretKind::MerchantPayoutGateway, &envelope)
        .map_err(|_| "secret_unavailable")?;
    let account_envelope =
        decode_payout_secret(&claim.account_secret).map_err(|_| "account_unavailable")?;
    let account = cipher
        .decrypt(
            SystemSecretKind::MerchantPayoutAccount(claim.payout.merchant_id),
            &account_envelope,
        )
        .map_err(|_| "account_unavailable")?;
    let body=Zeroizing::new(serde_json::to_vec(&if claim.submit {serde_json::json!({"event_key":claim.payout.event_key,"amount_minor":claim.payout.amount_minor,"currency":claim.payout.currency,"account":account.expose_secret()})}else{serde_json::json!({"event_key":claim.payout.event_key})}).map_err(|_|"invalid_request")?);
    let timestamp = unix_time()?.to_string();
    let signature = signature(key.expose_secret().as_bytes(), &timestamp, &body)?;
    let mut headers = HeaderMap::new();
    headers.insert("content-type", HeaderValue::from_static("application/json"));
    for (name, value) in [
        ("idempotency-key", claim.payout.event_key.as_str()),
        ("x-anyflows-timestamp", timestamp.as_str()),
        ("x-anyflows-signature", signature.as_str()),
    ] {
        let mut value = HeaderValue::from_str(value).map_err(|_| "invalid_request")?;
        value.set_sensitive(true);
        headers.insert(name, value);
    }
    let target = format!(
        "{}/payouts{}",
        claim.gateway_url,
        if claim.submit { "" } else { "/query" }
    );
    let client = clients
        .get(Some(Duration::from_secs(25)))
        .map_err(|_| "gateway_unavailable")?;
    let response = client
        .execute(
            Method::POST,
            &target,
            headers,
            Some(Body::from(body.to_vec())),
        )
        .await
        .map_err(|_| "gateway_unavailable")?;
    if response.status() != StatusCode::OK {
        return Err("gateway_http_error");
    }
    let response_time = response
        .headers()
        .get("x-anyflows-timestamp")
        .and_then(|v| v.to_str().ok())
        .ok_or("unsigned_response")?
        .to_owned();
    let response_signature = response
        .headers()
        .get("x-anyflows-signature")
        .and_then(|v| v.to_str().ok())
        .ok_or("unsigned_response")?
        .to_owned();
    if response
        .content_length()
        .is_some_and(|size| size > 16 * 1024)
    {
        return Err("invalid_response");
    }
    let mut stream = response.into_bytes_stream();
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next_chunk().await {
        let chunk = chunk.map_err(|_| "gateway_unavailable")?;
        if bytes.len() + chunk.len() > 16 * 1024 {
            return Err("invalid_response");
        }
        bytes.extend_from_slice(&chunk);
    }
    verify(
        key.expose_secret().as_bytes(),
        &response_time,
        &bytes,
        &response_signature,
    )?;
    decode(&bytes, claim)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    event_key: String,
    status: String,
    amount_minor: i64,
    currency: String,
    #[serde(default)]
    reference: String,
}
fn decode(bytes: &[u8], claim: &MerchantPayoutClaim) -> Result<Outcome, &'static str> {
    let receipt: Receipt = serde_json::from_slice(bytes).map_err(|_| "invalid_response")?;
    if receipt.event_key != claim.payout.event_key
        || receipt.amount_minor != claim.payout.amount_minor
        || receipt.currency != claim.payout.currency
        || receipt.reference.len() > 256
        || receipt.reference.chars().any(char::is_control)
    {
        return Err("receipt_mismatch");
    }
    Ok(match receipt.status.as_str() {
        "paid" if !receipt.reference.is_empty() => Outcome {
            status: "paid",
            reference: receipt.reference,
            reason: None,
        },
        "rejected" => Outcome {
            status: "failed",
            reference: receipt.reference,
            reason: Some("gateway_rejected"),
        },
        "processing" => Outcome {
            status: "processing",
            reference: receipt.reference,
            reason: None,
        },
        "unknown" | "not_found" => Outcome::unknown("gateway_unknown"),
        _ => return Err("invalid_response"),
    })
}
fn unix_time() -> Result<u64, &'static str> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs())
        .map_err(|_| "clock_unavailable")
}
fn signature(key: &[u8], timestamp: &str, body: &[u8]) -> Result<String, &'static str> {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).map_err(|_| "secret_unavailable")?;
    mac.update(timestamp.as_bytes());
    mac.update(b"\n");
    mac.update(body);
    let mut result = String::with_capacity(64);
    for byte in mac.finalize().into_bytes() {
        use std::fmt::Write;
        write!(&mut result, "{byte:02x}").map_err(|_| "invalid_request")?;
    }
    Ok(result)
}
fn verify(key: &[u8], timestamp: &str, body: &[u8], signature: &str) -> Result<(), &'static str> {
    let stamp = timestamp.parse::<u64>().map_err(|_| "invalid_signature")?;
    if unix_time()?.abs_diff(stamp) > 300
        || signature.len() != 64
        || !signature.bytes().all(|value| value.is_ascii_hexdigit())
    {
        return Err("invalid_signature");
    }
    let mut decoded = [0_u8; 32];
    for (index, byte) in decoded.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&signature[index * 2..index * 2 + 2], 16)
            .map_err(|_| "invalid_signature")?;
    }
    let mut mac = Hmac::<Sha256>::new_from_slice(key).map_err(|_| "secret_unavailable")?;
    mac.update(timestamp.as_bytes());
    mac.update(b"\n");
    mac.update(body);
    mac.verify_slice(&decoded).map_err(|_| "invalid_signature")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn merchant_payout_signatures_bind_timestamp_body_and_key() {
        let time = unix_time().unwrap().to_string();
        let body = b"{\"status\":\"paid\"}";
        let signed = signature(b"test-key", &time, body).unwrap();
        assert!(verify(b"test-key", &time, body, &signed).is_ok());
        assert!(verify(b"other", &time, body, &signed).is_err());
        assert!(verify(b"test-key", &time, b"{}", &signed).is_err());
        assert!(verify(b"test-key", "0", body, &signed).is_err());
    }
}
