use std::{fmt, time::Duration};

use af_domain::{Quota, UserId};
use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool,
    entity::{invite_rebate_events, users},
};

const ENABLED_USER_STATUS: i16 = 1;
const RECENT_REBATE_LIMIT: u64 = 10;

/// 用户邀请中心展示的一条脱敏返利到账事实。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserInvitationRebateRecord {
    quota_amount: Quota,
    credited_at: u64,
}

impl UserInvitationRebateRecord {
    /// 返回本次邀请返利到账额度。
    #[must_use]
    pub const fn quota_amount(self) -> Quota {
        self.quota_amount
    }

    /// 返回到账时间的 Unix 秒数。
    #[must_use]
    pub const fn credited_at(self) -> u64 {
        self.credited_at
    }
}

/// 当前用户自己的邀请统计与脱敏返利记录。
pub struct UserInvitationSummaryRecord {
    invite_code: String,
    invited_count: u64,
    credited_count: u64,
    current_rebate_quota: Quota,
    historical_rebate_quota: Quota,
    recent_rebates: Vec<UserInvitationRebateRecord>,
}

impl UserInvitationSummaryRecord {
    /// 返回当前用户不可枚举身份信息的随机邀请码。
    #[must_use]
    pub fn invite_code(&self) -> &str {
        &self.invite_code
    }

    /// 返回历史上绑定当前邀请人的注册用户数。
    #[must_use]
    pub const fn invited_count(&self) -> u64 {
        self.invited_count
    }

    /// 返回已经产生返利到账事件的邀请数。
    #[must_use]
    pub const fn credited_count(&self) -> u64 {
        self.credited_count
    }

    /// 返回当前返利累计额度。
    #[must_use]
    pub const fn current_rebate_quota(&self) -> Quota {
        self.current_rebate_quota
    }

    /// 返回历史返利累计额度。
    #[must_use]
    pub const fn historical_rebate_quota(&self) -> Quota {
        self.historical_rebate_quota
    }

    /// 返回最多十条最近返利，不含被邀请用户身份。
    #[must_use]
    pub fn recent_rebates(&self) -> &[UserInvitationRebateRecord] {
        &self.recent_rebates
    }
}

impl fmt::Debug for UserInvitationSummaryRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("UserInvitationSummaryRecord(<redacted>)")
    }
}

/// 邀请汇总查询结果；不存在、禁用和软删除用户统一视为会话失效。
pub enum UserInvitationLookupOutcome {
    /// 返回当前用户自己的邀请汇总。
    Found(UserInvitationSummaryRecord),
    /// 当前用户已经不可用。
    NotFound,
}

impl fmt::Debug for UserInvitationLookupOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Found(_) => formatter.write_str("UserInvitationLookupOutcome::Found(<redacted>)"),
            Self::NotFound => formatter.write_str("UserInvitationLookupOutcome::NotFound"),
        }
    }
}

/// 用户邀请读仓储构造错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum UserInvitationRepositoryConfigError {
    /// 零超时无法形成有效的数据库读取截止时间。
    #[error("用户邀请查询超时必须大于零")]
    ZeroOperationTimeout,
}

/// 用户邀请读仓储的闭合内部错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum UserInvitationRepositoryError {
    /// 数据库查询失败。
    #[error("用户邀请数据库查询失败")]
    Query,
    /// 数据库查询超过硬截止时间。
    #[error("用户邀请数据库查询超时")]
    Timeout,
    /// 用户、计数或返利事实违反持久化不变量。
    #[error("用户邀请持久化状态损坏")]
    Invariant,
}

