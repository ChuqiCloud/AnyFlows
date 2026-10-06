use std::fmt;

use af_domain::UserId;
use sea_orm::entity::prelude::Json;
use thiserror::Error;

/// 单个用户允许同时保留的未失效分享数量。
pub const MAX_ACTIVE_PLAYGROUND_SHARES_PER_USER: usize = 32;
/// 单份分享快照序列化后的最大字节数。
pub const MAX_PLAYGROUND_SHARE_SNAPSHOT_BYTES: usize = 512 * 1024;

/// 已由应用层验证、等待持久化的分享快照。
pub struct PlaygroundShareWrite {
    pub(super) owner_user_id: UserId,
    pub(super) token_hash: String,
    pub(super) snapshot: Json,
    pub(super) expires_at: i64,
}

impl PlaygroundShareWrite {
    /// 组装分享所有者、令牌摘要、不可变快照和 Unix 秒级过期时间。
    #[must_use]
    pub fn new(owner_user_id: UserId, token_hash: String, snapshot: Json, expires_at: i64) -> Self {
        Self {
            owner_user_id,
            token_hash,
            snapshot,
            expires_at,
        }
    }
}

impl fmt::Debug for PlaygroundShareWrite {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PlaygroundShareWrite(<redacted>)")
    }
}

/// 新分享落库后的非敏感时间信息。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlaygroundShareCreatedRecord {
    pub(super) created_at: i64,
    pub(super) expires_at: i64,
}

impl PlaygroundShareCreatedRecord {
    /// 返回服务器确认的创建时间。
    #[must_use]
    pub const fn created_at(self) -> i64 {
        self.created_at
    }

    /// 返回服务器确认的过期时间。
    #[must_use]
    pub const fn expires_at(self) -> i64 {
        self.expires_at
    }
}

/// 公开读取到的一份有效分享；调试输出不会呈现快照正文。
pub struct PlaygroundShareRecord {
    pub(super) snapshot: Json,
    pub(super) created_at: i64,
    pub(super) expires_at: i64,
}

impl PlaygroundShareRecord {
    /// 消费记录并返回快照及其时间边界。
    #[must_use]
    pub fn into_parts(self) -> (Json, i64, i64) {
        (self.snapshot, self.created_at, self.expires_at)
    }
}

impl fmt::Debug for PlaygroundShareRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PlaygroundShareRecord(<redacted>)")
    }
}

/// 创建者撤销分享后的闭合结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlaygroundShareRevokeOutcome {
    /// 本次请求写入了撤销时间。
    Revoked,
    /// 令牌未知、已失效、已撤销或不属于当前用户。
    NotFound,
}

/// 分享仓储构造错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum PlaygroundShareRepositoryConfigError {
    /// 零超时无法形成有效的数据库操作截止时间。
    #[error("Playground 分享仓储超时必须大于零")]
    ZeroOperationTimeout,
}

/// 分享仓储错误；不携带令牌、摘要、用户标识或快照正文。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum PlaygroundShareRepositoryError {
    /// 写入参数不满足摘要、容量或时间边界。
    #[error("Playground 分享写入参数无效")]
    InvalidInput,
    /// 当前会话用户已失效，不能创建新分享。
    #[error("Playground 分享所有者不可用")]
    OwnerUnavailable,
    /// 当前用户的有效分享数量已经达到上限。
    #[error("Playground 有效分享数量已达上限")]
    LimitReached,
    /// 极小概率下令牌摘要发生唯一冲突。
    #[error("Playground 分享令牌冲突")]
    TokenConflict,
    /// 获取连接、事务或执行数据库语句失败。
    #[error("Playground 分享数据库操作失败")]
    Query,
    /// 数据库操作超过硬截止时间。
    #[error("Playground 分享数据库操作超时")]
    Timeout,
    /// 持久化记录违反分享不变量。
    #[error("Playground 分享持久化状态损坏")]
    Invariant,
}
