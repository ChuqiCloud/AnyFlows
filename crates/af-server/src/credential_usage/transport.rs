use std::net::IpAddr;

use af_account::DecryptedSystemSecret;
use af_db::EncryptedCredentialEnvelope;
use af_domain::{CredentialKind, UpstreamUsageProvider, UpstreamUsageScope};
use af_http::CredentialUsageWindow;
use af_httpclient::{HeaderMap, HeaderName, HeaderValue, Method, PooledClient, StatusCode};
use serde_json::Value;
use url::Url;

use super::*;

const MAX_BYTES: usize = 128 * 1024;
const CODEX_USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";

#[derive(Clone, Copy, Debug)]
pub(super) enum UsageFetchError {
    Unsupported,
    Authentication,
    RateLimited,
    Unavailable,
    Invalid,
}

impl UsageFetchError {
    pub(super) const fn code(self) -> &'static str {
        match self {
            Self::Unsupported => "unsupported_endpoint",
            Self::Authentication => "upstream_auth",
            Self::RateLimited => "upstream_rate_limited",
            Self::Unavailable => "upstream_unavailable",
            Self::Invalid => "invalid_upstream_response",
        }
    }
    pub(super) const fn unsupported(self) -> bool {
        matches!(self, Self::Unsupported)
    }
}

pub(super) fn effective_provider(target: &UpstreamUsageTarget) -> UpstreamUsageProvider {
    target.profile.as_ref().map_or_else(
        || match target.config.provider {
            UpstreamUsageProvider::Auto
                if target.kind == CredentialKind::Oauth
                    && target.oauth_provider.as_deref() == Some("codex") =>
            {
                UpstreamUsageProvider::Codex
            }
            UpstreamUsageProvider::Auto => UpstreamUsageProvider::None,
            provider => provider,
        },
        |profile| profile.provider,
    )
}

