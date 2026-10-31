use std::{fmt, future::Future, pin::Pin};

use af_db::{
    PasskeyAdminRevokeOutcome, PasskeyRepository, PasskeyRepositoryError, UserProfileRepository,
    UserProfileRepositoryError, UserTwoFactorMutationOutcome,
};
use af_domain::UserId;
use thiserror::Error;
use time::OffsetDateTime;

use crate::{SessionPrincipal, SessionRole, UserPasskey, UserTwoFactorStatus};

/// 管理端用户安全设置的脱敏快照。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminUserSecuritySnapshot {
    user_id: UserId,
    two_factor: UserTwoFactorStatus,
    passkeys: Vec<UserPasskey>,
}

impl AdminUserSecuritySnapshot {
    fn new(user_id: UserId, two_factor: UserTwoFactorStatus, passkeys: Vec<UserPasskey>) -> Self {
        Self {
            user_id,
            two_factor,
            passkeys,
        }
    }

    /// 返回目标用户标识。
    #[must_use]
    pub const fn user_id(&self) -> UserId {
        self.user_id
    }

    /// 返回 TOTP 状态；不包含密钥、二维码或备份码。
    #[must_use]
    pub const fn two_factor(&self) -> UserTwoFactorStatus {
        self.two_factor
    }

    /// 返回脱敏 Passkey 目录。
    #[must_use]
    pub fn passkeys(&self) -> &[UserPasskey] {
        &self.passkeys
    }
}

/// 管理端用户安全操作错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AdminUserSecurityError {
    /// 当前主体不是管理员。
    #[error("管理员权限不足")]
    Forbidden,
    /// 目标用户或凭证不存在。
    #[error("安全目标不存在")]
    NotFound,
    /// 数据库或持久化状态失败。
    #[error("用户安全设置内部失败")]
    Internal,
}

/// 管理端用户安全读取 Future。
pub type AdminUserSecurityReadFuture<'a> = Pin<
    Box<dyn Future<Output = Result<AdminUserSecuritySnapshot, AdminUserSecurityError>> + Send + 'a>,
>;

/// 管理端用户安全写入 Future。
pub type AdminUserSecurityWriteFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), AdminUserSecurityError>> + Send + 'a>>;

/// 管理员用户安全设置应用端口。
pub trait AdminUserSecurityService: Send + Sync {
    /// 读取目标用户的 TOTP 时间状态和 Passkey 目录。
    fn get<'a>(
        &'a self,
        principal: SessionPrincipal,
        user_id: UserId,
    ) -> AdminUserSecurityReadFuture<'a>;

    /// 清理目标用户 TOTP 与待绑定状态，并撤销旧会话。
    fn reset_two_factor<'a>(
        &'a self,
        principal: SessionPrincipal,
        user_id: UserId,
    ) -> AdminUserSecurityReadFuture<'a>;

    /// 撤销目标用户指定 Passkey，并撤销旧会话。
    fn revoke_passkey<'a>(
        &'a self,
        principal: SessionPrincipal,
        user_id: UserId,
        passkey_id: i64,
    ) -> AdminUserSecurityWriteFuture<'a>;
}

/// 使用数据库仓储的管理员用户安全服务。
#[derive(Clone)]
pub struct DatabaseAdminUserSecurityService {
    user_profile_repository: UserProfileRepository,
    passkey_repository: PasskeyRepository,
}

impl DatabaseAdminUserSecurityService {
    /// 使用已有用户资料和 Passkey 仓储构造服务。
    #[must_use]
    pub const fn new(
        user_profile_repository: UserProfileRepository,
        passkey_repository: PasskeyRepository,
    ) -> Self {
        Self {
            user_profile_repository,
            passkey_repository,
        }
    }

