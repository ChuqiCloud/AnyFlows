use std::{fmt, future::Future, pin::Pin};

use af_db::{
    IssuedRedemptionCode, MAX_REDEMPTION_BATCH_CODES, MAX_REDEMPTION_BATCH_NAME_BYTES,
    MAX_REDEMPTION_BATCH_PAGE_SIZE, PresentedRedemptionCode, RedemptionBatchRecord,
};
use af_domain::{Quota, RedemptionBatchId, RedemptionBatchStatus, UserId};
use thiserror::Error;
use zeroize::Zeroize as _;

use crate::SessionPrincipal;

use super::audit::{AdminRedemptionAuditQuery, RedemptionAuditFuture};

/// 管理端兑换码批次默认分页数量。
pub const DEFAULT_ADMIN_REDEMPTION_BATCH_PAGE_SIZE: usize = 25;

/// 管理员可读取的兑换码批次汇总。
pub struct AdminRedemptionBatch {
    batch_id: RedemptionBatchId,
    name: String,
    created_by_user_id: UserId,
    status: RedemptionBatchStatus,
    quota_amount: Quota,
    code_count: usize,
    redeemed_count: usize,
    version: u64,
    expires_at: Option<u64>,
    disabled_at: Option<u64>,
    created_at: u64,
    updated_at: u64,
}

impl AdminRedemptionBatch {
    /// 从已校验仓储记录和聚合数量组装管理端汇总。
    #[must_use]
    pub fn from_record(record: &RedemptionBatchRecord, redeemed_count: usize) -> Self {
        Self {
            batch_id: record.batch_id(),
            name: record.name().to_owned(),
            created_by_user_id: record.created_by_user_id(),
            status: record.status(),
            quota_amount: record.quota_amount(),
            code_count: record.code_count(),
            redeemed_count,
            version: record.version(),
            expires_at: record.expires_at(),
            disabled_at: record.disabled_at(),
            created_at: record.created_at(),
            updated_at: record.updated_at(),
        }
    }

    /// 返回稳定批次标识。
    #[must_use]
    pub const fn batch_id(&self) -> RedemptionBatchId {
        self.batch_id
    }

    /// 返回批次名称。
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 返回创建该批次的管理员用户。
    #[must_use]
    pub const fn created_by_user_id(&self) -> UserId {
        self.created_by_user_id
    }

    /// 返回批次启停状态。
    #[must_use]
    pub const fn status(&self) -> RedemptionBatchStatus {
        self.status
    }

    /// 返回每个兑换码到账的额度。
    #[must_use]
    pub const fn quota_amount(&self) -> Quota {
        self.quota_amount
    }

    /// 返回批次签发数量。
    #[must_use]
    pub const fn code_count(&self) -> usize {
        self.code_count
    }

    /// 返回已经到账的兑换码数量。
    #[must_use]
    pub const fn redeemed_count(&self) -> usize {
        self.redeemed_count
    }

    /// 返回当前 CAS 版本。
    #[must_use]
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// 返回可选过期时间 Unix 秒数。
    #[must_use]
    pub const fn expires_at(&self) -> Option<u64> {
        self.expires_at
    }

    /// 返回可选禁用时间 Unix 秒数。
    #[must_use]
    pub const fn disabled_at(&self) -> Option<u64> {
        self.disabled_at
    }

    /// 返回创建时间 Unix 秒数。
    #[must_use]
    pub const fn created_at(&self) -> u64 {
        self.created_at
    }

    /// 返回最后审计更新时间 Unix 秒数。
    #[must_use]
    pub const fn updated_at(&self) -> u64 {
        self.updated_at
    }
}

impl fmt::Debug for AdminRedemptionBatch {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminRedemptionBatch(<redacted>)")
    }
}

/// 一页管理员兑换码批次汇总。
pub struct AdminRedemptionBatchPage {
    batches: Vec<AdminRedemptionBatch>,
    next_cursor: Option<i64>,
}

impl AdminRedemptionBatchPage {
    /// 组合批次汇总和下一页游标，供生产服务与测试替代实现使用。
    #[must_use]
    pub fn from_parts(batches: Vec<AdminRedemptionBatch>, next_cursor: Option<i64>) -> Self {
        Self {
            batches,
            next_cursor,
        }
    }

    /// 返回当前页批次汇总。
    #[must_use]
    pub fn batches(&self) -> &[AdminRedemptionBatch] {
        &self.batches
    }

    /// 返回下一页数据库主键游标。
    #[must_use]
    pub const fn next_cursor(&self) -> Option<i64> {
        self.next_cursor
    }
}

impl fmt::Debug for AdminRedemptionBatchPage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminRedemptionBatchPage(<redacted>)")
    }
}

/// 管理端批次稳定游标查询。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdminRedemptionBatchListQuery {
    pub(super) before: Option<i64>,
    pub(super) limit: usize,
}