impl UpstreamCredentialUsageProbe {
    pub(super) async fn fetch(
        &self,
        target: &UpstreamUsageTarget,
    ) -> Result<CredentialUsageSnapshot, UsageFetchError> {
        let provider = effective_provider(target);
        if provider == UpstreamUsageProvider::None {
            return Err(UsageFetchError::Unsupported);
        }
        if provider == UpstreamUsageProvider::Codex
            && (target.kind != CredentialKind::Oauth || target.quota_dimension != "global")
        {
            return Err(UsageFetchError::Unsupported);
        }
        if target.config.scope == UpstreamUsageScope::Account && target.profile.is_none() {
            return Err(UsageFetchError::Unsupported);
        }
        if provider == UpstreamUsageProvider::Sub2api && target.profile.is_none() {
            return Err(UsageFetchError::Unsupported);
        }
        let client = if let Some(pools) = &self.proxy_pools {
            pools
                .route_client(
                    target.proxy_id,
                    target.channel_id,
                    target.secret_owner_id,
                    &self.clients,
                    Some(Duration::from_secs(20)),
                )
                .await
                .map_err(|_| UsageFetchError::Unavailable)?
        } else {
            if target.proxy_id.is_some() {
                return Err(UsageFetchError::Unavailable);
            }
            self.clients
                .get(Some(Duration::from_secs(20)))
                .map_err(|_| UsageFetchError::Unavailable)?
        };
        let account_secret: Option<DecryptedSystemSecret> = target
            .profile_secret
            .as_deref()
            .map(|encoded| {
                let value: Value =
                    serde_json::from_str(encoded).map_err(|_| UsageFetchError::Invalid)?;
                let key = value
                    .get("key_id")
                    .and_then(Value::as_str)
                    .ok_or(UsageFetchError::Invalid)?;
                let nonce: [u8; 24] = URL_SAFE_NO_PAD
                    .decode(
                        value
                            .get("nonce")
                            .and_then(Value::as_str)
                            .ok_or(UsageFetchError::Invalid)?,
                    )
                    .map_err(|_| UsageFetchError::Invalid)?
                    .try_into()
                    .map_err(|_| UsageFetchError::Invalid)?;
                let ciphertext = URL_SAFE_NO_PAD
                    .decode(
                        value
                            .get("ciphertext")
                            .and_then(Value::as_str)
                            .ok_or(UsageFetchError::Invalid)?,
                    )
                    .map_err(|_| UsageFetchError::Invalid)?;
                let envelope = EncryptedCredentialEnvelope::new(key, nonce, ciphertext)
                    .map_err(|_| UsageFetchError::Invalid)?;
                self.cipher
                    .decrypt_with_aad(PROFILE_AAD, &envelope)
                    .map_err(|_| UsageFetchError::Invalid)
            })
            .transpose()?;
        let credential = if account_secret.is_none() {
            Some(
                self.decryptor
                    .decrypt_envelope(
                        target.channel_id,
                        target.secret_owner_id.get(),
                        target.kind,
                        &target.envelope,
                    )
                    .map_err(|_| UsageFetchError::Authentication)?,
            )
        } else {
            None
        };
        let token = account_secret
            .as_ref()
            .map(DecryptedSystemSecret::expose_secret)
            .or_else(|| credential.as_ref().map(|c| c.expose_secret()))
            .ok_or(UsageFetchError::Authentication)?;
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("authorization"),
            HeaderValue::from_str(&format!("Bearer {token}"))
                .map_err(|_| UsageFetchError::Invalid)?,
        );
        headers.insert(
            HeaderName::from_static("accept"),
            HeaderValue::from_static("application/json"),
        );
        if let Some(user) = target
            .profile
            .as_ref()
            .and_then(|p| p.platform_user_id.as_deref())
            && provider == UpstreamUsageProvider::NewApi
        {
            headers.insert(
                HeaderName::from_static("new-api-user"),
                HeaderValue::from_str(user).map_err(|_| UsageFetchError::Invalid)?,
            );
        }
        let mut snapshot = if provider == UpstreamUsageProvider::Codex {
            for (name, value) in [
                ("openai-beta", "codex-1"),
                ("originator", "Codex Desktop"),
                ("oai-language", "zh-CN"),
            ] {
                headers.insert(
                    HeaderName::from_static(name),
                    HeaderValue::from_static(value),
                );
            }
            if let Some(account) = &target.oauth_account_key {
                headers.insert(
                    HeaderName::from_static("chatgpt-account-id"),
                    HeaderValue::from_str(account).map_err(|_| UsageFetchError::Invalid)?,
                );
            }
            let value = request(&client, CODEX_USAGE_URL, headers).await?;
            parse_codex(&value)?
        } else {
            let base = target
                .profile
                .as_ref()
                .map(|p| p.base_url.as_str())
                .or(target.base_url.as_deref())
                .ok_or(UsageFetchError::Unsupported)?;
            let endpoint = match (provider, target.config.scope) {
                (
                    UpstreamUsageProvider::NewApi | UpstreamUsageProvider::OneApi,
                    UpstreamUsageScope::Key,
                ) => "/api/usage/token/",
                (
                    UpstreamUsageProvider::NewApi | UpstreamUsageProvider::OneApi,
                    UpstreamUsageScope::Account,
                ) => "/api/user/self",
                (UpstreamUsageProvider::Sub2api, UpstreamUsageScope::Account) => "/api/v1/auth/me",
                _ => return Err(UsageFetchError::Unsupported),
            };
            let url = management_url(base, endpoint)?;
            let value = request(&client, &url, headers.clone()).await?;
            let mut parsed = parse_panel(&value, target)?;
            if provider == UpstreamUsageProvider::Sub2api {
                let summary = management_url(base, "/api/v1/subscriptions/summary")?;
                if let Ok(value) = request(&client, &summary, headers).await {
                    parsed.subscription = sanitize_subscription(&value);
                }
            }
            parsed
        };
        snapshot.provider = Some(provider.as_str().to_owned());
        snapshot.status = CredentialUsageStatus::Available;
        Ok(snapshot)
    }
}

async fn request(
    client: &PooledClient,
    url: &str,
    headers: HeaderMap,
) -> Result<Value, UsageFetchError> {
    let response = client
        .execute(Method::GET, url, headers, None)
        .await
        .map_err(|_| UsageFetchError::Unavailable)?;
    match response.status() {
        StatusCode::OK => {}
        StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED | StatusCode::NOT_IMPLEMENTED => {
            return Err(UsageFetchError::Unsupported);
        }
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
            return Err(UsageFetchError::Authentication);
        }
        StatusCode::TOO_MANY_REQUESTS => return Err(UsageFetchError::RateLimited),
        _ => return Err(UsageFetchError::Unavailable),
    }
    if response
        .content_length()
        .is_some_and(|size| size > MAX_BYTES as u64)
    {
        return Err(UsageFetchError::Invalid);
    }
    let mut body = Vec::new();
    let mut chunks = response.into_bytes_stream();
    while let Some(chunk) = chunks.next().await {
        let chunk = chunk.map_err(|_| UsageFetchError::Unavailable)?;
        if chunk.len() > MAX_BYTES - body.len() {
            return Err(UsageFetchError::Invalid);
        }
        body.extend_from_slice(&chunk);
    }
    let value: Value = serde_json::from_slice(&body).map_err(|_| UsageFetchError::Invalid)?;
    if value.get("success") == Some(&Value::Bool(false)) || value.get("error").is_some() {
        return Err(UsageFetchError::Authentication);
    }
    Ok(value)
}

