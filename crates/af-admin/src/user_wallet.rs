use std::{fmt, future::Future, pin::Pin};

use af_db::{
    WalletBalanceLookupOutcome, WalletLedgerEntryRecord, WalletLedgerEntryType, WalletLedgerError,
    WalletLedgerListOutcome, WalletLedgerRepository,
};
use thiserror::Error;

use crate::SessionPrincipal;

/// 当前用户钱包账本默认分页数量。
pub const DEFAULT_USER_WALLET_PAGE_SIZE: usize = 25;

/// 当前用户可见的钱包账本事件类型。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserWalletEntryType {
    /// 用户创建或迁移时固化的初始余额。
    OpeningBalance,
    /// 管理员执行的人工余额调整。
    AdminAdjustment,
    /// 已验证支付事件完成的充值到账。
    Topup,
    /// 一次性兑换码消费完成的额度到账。
    Redemption,
    /// 邀请关系触发的注册返利到账。
    InviteRebate,
}

/// 当前用户自己的钱包额度状态。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserWalletSummary {
    balance: i64,
    used_quota: i64,
    frozen_quota: i64,
}

impl UserWalletSummary {
    /// 从已经通过额度值对象校验的字段组装响应模型。
    #[must_use]
    pub const fn from_parts(balance: i64, used_quota: i64, frozen_quota: i64) -> Self {
        Self {
            balance,
            used_quota,
            frozen_quota,
        }
    }

    /// 返回当前可用余额。
    #[must_use]
    pub const fn balance(self) -> i64 {
        self.balance
    }

    /// 返回累计完成结算的消耗额度。
    #[must_use]
    pub const fn used_quota(self) -> i64 {
        self.used_quota
    }

    /// 返回在途请求已经预扣但尚未终态结算的额度。
    #[must_use]
    pub const fn frozen_quota(self) -> i64 {
        self.frozen_quota
    }
}

/// 当前用户可见的一条钱包余额变更事实。
pub struct UserWalletEntry {
    id: i64,
    entry_type: UserWalletEntryType,
    quota_delta: i64,
    balance_before: i64,
    balance_after: i64,
    reason: Option<String>,
    created_at: i64,
}

impl UserWalletEntry {
    /// 从已经校验的账本字段组装当前用户读取模型。
    #[allow(
        clippy::too_many_arguments,
        reason = "字段与不可变钱包账本事实一一对应"
    )]
    #[must_use]
    pub fn from_parts(
        id: i64,
        entry_type: UserWalletEntryType,
        quota_delta: i64,
        balance_before: i64,
        balance_after: i64,
        reason: Option<String>,
        created_at: i64,
    ) -> Self {
        Self {
            id,
            entry_type,
            quota_delta,
            balance_before,
            balance_after,
            reason,
            created_at,
        }
    }

    /// 返回账本主键。
    #[must_use]
    pub const fn id(&self) -> i64 {
        self.id
    }

    /// 返回账本业务类型。
    #[must_use]
    pub const fn entry_type(&self) -> UserWalletEntryType {
        self.entry_type
    }

    /// 返回本次有符号额度增量。
    #[must_use]
    pub const fn quota_delta(&self) -> i64 {
        self.quota_delta
    }

    /// 返回事务内变更前余额。
    #[must_use]
    pub const fn balance_before(&self) -> i64 {
        self.balance_before
    }

    /// 返回事务内变更后余额。
    #[must_use]
    pub const fn balance_after(&self) -> i64 {
        self.balance_after
    }

    /// 返回面向用户的人工调账原因；自动到账事件没有原因。
    #[must_use]
    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }

    /// 返回创建时间 Unix 秒数。
    #[must_use]
    pub const fn created_at(&self) -> i64 {
        self.created_at
    }

    fn from_record(record: WalletLedgerEntryRecord) -> Self {
        Self {
            id: record.id(),
            entry_type: match record.entry_type() {
                WalletLedgerEntryType::OpeningBalance => UserWalletEntryType::OpeningBalance,
                WalletLedgerEntryType::AdminAdjustment => UserWalletEntryType::AdminAdjustment,
                WalletLedgerEntryType::Topup => UserWalletEntryType::Topup,
                WalletLedgerEntryType::Redemption => UserWalletEntryType::Redemption,
                WalletLedgerEntryType::InviteRebate => UserWalletEntryType::InviteRebate,
            },
            quota_delta: record.quota_delta().units(),
            balance_before: record.balance_before().units(),
            balance_after: record.balance_after().units(),
            reason: record.reason().map(str::to_owned),
            created_at: record.created_at(),
        }
    }
}

impl fmt::Debug for UserWalletEntry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("UserWalletEntry(<redacted>)")
    }
}

/// 一页当前用户自己的钱包账本。
pub struct UserWalletPage {
    entries: Vec<UserWalletEntry>,
    next_cursor: Option<i64>,
}

impl UserWalletPage {
    /// 从按主键倒序排列的条目组装读取结果。
    #[must_use]
    pub fn from_parts(entries: Vec<UserWalletEntry>, next_cursor: Option<i64>) -> Self {
        Self {
            entries,
            next_cursor,
        }
    }

    /// 返回当前页账本条目。
    #[must_use]
    pub fn entries(&self) -> &[UserWalletEntry] {
        &self.entries
    }

    /// 返回下一页账本主键游标。
    #[must_use]
    pub const fn next_cursor(&self) -> Option<i64> {
        self.next_cursor
    }
}

