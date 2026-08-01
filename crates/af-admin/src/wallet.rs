use std::{fmt, future::Future, pin::Pin};

use af_db::{
    WalletAdjustmentOutcome, WalletAdjustmentWrite, WalletLedgerEntryRecord, WalletLedgerEntryType,
    WalletLedgerError, WalletLedgerListOutcome, WalletLedgerRepository,
};
use af_domain::{QuotaDelta, UserId, WalletEventId};
use thiserror::Error;

use crate::{SessionPrincipal, SessionRole};

/// 管理端钱包账本默认分页数量。
pub const DEFAULT_ADMIN_WALLET_PAGE_SIZE: usize = 50;

/// 管理端钱包账本事件类型。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdminWalletEntryType {
    /// 用户初始非零余额或迁移基线。
    OpeningBalance,
    /// 管理员人工有符号调账。
    AdminAdjustment,
    /// 已验证支付事件完成的充值到账。
    Topup,
    /// 一次性兑换码原子消费完成的额度到账。
    Redemption,
    /// 邀请关系触发的注册返利额度到账。
    InviteRebate,
}

/// 管理端可见的一条钱包余额变更事实。
pub struct AdminWalletEntry {
    id: i64,
    event_id: WalletEventId,
    user_id: UserId,
    actor_user_id: Option<UserId>,
    entry_type: AdminWalletEntryType,
    quota_delta: i64,
    balance_before: i64,
    balance_after: i64,
    reason: Option<String>,
    created_at: i64,
}

impl AdminWalletEntry {
    /// 从已经校验的应用层字段组装钱包账本读取模型。
    #[allow(
        clippy::too_many_arguments,
        reason = "字段与不可变钱包账本事实一一对应"
    )]
    #[must_use]
    pub fn from_parts(
        id: i64,
        event_id: WalletEventId,
        user_id: UserId,
        actor_user_id: Option<UserId>,
        entry_type: AdminWalletEntryType,
        quota_delta: i64,
        balance_before: i64,
        balance_after: i64,
        reason: Option<String>,
        created_at: i64,
    ) -> Self {
        Self {
            id,
            event_id,
            user_id,
            actor_user_id,
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

    /// 返回稳定钱包事件标识。
    #[must_use]
    pub const fn event_id(&self) -> WalletEventId {
        self.event_id
    }

    /// 返回余额所属用户。
    #[must_use]
    pub const fn user_id(&self) -> UserId {
        self.user_id
    }

    /// 返回执行变更的管理员；系统基线与充值到账没有管理员主体。
    #[must_use]
    pub const fn actor_user_id(&self) -> Option<UserId> {
        self.actor_user_id
    }

    /// 返回账本业务类型。
    #[must_use]
    pub const fn entry_type(&self) -> AdminWalletEntryType {
        self.entry_type
    }

    /// 返回有符号额度增量。
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

    /// 返回管理员填写的调账原因；系统事件没有人工原因。
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
            event_id: record.event_id(),
            user_id: record.user_id(),
            actor_user_id: record.actor_user_id(),
            entry_type: match record.entry_type() {
                WalletLedgerEntryType::OpeningBalance => AdminWalletEntryType::OpeningBalance,
                WalletLedgerEntryType::AdminAdjustment => AdminWalletEntryType::AdminAdjustment,
                WalletLedgerEntryType::Topup => AdminWalletEntryType::Topup,
                WalletLedgerEntryType::Redemption => AdminWalletEntryType::Redemption,
                WalletLedgerEntryType::InviteRebate => AdminWalletEntryType::InviteRebate,
            },
            quota_delta: record.quota_delta().units(),
            balance_before: record.balance_before().units(),
            balance_after: record.balance_after().units(),
            reason: record.reason().map(str::to_owned),
            created_at: record.created_at(),
        }
    }
}

impl fmt::Debug for AdminWalletEntry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminWalletEntry(<redacted>)")
    }
}

/// 一页管理端钱包账本。
pub struct AdminWalletPage {
    entries: Vec<AdminWalletEntry>,
    next_cursor: Option<i64>,
}

impl AdminWalletPage {
    /// 从已经验证且按主键倒序排列的条目组装一页读取结果。
    #[must_use]
    pub fn from_parts(entries: Vec<AdminWalletEntry>, next_cursor: Option<i64>) -> Self {
        Self {
            entries,
            next_cursor,
        }
    }

    /// 返回当前页账本条目。
    #[must_use]
    pub fn entries(&self) -> &[AdminWalletEntry] {
        &self.entries
    }

