//! 企业 SSO 登录事务的协议材料与固定回调边界。
//!
//! 本模块只生成和校验短期内存材料；state、PKCE verifier、nonce 和断言正文
//! 不应写入日志或数据库，持久化层只接收调用方计算好的摘要。

use std::fmt;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sha2::{Digest, Sha256};
use thiserror::Error;
use url::form_urlencoded;

/// 企业 OIDC 固定回调路径。
pub const ORGANIZATION_SSO_OIDC_CALLBACK_PATH: &str = "/api/auth/organization-sso/oidc/callback";
/// 企业 SAML 固定回调路径。
pub const ORGANIZATION_SSO_SAML_CALLBACK_PATH: &str = "/api/auth/organization-sso/saml/callback";

const RANDOM_BYTES: usize = 32;
const MAX_CALLBACK_BYTES: usize = 16 * 1024;
const MAX_SAML_RESPONSE_BYTES: usize = 256 * 1024;

/// 企业 SSO 登录材料；Debug 只显示摘要和长度，不显示秘密原文。
#[derive(Clone, PartialEq, Eq)]
pub struct OrganizationSsoLoginStart {
    state: String,
    state_digest: String,
    pkce_verifier: String,
    pkce_challenge: String,
    nonce: String,
    request_digest: String,
}

impl OrganizationSsoLoginStart {
    /// 生成一次登录使用的 state、PKCE S256 和 nonce，并绑定请求摘要。
    pub fn new(request_digest: impl Into<String>) -> Result<Self, OrganizationSsoLoginError> {
        let request_digest = request_digest.into();
        validate_digest(&request_digest)?;
        let state = random_parameter()?;
        let pkce_verifier = random_parameter()?;
        let nonce = random_parameter()?;
        let state_digest = digest(state.as_bytes());
        let pkce_challenge = digest_base64(pkce_verifier.as_bytes());
        Ok(Self {
            state,
            state_digest,
            pkce_verifier,
            pkce_challenge,
            nonce,
            request_digest,
        })
    }

    #[must_use]
    pub fn state(&self) -> &str {
        &self.state
    }

    #[must_use]
    pub fn state_digest(&self) -> &str {
        &self.state_digest
    }

    #[must_use]
    pub fn pkce_verifier(&self) -> &str {
        &self.pkce_verifier
    }

    #[must_use]
    pub fn pkce_challenge(&self) -> &str {
        &self.pkce_challenge
    }

    #[must_use]
    pub fn nonce(&self) -> &str {
        &self.nonce
    }

    #[must_use]
    pub fn request_digest(&self) -> &str {
        &self.request_digest
    }
}

impl fmt::Debug for OrganizationSsoLoginStart {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationSsoLoginStart")
            .field("state_digest", &self.state_digest)
            .field("pkce_verifier", &"<redacted>")
            .field("pkce_challenge", &self.pkce_challenge)
            .field("nonce", &"<redacted>")
            .field("request_digest", &self.request_digest)
            .finish()
    }
}

/// OIDC 回调中允许的最小材料；授权码只在内存中交给交换客户端。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrganizationSsoOidcCallback {
    code: String,
    state: String,
}

impl OrganizationSsoOidcCallback {
    /// 解析固定 OIDC 回调查询，只接受 `code` 与 `state` 两个字段。
    pub fn parse(query: &str) -> Result<Self, OrganizationSsoLoginError> {
        if query.len() > MAX_CALLBACK_BYTES {
            return Err(OrganizationSsoLoginError::CallbackTooLarge);
        }
        let mut code = None;
        let mut state = None;
        for (key, value) in form_urlencoded::parse(query.as_bytes()) {
            match key.as_ref() {
                "code" if code.is_none() => code = Some(value.into_owned()),
                "state" if state.is_none() => state = Some(value.into_owned()),
                _ => return Err(OrganizationSsoLoginError::UnexpectedCallbackField),
            }
        }
        let code = code.filter(|value| valid_parameter(value));
        let state = state.filter(|value| valid_parameter(value));
        match (code, state) {
            (Some(code), Some(state)) => Ok(Self { code, state }),
            _ => Err(OrganizationSsoLoginError::MissingCallbackField),
        }
    }

    #[must_use]
    pub fn code(&self) -> &str {
        &self.code
    }

    #[must_use]
    pub fn state(&self) -> &str {
        &self.state
    }
}

/// SAML SP 发起回调中允许的材料；响应正文只在内存中交给签名验证器。
#[derive(Clone, PartialEq, Eq)]
pub struct OrganizationSsoSamlCallback {
    response: String,
    relay_state: String,
}

