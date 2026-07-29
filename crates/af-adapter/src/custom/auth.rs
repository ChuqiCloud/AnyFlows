use std::fmt;

use af_domain::CredentialKind;
use af_httpclient::{HeaderMap, HeaderName, HeaderValue};
use zeroize::Zeroizing;

use crate::credential::clear_authentication_headers;
use crate::request::is_forbidden_request_header;
use crate::{AdaptorError, AdaptorResult, Credential, MAX_UPSTREAM_REQUEST_HEADER_VALUE_BYTES};

/// Custom Header 凭据模板的最大字节数。
pub const MAX_CUSTOM_AUTH_TEMPLATE_BYTES: usize = 512;
/// Custom Header 凭据模板唯一支持的动态占位符。
pub const CUSTOM_CREDENTIAL_PLACEHOLDER: &str = "{credential}";

/// 已验证的 Custom Header 认证配置。
///
/// Header 名不能覆盖目标、分帧、代理、Cookie 或 JSON 协议基线；模板只允许一个
/// `{credential}`，不会执行环境变量、脚本或其他字符串插值。
#[derive(Clone, Eq, PartialEq)]
pub struct CustomHeaderAuthentication {
    name: HeaderName,
    prefix: String,
    suffix: String,
}

impl CustomHeaderAuthentication {
    /// 创建自定义 Header 凭据模板。
    pub fn new(name: impl AsRef<str>, template: impl Into<String>) -> AdaptorResult<Self> {
        let raw_name = name.as_ref();
        if raw_name.is_empty() || raw_name.trim() != raw_name || !raw_name.is_ascii() {
            return Err(AdaptorError::InvalidCustomAuthentication);
        }
        let name = HeaderName::from_bytes(raw_name.as_bytes())
            .map_err(|_| AdaptorError::InvalidCustomAuthentication)?;
        if is_forbidden_request_header(&name)
            || matches!(name.as_str(), "content-type" | "accept" | "x-request-id")
        {
            return Err(AdaptorError::InvalidCustomAuthentication);
        }

        let template = template.into();
        if template.is_empty()
            || template.len() > MAX_CUSTOM_AUTH_TEMPLATE_BYTES
            || template.trim() != template
            || !template.is_ascii()
            || template.bytes().any(|byte| byte.is_ascii_control())
            || template
                .match_indices(CUSTOM_CREDENTIAL_PLACEHOLDER)
                .count()
                != 1
        {
            return Err(AdaptorError::InvalidCustomAuthentication);
        }
        let Some((prefix, suffix)) = template.split_once(CUSTOM_CREDENTIAL_PLACEHOLDER) else {
            return Err(AdaptorError::InvalidCustomAuthentication);
        };
        if prefix.contains(['{', '}']) || suffix.contains(['{', '}']) {
            return Err(AdaptorError::InvalidCustomAuthentication);
        }
        Ok(Self {
            name,
            prefix: prefix.to_owned(),
            suffix: suffix.to_owned(),
        })
    }

    fn render(&self, credential: &str) -> AdaptorResult<HeaderValue> {
        let capacity = self
            .prefix
            .len()
            .checked_add(credential.len())
            .and_then(|size| size.checked_add(self.suffix.len()))
            .ok_or(AdaptorError::InvalidHeader)?;
        if capacity > MAX_UPSTREAM_REQUEST_HEADER_VALUE_BYTES {
            return Err(AdaptorError::InvalidHeader);
        }
        let mut rendered = Zeroizing::new(String::with_capacity(capacity));
        rendered.push_str(&self.prefix);
        rendered.push_str(credential);
        rendered.push_str(&self.suffix);
        let mut value =
            HeaderValue::from_str(rendered.as_str()).map_err(|_| AdaptorError::InvalidHeader)?;
        value.set_sensitive(true);
        Ok(value)
    }
}

impl fmt::Debug for CustomHeaderAuthentication {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CustomHeaderAuthentication")
            .field("name", &"<已脱敏>")
            .field("template", &"<已脱敏>")
            .finish()
    }
}

/// Custom 渠道允许的闭合认证方案。
#[derive(Clone, Eq, PartialEq)]
pub enum CustomAuthentication {
    /// 不发送凭据，并在最终化阶段清理所有已知认证载体。
    None,
    /// 使用标准 `Authorization: Bearer`。
    Bearer,
    /// 使用受限 Header 凭据模板。
    Header(CustomHeaderAuthentication),
}

impl CustomAuthentication {
    /// 在 Header 覆盖完成前后均可调用；每次都会先移除旧认证载体。
    pub(crate) fn apply(
        &self,
        headers: &mut HeaderMap,
        credential: &Credential,
    ) -> AdaptorResult<()> {
        clear_authentication_headers(headers);
        if let Self::Header(authentication) = self {
            headers.remove(&authentication.name);
        }

        let (name, value) = match self {
            Self::None => return Ok(()),
            Self::Bearer => {
                let secret = simple_credential(credential)?;
                let capacity = 7_usize
                    .checked_add(secret.len())
                    .ok_or(AdaptorError::InvalidHeader)?;
                if capacity > MAX_UPSTREAM_REQUEST_HEADER_VALUE_BYTES {
                    return Err(AdaptorError::InvalidHeader);
                }
                let mut rendered = Zeroizing::new(String::with_capacity(capacity));
                rendered.push_str("Bearer ");
                rendered.push_str(secret);
                let mut value = HeaderValue::from_str(rendered.as_str())
                    .map_err(|_| AdaptorError::InvalidHeader)?;
                value.set_sensitive(true);
                (HeaderName::from_static("authorization"), value)
            }
            Self::Header(authentication) => (
                authentication.name.clone(),
                authentication.render(simple_credential(credential)?)?,
            ),
        };
        headers.insert(name, value);
        Ok(())
    }
}

impl fmt::Debug for CustomAuthentication {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::None => formatter.write_str("CustomAuthentication::None"),
            Self::Bearer => formatter.write_str("CustomAuthentication::Bearer"),
            Self::Header(authentication) => formatter
                .debug_tuple("CustomAuthentication::Header")
                .field(authentication)
                .finish(),
        }
    }
}

fn simple_credential(credential: &Credential) -> AdaptorResult<&str> {
    if !matches!(
        credential.kind(),
        CredentialKind::ApiKey | CredentialKind::Oauth
    ) {
        return Err(AdaptorError::UnsupportedCredential {
            kind: credential.kind(),
        });
    }
    Ok(credential.expose_secret())
}