    /// 返回下一页账本主键游标。
    #[must_use]
    pub const fn next_cursor(&self) -> Option<i64> {
        self.next_cursor
    }
}

impl fmt::Debug for AdminWalletPage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminWalletPage(<redacted>)")
    }
}

/// 钱包账本稳定游标查询。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdminWalletListQuery {
    before: Option<i64>,
    limit: usize,
}

impl AdminWalletListQuery {
    /// 校验正数游标和有界分页数量。
    pub fn new(before: Option<i64>, limit: usize) -> Result<Self, AdminWalletError> {
        if before.is_some_and(|value| value <= 0)
            || !(1..=af_db::MAX_WALLET_LEDGER_PAGE_SIZE).contains(&limit)
        {
            return Err(AdminWalletError::InvalidInput);
        }
        Ok(Self { before, limit })
    }
}

impl Default for AdminWalletListQuery {
    fn default() -> Self {
        Self {
            before: None,
            limit: DEFAULT_ADMIN_WALLET_PAGE_SIZE,
        }
    }
}

/// 一次管理员有符号调账命令。
pub struct AdminWalletAdjustmentCommand {
    event_id: WalletEventId,
    quota_delta: QuotaDelta,
    reason: String,
}

impl AdminWalletAdjustmentCommand {
    /// 校验非零增量；原因及系统保留事件键由共享账本写模型继续闭合。
    pub fn new(
        event_id: WalletEventId,
        quota_delta: i64,
        reason: String,
    ) -> Result<Self, AdminWalletError> {
        let quota_delta = QuotaDelta::new(quota_delta)
            .ok()
            .filter(|delta| !delta.is_zero())
            .ok_or(AdminWalletError::InvalidInput)?;
        if reason.is_empty()
            || reason.trim() != reason
            || reason.len() > af_db::MAX_WALLET_ADJUSTMENT_REASON_BYTES
            || reason.chars().any(char::is_control)
            || event_id.is_system_opening()
        {
            return Err(AdminWalletError::InvalidInput);
        }
        Ok(Self {
            event_id,
            quota_delta,
            reason,
        })
    }
}

impl fmt::Debug for AdminWalletAdjustmentCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminWalletAdjustmentCommand(<redacted>)")
    }
}

/// 管理端调账结果，保留新提交与幂等重放的区别。
pub enum AdminWalletAdjustmentResult {
    /// 本次请求提交了新账本事件。
    Applied(AdminWalletEntry),
    /// 相同业务事实已经提交，本次未重复改变余额。
    Existing(AdminWalletEntry),
}

/// 管理端钱包服务错误；不携带事件、主体、额度或原因。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AdminWalletError {
    /// 请求字段或分页参数无效。
    #[error("管理钱包请求无效")]
    InvalidInput,
    /// 当前会话不是管理员。
    #[error("管理钱包权限不足")]
    Forbidden,
    /// 目标用户不存在或已经软删除。
    #[error("管理钱包用户不存在")]
    NotFound,
    /// 相同事件键已经绑定不同事实。
    #[error("管理钱包事件冲突")]
    Conflict,
    /// 负向调账会使余额低于零。
    #[error("管理钱包余额不足")]
    InsufficientQuota,
    /// 正向调账会超过整数上界。
    #[error("管理钱包余额溢出")]
    Overflow,
    /// 调账结果未知，调用方必须复用同一事件键查询或重试。
    #[error("管理钱包调账结果未知")]
    OutcomeUnknown,
    /// 数据库失败或持久化状态损坏。
    #[error("管理钱包内部失败")]
    Internal,
}

/// 管理端钱包列表调用的对象安全 Future。
pub type AdminWalletListFuture<'a> =
    Pin<Box<dyn Future<Output = Result<AdminWalletPage, AdminWalletError>> + Send + 'a>>;
/// 管理端钱包调账调用的对象安全 Future。
pub type AdminWalletAdjustmentFuture<'a> = Pin<
    Box<dyn Future<Output = Result<AdminWalletAdjustmentResult, AdminWalletError>> + Send + 'a>,
>;

/// 管理端钱包应用端口；角色校验必须在进入仓储前完成。
pub trait AdminWalletService: Send + Sync {
    /// 读取指定用户的一页不可变余额账本。
    fn list<'a>(
        &'a self,
        principal: SessionPrincipal,
        user_id: UserId,
        query: AdminWalletListQuery,
    ) -> AdminWalletListFuture<'a>;

    /// 使用明确事件键原子追加一次管理员有符号调账。
    fn adjust<'a>(
        &'a self,
        principal: SessionPrincipal,
        user_id: UserId,
        command: AdminWalletAdjustmentCommand,
    ) -> AdminWalletAdjustmentFuture<'a>;
}

