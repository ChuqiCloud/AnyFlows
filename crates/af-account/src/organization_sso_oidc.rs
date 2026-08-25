//! 企业 SSO OIDC ID Token 的纯验证边界。
//!
//! 本模块只消费已由受控 HTTP 层取得的 JWKS 与 ID Token，不发起网络请求，
//! 也不持久化 token、JWKS 或外部 subject 原文。

use std::{
    fmt,
    net::IpAddr,
    time::{SystemTime, UNIX_EPOCH},
};

use jsonwebtoken::{
    Algorithm, DecodingKey, Validation, decode, decode_header,
    jwk::{Jwk, JwkSet, KeyAlgorithm, KeyOperations, PublicKeyUse},
};
use serde::Deserialize;
use sha2::{Digest as _, Sha256};
use thiserror::Error;
use url::Url;

use crate::PlainOrganizationInvitationEmail;

const MAX_ISSUER_BYTES: usize = 2_048;
const MAX_CLIENT_ID_BYTES: usize = 255;
const MAX_ID_TOKEN_BYTES: usize = 16 * 1024;
const MAX_KEY_ID_BYTES: usize = 255;
const MAX_NONCE_BYTES: usize = 512;
const MAX_SUBJECT_BYTES: usize = 255;
const CLOCK_SKEW_SECONDS: i64 = 60;

/// 企业 OIDC ID Token 验证所需的固定 Provider 快照。
#[derive(Clone, Eq, PartialEq)]
pub struct OrganizationSsoOidcIdTokenVerificationConfig {
    issuer: String,
    audience: String,
}

impl OrganizationSsoOidcIdTokenVerificationConfig {
    /// 构造不可变验证快照，issuer 与 audience 不接受回调请求覆盖。
    pub fn new(issuer: String, audience: String) -> Result<Self, OrganizationSsoOidcIdTokenError> {
        validate_issuer(&issuer)?;
        if !valid_ascii_parameter(&audience, MAX_CLIENT_ID_BYTES) {
            return Err(OrganizationSsoOidcIdTokenError::InvalidConfiguration);
        }
        Ok(Self { issuer, audience })
    }

    /// 返回已规范化校验的 issuer，仅供受控 discovery 层进行精确比对。
    #[must_use]
    pub fn issuer(&self) -> &str {
        &self.issuer
    }

    /// 返回已校验的 client identifier，供 ID Token audience/azp 验证使用。
    #[must_use]
    pub fn audience(&self) -> &str {
        &self.audience
    }
}

impl fmt::Debug for OrganizationSsoOidcIdTokenVerificationConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationSsoOidcIdTokenVerificationConfig(<redacted>)")
    }
}

/// 已通过企业 OIDC ID Token 验证的最小身份投影。
pub struct VerifiedOrganizationSsoOidcIdentity {
    subject: String,
    invitation_email: PlainOrganizationInvitationEmail,
}

impl VerifiedOrganizationSsoOidcIdentity {
    /// 返回经过签名与声明验证的 OIDC subject，调用方不得写入日志或普通响应。
    #[must_use]
    pub fn subject(&self) -> &str {
        &self.subject
    }

    /// 返回已验证且已规范化的邮箱，只可用于企业邀请指纹计算。
    #[must_use]
    pub fn invitation_email(&self) -> &PlainOrganizationInvitationEmail {
        &self.invitation_email
    }

    /// 返回经过验证的 subject 域分离摘要；沿用既有断言摘要域以兼容在途票据。
    #[must_use]
    pub fn subject_digest(&self) -> String {
        let mut digest = Sha256::new();
        digest.update(b"organization-sso-oidc-assertion-v1");
        digest.update([0]);
        digest.update(self.subject.as_bytes());
        encode_digest(&digest.finalize())
    }

    /// 返回身份断言摘要；保留该名称供已有回调完成路径兼容使用。
    #[must_use]
    pub fn assertion_digest(&self) -> String {
        self.subject_digest()
    }
}

impl fmt::Debug for VerifiedOrganizationSsoOidcIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("VerifiedOrganizationSsoOidcIdentity(<redacted>)")
    }
}

/// 企业 OIDC ID Token 的稳定拒绝分类，不包含 token、JWKS 或声明原文。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationSsoOidcIdTokenError {
    /// Provider 快照不满足受控 OIDC 配置边界。
    #[error("企业 OIDC Provider 配置无效")]
    InvalidConfiguration,
    /// ID Token、JWKS 或声明不满足固定验证规则。
    #[error("企业 OIDC ID Token 验证失败")]
    Rejected,
    /// 本机时间不可用于安全地验证时间声明。
    #[error("企业 OIDC ID Token 时间校验失败")]
    TimeUnavailable,
}

