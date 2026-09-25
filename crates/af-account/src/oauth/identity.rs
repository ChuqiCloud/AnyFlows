use std::{
    fmt,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::Deserialize;
use uuid::Uuid;
use zeroize::Zeroizing;

use super::UpstreamOAuthProvider;

/// OAuth 身份字段的数据库边界；该值只用于路由和管理展示，不保存令牌正文。
pub(super) const MAX_OAUTH_IDENTITY_BYTES: usize = 255;
const MAX_CODEX_JWT_SEGMENT_BYTES: usize = 12 * 1_024;
const MAX_CODEX_JWT_PAYLOAD_BYTES: usize = 8 * 1_024;

/// 从 provider 响应中提取出的非敏感 OAuth 身份元数据。
#[derive(Clone, Default, Eq, PartialEq)]
pub(super) struct OAuthIdentityMetadata {
    account_key: Option<String>,
    project_id: Option<String>,
}

impl OAuthIdentityMetadata {
    /// 解析首次授权响应；Codex 必须提供可解析的账号身份，其他 provider 可缺省。
    pub(super) fn parse_initial(
        provider: UpstreamOAuthProvider,
        id_token: Option<&str>,
        claude_account_uuid: Option<&str>,
    ) -> Result<Self, OAuthIdentityError> {
        Self::parse(provider, id_token, claude_account_uuid, true)
    }

    /// 解析刷新响应；响应未携带身份时返回空补丁，由持久化层保留已有字段。
    pub(super) fn parse_refresh(
        provider: UpstreamOAuthProvider,
        id_token: Option<&str>,
        claude_account_uuid: Option<&str>,
    ) -> Result<Self, OAuthIdentityError> {
        Self::parse(provider, id_token, claude_account_uuid, false)
    }

    fn parse(
        provider: UpstreamOAuthProvider,
        id_token: Option<&str>,
        claude_account_uuid: Option<&str>,
        require_codex_identity: bool,
    ) -> Result<Self, OAuthIdentityError> {
        match provider {
            UpstreamOAuthProvider::Codex => match id_token {
                Some(id_token) => Ok(Self {
                    account_key: Some(parse_codex_account_key(id_token)?),
                    project_id: None,
                }),
                None if require_codex_identity => Err(OAuthIdentityError::MissingCodexAccountKey),
                None => Ok(Self::default()),
            },
            UpstreamOAuthProvider::ClaudeCode => Ok(Self {
                account_key: claude_account_uuid
                    .map(parse_claude_account_key)
                    .transpose()?,
                project_id: None,
            }),
            UpstreamOAuthProvider::Gemini | UpstreamOAuthProvider::Antigravity => {
                Ok(Self::default())
            }
        }
    }

    pub(super) fn account_key(&self) -> Option<&str> {
        self.account_key.as_deref()
    }

    pub(super) fn project_id(&self) -> Option<&str> {
        self.project_id.as_deref()
    }

    pub(super) fn into_parts(self) -> (Option<String>, Option<String>) {
        (self.account_key, self.project_id)
    }

    #[cfg(test)]
    pub(super) fn for_test(account_key: Option<String>, project_id: Option<String>) -> Self {
        Self {
            account_key,
            project_id,
        }
    }
}

impl fmt::Debug for OAuthIdentityMetadata {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OAuthIdentityMetadata")
            .field(
                "account_key",
                &self.account_key.as_ref().map(|_| "<已脱敏>"),
            )
            .field("project_id", &self.project_id.as_ref().map(|_| "<已脱敏>"))
            .finish()
    }
}

/// 身份字段解析失败的内部分类；上层统一映射为无效 token 响应。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum OAuthIdentityError {
    MissingCodexAccountKey,
    InvalidCodexIdToken,
    InvalidClaudeAccountKey,
}

#[derive(Deserialize)]
struct CodexIdTokenClaims {
    #[serde(rename = "https://api.openai.com/auth")]
    auth: Option<CodexAuthClaims>,
    #[serde(rename = "https://api.openai.com/auth.chatgpt_account_id")]
    legacy_account_key: Option<String>,
}

#[derive(Deserialize)]
struct CodexAuthClaims {
    #[serde(default)]
    chatgpt_account_id: Option<String>,
}

#[derive(Deserialize)]
struct CodexAccessTokenClaims {
    exp: u64,
}

#[derive(Deserialize)]
pub(super) struct ClaudeAccountKey {
    #[serde(default)]
    uuid: Option<String>,
}

impl ClaudeAccountKey {
    pub(super) fn uuid(&self) -> Option<&str> {
        self.uuid.as_deref()
    }
}

