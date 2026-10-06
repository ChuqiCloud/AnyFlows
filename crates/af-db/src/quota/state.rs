use af_domain::{BillingContractPriceSnapshot, GatewayPrincipal, Quota, QuotaWindowRetryAfter};
use rust_decimal::Decimal;
use sea_orm::{QueryResult, entity::prelude::TimeDateTimeWithTimeZone};
use thiserror::Error;

/// 持久化预留选择的资金来源。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i16)]
pub(crate) enum QuotaFundingSource {
    /// 使用用户钱包预留和结算。
    Wallet = 1,
    /// 优先使用绑定到请求的订阅窗口，溢出部分由钱包结算。
    Subscription = 2,
}

impl QuotaFundingSource {
    pub(crate) const fn code(self) -> i16 {
        self as i16
    }

    fn from_code(code: i16) -> Result<Self, QuotaRepositoryError> {
        match code {
            1 => Ok(Self::Wallet),
            2 => Ok(Self::Subscription),
            _ => Err(QuotaRepositoryError::Invariant),
        }
    }
}

/// 计费预留的业务用途；用途决定资金来源与结算上限，创建后不可变。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i16)]
pub enum QuotaReservationKind {
    /// 同步或流式请求，可使用订阅并在实际用量超出预扣时补扣。
    Request = 1,
    /// 异步批量任务，只冻结钱包且实际结算不得超过冻结上限。
    BatchTask = 2,
}

impl QuotaReservationKind {
    pub(crate) const fn code(self) -> i16 {
        self as i16
    }

    pub(crate) const fn allows_subscription(self) -> bool {
        matches!(self, Self::Request)
    }

    pub(crate) const fn allows_supplement(self) -> bool {
        matches!(self, Self::Request)
    }

    pub(crate) const fn uses_token_windows(self) -> bool {
        matches!(self, Self::Request)
    }

    pub(crate) const fn uses_group_windows(self) -> bool {
        matches!(self, Self::Request)
    }

    fn from_code(code: i16) -> Result<Self, QuotaRepositoryError> {
        match code {
            1 => Ok(Self::Request),
            2 => Ok(Self::BatchTask),
            _ => Err(QuotaRepositoryError::Invariant),
        }
    }
}

/// 计费预留的持久化状态。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i16)]
pub enum QuotaReservationStatus {
    /// 已冻结预估额度，等待请求终态。
    Reserved = 1,
    /// 已固化实际额度，等待余额调整成功。
    SettlementPending = 2,
    /// 余额与累计用量已完成最终结算。
    Settled = 3,
    /// 预留额度已完整退还。
    Refunded = 4,
}

impl QuotaReservationStatus {
    pub(crate) const fn code(self) -> i16 {
        self as i16
    }

    fn from_code(code: i16) -> Result<Self, QuotaRepositoryError> {
        match code {
            1 => Ok(Self::Reserved),
            2 => Ok(Self::SettlementPending),
            3 => Ok(Self::Settled),
            4 => Ok(Self::Refunded),
            _ => Err(QuotaRepositoryError::Invariant),
        }
    }
}

/// 幂等额度变更的执行结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QuotaMutationOutcome {
    /// 本次调用完成了新的持久化状态转换。
    Applied,
    /// 相同幂等调用已经存在，未再次调整额度。
    Existing(QuotaReservationStatus),
}