pub(super) fn validate_base_url(base: &str) -> Result<Url, UsageFetchError> {
    let url = Url::parse(base).map_err(|_| UsageFetchError::Invalid)?;
    let host = url.host_str().ok_or(UsageFetchError::Invalid)?;
    if base.len() > 2048
        || url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || host == "localhost"
        || host.ends_with(".localhost")
        || host.ends_with(".local")
        || host.ends_with(".internal")
    {
        return Err(UsageFetchError::Invalid);
    }
    if let Ok(ip) = host.trim_matches(['[', ']']).parse::<IpAddr>() {
        let public = match ip {
            IpAddr::V4(ip) => {
                !ip.is_private()
                    && !ip.is_loopback()
                    && !ip.is_link_local()
                    && !ip.is_unspecified()
                    && !ip.is_documentation()
                    && !ip.is_broadcast()
                    && ip.octets()[0] != 0
                    && ip.octets()[0] < 224
            }
            IpAddr::V6(ip) => {
                (ip.segments()[0] & 0xe000) == 0x2000
                    && !(ip.segments()[0] == 0x2001 && ip.segments()[1] == 0x0db8)
            }
        };
        if !public {
            return Err(UsageFetchError::Invalid);
        }
    }
    Ok(url)
}

pub(super) fn management_url(base: &str, path: &str) -> Result<String, UsageFetchError> {
    validate_base_url(base)?;
    let trimmed = base.trim_end_matches('/');
    let root = trimmed
        .strip_suffix("/v1")
        .or_else(|| trimmed.strip_suffix("/v1beta"))
        .unwrap_or(trimmed);
    Ok(format!("{root}{path}"))
}

fn decimal(value: Option<&Value>) -> Option<Decimal> {
    let value = value?;
    let text = match value {
        Value::String(v) => v.clone(),
        Value::Number(n) => n.to_string(),
        _ => return None,
    };
    if text.len() > 64 {
        return None;
    }
    text.parse().ok()
}

pub(super) fn parse_panel(
    value: &Value,
    target: &UpstreamUsageTarget,
) -> Result<CredentialUsageSnapshot, UsageFetchError> {
    let data = value.get("data").unwrap_or(value);
    let provider = effective_provider(target);
    let mut snapshot = CredentialUsageSnapshot::unavailable();
    snapshot.scope = Some(
        match target.config.scope {
            UpstreamUsageScope::Key => "key",
            UpstreamUsageScope::Account => "account",
        }
        .to_owned(),
    );
    let (total, used, remaining) = if provider == UpstreamUsageProvider::Sub2api {
        let remaining = decimal(data.get("balance")).ok_or(UsageFetchError::Invalid)?;
        snapshot.currency = Some("USD".to_owned());
        snapshot.unit = Some("USD".to_owned());
        (None, None, Some(remaining))
    } else if target.config.scope == UpstreamUsageScope::Key {
        // Older billing_subscription endpoints may return an account balance and cannot establish key scope.
        if data.get("object").and_then(Value::as_str) != Some("token_usage") {
            return Err(UsageFetchError::Unsupported);
        }
        snapshot.unlimited = data
            .get("unlimited_quota")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        snapshot.expires_at = data
            .get("expires_at")
            .and_then(Value::as_i64)
            .filter(|at| *at > 0);
        (
            decimal(data.get("total_granted")),
            decimal(data.get("total_used")),
            decimal(data.get("total_available")),
        )
    } else {
        let quota = decimal(data.get("quota")).ok_or(UsageFetchError::Invalid)?;
        let used = decimal(data.get("used_quota"));
        // Account management APIs report remaining quota. Do not reuse ambiguous legacy billing data.
        (
            used.and_then(|used| quota.checked_add(used)),
            used,
            Some(quota),
        )
    };
    if remaining.is_none() && !snapshot.unlimited {
        return Err(UsageFetchError::Invalid);
    }
    let divisor = if provider == UpstreamUsageProvider::Sub2api {
        None
    } else {
        target
            .profile
            .as_ref()
            .map_or(target.config.quota_per_unit.as_deref(), |p| {
                p.quota_per_unit.as_deref()
            })
            .and_then(|v| v.parse::<Decimal>().ok())
    };
    let convert = |amount: Option<Decimal>| -> Result<Option<String>, UsageFetchError> {
        amount
            .map(|amount| match divisor {
                Some(divisor) => amount.checked_div(divisor).ok_or(UsageFetchError::Invalid),
                None => Ok(amount),
            })
            .transpose()
            .map(|v| v.map(|n| n.normalize().to_string()))
    };
    if provider != UpstreamUsageProvider::Sub2api {
        snapshot.currency = divisor.and_then(|_| {
            target
                .profile
                .as_ref()
                .map_or_else(|| target.config.currency.clone(), |p| p.currency.clone())
        });
        snapshot.unit = Some(
            snapshot
                .currency
                .clone()
                .unwrap_or_else(|| if divisor.is_some() { "unit" } else { "quota" }.to_owned()),
        );
    }
    snapshot.total = if snapshot.unlimited {
        None
    } else {
        convert(total)?
    };
    snapshot.used = convert(used)?;
    snapshot.remaining = if snapshot.unlimited {
        None
    } else {
        convert(remaining)?
    };
    snapshot.source = Some(
        if provider == UpstreamUsageProvider::Sub2api {
            "auth_me"
        } else if target.config.scope == UpstreamUsageScope::Key {
            "token_usage"
        } else {
            "user_self"
        }
        .to_owned(),
    );
    Ok(snapshot)
}

