use std::fmt;

use crate::{OrganizationId, OrganizationServiceAccountId};

const RUNTIME_KEY_BYTES: usize = 16;

/// 公共计费层接收的服务账号定位事实。
///
/// 发行版负责校验和构造该值；公共核心只保存经过校验的标识，不包含密钥明文、摘要或
/// 企业服务账号管理逻辑。
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct OrganizationServiceAccountLocator {
    organization_id: OrganizationId,
    service_account_id: OrganizationServiceAccountId,
}

impl OrganizationServiceAccountLocator {
    #[must_use]
    pub const fn new(
        organization_id: OrganizationId,
        service_account_id: OrganizationServiceAccountId,
    ) -> Self {
        Self {
            organization_id,
            service_account_id,
        }
    }

    #[must_use]
    pub const fn organization_id(self) -> OrganizationId {
        self.organization_id
    }

    #[must_use]
    pub const fn service_account_id(self) -> OrganizationServiceAccountId {
        self.service_account_id
    }
}

impl fmt::Debug for OrganizationServiceAccountLocator {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationServiceAccountLocator(<redacted>)")
    }
}

/// 服务账号或当前凭据的公开运行时标识。
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct OrganizationServiceAccountKey([u8; RUNTIME_KEY_BYTES]);

/// 服务账号运行时标识不能由全零字节构成。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct OrganizationServiceAccountKeyError;

impl OrganizationServiceAccountKey {
    pub const fn new(
        bytes: [u8; RUNTIME_KEY_BYTES],
    ) -> Result<Self, OrganizationServiceAccountKeyError> {
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] != 0 {
                return Ok(Self(bytes));
            }
            index += 1;
        }
        Err(OrganizationServiceAccountKeyError)
    }
}

impl fmt::Debug for OrganizationServiceAccountKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationServiceAccountKey(<redacted>)")
    }
}

/// 已由发行版鉴权边界固定的服务账号运行时身份。
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct OrganizationServiceAccountRuntimeIdentity {
    locator: OrganizationServiceAccountLocator,
    account_key: OrganizationServiceAccountKey,
    credential_key: OrganizationServiceAccountKey,
}

impl OrganizationServiceAccountRuntimeIdentity {
    #[must_use]
    pub const fn new(
        locator: OrganizationServiceAccountLocator,
        account_key: OrganizationServiceAccountKey,
        credential_key: OrganizationServiceAccountKey,
    ) -> Self {
        Self {
            locator,
            account_key,
            credential_key,
        }
    }

    #[must_use]
    pub const fn locator(self) -> OrganizationServiceAccountLocator {
        self.locator
    }

    #[must_use]
    pub const fn account_key(self) -> OrganizationServiceAccountKey {
        self.account_key
    }

    #[must_use]
    pub const fn credential_key(self) -> OrganizationServiceAccountKey {
        self.credential_key
    }
}

impl fmt::Debug for OrganizationServiceAccountRuntimeIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationServiceAccountRuntimeIdentity(<redacted>)")
    }
}
