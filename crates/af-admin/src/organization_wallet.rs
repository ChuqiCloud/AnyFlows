use std::{fmt, future::Future, pin::Pin, sync::Arc};

use af_db::{
    MAX_ORGANIZATION_WALLET_LEDGER_PAGE_SIZE, OrganizationWalletAdjustmentOutcome,
    OrganizationWalletAdjustmentWrite, OrganizationWalletBalanceLookupOutcome,
    OrganizationWalletError as RepositoryError, OrganizationWalletLedgerEntryRecord,
    OrganizationWalletLedgerEntryType, OrganizationWalletLedgerListOutcome,
    OrganizationWalletRepository,
};
use af_domain::{OrganizationId, OrganizationPermission, Quota, QuotaDelta, UserId, WalletEventId};
use thiserror::Error;

use crate::OrganizationContext;

pub const DEFAULT_ORGANIZATION_WALLET_PAGE_SIZE: usize = 50;

/// 组织钱包当前余额摘要，不暴露数据库模型或内部版本字段以外的实现细节。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationWalletSummary {
    organization_id: OrganizationId,
    balance: Quota,
    used_quota: Quota,
    frozen_quota: Quota,
    version: i64,
}

impl OrganizationWalletSummary {
    pub const fn organization_id(self) -> OrganizationId {
        self.organization_id
    }

    pub const fn balance(self) -> Quota {
        self.balance
    }

    pub const fn used_quota(self) -> Quota {
        self.used_quota
    }

    pub const fn frozen_quota(self) -> Quota {
        self.frozen_quota
    }

    pub const fn version(self) -> i64 {
        self.version
    }
}

/// 组织钱包账本中可安全返回给 HTTP 层的不可变事实。
pub struct OrganizationWalletEntry {
    id: i64,
    organization_id: OrganizationId,
    event_id: WalletEventId,
    actor_user_id: Option<UserId>,
    entry_type: OrganizationWalletLedgerEntryType,
    quota_delta: QuotaDelta,
    balance_before: Quota,
    balance_after: Quota,
    source: String,
    reason: Option<String>,
    created_at: i64,
}

impl OrganizationWalletEntry {
    pub const fn id(&self) -> i64 {
        self.id
    }

    pub const fn organization_id(&self) -> OrganizationId {
        self.organization_id
    }

    pub const fn event_id(&self) -> WalletEventId {
        self.event_id
    }

    pub const fn actor_user_id(&self) -> Option<UserId> {
        self.actor_user_id
    }

    pub const fn entry_type(&self) -> OrganizationWalletLedgerEntryType {
        self.entry_type
    }

    pub const fn quota_delta(&self) -> QuotaDelta {
        self.quota_delta
    }

    pub const fn balance_before(&self) -> Quota {
        self.balance_before
    }

    pub const fn balance_after(&self) -> Quota {
        self.balance_after
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn reason(&self) -> Option<&str> {
        self.reason.as_deref()
    }

    pub const fn created_at(&self) -> i64 {
        self.created_at
    }
}

impl From<OrganizationWalletLedgerEntryRecord> for OrganizationWalletEntry {
    fn from(record: OrganizationWalletLedgerEntryRecord) -> Self {
        Self {
            id: record.id(),
            organization_id: record.organization_id(),
            event_id: record.event_id(),
            actor_user_id: record.actor_user_id(),
            entry_type: record.entry_type(),
            quota_delta: record.quota_delta(),
            balance_before: record.balance_before(),
            balance_after: record.balance_after(),
            source: record.source().to_owned(),
            reason: record.reason().map(str::to_owned),
            created_at: record.created_at(),
        }
    }
}

impl fmt::Debug for OrganizationWalletEntry {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationWalletEntry(<redacted>)")
    }
}

pub struct OrganizationWalletPage {
    entries: Vec<OrganizationWalletEntry>,
    next_cursor: Option<i64>,
}