impl fmt::Debug for UserWalletPage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("UserWalletPage(<redacted>)")
    }
}

/// 当前用户钱包账本稳定游标查询。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserWalletListQuery {
    before: Option<i64>,
    limit: usize,
}

impl UserWalletListQuery {
    /// 校验正数游标和有界分页数量。
    pub fn new(before: Option<i64>, limit: usize) -> Result<Self, UserWalletError> {
        if before.is_some_and(|value| value <= 0)
            || !(1..=af_db::MAX_WALLET_LEDGER_PAGE_SIZE).contains(&limit)
        {
            return Err(UserWalletError::InvalidInput);
        }
        Ok(Self { before, limit })
    }

    /// 返回只读取更早记录的账本主键游标。
    #[must_use]
    pub const fn before(self) -> Option<i64> {
        self.before
    }

    /// 返回本页最大记录数。
    #[must_use]
    pub const fn limit(self) -> usize {
        self.limit
    }
}

impl Default for UserWalletListQuery {
    fn default() -> Self {
        Self {
            before: None,
            limit: DEFAULT_USER_WALLET_PAGE_SIZE,
        }
    }
}

/// 当前用户钱包读取错误；不携带主体、余额或账本内容。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum UserWalletError {
    /// 分页参数违反公开边界。
    #[error("当前用户钱包请求无效")]
    InvalidInput,
    /// 当前会话用户已经不存在或失效。
    #[error("当前用户钱包会话无效")]
    InvalidSession,
    /// 数据库失败或持久化状态损坏。
    #[error("当前用户钱包内部失败")]
    Internal,
}

/// 当前用户钱包摘要读取的对象安全 Future。
pub type UserWalletSummaryFuture<'a> =
    Pin<Box<dyn Future<Output = Result<UserWalletSummary, UserWalletError>> + Send + 'a>>;
/// 当前用户钱包账本读取的对象安全 Future。
pub type UserWalletListFuture<'a> =
    Pin<Box<dyn Future<Output = Result<UserWalletPage, UserWalletError>> + Send + 'a>>;

/// 当前用户钱包应用端口；所有权只能从会话主体推导。
pub trait UserWalletService: Send + Sync {
    /// 读取当前会话用户自己的余额状态。
    fn summary<'a>(&'a self, principal: SessionPrincipal) -> UserWalletSummaryFuture<'a>;

    /// 读取当前会话用户自己的一页不可变账本。
    fn list<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: UserWalletListQuery,
    ) -> UserWalletListFuture<'a>;
}

/// 使用共享钱包仓储实现当前用户只读钱包服务。
pub struct DatabaseUserWalletService {
    repository: WalletLedgerRepository,
}

impl DatabaseUserWalletService {
    /// 绑定已经配置操作截止时间的钱包账本仓储。
    #[must_use]
    pub const fn new(repository: WalletLedgerRepository) -> Self {
        Self { repository }
    }
}

impl UserWalletService for DatabaseUserWalletService {
    fn summary<'a>(&'a self, principal: SessionPrincipal) -> UserWalletSummaryFuture<'a> {
        Box::pin(async move {
            match self
                .repository
                .balance(principal.user_id())
                .await
                .map_err(map_repository_error)?
            {
                WalletBalanceLookupOutcome::Found(balance) => Ok(UserWalletSummary {
                    balance: balance.balance().units(),
                    used_quota: balance.used_quota().units(),
                    frozen_quota: balance.frozen_quota().units(),
                }),
                WalletBalanceLookupOutcome::NotFound => Err(UserWalletError::InvalidSession),
            }
        })
    }

    fn list<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: UserWalletListQuery,
    ) -> UserWalletListFuture<'a> {
        Box::pin(async move {
            match self
                .repository
                .list(principal.user_id(), query.before, query.limit)
                .await
                .map_err(map_repository_error)?
            {
                WalletLedgerListOutcome::Found(page) => {
                    let (entries, next_cursor) = page.into_parts();
                    Ok(UserWalletPage {
                        entries: entries
                            .into_iter()
                            .map(UserWalletEntry::from_record)
                            .collect(),
                        next_cursor,
                    })
                }
                WalletLedgerListOutcome::NotFound => Err(UserWalletError::InvalidSession),
            }
        })
    }
}

impl fmt::Debug for DatabaseUserWalletService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseUserWalletService(<redacted>)")
    }
}

fn map_repository_error(error: WalletLedgerError) -> UserWalletError {
    match error {
        WalletLedgerError::InvalidInput => UserWalletError::InvalidInput,
        WalletLedgerError::Conflict
        | WalletLedgerError::InsufficientQuota
        | WalletLedgerError::Overflow
        | WalletLedgerError::Query
        | WalletLedgerError::OutcomeUnknown
        | WalletLedgerError::Timeout
        | WalletLedgerError::Invariant => UserWalletError::Internal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_query_rejects_invalid_cursor_and_page_size() {
        assert_eq!(
            UserWalletListQuery::new(Some(0), 25),
            Err(UserWalletError::InvalidInput)
        );
        assert_eq!(
            UserWalletListQuery::new(None, 0),
            Err(UserWalletError::InvalidInput)
        );
        assert_eq!(
            UserWalletListQuery::new(None, af_db::MAX_WALLET_LEDGER_PAGE_SIZE + 1),
            Err(UserWalletError::InvalidInput)
        );
        assert_eq!(
            UserWalletListQuery::new(Some(9), 25).unwrap().before(),
            Some(9)
        );
    }
}