/// 企业 OIDC ID Token 纯验证器；网络读取必须由外层受控 discovery/JWKS 适配器完成。
#[derive(Clone)]
pub struct OrganizationSsoOidcIdTokenVerifier {
    config: OrganizationSsoOidcIdTokenVerificationConfig,
}

impl OrganizationSsoOidcIdTokenVerifier {
    /// 绑定创建登录事务时锁定的 OIDC issuer 与 client identifier。
    #[must_use]
    pub const fn new(config: OrganizationSsoOidcIdTokenVerificationConfig) -> Self {
        Self { config }
    }

    /// 验证 RS256 签名、JWKS key id、issuer、audience/azp、nonce 与时间声明。
    pub fn verify(
        &self,
        id_token: &str,
        jwks: &JwkSet,
        expected_nonce: &str,
    ) -> Result<VerifiedOrganizationSsoOidcIdentity, OrganizationSsoOidcIdTokenError> {
        if id_token.is_empty() || id_token.len() > MAX_ID_TOKEN_BYTES || !id_token.is_ascii() {
            return Err(OrganizationSsoOidcIdTokenError::Rejected);
        }
        if !valid_ascii_parameter(expected_nonce, MAX_NONCE_BYTES) {
            return Err(OrganizationSsoOidcIdTokenError::Rejected);
        }

        let header =
            decode_header(id_token).map_err(|_| OrganizationSsoOidcIdTokenError::Rejected)?;
        if header.alg != Algorithm::RS256 {
            return Err(OrganizationSsoOidcIdTokenError::Rejected);
        }
        let key_id = header
            .kid
            .filter(|value| valid_ascii_parameter(value, MAX_KEY_ID_BYTES))
            .ok_or(OrganizationSsoOidcIdTokenError::Rejected)?;
        let jwk = signing_jwk(jwks, &key_id)?;

        let mut validation = Validation::new(Algorithm::RS256);
        validation.leeway = 0;
        // 时间声明使用同一份系统时钟统一复验，避免库默认宽限与业务规则不一致。
        validation.validate_exp = false;
        validation.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
        validation.set_issuer(&[self.config.issuer()]);
        validation.set_audience(&[self.config.audience()]);
        let claims = decode::<OrganizationSsoOidcIdTokenClaims>(
            id_token,
            &DecodingKey::from_jwk(jwk).map_err(|_| OrganizationSsoOidcIdTokenError::Rejected)?,
            &validation,
        )
        .map_err(|_| OrganizationSsoOidcIdTokenError::Rejected)?
        .claims;

        validate_claims(
            &claims,
            self.config.audience(),
            expected_nonce,
            current_timestamp()?,
        )
    }
}

impl fmt::Debug for OrganizationSsoOidcIdTokenVerifier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationSsoOidcIdTokenVerifier(<configuration>)")
    }
}

#[derive(Deserialize)]
struct OrganizationSsoOidcIdTokenClaims {
    sub: String,
    email: String,
    email_verified: bool,
    aud: OrganizationSsoOidcAudience,
    azp: Option<String>,
    exp: i64,
    iat: i64,
    nonce: String,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum OrganizationSsoOidcAudience {
    Single(String),
    Multiple(Vec<String>),
}

fn validate_claims(
    claims: &OrganizationSsoOidcIdTokenClaims,
    expected_audience: &str,
    expected_nonce: &str,
    now: i64,
) -> Result<VerifiedOrganizationSsoOidcIdentity, OrganizationSsoOidcIdTokenError> {
    if now < 0
        || claims.iat < 0
        || claims.exp <= claims.iat
        || claims.iat > now.saturating_add(CLOCK_SKEW_SECONDS)
        || claims.exp <= now.saturating_sub(CLOCK_SKEW_SECONDS)
        || claims.nonce != expected_nonce
        || !valid_subject(&claims.sub)
        || !claims.email_verified
    {
        return Err(OrganizationSsoOidcIdTokenError::Rejected);
    }
    match &claims.aud {
        OrganizationSsoOidcAudience::Single(audience)
            if audience == expected_audience
                && claims
                    .azp
                    .as_deref()
                    .is_none_or(|azp| azp == expected_audience) => {}
        OrganizationSsoOidcAudience::Multiple(audiences)
            if audiences
                .iter()
                .any(|audience| audience == expected_audience)
                && claims.azp.as_deref() == Some(expected_audience) => {}
        _ => return Err(OrganizationSsoOidcIdTokenError::Rejected),
    }
    let invitation_email = PlainOrganizationInvitationEmail::new(claims.email.clone())
        .map_err(|_| OrganizationSsoOidcIdTokenError::Rejected)?;
    Ok(VerifiedOrganizationSsoOidcIdentity {
        subject: claims.sub.clone(),
        invitation_email,
    })
}

fn current_timestamp() -> Result<i64, OrganizationSsoOidcIdTokenError> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| OrganizationSsoOidcIdTokenError::TimeUnavailable)?
        .as_secs();
    i64::try_from(seconds).map_err(|_| OrganizationSsoOidcIdTokenError::TimeUnavailable)
}

