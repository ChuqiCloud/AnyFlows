use std::{fmt, future::Future, pin::Pin};

use af_db::{
    UserInvitationLookupOutcome, UserInvitationRepository, UserInvitationRepositoryError,
    UserInvitationSummaryRecord,
};
use af_domain::Quota;
use thiserror::Error;

use crate::SessionPrincipal;

/// 当前用户邀请中心展示的一条脱敏到账记录。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserInvitationRebate {
    quota_amount: Quota,
    credited_at: u64,
}

impl UserInvitationRebate {
    /// 组合已经通过业务校验的脱敏到账事实，供端口适配器与测试实现使用。
    #[must_use]
    pub const fn from_parts(quota_amount: Quota, credited_at: u64) -> Self {
        Self {
            quota_amount,
            credited_at,
        }
    }

    /// 返回本次到账额度。
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

/// 当前会话用户自己的邀请汇总。
pub struct UserInvitationSummary {
    invite_code: String,
    invited_count: u64,
    credited_count: u64,
    current_rebate_quota: Quota,
    historical_rebate_quota: Quota,
    recent_rebates: Vec<UserInvitationRebate>,
}

impl UserInvitationSummary {
    /// 组合已经通过业务校验的当前用户邀请汇总，供端口替代实现使用。
    #[allow(clippy::too_many_arguments, reason = "字段与邀请中心稳定响应一一对应")]
    #[must_use]
    pub fn from_parts(
        invite_code: String,
        invited_count: u64,
        credited_count: u64,
        current_rebate_quota: Quota,
        historical_rebate_quota: Quota,
        recent_rebates: Vec<UserInvitationRebate>,
    ) -> Self {
        Self {
            invite_code,
            invited_count,
            credited_count,
            current_rebate_quota,
            historical_rebate_quota,
            recent_rebates,
        }
    }

    /// 返回当前用户邀请码。
    #[must_use]
    pub fn invite_code(&self) -> &str {
        &self.invite_code
    }

    /// 返回已建立邀请关系的注册用户数。
    #[must_use]
    pub const fn invited_count(&self) -> u64 {
        self.invited_count
    }

    /// 返回已产生返利事件的邀请数。
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

    /// 返回不包含被邀请用户身份的最近到账记录。
    #[must_use]
    pub fn recent_rebates(&self) -> &[UserInvitationRebate] {
        &self.recent_rebates
    }
}

impl fmt::Debug for UserInvitationSummary {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("UserInvitationSummary(<redacted>)")
    }
}

/// 当前用户邀请中心应用错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum UserInvitationError {
    /// 当前会话对应用户已经不可用。
    #[error("邀请中心会话无效")]
    InvalidSession,
    /// 数据库查询或持久化状态失败。
    #[error("邀请中心内部失败")]
    Internal,
}

/// 读取当前用户邀请汇总的对象安全 Future。
pub type UserInvitationReadFuture<'a> =
    Pin<Box<dyn Future<Output = Result<UserInvitationSummary, UserInvitationError>> + Send + 'a>>;

/// 当前用户邀请中心应用端口；主体只能来自已验证登录会话。
pub trait UserInvitationService: Send + Sync {
    /// 读取当前会话用户自己的邀请码、统计和脱敏到账记录。
    fn get<'a>(&'a self, principal: SessionPrincipal) -> UserInvitationReadFuture<'a>;
}

/// 使用独立邀请读仓储的生产应用服务。
pub struct DatabaseUserInvitationService {
    repository: UserInvitationRepository,
}

impl DatabaseUserInvitationService {
    /// 绑定已经配置截止时间的邀请读仓储。
    #[must_use]
    pub const fn new(repository: UserInvitationRepository) -> Self {
        Self { repository }
    }
}

impl UserInvitationService for DatabaseUserInvitationService {
    fn get<'a>(&'a self, principal: SessionPrincipal) -> UserInvitationReadFuture<'a> {
        Box::pin(async move {
            match self
                .repository
                .get(principal.user_id())
                .await
                .map_err(map_repository_error)?
            {
                UserInvitationLookupOutcome::Found(record) => Ok(summary_from_record(record)),
                UserInvitationLookupOutcome::NotFound => Err(UserInvitationError::InvalidSession),
            }
        })
    }
}

impl fmt::Debug for DatabaseUserInvitationService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseUserInvitationService(<redacted>)")
    }
}

fn summary_from_record(record: UserInvitationSummaryRecord) -> UserInvitationSummary {
    UserInvitationSummary {
        invite_code: record.invite_code().to_owned(),
        invited_count: record.invited_count(),
        credited_count: record.credited_count(),
        current_rebate_quota: record.current_rebate_quota(),
        historical_rebate_quota: record.historical_rebate_quota(),
        recent_rebates: record
            .recent_rebates()
            .iter()
            .map(|rebate| UserInvitationRebate {
                quota_amount: rebate.quota_amount(),
                credited_at: rebate.credited_at(),
            })
            .collect(),
    }
}

fn map_repository_error(error: UserInvitationRepositoryError) -> UserInvitationError {
    match error {
        UserInvitationRepositoryError::Query
        | UserInvitationRepositoryError::Timeout
        | UserInvitationRepositoryError::Invariant => UserInvitationError::Internal,
    }
}
