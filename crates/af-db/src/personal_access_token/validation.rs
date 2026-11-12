use af_domain::IpCidr;
use serde_json::Value;

use super::{
    MAX_PERSONAL_ACCESS_TOKEN_IPS, MAX_PERSONAL_ACCESS_TOKEN_NAME_BYTES,
    MAX_PERSONAL_ACCESS_TOKEN_SCOPES, PersonalAccessTokenRepositoryError as Error,
};

const USER_SCOPES: &[&str] = &[
    "account:read",
    "account:write",
    "account_security:read",
    "account_security:write",
    "models:read",
    "usage:read",
    "billing:read",
    "billing:write",
    "api_keys:read",
    "api_keys:write",
    "routes:read",
    "routes:write",
    "merchant:read",
    "merchant:write",
    "merchant_secrets:write",
    "merchant_finance:read",
    "merchant_finance:write",
    "notifications:read",
    "notifications:write",
    "playground:read",
    "playground:write",
    "verification:read",
    "verification:write",
    "subscriptions:read",
    "subscriptions:write",
    "invitations:read",
    "api_catalog:read",
];

pub(super) fn text(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.len() <= max
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

pub(super) fn fields(
    name: &str,
    scopes: &[String],
    expires_at: Option<i64>,
    ips: Option<&[String]>,
    now: i64,
) -> Result<(), Error> {
    if !text(name, MAX_PERSONAL_ACCESS_TOKEN_NAME_BYTES)
        || scopes.is_empty()
        || scopes.len() > MAX_PERSONAL_ACCESS_TOKEN_SCOPES
        || scopes.iter().any(|scope| !valid_scope(scope))
        || expires_at.is_some_and(|expiry| expiry < 0 || expiry > now.saturating_add(366 * 86_400))
        || serde_json::to_vec(scopes)
            .map_err(|_| Error::InvalidInput)?
            .len()
            > 128 * 1024
        || ips.is_some_and(|values| {
            values.is_empty()
                || values.len() > MAX_PERSONAL_ACCESS_TOKEN_IPS
                || values.iter().any(|value| value.parse::<IpCidr>().is_err())
        })
    {
        return Err(Error::InvalidInput);
    }
    let mut unique = scopes.to_vec();
    unique.sort();
    unique.dedup();
    if unique.len() != scopes.len() {
        return Err(Error::InvalidInput);
    }
    Ok(())
}

pub(super) fn grant(role: i16, scopes: &[String]) -> Result<(), Error> {
    if !matches!(role, 0 | 1) {
        return Err(Error::Invariant);
    }
    if role == 0
        && scopes
            .iter()
            .any(|scope| !USER_SCOPES.contains(&scope.as_str()))
    {
        return Err(Error::Forbidden);
    }
    Ok(())
}

pub(super) fn effective_grant(role: i16, mut scopes: Vec<String>) -> Result<Vec<String>, Error> {
    if !matches!(role, 0 | 1) {
        return Err(Error::Invariant);
    }
    if role == 0 {
        scopes.retain(|scope| USER_SCOPES.contains(&scope.as_str()));
    }
    Ok(scopes)
}

pub(super) fn valid_scope(scope: &str) -> bool {
    if USER_SCOPES.contains(&scope) {
        return true;
    }
    let parts: Vec<_> = scope.split(':').collect();
    parts.len() == 3
        && parts[0] == "admin"
        && parts[2] == "execute"
        && text(scope, 128)
        && !parts[1].is_empty()
        && parts[1]
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

pub(super) fn json_strings(value: &Value) -> Result<Vec<String>, Error> {
    serde_json::from_value(value.clone()).map_err(|_| Error::Invariant)
}

pub(super) fn json_ips(value: Option<&Value>) -> Result<Option<Vec<String>>, Error> {
    value.map(json_strings).transpose()
}

pub(super) fn payload_digest(
    name: &str,
    scopes: &[String],
    expiry: Option<i64>,
    ips: Option<&[String]>,
    enabled: bool,
    version: Option<i64>,
) -> Result<String, Error> {
    use sha2::{Digest, Sha256};
    let mut scopes = scopes.to_vec();
    scopes.sort();
    scopes.dedup();
    let ips = ips
        .map(|ips| {
            ips.iter()
                .map(|ip| {
                    let cidr = ip.parse::<IpCidr>().map_err(|_| Error::InvalidInput)?;
                    serde_json::to_value(cidr)
                        .map_err(|_| Error::InvalidInput)?
                        .as_str()
                        .map(str::to_owned)
                        .ok_or(Error::InvalidInput)
                })
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?
        .map(|mut ips| {
            ips.sort();
            ips.dedup();
            ips
        });
    let bytes = serde_json::to_vec(&serde_json::json!({
        "name": name, "scopes": scopes, "expires_at": expiry, "allow_ips": ips,
        "enabled": enabled, "version": version,
    }))
    .map_err(|_| Error::InvalidInput)?;
    let mut hasher = Sha256::new();
    hasher.update(b"anyflows:personal-access-token:operation:v1\0");
    hasher.update(bytes);
    Ok(format!("{:x}", hasher.finalize()))
}