/// 使用数据库追加账本实现管理端钱包服务。
pub struct DatabaseAdminWalletService {
    repository: WalletLedgerRepository,
}

impl DatabaseAdminWalletService {
    /// 绑定已经配置操作截止时间的钱包账本仓储。
    #[must_use]
    pub const fn new(repository: WalletLedgerRepository) -> Self {
        Self { repository }
    }
}

impl AdminWalletService for DatabaseAdminWalletService {
    fn list<'a>(
        &'a self,
        principal: SessionPrincipal,
        user_id: UserId,
        query: AdminWalletListQuery,
    ) -> AdminWalletListFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            match self
                .repository
                .list(user_id, query.before, query.limit)
                .await
                .map_err(map_repository_error)?
            {
                WalletLedgerListOutcome::Found(page) => {
                    let (entries, next_cursor) = page.into_parts();
                    Ok(AdminWalletPage {
                        entries: entries
                            .into_iter()
                            .map(AdminWalletEntry::from_record)
                            .collect(),
                        next_cursor,
                    })
                }
                WalletLedgerListOutcome::NotFound => Err(AdminWalletError::NotFound),
            }
        })
    }

    fn adjust<'a>(
        &'a self,
        principal: SessionPrincipal,
        user_id: UserId,
        command: AdminWalletAdjustmentCommand,
    ) -> AdminWalletAdjustmentFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            let write = WalletAdjustmentWrite::new(
                command.event_id,
                user_id,
                principal.user_id(),
                command.quota_delta,
                command.reason,
            )
            .map_err(map_repository_error)?;
            match self
                .repository
                .adjust(write)
                .await
                .map_err(map_repository_error)?
            {
                WalletAdjustmentOutcome::Applied(entry) => Ok(
                    AdminWalletAdjustmentResult::Applied(AdminWalletEntry::from_record(entry)),
                ),
                WalletAdjustmentOutcome::Existing(entry) => Ok(
                    AdminWalletAdjustmentResult::Existing(AdminWalletEntry::from_record(entry)),
                ),
                WalletAdjustmentOutcome::NotFound => Err(AdminWalletError::NotFound),
            }
        })
    }
}

impl fmt::Debug for DatabaseAdminWalletService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseAdminWalletService(<redacted>)")
    }
}

fn require_admin(principal: SessionPrincipal) -> Result<(), AdminWalletError> {
    if principal.role() == SessionRole::Admin {
        Ok(())
    } else {
        Err(AdminWalletError::Forbidden)
    }
}

fn map_repository_error(error: WalletLedgerError) -> AdminWalletError {
    match error {
        WalletLedgerError::InvalidInput => AdminWalletError::InvalidInput,
        WalletLedgerError::Conflict => AdminWalletError::Conflict,
        WalletLedgerError::InsufficientQuota => AdminWalletError::InsufficientQuota,
        WalletLedgerError::Overflow => AdminWalletError::Overflow,
        WalletLedgerError::OutcomeUnknown => AdminWalletError::OutcomeUnknown,
        WalletLedgerError::Query | WalletLedgerError::Timeout | WalletLedgerError::Invariant => {
            AdminWalletError::Internal
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_rejects_zero_reserved_events_and_unusable_reasons() {
        let event_id = WalletEventId::new([0x80; 16]).unwrap();
        assert_eq!(
            AdminWalletAdjustmentCommand::new(event_id, 0, "调账".to_owned()).unwrap_err(),
            AdminWalletError::InvalidInput
        );
        assert_eq!(
            AdminWalletAdjustmentCommand::new(event_id, 1, " 调账 ".to_owned()).unwrap_err(),
            AdminWalletError::InvalidInput
        );
        let reserved =
            WalletEventId::from_persistence_key("00000000000000010000000000000001").unwrap();
        assert_eq!(
            AdminWalletAdjustmentCommand::new(reserved, 1, "调账".to_owned()).unwrap_err(),
            AdminWalletError::InvalidInput
        );
    }

    #[test]
    fn normal_user_is_rejected_before_wallet_repository_access() {
        let principal = SessionPrincipal::new(UserId::new(1).unwrap(), SessionRole::User);
        assert_eq!(require_admin(principal), Err(AdminWalletError::Forbidden));
    }
}
