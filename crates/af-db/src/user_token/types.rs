use std::fmt;

use af_domain::{TokenId, UserId};
use thiserror::Error;

use crate::AdminTokenRecord;

/// 每个用户允许保留的未软删除 API Key 数量。
pub const MAX_USER_TOKENS_PER_USER: usize = 32;
/// 用户 Key 列表单页允许返回的最大记录数。
pub const MAX_USER_TOKEN_PAGE_SIZE: usize = 100;

/// 普通用户可读取的非敏感 API Key 快照。
pub struct UserTokenRecord {
    inner: AdminTokenRecord,
}

impl UserTokenRecord {
    pub(super) fn from_admin_record(
        record: AdminTokenRecord,
        expected_owner: UserId,
    ) -> Result<Self, UserTokenRepositoryError> {
        if record.user_id() != expected_owner {
            return Err(super::record_internal_error(
                UserTokenRepositoryError::Invariant,
            ));
        }
        Ok(Self { inner: record })
    }

    /// 返回 Key 主键。
    #[must_use]
    pub const fn token_id(&self) -> TokenId {
        self.inner.token_id()
    }

    /// 返回不可用于鉴权的展示前缀。
    #[must_use]
    pub fn key_prefix(&self) -> &str {
        self.inner.key_prefix()
    }

    /// 返回 Key 名称。
    #[must_use]
    pub fn name(&self) -> &str {
        self.inner.name()
    }

    /// 返回数据库状态码。
    #[must_use]
    pub const fn status(&self) -> i16 {
        self.inner.status()
    }

    /// 返回有限 Key 的剩余额度。
    #[must_use]
    pub const fn remain_quota(&self) -> i64 {
        self.inner.remain_quota()
    }

    /// 返回是否跳过 Key 自身额度上限。
    #[must_use]
    pub const fn unlimited_quota(&self) -> bool {
        self.inner.unlimited_quota()
    }

    /// 返回累计已用额度。
    #[must_use]
    pub const fn used_quota(&self) -> i64 {
        self.inner.used_quota()
    }

    /// 返回可选过期时间的 Unix 秒数。
    #[must_use]
    pub const fn expired_at(&self) -> Option<i64> {
        self.inner.expired_at()
    }

    /// 返回可选模型白名单。
    #[must_use]
    pub fn model_limits(&self) -> Option<&[String]> {
        self.inner.model_limits()
    }

    /// 返回可选 IP/CIDR 白名单。
    #[must_use]
    pub fn allow_ips(&self) -> Option<&[String]> {
        self.inner.allow_ips()
    }

    /// 返回创建时间的 Unix 秒数。
    #[must_use]
    pub const fn created_at(&self) -> i64 {
        self.inner.created_at()
    }

    /// 返回最近更新时间的 Unix 秒数。
    #[must_use]
    pub const fn updated_at(&self) -> i64 {
        self.inner.updated_at()
    }
}

impl fmt::Debug for UserTokenRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("UserTokenRecord(<redacted>)")
    }
}

/// 一页所有者范围 Key 结果。
pub struct UserTokenPageRecord {
    pub(super) tokens: Vec<UserTokenRecord>,
    pub(super) next_cursor: Option<TokenId>,
}

impl UserTokenPageRecord {
    /// 消费页面并返回记录和下一游标。
    #[must_use]
    pub fn into_parts(self) -> (Vec<UserTokenRecord>, Option<TokenId>) {
        (self.tokens, self.next_cursor)
    }
}

impl fmt::Debug for UserTokenPageRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("UserTokenPageRecord(<redacted>)")
    }
}

/// 所有者范围 Key 详情查询结果。
pub enum UserTokenLookupOutcome {
    /// 找到当前用户拥有的未软删除 Key。
    Found(Box<UserTokenRecord>),
    /// Key 不存在、已经软删除或属于其他用户。
    NotFound,
}

/// 用户签发 Key 时写入的密钥材料与公开配置。
pub struct UserTokenCreateRecord {
    pub(super) owner_user_id: UserId,
    pub(super) key_hash: String,
    pub(super) key_prefix: String,
    pub(super) fields: UserTokenWriteRecord,
}

impl UserTokenCreateRecord {
    /// 组装已由应用层校验的签发记录。
    #[must_use]
    pub fn new(
        owner_user_id: UserId,
        key_hash: String,
        key_prefix: String,
        fields: UserTokenWriteRecord,
    ) -> Self {
        Self {
            owner_user_id,
            key_hash,
            key_prefix,
            fields,
        }
    }
}

impl fmt::Debug for UserTokenCreateRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("UserTokenCreateRecord(<redacted>)")
    }
}

/// 普通用户创建或更新 Key 时允许写入的字段。
pub struct UserTokenWriteRecord {
    pub(super) name: String,
    pub(super) status: i16,
    pub(super) remain_quota: i64,
    pub(super) unlimited_quota: bool,
    pub(super) expired_at: Option<i64>,
    pub(super) model_limits: Option<Vec<String>>,
    pub(super) allow_ips: Option<Vec<String>>,
}

impl UserTokenWriteRecord {
    /// 组装不包含所有者、分组和多窗口字段的用户写入记录。
    #[allow(clippy::too_many_arguments, reason = "字段与用户 Key 写入契约一一对应")]
    #[must_use]
    pub fn new(
        name: String,
        status: i16,
        remain_quota: i64,
        unlimited_quota: bool,
        expired_at: Option<i64>,
        model_limits: Option<Vec<String>>,
        allow_ips: Option<Vec<String>>,
    ) -> Self {
        Self {
            name,
            status,
            remain_quota,
            unlimited_quota,
            expired_at,
            model_limits,
            allow_ips,
        }
    }
}

impl fmt::Debug for UserTokenWriteRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("UserTokenWriteRecord(<redacted>)")
    }
}

/// 所有者范围 Key 更新结果。
pub enum UserTokenMutationOutcome {
    /// 用户可控字段已经更新。
    Mutated(Box<UserTokenRecord>),
    /// Key 不存在、已经软删除或属于其他用户。
    NotFound,
}

/// 所有者范围 Key 软删除结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserTokenDeleteOutcome {
    /// Key 已写入软删除墓碑。
    Deleted,
    /// Key 不存在、已经软删除或属于其他用户。
    NotFound,
}

/// 用户 Key 仓储构造错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum UserTokenRepositoryConfigError {
    /// 零超时无法形成有效截止时间。
    #[error("用户 API Key 查询超时必须大于零")]
    ZeroLookupTimeout,
}

/// 用户 Key 仓储错误；不携带 Key、摘要、白名单或数据库诊断。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum UserTokenRepositoryError {
    /// 输入字段不满足持久化边界。
    #[error("用户 API Key 参数无效")]
    InvalidInput,
    /// 当前登录用户在写入前已经失效。
    #[error("用户 API Key 所有者不可用")]
    OwnerUnavailable,
    /// 当前用户的未软删除 Key 已达上限。
    #[error("用户 API Key 数量已达上限")]
    LimitReached,
    /// 获取连接、事务或执行 SQL 失败。
    #[error("用户 API Key 数据库操作失败")]
    Query,
    /// 数据库操作超过硬截止时间。
    #[error("用户 API Key 数据库操作超时")]
    Timeout,
    /// 关联或持久化数据违反不变量。
    #[error("用户 API Key 持久化状态损坏")]
    Invariant,
}