fn parse_codex_account_key(id_token: &str) -> Result<String, OAuthIdentityError> {
    let payload = decode_codex_jwt_payload(id_token)?;
    let claims = serde_json::from_slice::<CodexIdTokenClaims>(&payload)
        .map_err(|_| OAuthIdentityError::InvalidCodexIdToken)?;
    let account_key = claims
        .auth
        .and_then(|auth| auth.chatgpt_account_id)
        .or(claims.legacy_account_key)
        .ok_or(OAuthIdentityError::MissingCodexAccountKey)?;
    if !valid_identity_component(&account_key) {
        return Err(OAuthIdentityError::InvalidCodexIdToken);
    }
    Ok(account_key)
}

pub(super) fn codex_access_token_expires_in(access_token: &str) -> Option<Duration> {
    let payload = decode_codex_jwt_payload(access_token).ok()?;
    let claims = serde_json::from_slice::<CodexAccessTokenClaims>(&payload).ok()?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
    let seconds = claims.exp.checked_sub(now)?;
    (seconds > 0).then(|| Duration::from_secs(seconds))
}

fn decode_codex_jwt_payload(token: &str) -> Result<Zeroizing<Vec<u8>>, OAuthIdentityError> {
    let mut segments = token.split('.');
    let header = segments.next();
    let payload = segments.next();
    let signature = segments.next();
    if header.is_none() || payload.is_none() || signature.is_none() || segments.next().is_some() {
        return Err(OAuthIdentityError::InvalidCodexIdToken);
    }
    let header = decode_jwt_segment(header.unwrap(), MAX_CODEX_JWT_SEGMENT_BYTES)?;
    let payload = decode_jwt_segment(payload.unwrap(), MAX_CODEX_JWT_PAYLOAD_BYTES)?;
    let _signature = decode_jwt_segment(signature.unwrap(), MAX_CODEX_JWT_SEGMENT_BYTES)?;
    if serde_json::from_slice::<serde_json::Map<String, serde_json::Value>>(&header).is_err() {
        return Err(OAuthIdentityError::InvalidCodexIdToken);
    }
    Ok(payload)
}

fn decode_jwt_segment(
    segment: &str,
    maximum_decoded_bytes: usize,
) -> Result<Zeroizing<Vec<u8>>, OAuthIdentityError> {
    if segment.is_empty() || segment.len() > MAX_CODEX_JWT_SEGMENT_BYTES {
        return Err(OAuthIdentityError::InvalidCodexIdToken);
    }
    let decoded = URL_SAFE_NO_PAD
        .decode(segment.as_bytes())
        .map_err(|_| OAuthIdentityError::InvalidCodexIdToken)?;
    if decoded.is_empty() || decoded.len() > maximum_decoded_bytes {
        return Err(OAuthIdentityError::InvalidCodexIdToken);
    }
    Ok(Zeroizing::new(decoded))
}

fn parse_claude_account_key(value: &str) -> Result<String, OAuthIdentityError> {
    if !valid_identity_component(value) {
        return Err(OAuthIdentityError::InvalidClaudeAccountKey);
    }
    let uuid = Uuid::parse_str(value).map_err(|_| OAuthIdentityError::InvalidClaudeAccountKey)?;
    Ok(uuid.hyphenated().to_string())
}

fn valid_identity_component(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_OAUTH_IDENTITY_BYTES
        && value.trim() == value
        && value.is_ascii()
        && !value
            .bytes()
            .any(|byte| byte.is_ascii_whitespace() || byte.is_ascii_control())
}

#[cfg(test)]
mod tests {
    use super::*;

    const CODEX_ACCOUNT_CLAIM: &str = "https://api.openai.com/auth";
    const CODEX_ACCOUNT: &str = "org-codex-account";
    const CLAUDE_ACCOUNT: &str = "550E8400-E29B-41D4-A716-446655440000";