fn validate_issuer(value: &str) -> Result<(), OrganizationSsoOidcIdTokenError> {
    if value.is_empty() || value.len() > MAX_ISSUER_BYTES || value.trim() != value {
        return Err(OrganizationSsoOidcIdTokenError::InvalidConfiguration);
    }
    let url =
        Url::parse(value).map_err(|_| OrganizationSsoOidcIdTokenError::InvalidConfiguration)?;
    if url.as_str() != value
        || url.scheme() != "https"
        || url.host_str().is_none()
        || url.username() != ""
        || url.password().is_some()
        || url.port().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || blocked_issuer_host(url.host_str().unwrap_or_default())
    {
        return Err(OrganizationSsoOidcIdTokenError::InvalidConfiguration);
    }
    Ok(())
}

/// 从受控 JWKS 中选择唯一、可用于 RS256 签名验证的 key，避免重复 `kid` 或加密 key 被误用。
fn signing_jwk<'a>(
    jwks: &'a JwkSet,
    key_id: &str,
) -> Result<&'a Jwk, OrganizationSsoOidcIdTokenError> {
    let mut matches = jwks
        .keys
        .iter()
        .filter(|jwk| jwk.common.key_id.as_deref() == Some(key_id));
    let jwk = matches
        .next()
        .filter(|jwk| {
            matches!(
                jwk.common.public_key_use.as_ref(),
                None | Some(PublicKeyUse::Signature)
            ) && jwk
                .common
                .key_operations
                .as_deref()
                .is_none_or(|operations| operations.contains(&KeyOperations::Verify))
                && jwk
                    .common
                    .key_algorithm
                    .as_ref()
                    .is_none_or(|algorithm| *algorithm == KeyAlgorithm::RS256)
        })
        .ok_or(OrganizationSsoOidcIdTokenError::Rejected)?;
    if matches.next().is_some() {
        return Err(OrganizationSsoOidcIdTokenError::Rejected);
    }
    Ok(jwk)
}

fn blocked_issuer_host(host: &str) -> bool {
    let normalized = host.trim_end_matches('.').to_ascii_lowercase();
    normalized == "localhost"
        || normalized.ends_with(".localhost")
        || normalized == "localhost.localdomain"
        || normalized.parse::<IpAddr>().is_ok()
}

fn valid_ascii_parameter(value: &str, max_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_bytes
        && value.trim() == value
        && value.bytes().all(|byte| byte.is_ascii_graphic())
}