    async fn read_snapshot(
        &self,
        user_id: UserId,
    ) -> Result<AdminUserSecuritySnapshot, AdminUserSecurityError> {
        let state = self
            .user_profile_repository
            .admin_two_factor_state(user_id)
            .await
            .map_err(map_profile_error)?
            .ok_or(AdminUserSecurityError::NotFound)?;
        let now = OffsetDateTime::now_utc();
        let pending_expired = state
            .pending_expires_at
            .is_some_and(|expires_at| expires_at <= now);
        let status = UserTwoFactorStatus::new(
            state.enabled,
            (!pending_expired)
                .then_some(state.pending_created_at)
                .flatten()
                .and_then(unix_timestamp),
            (!pending_expired)
                .then_some(state.pending_expires_at)
                .flatten()
                .and_then(unix_timestamp),
            state.bound_at.and_then(unix_timestamp),
            state.last_verified_at.and_then(unix_timestamp),
        );
        let passkeys = self
            .passkey_repository
            .list(user_id)
            .await
            .map_err(map_passkey_error)?
            .into_iter()
            .map(|record| {
                UserPasskey::new(
                    record.id(),
                    record.display_name().to_owned(),
                    record.created_at().unix_timestamp(),
                    record.last_used_at().map(|value| value.unix_timestamp()),
                    record.revoked_at().map(|value| value.unix_timestamp()),
                )
            })
            .collect();
        Ok(AdminUserSecuritySnapshot::new(user_id, status, passkeys))
    }
}

impl fmt::Debug for DatabaseAdminUserSecurityService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseAdminUserSecurityService(<redacted>)")
    }
}

impl AdminUserSecurityService for DatabaseAdminUserSecurityService {
    fn get<'a>(
        &'a self,
        principal: SessionPrincipal,
        user_id: UserId,
    ) -> AdminUserSecurityReadFuture<'a> {
        Box::pin(async move {
            ensure_admin(principal)?;
            self.read_snapshot(user_id).await
        })
    }

    fn reset_two_factor<'a>(
        &'a self,
        principal: SessionPrincipal,
        user_id: UserId,
    ) -> AdminUserSecurityReadFuture<'a> {
        Box::pin(async move {
            ensure_admin(principal)?;
            match self
                .user_profile_repository
                .admin_reset_two_factor(user_id)
                .await
                .map_err(map_profile_error)?
            {
                UserTwoFactorMutationOutcome::NotFound => Err(AdminUserSecurityError::NotFound),
                UserTwoFactorMutationOutcome::Updated
                | UserTwoFactorMutationOutcome::NotEnabled => self.read_snapshot(user_id).await,
                UserTwoFactorMutationOutcome::Rejected
                | UserTwoFactorMutationOutcome::AlreadyEnabled => {
                    Err(AdminUserSecurityError::Internal)
                }
            }
        })
    }

    fn revoke_passkey<'a>(
        &'a self,
        principal: SessionPrincipal,
        user_id: UserId,
        passkey_id: i64,
    ) -> AdminUserSecurityWriteFuture<'a> {
        Box::pin(async move {
            ensure_admin(principal)?;
            match self
                .passkey_repository
                .admin_revoke(user_id, passkey_id, af_db::DatabaseTimestamp::now_utc())
                .await
                .map_err(map_passkey_error)?
            {
                PasskeyAdminRevokeOutcome::Revoked => Ok(()),
                PasskeyAdminRevokeOutcome::NotFound | PasskeyAdminRevokeOutcome::AlreadyRevoked => {
                    Err(AdminUserSecurityError::NotFound)
                }
            }
        })
    }
}

fn ensure_admin(principal: SessionPrincipal) -> Result<(), AdminUserSecurityError> {
    if principal.role() == SessionRole::Admin {
        Ok(())
    } else {
        Err(AdminUserSecurityError::Forbidden)
    }
}

fn unix_timestamp(value: af_db::DatabaseTimestamp) -> Option<u64> {
    u64::try_from(value.unix_timestamp()).ok()
}

fn map_profile_error(error: UserProfileRepositoryError) -> AdminUserSecurityError {
    match error {
        UserProfileRepositoryError::Query
        | UserProfileRepositoryError::Timeout
        | UserProfileRepositoryError::Invariant
        | UserProfileRepositoryError::Conflict
        | UserProfileRepositoryError::Entropy => AdminUserSecurityError::Internal,
    }
}

fn map_passkey_error(error: PasskeyRepositoryError) -> AdminUserSecurityError {
    match error {
        PasskeyRepositoryError::Query
        | PasskeyRepositoryError::Timeout
        | PasskeyRepositoryError::Invariant
        | PasskeyRepositoryError::Conflict
        | PasskeyRepositoryError::InvalidConfiguration
        | PasskeyRepositoryError::NotFound
        | PasskeyRepositoryError::Expired
        | PasskeyRepositoryError::Consumed => AdminUserSecurityError::Internal,
    }
}