    fn codex_id_token(payload: &str) -> String {
        format!(
            "{}.{}.{}",
            encode_segment(br#"{"alg":"RS256","typ":"JWT"}"#),
            encode_segment(payload.as_bytes()),
            encode_segment(b"signature")
        )
    }

    fn encode_segment(value: &[u8]) -> String {
        URL_SAFE_NO_PAD.encode(value)
    }

    #[test]
    fn codex_claim_is_extracted_only_from_a_valid_jwt() {
        let token = codex_id_token(&format!(
            r#"{{"{CODEX_ACCOUNT_CLAIM}":{{"chatgpt_account_id":"{CODEX_ACCOUNT}"}}}}"#
        ));
        let identity =
            OAuthIdentityMetadata::parse_initial(UpstreamOAuthProvider::Codex, Some(&token), None)
                .unwrap();
        assert_eq!(identity.account_key(), Some(CODEX_ACCOUNT));
        assert_eq!(identity.project_id(), None);

        let legacy = codex_id_token(&format!(
            r#"{{"https://api.openai.com/auth.chatgpt_account_id":"{CODEX_ACCOUNT}"}}"#
        ));
        assert_eq!(parse_codex_account_key(&legacy).unwrap(), CODEX_ACCOUNT);

        let duplicate = codex_id_token(&format!(
            r#"{{"{CODEX_ACCOUNT_CLAIM}":{{"chatgpt_account_id":"first","chatgpt_account_id":"second"}}}}"#
        ));
        let oversized = codex_id_token(&format!(
            r#"{{"{CODEX_ACCOUNT_CLAIM}":{{"chatgpt_account_id":"{}"}}}}"#,
            "x".repeat(MAX_OAUTH_IDENTITY_BYTES + 1)
        ));
        for invalid in [
            "not-a-jwt".to_owned(),
            duplicate,
            oversized,
            codex_id_token(&format!(
                r#"{{"{CODEX_ACCOUNT_CLAIM}":{{"chatgpt_account_id":"bad id"}}}}"#
            )),
        ] {
            assert_eq!(
                OAuthIdentityMetadata::parse_initial(
                    UpstreamOAuthProvider::Codex,
                    Some(&invalid),
                    None,
                )
                .unwrap_err(),
                OAuthIdentityError::InvalidCodexIdToken
            );
        }
        assert_eq!(
            OAuthIdentityMetadata::parse_initial(UpstreamOAuthProvider::Codex, None, None)
                .unwrap_err(),
            OAuthIdentityError::MissingCodexAccountKey
        );
        assert_eq!(
            parse_codex_account_key(&codex_id_token("{}")).unwrap_err(),
            OAuthIdentityError::MissingCodexAccountKey
        );
    }

    #[test]
    fn codex_access_token_expiration_uses_bounded_jwt_claims() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let valid = codex_id_token(&format!(r#"{{"exp":{}}}"#, now + 3_600));
        let lifetime = codex_access_token_expires_in(&valid).unwrap().as_secs();
        assert!((3_599..=3_600).contains(&lifetime));
        for invalid in [
            codex_id_token("{}"),
            codex_id_token(&format!(r#"{{"exp":{now}}}"#)),
            codex_id_token(&format!(r#"{{"exp":{}}}"#, now.saturating_sub(1))),
            "opaque-access-token".to_owned(),
        ] {
            assert!(codex_access_token_expires_in(&invalid).is_none());
        }
    }

    #[test]
    fn claude_uuid_is_normalized_and_optional() {
        let identity = OAuthIdentityMetadata::parse_initial(
            UpstreamOAuthProvider::ClaudeCode,
            None,
            Some(CLAUDE_ACCOUNT),
        )
        .unwrap();
        assert_eq!(
            identity.account_key(),
            Some("550e8400-e29b-41d4-a716-446655440000")
        );
        assert_eq!(
            OAuthIdentityMetadata::parse_initial(UpstreamOAuthProvider::ClaudeCode, None, None)
                .unwrap(),
            OAuthIdentityMetadata::default()
        );
        for invalid in ["", "not-a-uuid", " 550e8400-e29b-41d4-a716-446655440000"] {
            assert_eq!(
                OAuthIdentityMetadata::parse_initial(
                    UpstreamOAuthProvider::ClaudeCode,
                    None,
                    Some(invalid),
                )
                .unwrap_err(),
                OAuthIdentityError::InvalidClaudeAccountKey
            );
        }
    }

    #[test]
    fn refresh_keeps_missing_identity_empty_for_all_providers() {
        for provider in [
            UpstreamOAuthProvider::Codex,
            UpstreamOAuthProvider::Gemini,
            UpstreamOAuthProvider::Antigravity,
        ] {
            assert_eq!(
                OAuthIdentityMetadata::parse_refresh(provider, None, None).unwrap(),
                OAuthIdentityMetadata::default()
            );
        }
    }

    #[test]
    fn debug_does_not_expose_identity_values() {
        let token = codex_id_token(&format!(
            r#"{{"{CODEX_ACCOUNT_CLAIM}":{{"chatgpt_account_id":"{CODEX_ACCOUNT}"}}}}"#
        ));
        let identity =
            OAuthIdentityMetadata::parse_initial(UpstreamOAuthProvider::Codex, Some(&token), None)
                .unwrap();
        let rendered = format!("{identity:?}");
        assert!(!rendered.contains(CODEX_ACCOUNT));
    }
}
