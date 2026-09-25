use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use af_admin::{AdminChannelWriter, SessionPrincipal, SessionRole};
use af_domain::{ChannelId, CredentialId};
use af_http::{
    AdminCredentialUsageFuture, AdminCredentialUsageProbe, CredentialUsageSnapshot,
    CredentialUsageStatus, CredentialUsageWindow,
};
use af_httpclient::{HeaderMap, HeaderName, HeaderValue, HttpClientProvider, Method, StatusCode};
use futures_util::StreamExt;
use serde::Deserialize;

const CODEX_USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";
const MAX_USAGE_BYTES: usize = 64 * 1024;
const USAGE_CACHE_TTL: Duration = Duration::from_secs(30);

pub(crate) struct CodexCredentialUsageProbe {
    writer: Arc<dyn AdminChannelWriter>,
    clients: HttpClientProvider,
    cache: RwLock<HashMap<(i64, i64), (Instant, CredentialUsageSnapshot)>>,
}

impl CodexCredentialUsageProbe {
    pub(crate) fn new(writer: Arc<dyn AdminChannelWriter>, clients: HttpClientProvider) -> Self {
        Self {
            writer,
            clients,
            cache: RwLock::new(HashMap::new()),
        }
    }

    async fn query(
        &self,
        principal: SessionPrincipal,
        channel_id: ChannelId,
        credential_id: CredentialId,
    ) -> Option<CredentialUsageSnapshot> {
        if principal.role() != SessionRole::Admin {
            return None;
        }
        let key = (channel_id.get(), credential_id.get());
        if let Some(snapshot) = self
            .cache
            .read()
            .ok()?
            .get(&key)
            .filter(|(cached_at, _)| cached_at.elapsed() < USAGE_CACHE_TTL)
            .map(|(_, snapshot)| snapshot.clone())
        {
            return Some(snapshot);
        }
        let credentials = self
            .writer
            .export_oauth_credentials(principal, channel_id)
            .await
            .ok()?;
        let credential = credentials
            .into_iter()
            .find(|item| item.credential_id() == credential_id)?;
        let client = self.clients.get(Some(Duration::from_secs(20))).ok()?;
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("authorization"),
            HeaderValue::from_str(&format!("Bearer {}", credential.secret().access_token()))
                .ok()?,
        );
        if let Some(account_key) = credential.oauth_account_key() {
            headers.insert(
                HeaderName::from_static("chatgpt-account-id"),
                HeaderValue::from_str(account_key).ok()?,
            );
        }
        headers.insert(
            HeaderName::from_static("openai-beta"),
            HeaderValue::from_static("codex-1"),
        );
        headers.insert(
            HeaderName::from_static("originator"),
            HeaderValue::from_static("Codex Desktop"),
        );
        headers.insert(
            HeaderName::from_static("oai-language"),
            HeaderValue::from_static("zh-CN"),
        );
        headers.insert(
            HeaderName::from_static("accept"),
            HeaderValue::from_static("application/json"),
        );
        let response = client
            .execute(Method::GET, CODEX_USAGE_URL, headers, None)
            .await
            .ok()?;
        if response.status() != StatusCode::OK
            || response
                .content_length()
                .is_some_and(|length| length > MAX_USAGE_BYTES as u64)
        {
            return None;
        }
        let mut body = Vec::new();
        let mut stream = response.into_bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.ok()?;
            if chunk.len() > MAX_USAGE_BYTES - body.len() {
                return None;
            }
            body.extend_from_slice(&chunk);
        }
        let usage: CodexUsage = serde_json::from_slice(&body).ok()?;
        let mut windows = Vec::new();
        if let Some(limits) = usage.rate_limit {
            for window in [limits.primary_window, limits.secondary_window]
                .into_iter()
                .flatten()
            {
                if let Some(window) = window.into_public() {
                    windows.push(window);
                }
            }
        }
        let credits_balance = usage
            .credits
            .and_then(|credits| credits.balance)
            .filter(|balance| {
                balance.len() <= 64 && balance.parse::<rust_decimal::Decimal>().is_ok()
            });
        if windows.is_empty() && credits_balance.is_none() {
            return None;
        }
        let fetched_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .and_then(|duration| i64::try_from(duration.as_secs()).ok());
        let snapshot = CredentialUsageSnapshot {
            status: CredentialUsageStatus::Available,
            windows,
            credits_balance,
            fetched_at,
        };
        if let Ok(mut cache) = self.cache.write() {
            if cache.len() >= 1024 {
                cache.clear();
            }
            cache.insert(key, (Instant::now(), snapshot.clone()));
        }
        Some(snapshot)
    }
}

impl AdminCredentialUsageProbe for CodexCredentialUsageProbe {
    fn probe<'a>(
        &'a self,
        principal: SessionPrincipal,
        channel_id: ChannelId,
        credential_id: CredentialId,
    ) -> AdminCredentialUsageFuture<'a> {
        Box::pin(async move {
            self.query(principal, channel_id, credential_id)
                .await
                .unwrap_or_else(CredentialUsageSnapshot::unavailable)
        })
    }
}

#[derive(Deserialize)]
struct CodexUsage {
    rate_limit: Option<CodexRateLimit>,
    credits: Option<CodexCredits>,
}

#[derive(Deserialize)]
struct CodexRateLimit {
    primary_window: Option<CodexWindow>,
    secondary_window: Option<CodexWindow>,
}

#[derive(Deserialize)]
struct CodexWindow {
    used_percent: f64,
    limit_window_seconds: i64,
    reset_at: Option<i64>,
}

impl CodexWindow {
    fn into_public(self) -> Option<CredentialUsageWindow> {
        (self.used_percent.is_finite()
            && (0.0..=100.0).contains(&self.used_percent)
            && self.limit_window_seconds > 0)
            .then_some(CredentialUsageWindow {
                window_seconds: self.limit_window_seconds,
                used_percent: self.used_percent,
                reset_at: self.reset_at.filter(|value| *value > 0),
            })
    }
}

#[derive(Deserialize)]
struct CodexCredits {
    balance: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_windows_reject_invalid_values_without_exposing_upstream_metadata() {
        let valid: CodexUsage = serde_json::from_str(r#"{
            "rate_limit":{"primary_window":{"used_percent":25.5,"limit_window_seconds":18000,"reset_at":2000}},
            "credits":{"balance":"12.50"},"email":"private@example.com"
        }"#).unwrap();
        let window = valid
            .rate_limit
            .unwrap()
            .primary_window
            .unwrap()
            .into_public()
            .unwrap();
        assert_eq!(window.window_seconds, 18000);
        assert_eq!(window.used_percent, 25.5);
        assert_eq!(window.reset_at, Some(2000));
        assert!(
            CodexWindow {
                used_percent: 101.0,
                limit_window_seconds: 18000,
                reset_at: None
            }
            .into_public()
            .is_none()
        );
    }
}