/// 原子额度仓储错误；不携带主体标识、额度数值或底层数据库诊断。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum QuotaRepositoryError {
    /// 免费请求不得创建持久化预留。
    #[error("预扣额度必须大于零")]
    ZeroAmount,
    /// 仓储超时或预留期限配置无效。
    #[error("额度仓储配置无效")]
    InvalidConfiguration,
    /// 指定的计费预留不存在。
    #[error("计费预留不存在")]
    NotFound,
    /// 幂等键已绑定不同参数，或当前状态不允许该操作。
    #[error("计费预留状态冲突")]
    Conflict,
    /// 用户钱包不足以完成预扣或结算补扣。
    #[error("用户额度不足")]
    UserQuotaInsufficient,
    /// 组织计费扩展未注入；公共核心无法处理组织主体。
    #[error("组织额度扩展不可用")]
    ExtensionUnavailable,
    /// 企业钱包、分层预算或企业计费保护阻止本次预扣或补扣。
    #[error("企业额度不足或暂不可用")]
    OrganizationQuotaInsufficient,
    /// 有限令牌不足以完成预扣或结算补扣。
    #[error("令牌额度不足")]
    TokenQuotaInsufficient,
    /// 令牌 5h/1d/7d 窗口无法容纳本次预扣；可恢复时携带最长等待时间。
    #[error("令牌额度窗口不足")]
    TokenWindowQuotaInsufficient {
        retry_after: Option<QuotaWindowRetryAfter>,
    },
    /// 分组日、周、月共享窗口无法容纳本次预扣；可恢复时携带最长等待时间。
    #[error("分组额度窗口不足")]
    GroupWindowQuotaInsufficient {
        retry_after: Option<QuotaWindowRetryAfter>,
    },
    /// 批量任务实际额度超过创建任务时冻结的上限。
    #[error("批量任务实际额度超过冻结上限")]
    ActualExceedsReservation,
    /// 获取连接或执行非提交数据库操作失败。
    #[error("额度仓储数据库操作失败")]
    Query,
    /// 超时或提交失败使调用方无法确定事务终态。
    #[error("额度仓储操作结果未知")]
    OutcomeUnknown,
    /// 持久化记录、关联关系或计数器违反计费不变量。
    #[error("额度仓储持久化状态损坏")]
    Invariant,
}

pub(crate) struct ReservationState {
    pub(crate) user_id: i64,
    pub(crate) token_id: i64,
    pub(crate) group_id: i64,
    pub(crate) organization_id: Option<i64>,
    pub(crate) status: QuotaReservationStatus,
    pub(crate) reservation_kind: QuotaReservationKind,
    pub(crate) funding_source: QuotaFundingSource,
    pub(crate) reserved_quota: i64,
    pub(crate) token_reserved_quota: i64,
    pub(crate) actual_quota: Option<i64>,
    pub(crate) contract_price: Option<BillingContractPriceSnapshot>,
}

impl ReservationState {
    pub(crate) fn try_from_result(result: &QueryResult) -> Result<Self, QuotaRepositoryError> {
        let status = QuotaReservationStatus::from_code(get(result, "status")?)?;
        let organization_id: Option<i64> = get(result, "organization_id")?;
        let contract_price = contract_price_from_result(result, organization_id)?;
        let state = Self {
            user_id: get(result, "user_id")?,
            token_id: get(result, "token_id")?,
            group_id: get(result, "group_id")?,
            organization_id,
            status,
            reservation_kind: QuotaReservationKind::from_code(get(result, "reservation_kind")?)?,
            funding_source: QuotaFundingSource::from_code(get(result, "funding_source")?)?,
            reserved_quota: get(result, "reserved_quota")?,
            token_reserved_quota: get(result, "token_reserved_quota")?,
            actual_quota: get(result, "actual_quota")?,
            contract_price,
        };
        let finalized_at: Option<TimeDateTimeWithTimeZone> = get(result, "finalized_at")?;
        state.validate(finalized_at.is_some())?;
        Ok(state)
    }

    pub(crate) fn matches_precharge(
        &self,
        principal: GatewayPrincipal,
        amount: Quota,
        reservation_kind: QuotaReservationKind,
    ) -> bool {
        self.user_id == principal.user_id().get()
            && self.token_id == principal.token_id().get()
            && self.group_id == principal.group_id().get()
            && self.organization_id
                == principal
                    .organization_principal()
                    .map(|organization| organization.organization_id().get())
            && self.reservation_kind == reservation_kind
            && self.reserved_quota == amount.units()
    }

    pub(crate) fn matches_actual(&self, actual: Quota) -> bool {
        self.actual_quota == Some(actual.units())
    }

    /// 校验无锁预读期间 reservation 的主体与预扣快照没有被替换。
    pub(crate) fn matches_immutable_snapshot(&self, observed: &Self) -> bool {
        self.user_id == observed.user_id
            && self.token_id == observed.token_id
            && self.group_id == observed.group_id
            && self.organization_id == observed.organization_id
            && self.reservation_kind == observed.reservation_kind
            && self.funding_source == observed.funding_source
            && self.reserved_quota == observed.reserved_quota
            && self.token_reserved_quota == observed.token_reserved_quota
            && self.contract_price == observed.contract_price
    }