impl AdminRedemptionBatchListQuery {
    /// 校验正数游标和有界分页数量。
    pub fn new(before: Option<i64>, limit: usize) -> Result<Self, RedemptionServiceError> {
        if before.is_some_and(|value| value <= 0)
            || !(1..=MAX_REDEMPTION_BATCH_PAGE_SIZE).contains(&limit)
        {
            return Err(RedemptionServiceError::InvalidInput);
        }
        Ok(Self { before, limit })
    }
}

impl Default for AdminRedemptionBatchListQuery {
    fn default() -> Self {
        Self {
            before: None,
            limit: DEFAULT_ADMIN_REDEMPTION_BATCH_PAGE_SIZE,
        }
    }
}

/// 管理员创建同面额兑换码批次的结构化命令。
pub struct AdminRedemptionBatchCreateCommand {
    pub(super) name: String,
    pub(super) quota_amount: Quota,
    pub(super) code_count: usize,
    pub(super) expires_at: Option<u64>,
}

impl AdminRedemptionBatchCreateCommand {
    /// 校验名称、正整数额度、数量和可表示时间戳。
    pub fn new(
        name: String,
        quota_amount: i64,
        code_count: usize,
        expires_at: Option<i64>,
    ) -> Result<Self, RedemptionServiceError> {
        if !valid_name(&name) || !(1..=MAX_REDEMPTION_BATCH_CODES).contains(&code_count) {
            return Err(RedemptionServiceError::InvalidInput);
        }
        let quota_amount =
            Quota::new(quota_amount).map_err(|_| RedemptionServiceError::InvalidInput)?;
        if quota_amount.is_zero() || expires_at.is_some_and(|value| value <= 0) {
            return Err(RedemptionServiceError::InvalidInput);
        }
        let expires_at = expires_at
            .map(u64::try_from)
            .transpose()
            .map_err(|_| RedemptionServiceError::InvalidInput)?;
        Ok(Self {
            name,
            quota_amount,
            code_count,
            expires_at,
        })
    }
}

impl fmt::Debug for AdminRedemptionBatchCreateCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminRedemptionBatchCreateCommand(<redacted>)")
    }
}

/// 管理员以当前版本禁用兑换码批次的结构化命令。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdminRedemptionBatchDisableCommand {
    pub(super) expected_version: u64,
}

impl AdminRedemptionBatchDisableCommand {
    /// 校验可递增的正版本。
    pub fn new(expected_version: i64) -> Result<Self, RedemptionServiceError> {
        if expected_version <= 0 || expected_version == i64::MAX {
            return Err(RedemptionServiceError::InvalidInput);
        }
        let expected_version =
            u64::try_from(expected_version).map_err(|_| RedemptionServiceError::InvalidInput)?;
        Ok(Self { expected_version })
    }
}

/// 管理员禁用批次后的状态迁移结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AdminRedemptionBatchDisableResult {
    batch_id: RedemptionBatchId,
    version: u64,
    disabled_at: u64,
}

impl AdminRedemptionBatchDisableResult {
    pub(super) fn from_record(
        record: &RedemptionBatchRecord,
    ) -> Result<Self, RedemptionServiceError> {
        let disabled_at = record
            .disabled_at()
            .ok_or(RedemptionServiceError::Internal)?;
        Ok(Self {
            batch_id: record.batch_id(),
            version: record.version(),
            disabled_at,
        })
    }

    /// 返回已禁用批次标识。
    #[must_use]
    pub const fn batch_id(self) -> RedemptionBatchId {
        self.batch_id
    }

    /// 返回禁用后的新版本。
    #[must_use]
    pub const fn version(self) -> u64 {
        self.version
    }

    /// 返回服务端禁用时间 Unix 秒数。
    #[must_use]
    pub const fn disabled_at(self) -> u64 {
        self.disabled_at
    }
}

/// 只应在创建响应中展示一次的批次明文材料。
pub struct IssuedAdminRedemptionBatch {
    batch: AdminRedemptionBatch,
    codes: Vec<IssuedRedemptionCode>,
}

impl IssuedAdminRedemptionBatch {
    /// 组合已经持久化的批次与一次性签发材料。
    #[must_use]
    pub fn from_parts(batch: AdminRedemptionBatch, codes: Vec<IssuedRedemptionCode>) -> Self {
        Self { batch, codes }
    }

    /// 返回已持久化的批次汇总。
    #[must_use]
    pub const fn batch(&self) -> &AdminRedemptionBatch {
        &self.batch
    }

    /// 返回仅允许在本次响应读取的完整兑换码集合。
    #[must_use]
    pub fn codes(&self) -> &[IssuedRedemptionCode] {
        &self.codes
    }
}

impl fmt::Debug for IssuedAdminRedemptionBatch {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("IssuedAdminRedemptionBatch(<redacted>)")
    }
}

/// 当前用户提交的一次兑换尝试，明文由可清零类型持有。
pub struct UserRedemptionCommand {
    pub(super) code: PresentedRedemptionCode,
}

