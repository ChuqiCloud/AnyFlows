//! 企业 SSO 登录事务的协议材料与固定回调边界。
//!
//! 本模块只生成和校验短期登录材料；state、PKCE verifier、nonce 和断言正文
//! 不得写入日志或数据库明文字段，持久化层只能接收摘要或系统密钥保护的封套。

use std::{fmt, mem};

use base64::{
    Engine as _,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use url::form_urlencoded;
use zeroize::Zeroizing;

use crate::{DecryptedSystemSecret, PlainSystemSecret};

/// 企业 OIDC 固定回调路径。
pub const ORGANIZATION_SSO_OIDC_CALLBACK_PATH: &str = "/api/auth/organization-sso/oidc/callback";
/// 企业 SAML 固定回调路径。
pub const ORGANIZATION_SSO_SAML_CALLBACK_PATH: &str = "/api/auth/organization-sso/saml/callback";

const RANDOM_BYTES: usize = 32;
const MAX_CALLBACK_BYTES: usize = 16 * 1024;
const MAX_SAML_RESPONSE_BYTES: usize = 256 * 1024;

/// 企业 SSO 登录材料；Debug 只显示摘要和长度，不显示秘密原文。
#[derive(PartialEq, Eq)]
pub struct OrganizationSsoLoginStart {
    state: Zeroizing<String>,
    state_digest: String,
    pkce_verifier: Zeroizing<String>,
    pkce_challenge: String,
    nonce: Zeroizing<String>,
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
        let state_digest = organization_sso_login_state_digest(&state)?;
        let pkce_challenge = digest_base64(pkce_verifier.as_bytes());
        Ok(Self {
            state: Zeroizing::new(state),
            state_digest,
            pkce_verifier: Zeroizing::new(pkce_verifier),
            pkce_challenge,
            nonce: Zeroizing::new(nonce),
            request_digest,
        })
    }

    #[must_use]
    pub fn state(&self) -> &str {
        self.state.as_str()
    }

    #[must_use]
    pub fn state_digest(&self) -> &str {
        &self.state_digest
    }

    #[must_use]
    pub fn pkce_verifier(&self) -> &str {
        self.pkce_verifier.as_str()
    }

    #[must_use]
    pub fn pkce_challenge(&self) -> &str {
        &self.pkce_challenge
    }

    #[must_use]
    pub fn nonce(&self) -> &str {
        self.nonce.as_str()
    }

    #[must_use]
    pub fn request_digest(&self) -> &str {
        &self.request_digest
    }

    /// 消费启动材料并转交 OIDC 回调所需的短期秘密；调用方只能加密保存该对象。
    #[must_use]
    pub fn into_oidc_material(mut self) -> OrganizationSsoOidcLoginMaterial {
        OrganizationSsoOidcLoginMaterial {
            pkce_verifier: mem::take(&mut self.pkce_verifier),
            nonce: mem::take(&mut self.nonce),
        }
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

/// OIDC 回调恢复所需的短期秘密；仅可装入系统密钥封套或交给协议适配器。
#[derive(PartialEq, Eq)]
pub struct OrganizationSsoOidcLoginMaterial {
    pkce_verifier: Zeroizing<String>,
    nonce: Zeroizing<String>,
}

impl OrganizationSsoOidcLoginMaterial {
    /// 返回 PKCE verifier；调用方不得记录、复制或持久化该值。
    #[must_use]
    pub fn pkce_verifier(&self) -> &str {
        self.pkce_verifier.as_str()
    }

    /// 返回 ID Token nonce；调用方不得记录、复制或持久化该值。
    #[must_use]
    pub fn nonce(&self) -> &str {
        self.nonce.as_str()
    }

    /// 序列化为受系统密钥保护的短期明文，不产生独立的秘密字段副本。
    pub fn into_plain_system_secret(self) -> Result<PlainSystemSecret, OrganizationSsoLoginError> {
        let wire = OrganizationSsoOidcLoginMaterialWireRef {
            pkce_verifier: self.pkce_verifier.as_str(),
            nonce: self.nonce.as_str(),
        };
        let serialized = serde_json::to_string(&wire)
            .map_err(|_| OrganizationSsoLoginError::InvalidLoginMaterial)?;
        PlainSystemSecret::new(serialized)
            .map_err(|_| OrganizationSsoLoginError::InvalidLoginMaterial)
    }

    /// 从已解密的系统密钥恢复短期材料；未知字段和异常形状一律拒绝。
    pub fn from_decrypted_system_secret(
        secret: DecryptedSystemSecret,
    ) -> Result<Self, OrganizationSsoLoginError> {
        let serialized = secret.into_zeroizing();
        let wire: OrganizationSsoOidcLoginMaterialWire = serde_json::from_str(serialized.as_str())
            .map_err(|_| OrganizationSsoLoginError::InvalidLoginMaterial)?;
        if !valid_parameter(&wire.pkce_verifier) || !valid_parameter(&wire.nonce) {
            return Err(OrganizationSsoLoginError::InvalidLoginMaterial);
        }
        Ok(Self {
            pkce_verifier: Zeroizing::new(wire.pkce_verifier),
            nonce: Zeroizing::new(wire.nonce),
        })
    }
}

impl fmt::Debug for OrganizationSsoOidcLoginMaterial {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationSsoOidcLoginMaterial")
            .field("pkce_verifier", &"<redacted>")
            .field("nonce", &"<redacted>")
            .finish()
    }
}

/// SAML SP 发起请求需要在回调中复验的一次性 Request ID。
///
/// Request ID 只存在于短期加密材料中；数据库只保存其所属事务的摘要，
/// 任何 Debug 输出都不得暴露原始值。
#[derive(PartialEq, Eq)]
pub struct OrganizationSsoSamlLoginMaterial {
    request_id: Zeroizing<String>,
}

impl OrganizationSsoSamlLoginMaterial {
    /// 生成符合 SAML 消息标识形状的短期 Request ID。
    pub fn new() -> Result<Self, OrganizationSsoLoginError> {
        let mut request_id = String::from("_");
        request_id.push_str(&random_parameter()?);
        Ok(Self {
            request_id: Zeroizing::new(request_id),
        })
    }

    /// 返回 Request ID；调用方只能交给 SAML 请求构造器或对应加密封套。
    #[must_use]
    pub fn request_id(&self) -> &str {
        self.request_id.as_str()
    }

    /// 消费材料并移出 Request ID，避免在回调编排中复制敏感字符串。
    #[must_use]
    pub fn into_request_id(mut self) -> String {
        mem::take(&mut *self.request_id)
    }

    /// 序列化为受系统密钥保护的短期明文，不保留未清零的额外副本。
    pub fn into_plain_system_secret(self) -> Result<PlainSystemSecret, OrganizationSsoLoginError> {
        let wire = OrganizationSsoSamlLoginMaterialWireRef {
            request_id: self.request_id.as_str(),
        };
        let serialized = serde_json::to_string(&wire)
            .map_err(|_| OrganizationSsoLoginError::InvalidLoginMaterial)?;
        PlainSystemSecret::new(serialized)
            .map_err(|_| OrganizationSsoLoginError::InvalidLoginMaterial)
    }

    /// 从已解密系统密钥恢复 Request ID；未知字段和异常形状一律拒绝。
    pub fn from_decrypted_system_secret(
        secret: DecryptedSystemSecret,
    ) -> Result<Self, OrganizationSsoLoginError> {
        let serialized = secret.into_zeroizing();
        let wire: OrganizationSsoSamlLoginMaterialWire = serde_json::from_str(serialized.as_str())
            .map_err(|_| OrganizationSsoLoginError::InvalidLoginMaterial)?;
        if !wire.request_id.starts_with('_') || !valid_parameter(&wire.request_id) {
            return Err(OrganizationSsoLoginError::InvalidLoginMaterial);
        }
        Ok(Self {
            request_id: Zeroizing::new(wire.request_id),
        })
    }
}

impl fmt::Debug for OrganizationSsoSamlLoginMaterial {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationSsoSamlLoginMaterial")
            .field("request_id", &"<redacted>")
            .finish()
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OrganizationSsoOidcLoginMaterialWire {
    pkce_verifier: String,
    nonce: String,
}

#[derive(Serialize)]
struct OrganizationSsoOidcLoginMaterialWireRef<'a> {
    pkce_verifier: &'a str,
    nonce: &'a str,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OrganizationSsoSamlLoginMaterialWire {
    request_id: String,
}

#[derive(Serialize)]
struct OrganizationSsoSamlLoginMaterialWireRef<'a> {
    request_id: &'a str,
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
    /// 解析固定 SAML POST 表单，将受限 Base64 响应解码为仅供验签的 XML。
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
        let response = response
            .filter(|value| !value.is_empty() && value.len() <= MAX_SAML_RESPONSE_BYTES)
            .ok_or(OrganizationSsoLoginError::MissingCallbackField)?;
        let response = STANDARD
            .decode(response)
            .map_err(|_| OrganizationSsoLoginError::InvalidCallbackEncoding)?;
        if response.is_empty() || response.len() > MAX_SAML_RESPONSE_BYTES {
            return Err(OrganizationSsoLoginError::InvalidCallbackEncoding);
        }
        let response = String::from_utf8(response)
            .map_err(|_| OrganizationSsoLoginError::InvalidCallbackEncoding)?;
        let relay_state = relay_state.filter(|value| valid_parameter(value));
        match relay_state {
            Some(relay_state) => Ok(Self {
                response,
                relay_state,
            }),
            None => Err(OrganizationSsoLoginError::MissingCallbackField),
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

    /// 消费回调并移出已解码响应与 RelayState，避免在 HTTP 编排中复制断言正文。
    #[must_use]
    pub fn into_parts(self) -> (String, String) {
        (self.response, self.relay_state)
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
    #[error("企业 SSO state 无效")]
    InvalidState,
    #[error("企业 SSO 票据无效")]
    InvalidTicket,
    #[error("企业 SSO 随机材料生成失败")]
    Entropy,
    #[error("企业 SSO 登录材料无效")]
    InvalidLoginMaterial,
    #[error("企业 SSO 回调字段缺失")]
    MissingCallbackField,
    #[error("企业 SSO 回调字段不允许")]
    UnexpectedCallbackField,
    #[error("企业 SSO 回调编码无效")]
    InvalidCallbackEncoding,
    #[error("企业 SSO 回调过大")]
    CallbackTooLarge,
}

/// 计算可持久化的 state 摘要；调用方不得自行复制摘要算法。
pub fn organization_sso_login_state_digest(
    state: &str,
) -> Result<String, OrganizationSsoLoginError> {
    if !valid_parameter(state) {
        return Err(OrganizationSsoLoginError::InvalidState);
    }
    Ok(digest(state.as_bytes()))
}

/// 计算短期登录票据的域分离摘要，明文票据只可在本次响应中返回。
pub fn organization_sso_login_ticket_digest(
    ticket: &str,
) -> Result<String, OrganizationSsoLoginError> {
    if !valid_parameter(ticket) {
        return Err(OrganizationSsoLoginError::InvalidTicket);
    }
    Ok(digest_with_domain(
        b"organization-sso-login-ticket-v1",
        ticket.as_bytes(),
    ))
}

fn random_parameter() -> Result<String, OrganizationSsoLoginError> {
    let mut bytes = [0_u8; RANDOM_BYTES];
    getrandom::fill(&mut bytes).map_err(|_| OrganizationSsoLoginError::Entropy)?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

fn digest(value: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value);
    encode_digest(&hasher.finalize())
}

fn digest_with_domain(domain: &[u8], value: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update([0]);
    hasher.update(value);
    encode_digest(&hasher.finalize())
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
        assert_eq!(
            start.state_digest(),
            organization_sso_login_state_digest(start.state()).unwrap()
        );
        assert_eq!(
            start.pkce_challenge(),
            digest_base64(start.pkce_verifier().as_bytes())
        );
        assert!(!format!("{start:?}").contains(start.pkce_verifier()));
        assert!(!format!("{start:?}").contains(start.nonce()));
    }

    #[test]
    fn saml_request_material_is_generated_and_redacted() {
        let material = OrganizationSsoSamlLoginMaterial::new().unwrap();
        assert!(material.request_id().starts_with('_'));
        assert!(!format!("{material:?}").contains(material.request_id()));
        let secret = material.into_plain_system_secret().unwrap();
        assert!(!format!("{secret:?}").contains("request_id"));
    }

    #[test]
    fn state_and_ticket_digests_are_stable_and_domain_separated() {
        let state = "state-material";
        let ticket = "ticket-material";
        assert_eq!(
            organization_sso_login_state_digest(state).unwrap(),
            digest(state.as_bytes())
        );
        assert_ne!(
            organization_sso_login_ticket_digest(ticket).unwrap(),
            digest(ticket.as_bytes())
        );
        assert_eq!(
            organization_sso_login_state_digest(" state"),
            Err(OrganizationSsoLoginError::InvalidState)
        );
        assert_eq!(
            organization_sso_login_ticket_digest(""),
            Err(OrganizationSsoLoginError::InvalidTicket)
        );
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
        let response = "<samlp:Response>signed-assertion</samlp:Response>";
        let form = form_urlencoded::Serializer::new(String::new())
            .append_pair("SAMLResponse", &STANDARD.encode(response))
            .append_pair("RelayState", "state")
            .finish();
        let callback = OrganizationSsoSamlCallback::parse(&form).unwrap();
        assert_eq!(callback.response(), response);
        assert!(!format!("{callback:?}").contains("signed-assertion"));
        assert_eq!(
            OrganizationSsoSamlCallback::parse("SAMLResponse=YXNzZXJ0aW9u"),
            Err(OrganizationSsoLoginError::MissingCallbackField)
        );
        assert_eq!(
            OrganizationSsoSamlCallback::parse("SAMLResponse=not-base64&RelayState=state"),
            Err(OrganizationSsoLoginError::InvalidCallbackEncoding)
        );
    }
}