    fn validate(&self, finalized: bool) -> Result<(), QuotaRepositoryError> {
        let valid_funding = self.reservation_kind.allows_subscription()
            || self.funding_source == QuotaFundingSource::Wallet;
        let valid_actual = self.reservation_kind.allows_supplement()
            || self
                .actual_quota
                .is_none_or(|actual| actual <= self.reserved_quota);
        if self.user_id <= 0
            || self.token_id <= 0
            || self.group_id <= 0
            || self
                .organization_id
                .is_some_and(|organization_id| organization_id <= 0)
            || self.reserved_quota <= 0
            || (self.token_reserved_quota != 0 && self.token_reserved_quota != self.reserved_quota)
            || self.actual_quota.is_some_and(|value| value < 0)
            || self
                .contract_price
                .is_some_and(|price| self.organization_id != Some(price.organization_id().get()))
            || !valid_funding
            || !valid_actual
        {
            return Err(QuotaRepositoryError::Invariant);
        }

        let valid_state = match self.status {
            QuotaReservationStatus::Reserved => self.actual_quota.is_none() && !finalized,
            QuotaReservationStatus::SettlementPending => self.actual_quota.is_some() && !finalized,
            QuotaReservationStatus::Settled => self.actual_quota.is_some() && finalized,
            QuotaReservationStatus::Refunded => self.actual_quota.is_none() && finalized,
        };
        valid_state
            .then_some(())
            .ok_or(QuotaRepositoryError::Invariant)
    }
}

#[derive(Clone, Copy)]
pub(crate) struct GroupQuotaState {
    pub(crate) group_id: i64,
    pub(crate) daily_limit: Option<i64>,
    pub(crate) weekly_limit: Option<i64>,
    pub(crate) monthly_limit: Option<i64>,
    pub(crate) daily_usage: i64,
    pub(crate) weekly_usage: i64,
    pub(crate) monthly_usage: i64,
    pub(crate) daily_window_start: TimeDateTimeWithTimeZone,
    pub(crate) weekly_window_start: TimeDateTimeWithTimeZone,
    pub(crate) monthly_window_start: TimeDateTimeWithTimeZone,
}

impl GroupQuotaState {
    pub(crate) fn try_from_result(result: &QueryResult) -> Result<Self, QuotaRepositoryError> {
        let state = Self {
            group_id: get(result, "id")?,
            daily_limit: get(result, "daily_limit")?,
            weekly_limit: get(result, "weekly_limit")?,
            monthly_limit: get(result, "monthly_limit")?,
            daily_usage: get(result, "daily_usage")?,
            weekly_usage: get(result, "weekly_usage")?,
            monthly_usage: get(result, "monthly_usage")?,
            daily_window_start: get(result, "daily_window_start")?,
            weekly_window_start: get(result, "weekly_window_start")?,
            monthly_window_start: get(result, "monthly_window_start")?,
        };
        if state.group_id <= 0
            || [state.daily_limit, state.weekly_limit, state.monthly_limit]
                .into_iter()
                .flatten()
                .any(|limit| limit < 0)
            || [state.daily_usage, state.weekly_usage, state.monthly_usage]
                .into_iter()
                .any(|usage| usage < 0)
            || [
                state.daily_window_start,
                state.weekly_window_start,
                state.monthly_window_start,
            ]
            .into_iter()
            .any(|started_at| started_at.unix_timestamp() < 0)
        {
            return Err(QuotaRepositoryError::Invariant);
        }
        Ok(state)
    }
}

fn contract_price_from_result(
    result: &QueryResult,
    organization_id: Option<i64>,
) -> Result<Option<BillingContractPriceSnapshot>, QuotaRepositoryError> {
    let id: Option<i64> = get(result, "contract_price_id")?;
    let version: Option<i64> = get(result, "contract_price_version")?;
    let prices = [
        get::<Option<Decimal>>(result, "contract_input_price")?,
        get::<Option<Decimal>>(result, "contract_output_price")?,
        get::<Option<Decimal>>(result, "contract_cache_read_price")?,
        get::<Option<Decimal>>(result, "contract_cache_creation_5m_price")?,
        get::<Option<Decimal>>(result, "contract_cache_creation_1h_price")?,
    ];
    match (id, version, prices) {
        (None, None, [None, None, None, None, None]) => Ok(None),
        (
            Some(id),
            Some(version),
            [
                Some(input),
                Some(output),
                Some(cache_read),
                Some(cache_5m),
                Some(cache_1h),
            ],
        ) => BillingContractPriceSnapshot::from_persistence_parts(
            organization_id.ok_or(QuotaRepositoryError::Invariant)?,
            id,
            version,
            [input, output, cache_read, cache_5m, cache_1h],
        )
        .map(Some)
        .map_err(|_| QuotaRepositoryError::Invariant),
        _ => Err(QuotaRepositoryError::Invariant),
    }
}