impl UserRedemptionCommand {
    /// 解析唯一规范格式，并清零 HTTP DTO 转交的临时字符串。
    pub fn new(mut code: String) -> Result<Self, RedemptionServiceError> {
        let parsed = PresentedRedemptionCode::parse(&code);
        code.zeroize();
        Ok(Self {
            code: parsed.map_err(|_| RedemptionServiceError::CodeInvalid)?,
        })
    }
}

impl fmt::Debug for UserRedemptionCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("UserRedemptionCommand(<redacted>)")
    }
}

/// 当前用户成功兑换后的余额结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserRedemptionResult {
    quota_amount: Quota,
    balance_after: Quota,
    redeemed_at: u64,
    replayed: bool,
}

impl UserRedemptionResult {
    pub(super) const fn new(
        quota_amount: Quota,
        balance_after: Quota,
        redeemed_at: u64,
        replayed: bool,
    ) -> Self {
        Self {
            quota_amount,
            balance_after,
            redeemed_at,
            replayed,
        }
    }

    /// 返回本次到账额度。
    #[must_use]
    pub const fn quota_amount(self) -> Quota {
        self.quota_amount
    }

    /// 返回事务提交后的可用余额。
    #[must_use]
    pub const fn balance_after(self) -> Quota {
        self.balance_after
    }

    /// 返回首次到账时间 Unix 秒数。
    #[must_use]
    pub const fn redeemed_at(self) -> u64 {
        self.redeemed_at
    }

    /// 返回本次是否恢复了同一用户已经到账的事实。
    #[must_use]
    pub const fn replayed(self) -> bool {
        self.replayed
    }
}

/// 兑换码管理与用户兑换应用错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RedemptionServiceError {
    /// 请求字段、游标或时间边界无效。
    #[error("兑换码请求参数无效")]
    InvalidInput,
    /// 当前会话不是管理员。
    #[error("兑换码管理权限不足")]
    Forbidden,
    /// 当前会话用户已经不存在或失效。
    #[error("兑换码会话无效")]
    InvalidSession,
    /// 目标批次不存在。
    #[error("兑换码批次不存在")]
    BatchNotFound,
    /// 批次事实或版本与并发操作冲突。
    #[error("兑换码批次状态冲突")]
    BatchConflict,
    /// 兑换码格式无效或摘要不存在。
    #[error("兑换码无效")]
    CodeInvalid,
    /// 兑换码所属批次已停用。
    #[error("兑换码批次已停用")]
    BatchDisabled,
    /// 兑换码所属批次已经过期。
    #[error("兑换码已过期")]
    CodeExpired,
    /// 兑换码已经由其他用户消费。
    #[error("兑换码已使用")]
    CodeAlreadyUsed,
    /// 到账会超过钱包额度上界。
    #[error("兑换码到账超过余额上限")]
    BalanceOverflow,
    /// 数据库提交结果仍未知，调用方可重试同一请求确认。
    #[error("兑换码操作结果未知")]
    OutcomeUnknown,
    /// 数据库、时钟、随机源或持久化不变量失败。
    #[error("兑换码内部失败")]
    Internal,
}

pub type RedemptionListFuture<'a> = Pin<
    Box<dyn Future<Output = Result<AdminRedemptionBatchPage, RedemptionServiceError>> + Send + 'a>,
>;
pub type RedemptionCreateFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<IssuedAdminRedemptionBatch, RedemptionServiceError>> + Send + 'a,
    >,
>;
pub type RedemptionDisableFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<AdminRedemptionBatchDisableResult, RedemptionServiceError>>
            + Send
            + 'a,
    >,
>;
pub type RedemptionRedeemFuture<'a> =
    Pin<Box<dyn Future<Output = Result<UserRedemptionResult, RedemptionServiceError>> + Send + 'a>>;

/// 兑换码管理与当前用户兑换应用端口。
pub trait RedemptionService: Send + Sync {
    /// 读取管理员可见的批次运营统计和兑换事实时间筛选结果。
    fn audit<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: AdminRedemptionAuditQuery,
    ) -> RedemptionAuditFuture<'a>;

    /// 列出管理员可见的非敏感批次汇总。
    fn list<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: AdminRedemptionBatchListQuery,
    ) -> RedemptionListFuture<'a>;

    /// 创建同面额批次并一次性返回全部明文。
    fn create<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: AdminRedemptionBatchCreateCommand,
    ) -> RedemptionCreateFuture<'a>;

    /// 以当前版本整体停用一个批次。
    fn disable<'a>(
        &'a self,
        principal: SessionPrincipal,
        batch_id: RedemptionBatchId,
        command: AdminRedemptionBatchDisableCommand,
    ) -> RedemptionDisableFuture<'a>;

    /// 仅为当前会话用户原子消费一个兑换码。
    fn redeem<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: UserRedemptionCommand,
    ) -> RedemptionRedeemFuture<'a>;
}

fn valid_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_REDEMPTION_BATCH_NAME_BYTES
        && value.trim() == value
        && !value.chars().any(char::is_control)
}
