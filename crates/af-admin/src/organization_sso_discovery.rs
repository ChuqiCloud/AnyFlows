use std::time::{SystemTime, UNIX_EPOCH};

use af_db::{
    OrganizationSsoDiscoveryLocator, OrganizationSsoProviderRepository,
    OrganizationSsoRepositoryError,
};
use af_domain::{OrganizationKey, OrganizationSsoDomain};
use thiserror::Error;

const MAX_DISCOVERY_IDENTIFIER_BYTES: usize = 320;
const MAX_EMAIL_LOCAL_BYTES: usize = 64;

/// 公开发现的低信息结果；只表示是否允许显示通用企业登录动作。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationSsoDiscovery {
    available: bool,
}

impl OrganizationSsoDiscovery {
    pub const fn new(available: bool) -> Self {
        Self { available }
    }

    /// 返回服务端判定的通用企业 SSO 动作资格。
    #[must_use]
    pub const fn available(self) -> bool {
        self.available
    }
}

/// 公开发现的稳定错误；未知企业不会映射为该错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationSsoDiscoveryServiceError {
    /// 标识不是受支持的企业公开标识或邮箱格式。
    #[error("企业 SSO 发现请求无效")]
    InvalidInput,
    /// 发现依赖不可用或持久化状态无法安全解释。
    #[error("企业 SSO 发现服务暂不可用")]
    Internal,
}

/// 解析并执行公开企业 SSO 发现；不把企业资料带出仓储边界。
pub(crate) async fn discover(
    repository: &OrganizationSsoProviderRepository,
    identifier: String,
) -> Result<OrganizationSsoDiscovery, OrganizationSsoDiscoveryServiceError> {
    let locator = parse_locator(&identifier)?;
    let now = current_timestamp()?;
    match repository.discovery_available(locator, now).await {
        Ok(available) => Ok(OrganizationSsoDiscovery::new(available)),
        Err(OrganizationSsoRepositoryError::InvalidInput) => {
            Err(OrganizationSsoDiscoveryServiceError::InvalidInput)
        }
        Err(
            OrganizationSsoRepositoryError::NotFound
            | OrganizationSsoRepositoryError::Conflict
            | OrganizationSsoRepositoryError::Expired
            | OrganizationSsoRepositoryError::Unavailable,
        ) => Ok(OrganizationSsoDiscovery::new(false)),
        Err(
            OrganizationSsoRepositoryError::AlreadyProcessed
            | OrganizationSsoRepositoryError::InvalidTransition
            | OrganizationSsoRepositoryError::Query
            | OrganizationSsoRepositoryError::Timeout
            | OrganizationSsoRepositoryError::Invariant,
        ) => Err(OrganizationSsoDiscoveryServiceError::Internal),
    }
}

fn parse_locator(
    identifier: &str,
) -> Result<OrganizationSsoDiscoveryLocator, OrganizationSsoDiscoveryServiceError> {
    if identifier.is_empty()
        || identifier.len() > MAX_DISCOVERY_IDENTIFIER_BYTES
        || identifier.trim() != identifier
        || !identifier.is_ascii()
        || identifier
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
    {
        return Err(OrganizationSsoDiscoveryServiceError::InvalidInput);
    }
    if identifier.len() == 32
        && let Ok(key) = OrganizationKey::from_persistence_key(identifier)
    {
        return Ok(OrganizationSsoDiscoveryLocator::OrganizationKey(key));
    }

    let mut parts = identifier.split('@');
    let local = parts.next().unwrap_or_default();
    let domain = parts.next().unwrap_or_default();
    if parts.next().is_some()
        || local.is_empty()
        || local.len() > MAX_EMAIL_LOCAL_BYTES
        || local.starts_with('.')
        || local.ends_with('.')
        || local.contains("..")
        || !local.bytes().all(is_valid_email_local_byte)
        || domain.is_empty()
    {
        return Err(OrganizationSsoDiscoveryServiceError::InvalidInput);
    }
    let domain = OrganizationSsoDomain::new(domain.to_ascii_lowercase())
        .map_err(|_| OrganizationSsoDiscoveryServiceError::InvalidInput)?;
    Ok(OrganizationSsoDiscoveryLocator::Domain(domain))
}

const fn is_valid_email_local_byte(byte: u8) -> bool {
    matches!(
        byte,
        b'a'..=b'z'
            | b'A'..=b'Z'
            | b'0'..=b'9'
            | b'!'
            | b'#'
            | b'$'
            | b'%'
            | b'&'
            | b'\''
            | b'*'
            | b'+'
            | b'-'
            | b'/'
            | b'='
            | b'?'
            | b'^'
            | b'_'
            | b'`'
            | b'{'
            | b'|'
            | b'}'
            | b'~'
            | b'.'
    )
}

fn current_timestamp() -> Result<i64, OrganizationSsoDiscoveryServiceError> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| OrganizationSsoDiscoveryServiceError::Internal)?
        .as_secs();
    i64::try_from(seconds).map_err(|_| OrganizationSsoDiscoveryServiceError::Internal)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 公开标识和邮箱域名使用受限解析() {
        let key = parse_locator("0123456789abcdef0123456789abcdef").expect("企业 key 有效");
        assert!(matches!(
            key,
            OrganizationSsoDiscoveryLocator::OrganizationKey(_)
        ));

        let email = parse_locator("Member@Example.COM").expect("邮箱有效");
        assert!(matches!(email, OrganizationSsoDiscoveryLocator::Domain(_)));
        assert!(parse_locator("example.com").is_err());
        assert!(parse_locator("member@example.com/path").is_err());
        assert!(parse_locator("member@localhost").is_err());
        assert!(parse_locator(" member@example.com").is_err());
    }

    #[test]
    fn debug_projection_never包含输入标识() {
        let value = OrganizationSsoDiscovery::new(true);
        assert_eq!(
            format!("{value:?}"),
            "OrganizationSsoDiscovery { available: true }"
        );
    }
}