pub(crate) struct UserQuotaState {
    pub(crate) quota: i64,
    pub(crate) used_quota: i64,
    pub(crate) frozen_quota: i64,
    pub(crate) request_count: i64,
}

impl UserQuotaState {
    pub(crate) fn try_from_result(result: &QueryResult) -> Result<Self, QuotaRepositoryError> {
        let state = Self {
            quota: get(result, "quota")?,
            used_quota: get(result, "used_quota")?,
            frozen_quota: get(result, "frozen_quota")?,
            request_count: get(result, "request_count")?,
        };
        if state.quota < 0
            || state.used_quota < 0
            || state.frozen_quota < 0
            || state.request_count < 0
        {
            return Err(QuotaRepositoryError::Invariant);
        }
        Ok(state)
    }
}

#[derive(Clone, Copy)]
pub(crate) struct TokenQuotaState {
    pub(crate) user_id: i64,
    pub(crate) organization_id: Option<i64>,
    pub(crate) organization_membership_id: Option<i64>,
    pub(crate) organization_team_id: Option<i64>,
    pub(crate) remain_quota: i64,
    pub(crate) unlimited_quota: bool,
    pub(crate) used_quota: i64,
    pub(crate) rate_limit_5h: Option<i64>,
    pub(crate) rate_limit_1d: Option<i64>,
    pub(crate) rate_limit_7d: Option<i64>,
    pub(crate) usage_5h: i64,
    pub(crate) usage_1d: i64,
    pub(crate) usage_7d: i64,
    pub(crate) window_5h_start: TimeDateTimeWithTimeZone,
    pub(crate) window_1d_start: TimeDateTimeWithTimeZone,
    pub(crate) window_7d_start: TimeDateTimeWithTimeZone,
}

impl TokenQuotaState {
    pub(crate) fn try_from_result(result: &QueryResult) -> Result<Self, QuotaRepositoryError> {
        let state = Self {
            user_id: get(result, "user_id")?,
            organization_id: get(result, "organization_id")?,
            organization_membership_id: get(result, "organization_membership_id")?,
            organization_team_id: get(result, "organization_team_id")?,
            remain_quota: get(result, "remain_quota")?,
            unlimited_quota: get(result, "unlimited_quota")?,
            used_quota: get(result, "used_quota")?,
            rate_limit_5h: get(result, "rate_limit_5h")?,
            rate_limit_1d: get(result, "rate_limit_1d")?,
            rate_limit_7d: get(result, "rate_limit_7d")?,
            usage_5h: get(result, "usage_5h")?,
            usage_1d: get(result, "usage_1d")?,
            usage_7d: get(result, "usage_7d")?,
            window_5h_start: get(result, "window_5h_start")?,
            window_1d_start: get(result, "window_1d_start")?,
            window_7d_start: get(result, "window_7d_start")?,
        };
        if state.user_id <= 0
            || state.organization_id.is_some_and(|value| value <= 0)
            || state
                .organization_membership_id
                .is_some_and(|value| value <= 0)
            || state.organization_team_id.is_some_and(|value| value <= 0)
            || (state.organization_id.is_none()
                && (state.organization_membership_id.is_some()
                    || state.organization_team_id.is_some()))
            || (state.organization_id.is_some() && state.organization_membership_id.is_none())
            || state.remain_quota < 0
            || state.used_quota < 0
            || [
                state.rate_limit_5h,
                state.rate_limit_1d,
                state.rate_limit_7d,
            ]
            .into_iter()
            .flatten()
            .any(|limit| limit < 0)
            || [state.usage_5h, state.usage_1d, state.usage_7d]
                .into_iter()
                .any(|usage| usage < 0)
            || [
                state.window_5h_start,
                state.window_1d_start,
                state.window_7d_start,
            ]
            .into_iter()
            .any(|started_at| started_at.unix_timestamp() < 0)
        {
            return Err(QuotaRepositoryError::Invariant);
        }
        Ok(state)
    }
}

fn get<T>(result: &QueryResult, column: &str) -> Result<T, QuotaRepositoryError>
where
    T: sea_orm::TryGetable,
{
    result
        .try_get("", column)
        .map_err(|_| QuotaRepositoryError::Invariant)
}