impl OrganizationWalletPage {
    pub fn entries(&self) -> &[OrganizationWalletEntry] {
        &self.entries
    }

    pub const fn next_cursor(&self) -> Option<i64> {
        self.next_cursor
    }
}

pub struct OrganizationWalletListQuery {
    before: Option<i64>,
    limit: usize,
}

impl OrganizationWalletListQuery {
    pub fn new(before: Option<i64>, limit: usize) -> Result<Self, OrganizationWalletServiceError> {
        if before.is_some_and(|value| value <= 0)
            || !(1..=MAX_ORGANIZATION_WALLET_LEDGER_PAGE_SIZE).contains(&limit)
        {
            return Err(OrganizationWalletServiceError::InvalidRequest);
        }
        Ok(Self { before, limit })
    }

    pub const fn before(&self) -> Option<i64> {
        self.before
    }

    pub const fn limit(&self) -> usize {
        self.limit
    }
}

pub struct OrganizationWalletAdjustmentCommand {
    event_id: WalletEventId,
    quota_delta: QuotaDelta,
    reason: String,
}

impl OrganizationWalletAdjustmentCommand {
    pub fn new(
        event_id: WalletEventId,
        quota_delta: QuotaDelta,
        reason: String,
    ) -> Result<Self, OrganizationWalletServiceError> {
        if event_id.is_system_opening() || quota_delta.is_zero() || reason.is_empty() {
            return Err(OrganizationWalletServiceError::InvalidRequest);
        }
        Ok(Self {
            event_id,
            quota_delta,
            reason,
        })
    }
}

pub struct OrganizationWalletAdjustment {
    entry: OrganizationWalletEntry,
    replayed: bool,
}

impl OrganizationWalletAdjustment {
    pub fn entry(&self) -> &OrganizationWalletEntry {
        &self.entry
    }

    pub const fn replayed(&self) -> bool {
        self.replayed
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationWalletServiceError {
    #[error("组织钱包请求无效")]
    InvalidRequest,
    #[error("组织钱包权限不足")]
    Forbidden,
    #[error("组织钱包不存在")]
    NotFound,
    #[error("组织钱包事件冲突")]
    Conflict,
    #[error("组织钱包余额不足")]
    InsufficientQuota,
    #[error("组织钱包余额溢出")]
    Overflow,
    #[error("组织钱包内部错误")]
    Internal,
    #[error("组织钱包写入结果未知")]
    OutcomeUnknown,
}

pub type OrganizationWalletSummaryFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<OrganizationWalletSummary, OrganizationWalletServiceError>>
            + Send
            + 'a,
    >,
>;
pub type OrganizationWalletListFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<OrganizationWalletPage, OrganizationWalletServiceError>>
            + Send
            + 'a,
    >,
>;
pub type OrganizationWalletAdjustmentFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<OrganizationWalletAdjustment, OrganizationWalletServiceError>>
            + Send
            + 'a,
    >,
>;

pub trait OrganizationWalletService: Send + Sync {
    fn summary<'a>(&'a self, context: OrganizationContext) -> OrganizationWalletSummaryFuture<'a>;

    fn list<'a>(
        &'a self,
        context: OrganizationContext,
        query: OrganizationWalletListQuery,
    ) -> OrganizationWalletListFuture<'a>;

    fn adjust<'a>(
        &'a self,
        context: OrganizationContext,
        command: OrganizationWalletAdjustmentCommand,
    ) -> OrganizationWalletAdjustmentFuture<'a>;
}

pub struct DatabaseOrganizationWalletService {
    repository: Arc<OrganizationWalletRepository>,
}

impl DatabaseOrganizationWalletService {
    pub fn new(repository: Arc<OrganizationWalletRepository>) -> Self {
        Self { repository }
    }
}

