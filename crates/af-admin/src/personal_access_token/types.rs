use std::{fmt, future::Future, pin::Pin};

use af_domain::{GroupId, UserId};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::PersonalAccessTokenSecret;
use crate::SessionPrincipal;

pub use af_db::PersonalAccessTokenAccessAuditRecord as PersonalAccessTokenAccessRecord;
pub use af_db::{
    PersonalAccessTokenAuditIdentity, PersonalAccessTokenMutationContext,
    PersonalAccessTokenProofClaim, PersonalAccessTokenRejectionReason,
};

pub type PersonalAccessTokenFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, PersonalAccessTokenError>> + Send + 'a>>;

#[derive(Clone, Eq, PartialEq, Serialize)]
pub struct PersonalAccessToken {
    pub id: i64,
    pub name: String,
    pub token_hint: String,
    pub scopes: Vec<String>,
    pub enabled: bool,
    pub expires_at: Option<i64>,
    pub allow_ips: Option<Vec<String>>,
    pub created_at: i64,
    pub updated_at: i64,
    pub last_used_at: Option<i64>,
    pub last_used_ip: Option<String>,
    pub version: i64,
}

impl fmt::Debug for PersonalAccessToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PersonalAccessToken(<redacted>)")
    }
}

impl PersonalAccessToken {
    pub(super) fn from_record(record: af_db::PersonalAccessTokenRecord) -> Self {
        Self {
            id: record.id,
            name: record.name,
            token_hint: record.token_hint,
            scopes: record.scopes,
            enabled: record.enabled,
            expires_at: record.expires_at,
            allow_ips: record.allow_ips,
            created_at: record.created_at,
            updated_at: record.updated_at,
            last_used_at: record.last_used_at,
            last_used_ip: record.last_used_ip,
            version: record.version,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PersonalAccessTokenPage {
    pub tokens: Vec<PersonalAccessToken>,
    pub next_cursor: Option<i64>,
    pub capacity: usize,
}

#[derive(Serialize)]
pub struct IssuedPersonalAccessToken {
    pub token: PersonalAccessToken,
    pub secret: PersonalAccessTokenSecret,
}

impl fmt::Debug for IssuedPersonalAccessToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("IssuedPersonalAccessToken(<redacted>)")
    }
}

pub struct CreatePersonalAccessTokenCommand {
    pub name: String,
    pub scopes: Vec<String>,
    pub expires_at: Option<i64>,
    pub allow_ips: Option<Vec<String>>,
    pub proof: PersonalAccessTokenProofClaim,
}

pub struct UpdatePersonalAccessTokenCommand {
    pub name: String,
    pub scopes: Vec<String>,
    pub expires_at: Option<i64>,
    pub allow_ips: Option<Vec<String>>,
    pub enabled: bool,
    pub version: i64,
    pub proof: Option<PersonalAccessTokenProofClaim>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PersonalAccessTokenPolicy {
    pub enabled: bool,
    pub issuance_enabled: bool,
    pub max_tokens_per_user: usize,
    pub version: i64,
}

impl PersonalAccessTokenPolicy {
    pub(super) fn from_record(record: af_db::PersonalAccessTokenPolicyRecord) -> Self {
        Self {
            enabled: record.enabled,
            issuance_enabled: record.issuance_enabled,
            max_tokens_per_user: record.max_tokens_per_user,
            version: record.version,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersonalAccessTokenAuthentication {
    pub principal: SessionPrincipal,
    pub group_id: GroupId,
    pub token_id: i64,
    pub scopes: Vec<String>,
    pub expires_at: Option<i64>,
}

impl PersonalAccessTokenAuthentication {
    #[must_use]
    pub const fn principal(&self) -> SessionPrincipal {
        self.principal
    }
    #[must_use]
    pub const fn group_id(&self) -> GroupId {
        self.group_id
    }
    #[must_use]
    pub const fn token_id(&self) -> i64 {
        self.token_id
    }
    #[must_use]
    pub fn scopes(&self) -> &[String] {
        &self.scopes
    }
    #[must_use]
    pub const fn expires_at(&self) -> Option<i64> {
        self.expires_at
    }
    #[must_use]
    pub const fn user_id(&self) -> UserId {
        self.principal.user_id()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PersonalAccessTokenLookup {
    Authenticated(PersonalAccessTokenAuthentication),
    Rejected {
        audit: Option<PersonalAccessTokenAuditIdentity>,
    },
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum PersonalAccessTokenError {
    #[error("访问令牌参数无效")]
    InvalidInput,
    #[error("访问令牌无效")]
    InvalidToken,
    #[error("访问令牌权限不足")]
    Forbidden,
    #[error("访问令牌不存在")]
    NotFound,
    #[error("访问令牌版本冲突")]
    Conflict,
    #[error("访问令牌已达数量上限")]
    LimitReached,
    #[error("访问令牌功能已关闭")]
    Disabled,
    #[error("访问令牌签发已关闭")]
    IssuanceDisabled,
    #[error("验证证明无效或已使用")]
    InvalidProof,
    #[error("访问令牌请求速率超限")]
    RateLimited { retry_after: u32 },
    #[error("访问令牌服务不可用")]
    Internal,
}