fn valid_subject(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_SUBJECT_BYTES
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn encode_digest(digest: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(digest.len() * 2);
    for &byte in digest {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{EncodingKey, Header, encode};
    use serde_json::{Value, json};

    const ISSUER: &str = "https://idp.example.com/tenant";
    const AUDIENCE: &str = "anyflows-enterprise-client";
    const NONCE: &str = "nonce-material";
    const NOW: i64 = 1_800_000_000;
    const TEST_KEY_ID: &str = "organization-sso-test-key";
    const TEST_JWK_MODULUS: &str = "yRE6rHuNR0QbHO3H3Kt2pOKGVhQqGZXInOduQNxXzuKlvQTLUTv4l4sggh5_CYYi_cvI-SXVT9kPWSKXxJXBXd_4LkvcPuUakBoAkfh-eiFVMh2VrUyWyj3MFl0HTVF9KwRXLAcwkREiS3npThHRyIxuy0ZMeZfxVL5arMhw1SRELB8HoGfG_AtH89BIE9jDBHZ9dLelK9a184zAf8LwoPLxvJb3Il5nncqPcSfKDDodMFBIMc4lQzDKL5gvmiXLXB1AGLm8KBjfE8s3L5xqi-yUod-j8MtvIj812dkS4QMiRVN_by2h3ZY8LYVGrqZXZTcgn2ujn8uKjXLZVD5TdQ";
    // 该密钥仅用于公开的本地签名回归，不具备任何运行时或外部权限。
    const TEST_PRIVATE_KEY: &str = include_str!("fixtures/organization_sso_oidc_private.pem");

    fn claims(aud: OrganizationSsoOidcAudience) -> OrganizationSsoOidcIdTokenClaims {
        OrganizationSsoOidcIdTokenClaims {
            sub: "directory-subject-123".to_owned(),
            email: "member@example.com".to_owned(),
            email_verified: true,
            aud,
            azp: None,
            exp: NOW + 300,
            iat: NOW - 10,
            nonce: NONCE.to_owned(),
        }
    }

    #[test]
    fn configuration_requires_a_canonical_https_issuer_and_client_identifier() {
        assert!(
            OrganizationSsoOidcIdTokenVerificationConfig::new(
                ISSUER.to_owned(),
                AUDIENCE.to_owned()
            )
            .is_ok()
        );
        for issuer in [
            "http://idp.example.com",
            "https://user@idp.example.com",
            "https://idp.example.com?query=value",
            "https://idp.example.com#fragment",
            "https://idp.example.com:8443",
            "https://localhost/issuer",
            "https://127.0.0.1/issuer",
            " https://idp.example.com",
        ] {
            assert_eq!(
                OrganizationSsoOidcIdTokenVerificationConfig::new(
                    issuer.to_owned(),
                    AUDIENCE.to_owned()
                ),
                Err(OrganizationSsoOidcIdTokenError::InvalidConfiguration)
            );
        }
        assert_eq!(
            OrganizationSsoOidcIdTokenVerificationConfig::new(
                ISSUER.to_owned(),
                "client id".to_owned()
            ),
            Err(OrganizationSsoOidcIdTokenError::InvalidConfiguration)
        );
    }

    #[test]
    fn claims_require_exact_audience_nonce_and_azp_for_multiple_audiences() {
        let verified = validate_claims(
            &claims(OrganizationSsoOidcAudience::Single(AUDIENCE.to_owned())),
            AUDIENCE,
            NONCE,
            NOW,
        )
        .unwrap();
        assert_eq!(verified.subject(), "directory-subject-123");
        assert_eq!(
            verified.invitation_email().expose_email(),
            "member@example.com"
        );
        assert!(!format!("{verified:?}").contains(verified.subject()));

        let mut multiple = claims(OrganizationSsoOidcAudience::Multiple(vec![
            AUDIENCE.to_owned(),
            "another-client".to_owned(),
        ]));
        assert_eq!(
            validate_claims(&multiple, AUDIENCE, NONCE, NOW),
            Err(OrganizationSsoOidcIdTokenError::Rejected)
        );
        multiple.azp = Some(AUDIENCE.to_owned());
        assert!(validate_claims(&multiple, AUDIENCE, NONCE, NOW).is_ok());

        multiple.nonce = "different-nonce".to_owned();
        assert_eq!(
            validate_claims(&multiple, AUDIENCE, NONCE, NOW),
            Err(OrganizationSsoOidcIdTokenError::Rejected)
        );
    }

    #[test]
    fn claims_fail_closed_for_invalid_time_or_subject() {
        let mut invalid = claims(OrganizationSsoOidcAudience::Single(AUDIENCE.to_owned()));
        invalid.exp = NOW - CLOCK_SKEW_SECONDS;
        assert_eq!(
            validate_claims(&invalid, AUDIENCE, NONCE, NOW),
            Err(OrganizationSsoOidcIdTokenError::Rejected)
        );

        invalid = claims(OrganizationSsoOidcAudience::Single(AUDIENCE.to_owned()));
        invalid.iat = NOW + CLOCK_SKEW_SECONDS + 1;
        assert_eq!(
            validate_claims(&invalid, AUDIENCE, NONCE, NOW),
            Err(OrganizationSsoOidcIdTokenError::Rejected)
        );

        invalid = claims(OrganizationSsoOidcAudience::Single(AUDIENCE.to_owned()));
        invalid.sub = " subject".to_owned();
        assert_eq!(
            validate_claims(&invalid, AUDIENCE, NONCE, NOW),
            Err(OrganizationSsoOidcIdTokenError::Rejected)
        );
    }

    #[test]
    fn claims_require_a_verified_normalized_email() {
        let mut invalid = claims(OrganizationSsoOidcAudience::Single(AUDIENCE.to_owned()));
        invalid.email_verified = false;
        assert_eq!(
            validate_claims(&invalid, AUDIENCE, NONCE, NOW),
            Err(OrganizationSsoOidcIdTokenError::Rejected)
        );

        invalid = claims(OrganizationSsoOidcAudience::Single(AUDIENCE.to_owned()));
        invalid.email = "Member@example.com".to_owned();
        assert_eq!(
            validate_claims(&invalid, AUDIENCE, NONCE, NOW),
            Err(OrganizationSsoOidcIdTokenError::Rejected)
        );
    }

    #[test]
    fn assertion_digest_is_stable_domain_separated_and_redacted() {
        let identity = VerifiedOrganizationSsoOidcIdentity {
            subject: "directory-subject-123".to_owned(),
            invitation_email: PlainOrganizationInvitationEmail::new(
                "member@example.com".to_owned(),
            )
            .unwrap(),
        };
        let changed = VerifiedOrganizationSsoOidcIdentity {
            subject: "directory-subject-456".to_owned(),
            invitation_email: PlainOrganizationInvitationEmail::new(
                "member@example.com".to_owned(),
            )
            .unwrap(),
        };

        assert_eq!(identity.assertion_digest(), identity.assertion_digest());
        assert_eq!(identity.assertion_digest(), identity.subject_digest());
        assert_ne!(identity.assertion_digest(), changed.assertion_digest());
        assert_eq!(identity.assertion_digest().len(), 64);
        assert!(
            identity
                .assertion_digest()
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        );

        let debug = format!("{identity:?}");
        assert!(!debug.contains(identity.subject()));
    }

    #[test]
    fn verifier_accepts_a_real_rs256_signature_from_a_unique_signing_jwk() {
        let verifier = OrganizationSsoOidcIdTokenVerifier::new(
            OrganizationSsoOidcIdTokenVerificationConfig::new(
                ISSUER.to_owned(),
                AUDIENCE.to_owned(),
            )
            .unwrap(),
        );
        let token = signed_token(Some(json!(ISSUER)), json!(AUDIENCE), json!(NONCE));
        let verified = verifier
            .verify(&token, &test_jwks(json!({})), NONCE)
            .unwrap();

        assert_eq!(verified.subject(), "directory-subject-123");
    }

    #[test]
    fn verifier_rejects_missing_issuer_ambiguous_kid_and_non_signing_jwk() {
        let verifier = OrganizationSsoOidcIdTokenVerifier::new(
            OrganizationSsoOidcIdTokenVerificationConfig::new(
                ISSUER.to_owned(),
                AUDIENCE.to_owned(),
            )
            .unwrap(),
        );
        let token = signed_token(None, json!(AUDIENCE), json!(NONCE));
        assert_eq!(
            verifier.verify(&token, &test_jwks(json!({})), NONCE),
            Err(OrganizationSsoOidcIdTokenError::Rejected)
        );

        let token = signed_token(Some(json!(ISSUER)), json!(AUDIENCE), json!(NONCE));
        assert_eq!(
            verifier.verify(&token, &test_jwks(json!({"use": "enc"})), NONCE),
            Err(OrganizationSsoOidcIdTokenError::Rejected)
        );
        let mut first_jwks = test_jwks(json!({}));
        let mut second_jwks = test_jwks(json!({}));
        let duplicate_jwks = JwkSet {
            keys: vec![first_jwks.keys.remove(0), second_jwks.keys.remove(0)],
        };
        assert_eq!(
            verifier.verify(&token, &duplicate_jwks, NONCE),
            Err(OrganizationSsoOidcIdTokenError::Rejected)
        );
    }

    fn signed_token(issuer: Option<Value>, audience: Value, nonce: Value) -> String {
        let now = current_timestamp().unwrap();
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(TEST_KEY_ID.to_owned());
        let mut claims = json!({
            "sub": "directory-subject-123",
            "email": "member@example.com",
            "email_verified": true,
            "aud": audience,
            "azp": AUDIENCE,
            "exp": now + 300,
            "iat": now - 10,
            "nonce": nonce,
        });
        if let Some(issuer) = issuer {
            claims
                .as_object_mut()
                .unwrap()
                .insert("iss".to_owned(), issuer);
        }
        encode(
            &header,
            &claims,
            &EncodingKey::from_rsa_pem(TEST_PRIVATE_KEY.as_bytes()).unwrap(),
        )
        .unwrap()
    }

    fn test_jwks(extra: Value) -> JwkSet {
        let mut key = serde_json::Map::from_iter([
            ("kty".to_owned(), json!("RSA")),
            ("kid".to_owned(), json!(TEST_KEY_ID)),
            ("alg".to_owned(), json!("RS256")),
            ("n".to_owned(), json!(TEST_JWK_MODULUS)),
            ("e".to_owned(), json!("AQAB")),
        ]);
        for (name, value) in extra.as_object().unwrap() {
            key.insert(name.clone(), value.clone());
        }
        serde_json::from_value(json!({"keys": [key]})).unwrap()
    }
}