impl OrganizationWalletService for DatabaseOrganizationWalletService {
    fn summary<'a>(&'a self, context: OrganizationContext) -> OrganizationWalletSummaryFuture<'a> {
        Box::pin(async move {
            require_permission(&context, OrganizationPermission::WalletRead)?;
            match self
                .repository
                .balance(context.organization_id())
                .await
                .map_err(map_repository_error)?
            {
                OrganizationWalletBalanceLookupOutcome::Found(record) => {
                    Ok(OrganizationWalletSummary {
                        organization_id: record.organization_id(),
                        balance: record.quota(),
                        used_quota: record.used_quota(),
                        frozen_quota: record.frozen_quota(),
                        version: record.version(),
                    })
                }
                OrganizationWalletBalanceLookupOutcome::NotFound => {
                    Err(OrganizationWalletServiceError::NotFound)
                }
            }
        })
    }

    fn list<'a>(
        &'a self,
        context: OrganizationContext,
        query: OrganizationWalletListQuery,
    ) -> OrganizationWalletListFuture<'a> {
        Box::pin(async move {
            require_permission(&context, OrganizationPermission::WalletRead)?;
            let page = match self
                .repository
                .list(context.organization_id(), query.before(), query.limit())
                .await
                .map_err(map_repository_error)?
            {
                OrganizationWalletLedgerListOutcome::Found(page) => page,
                OrganizationWalletLedgerListOutcome::NotFound => {
                    return Err(OrganizationWalletServiceError::NotFound);
                }
            };
            let (entries, next_cursor) = page.into_parts();
            Ok(OrganizationWalletPage {
                entries: entries
                    .into_iter()
                    .map(OrganizationWalletEntry::from)
                    .collect(),
                next_cursor,
            })
        })
    }

    fn adjust<'a>(
        &'a self,
        context: OrganizationContext,
        command: OrganizationWalletAdjustmentCommand,
    ) -> OrganizationWalletAdjustmentFuture<'a> {
        Box::pin(async move {
            require_permission(&context, OrganizationPermission::WalletManage)?;
            let write = OrganizationWalletAdjustmentWrite::new(
                command.event_id,
                context.organization_id(),
                context.user_id(),
                command.quota_delta,
                command.reason,
            )
            .map_err(map_repository_error)?;
            let outcome = self
                .repository
                .adjust(write)
                .await
                .map_err(map_repository_error)?;
            match outcome {
                OrganizationWalletAdjustmentOutcome::Applied(entry) => {
                    Ok(OrganizationWalletAdjustment {
                        entry: entry.into(),
                        replayed: false,
                    })
                }
                OrganizationWalletAdjustmentOutcome::Existing(entry) => {
                    Ok(OrganizationWalletAdjustment {
                        entry: entry.into(),
                        replayed: true,
                    })
                }
                OrganizationWalletAdjustmentOutcome::NotFound => {
                    Err(OrganizationWalletServiceError::NotFound)
                }
            }
        })
    }
}

impl fmt::Debug for DatabaseOrganizationWalletService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseOrganizationWalletService(<repository>)")
    }
}

fn require_permission(
    context: &OrganizationContext,
    permission: OrganizationPermission,
) -> Result<(), OrganizationWalletServiceError> {
    context
        .allows(permission)
        .then_some(())
        .ok_or(OrganizationWalletServiceError::Forbidden)
}

fn map_repository_error(error: RepositoryError) -> OrganizationWalletServiceError {
    match error {
        RepositoryError::InvalidInput => OrganizationWalletServiceError::InvalidRequest,
        RepositoryError::Conflict => OrganizationWalletServiceError::Conflict,
        RepositoryError::InsufficientQuota => OrganizationWalletServiceError::InsufficientQuota,
        RepositoryError::Overflow => OrganizationWalletServiceError::Overflow,
        RepositoryError::OutcomeUnknown => OrganizationWalletServiceError::OutcomeUnknown,
        RepositoryError::Query | RepositoryError::Timeout | RepositoryError::Invariant => {
            OrganizationWalletServiceError::Internal
        }
    }
}
