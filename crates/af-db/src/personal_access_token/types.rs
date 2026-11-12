use af_domain::{GroupId, TrustedClientIp, UserId};
use thiserror::Error;

pub const MAX_PERSONAL_ACCESS_TOKENS_PER_USER: usize = 20;
pub const MAX_PERSONAL_ACCESS_TOKEN_PAGE_SIZE: usize = 100;
pub const MAX_PERSONAL_ACCESS_TOKEN_SCOPES: usize = 1024;
pub const MAX_PERSONAL_ACCESS_TOKEN_NAME_BYTES: usize = 128;
pub const MAX_PERSONAL_ACCESS_TOKEN_IPS: usize = 64;

#[derive(Clone, Eq, PartialEq)]
pub struct PersonalAccessTokenRecord {
    pub id: i64,
    pub user_id: UserId,
    pub token_hint: String,
    pub name: String,
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersonalAccessTokenPageRecord {
    pub tokens: Vec<PersonalAccessTokenRecord>,
    pub next_cursor: Option<i64>,
    pub capacity: usize,
}

#[derive(Clone, Eq, PartialEq)]
pub struct PersonalAccessTokenCreateRecord {
    pub user_id: UserId,
    pub secret_hash: String,
    pub token_hint: String,
    pub name: String,
    pub scopes: Vec<String>,
    pub expires_at: Option<i64>,
    pub allow_ips: Option<Vec<String>>,
}

#[derive(Clone, Eq, PartialEq)]
pub struct PersonalAccessTokenUpdateRecord {
    pub name: String,
    pub scopes: Vec<String>,
    pub expires_at: Option<i64>,
    pub allow_ips: Option<Vec<String>>,
    pub enabled: bool,
    pub expected_version: i64,
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum PersonalAccessTokenRejectionReason {
    #[error("invalid")]
    Invalid,
    #[error("expired")]
    Expired,
    #[error("disabled")]
    Disabled,
    #[error("ip_denied")]
    IpDenied,
    #[error("account_unavailable")]
    AccountUnavailable,
    #[error("policy_disabled")]
    PolicyDisabled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PersonalAccessTokenAuditIdentity {
    pub user_id: UserId,
    pub token_id: i64,
    pub reason: PersonalAccessTokenRejectionReason,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PersonalAccessTokenLookup {
    Authenticated {
        user_id: UserId,
        role: i16,
        group_id: GroupId,
        token: PersonalAccessTokenRecord,
    },
    Rejected(Option<PersonalAccessTokenAuditIdentity>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersonalAccessTokenPolicyRecord {
    pub enabled: bool,
    pub issuance_enabled: bool,
    pub max_tokens_per_user: usize,
    pub version: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersonalAccessTokenPolicyUpdateRecord {
    pub enabled: bool,
    pub issuance_enabled: bool,
    pub max_tokens_per_user: usize,
    pub expected_version: i64,
}

#[derive(Clone, Eq, PartialEq)]
pub struct PersonalAccessTokenProofRecord {
    pub proof_hash: String,
    pub user_id: UserId,
    pub session_version: i64,
    pub session_binding: String,
    pub purpose: String,
    pub target_id: Option<i64>,
    pub payload_digest: String,
    pub issued_at: i64,
    pub expires_at: i64,
}

#[derive(Clone, Eq, PartialEq)]
pub struct PersonalAccessTokenProofClaim {
    pub proof_hash: String,
    pub session_version: i64,
    pub session_binding: String,
    pub purpose: String,
    pub target_id: Option<i64>,
    pub payload_digest: String,
}

impl std::fmt::Debug for PersonalAccessTokenCreateRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PersonalAccessTokenCreateRecord(<redacted>)")
    }
}

impl std::fmt::Debug for PersonalAccessTokenRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PersonalAccessTokenRecord(<redacted>)")
    }
}

impl std::fmt::Debug for PersonalAccessTokenUpdateRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PersonalAccessTokenUpdateRecord(<redacted>)")
    }
}

impl std::fmt::Debug for PersonalAccessTokenProofRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PersonalAccessTokenProofRecord(<redacted>)")
    }
}

impl std::fmt::Debug for PersonalAccessTokenProofClaim {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PersonalAccessTokenProofClaim(<redacted>)")
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersonalAccessTokenAccessAuditRecord {
    pub user_id: Option<UserId>,
    pub token_id: Option<i64>,
    pub resource_id: Option<String>,
    pub request_id: String,
    pub method: String,
    pub route: String,
    pub required_scope: Option<String>,
    pub status_code: u16,
    pub allowed: bool,
    pub client_ip: TrustedClientIp,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersonalAccessTokenMutationContext {
    pub request_id: String,
    pub method: String,
    pub route: String,
    pub resource_id: Option<String>,
    pub client_ip: TrustedClientIp,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersonalAccessTokenAuditCleanupResult {
    pub deleted: u64,
    pub cutoff: i64,
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum PersonalAccessTokenRepositoryConfigError {
    #[error("令牌数据库超时必须大于零")]
    ZeroLookupTimeout,
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum PersonalAccessTokenRepositoryError {
    #[error("输入参数无效")]
    InvalidInput,
    #[error("令牌所有者不可用")]
    OwnerUnavailable,
    #[error("令牌签发已全局停用")]
    IssuanceDisabled,
    #[error("令牌功能已全局停用")]
    Disabled,
    #[error("令牌容量已达上限")]
    LimitReached,
    #[error("令牌不存在")]
    NotFound,
    #[error("令牌版本冲突")]
    Conflict,
    #[error("权限不足")]
    Forbidden,
    #[error("一次性验证证明无效、过期或已使用")]
    InvalidProof,
    #[error("访问令牌请求速率超限")]
    RateLimited { retry_after: u32 },
    #[error("数据库查询失败")]
    Query,
    #[error("数据库操作超时")]
    Timeout,
    #[error("持久化状态损坏")]
    Invariant,
}

impl PersonalAccessTokenRecord {
    #[must_use]
    pub const fn id(&self) -> i64 {
        self.id
    }
    #[must_use]
    pub const fn user_id(&self) -> UserId {
        self.user_id
    }
}