/// 只读取当前用户邀请汇总的数据库仓储。
#[derive(Clone)]
pub struct UserInvitationRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl UserInvitationRepository {
    /// 使用共享连接池和单次读取截止时间构造仓储。
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, UserInvitationRepositoryConfigError> {
        if operation_timeout.is_zero() {
            return Err(UserInvitationRepositoryConfigError::ZeroOperationTimeout);
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }

    /// 按当前会话用户标识读取自己的邀请统计与最近到账记录。
    pub async fn get(
        &self,
        user_id: UserId,
    ) -> Result<UserInvitationLookupOutcome, UserInvitationRepositoryError> {
        match timeout(self.operation_timeout, self.get_inner(user_id)).await {
            Ok(result) => result.map_err(record_internal_error),
            Err(_) => Err(record_internal_error(
                UserInvitationRepositoryError::Timeout,
            )),
        }
    }

    async fn get_inner(
        &self,
        user_id: UserId,
    ) -> Result<UserInvitationLookupOutcome, UserInvitationRepositoryError> {
        let connection = self.pool.connection();
        let Some(user) = users::Entity::find_by_id(user_id.get())
            .filter(users::Column::Status.eq(ENABLED_USER_STATUS))
            .filter(users::Column::DeletedAt.is_null())
            .one(connection)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| UserInvitationRepositoryError::Query)?
        else {
            return Ok(UserInvitationLookupOutcome::NotFound);
        };

        // 先取最近记录再取计数，注册事务并发提交时只会短暂少展示，不会让明细超过计数。
        let recent_models = invite_rebate_events::Entity::find()
            .filter(invite_rebate_events::Column::InviterUserId.eq(user_id.get()))
            .order_by_desc(invite_rebate_events::Column::CreditedAt)
            .order_by_desc(invite_rebate_events::Column::Id)
            .limit(RECENT_REBATE_LIMIT)
            .all(connection)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| UserInvitationRepositoryError::Query)?;
        let recent_rebates = recent_models
            .into_iter()
            .map(|model| rebate_record(user_id, model))
            .collect::<Result<Vec<_>, _>>()?;
        let credited_count = invite_rebate_events::Entity::find()
            .filter(invite_rebate_events::Column::InviterUserId.eq(user_id.get()))
            .count(connection)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| UserInvitationRepositoryError::Query)?;
        let invited_count = users::Entity::find()
            .filter(users::Column::InviterId.eq(user_id.get()))
            .count(connection)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| UserInvitationRepositoryError::Query)?;

        let current_rebate_quota =
            Quota::new(user.aff_quota).map_err(|_| UserInvitationRepositoryError::Invariant)?;
        let historical_rebate_quota = Quota::new(user.aff_history_quota)
            .map_err(|_| UserInvitationRepositoryError::Invariant)?;
        let recent_count = u64::try_from(recent_rebates.len())
            .map_err(|_| UserInvitationRepositoryError::Invariant)?;
        if !valid_invite_code(&user.aff_code)
            || current_rebate_quota > historical_rebate_quota
            || credited_count > invited_count
            || recent_count > credited_count
        {
            return Err(UserInvitationRepositoryError::Invariant);
        }

        Ok(UserInvitationLookupOutcome::Found(
            UserInvitationSummaryRecord {
                invite_code: user.aff_code,
                invited_count,
                credited_count,
                current_rebate_quota,
                historical_rebate_quota,
                recent_rebates,
            },
        ))
    }
}

impl fmt::Debug for UserInvitationRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("UserInvitationRepository")
            .field("operation_timeout", &self.operation_timeout)
            .finish_non_exhaustive()
    }
}

fn rebate_record(
    expected_inviter: UserId,
    model: invite_rebate_events::Model,
) -> Result<UserInvitationRebateRecord, UserInvitationRepositoryError> {
    let quota_amount =
        Quota::new(model.quota_amount).map_err(|_| UserInvitationRepositoryError::Invariant)?;
    let credited_at = u64::try_from(model.credited_at.unix_timestamp())
        .map_err(|_| UserInvitationRepositoryError::Invariant)?;
    if model.id <= 0
        || model.inviter_user_id != expected_inviter.get()
        || model.invitee_user_id <= 0
        || model.invitee_user_id == model.inviter_user_id
        || quota_amount.is_zero()
        || model.created_at < model.credited_at
    {
        return Err(UserInvitationRepositoryError::Invariant);
    }
    Ok(UserInvitationRebateRecord {
        quota_amount,
        credited_at,
    })
}

fn valid_invite_code(value: &str) -> bool {
    value.len() == 25
        && value.starts_with("af-")
        && value[3..]
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn record_internal_error(error: UserInvitationRepositoryError) -> UserInvitationRepositoryError {
    tracing::error!(
        target: "af_db::invitation",
        error_kind = match error {
            UserInvitationRepositoryError::Query => "user_invitation_query",
            UserInvitationRepositoryError::Timeout => "user_invitation_timeout",
            UserInvitationRepositoryError::Invariant => "user_invitation_invariant",
        },
        "用户邀请仓储操作失败"
    );
    error
}