impl OrganizationSsoSamlCallback {
    /// 解析固定 SAML 表单，只接受一次 `SAMLResponse` 和 `RelayState`。
    pub fn parse(form: &str) -> Result<Self, OrganizationSsoLoginError> {
        if form.len() > MAX_SAML_RESPONSE_BYTES {
            return Err(OrganizationSsoLoginError::CallbackTooLarge);
        }
        let mut response = None;
        let mut relay_state = None;
        for (key, value) in form_urlencoded::parse(form.as_bytes()) {
            match key.as_ref() {
                "SAMLResponse" if response.is_none() => response = Some(value.into_owned()),
                "RelayState" if relay_state.is_none() => relay_state = Some(value.into_owned()),
                _ => return Err(OrganizationSsoLoginError::UnexpectedCallbackField),
            }
        }
        let response =
            response.filter(|value| !value.is_empty() && value.len() <= MAX_SAML_RESPONSE_BYTES);
        let relay_state = relay_state.filter(|value| valid_parameter(value));
        match (response, relay_state) {
            (Some(response), Some(relay_state)) => Ok(Self {
                response,
                relay_state,
            }),
            _ => Err(OrganizationSsoLoginError::MissingCallbackField),
        }
    }

    #[must_use]
    pub fn response(&self) -> &str {
        &self.response
    }

    #[must_use]
    pub fn relay_state(&self) -> &str {
        &self.relay_state
    }
}

impl fmt::Debug for OrganizationSsoSamlCallback {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationSsoSamlCallback")
            .field("response", &"<redacted>")
            .field("relay_state", &self.relay_state)
            .finish()
    }
}

/// 企业 SSO 登录材料错误；不包含回调原文或敏感值。
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum OrganizationSsoLoginError {
    #[error("企业 SSO 摘要无效")]
    InvalidDigest,
    #[error("企业 SSO 随机材料生成失败")]
    Entropy,
    #[error("企业 SSO 回调字段缺失")]
    MissingCallbackField,
    #[error("企业 SSO 回调字段不允许")]
    UnexpectedCallbackField,
    #[error("企业 SSO 回调过大")]
    CallbackTooLarge,
}

fn random_parameter() -> Result<String, OrganizationSsoLoginError> {
    let mut bytes = [0_u8; RANDOM_BYTES];
    getrandom::fill(&mut bytes).map_err(|_| OrganizationSsoLoginError::Entropy)?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

fn digest(value: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value);
    let digest = hasher.finalize();
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

fn digest_base64(value: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value);
    URL_SAFE_NO_PAD.encode(hasher.finalize())
}

fn validate_digest(value: &str) -> Result<(), OrganizationSsoLoginError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(OrganizationSsoLoginError::InvalidDigest);
    }
    Ok(())
}

fn valid_parameter(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 512
        && value.trim() == value
        && value.bytes().all(|byte| byte.is_ascii_graphic())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_material_is_bound_and_redacted() {
        let start = OrganizationSsoLoginStart::new("a".repeat(64)).unwrap();
        assert_eq!(start.state_digest(), digest(start.state().as_bytes()));
        assert_eq!(
            start.pkce_challenge(),
            digest_base64(start.pkce_verifier().as_bytes())
        );
        assert!(!format!("{start:?}").contains(start.pkce_verifier()));
        assert!(!format!("{start:?}").contains(start.nonce()));
    }

    #[test]
    fn oidc_callback_rejects_duplicates_and_unknown_fields() {
        assert!(OrganizationSsoOidcCallback::parse("code=one&state=two").is_ok());
        assert_eq!(
            OrganizationSsoOidcCallback::parse("code=one&code=two&state=three"),
            Err(OrganizationSsoLoginError::UnexpectedCallbackField)
        );
        assert_eq!(
            OrganizationSsoOidcCallback::parse("code=one&state=two&redirect_uri=x"),
            Err(OrganizationSsoLoginError::UnexpectedCallbackField)
        );
    }

    #[test]
    fn saml_callback_requires_relay_state_and_redacts_response() {
        let callback =
            OrganizationSsoSamlCallback::parse("SAMLResponse=assertion&RelayState=state").unwrap();
        assert_eq!(callback.response(), "assertion");
        assert!(!format!("{callback:?}").contains("assertion"));
        assert_eq!(
            OrganizationSsoSamlCallback::parse("SAMLResponse=assertion"),
            Err(OrganizationSsoLoginError::MissingCallbackField)
        );
    }
}