pub(super) fn parse_codex(value: &Value) -> Result<CredentialUsageSnapshot, UsageFetchError> {
    let mut snapshot = CredentialUsageSnapshot::unavailable();
    snapshot.scope = Some("window".to_owned());
    snapshot.unit = Some("percent".to_owned());
    snapshot.source = Some("codex_usage".to_owned());
    if let Some(limits) = value.get("rate_limit") {
        for name in ["primary_window", "secondary_window"] {
            if let Some(window) = limits.get(name) {
                let used = window.get("used_percent").and_then(Value::as_f64);
                let seconds = window.get("limit_window_seconds").and_then(Value::as_i64);
                if let (Some(used), Some(seconds)) = (used, seconds)
                    && used.is_finite()
                    && (0.0..=100.0).contains(&used)
                    && seconds > 0
                {
                    snapshot.windows.push(CredentialUsageWindow {
                        window_seconds: seconds,
                        used_percent: used,
                        reset_at: window
                            .get("reset_at")
                            .and_then(Value::as_i64)
                            .filter(|at| *at > 0),
                    });
                }
            }
        }
    }
    snapshot.reset_at = snapshot.windows.iter().filter_map(|w| w.reset_at).min();
    snapshot.credits_balance = decimal(value.get("credits").and_then(|v| v.get("balance")))
        .map(|v| v.normalize().to_string());
    if snapshot.windows.is_empty() && snapshot.credits_balance.is_none() {
        return Err(UsageFetchError::Invalid);
    }
    Ok(snapshot)
}

pub(super) fn sanitize_subscription(value: &Value) -> Option<Value> {
    let data = value.get("data").unwrap_or(value);
    let rows = data
        .get("subscriptions")
        .and_then(Value::as_array)
        .or_else(|| data.as_array())?;
    let items:Vec<Value>=rows.iter().take(100).map(|row| {
        let name=row.get("group_name").or_else(||row.get("name")).and_then(Value::as_str)
            .filter(|name|name.len()<=128).unwrap_or("");
        serde_json::json!({"name":name,"expires_at":row.get("expires_at").and_then(Value::as_str).filter(|v|v.len()<=64),
            "daily_limit_usd":decimal(row.get("daily_limit_usd")).map(|n|n.normalize().to_string()),
            "daily_used_usd":decimal(row.get("daily_used_usd")).map(|n|n.normalize().to_string()),
            "monthly_limit_usd":decimal(row.get("monthly_limit_usd")).map(|n|n.normalize().to_string()),
            "monthly_used_usd":decimal(row.get("monthly_used_usd")).map(|n|n.normalize().to_string())})
    }).collect();
    Some(serde_json::json!({"items":items}))
}
