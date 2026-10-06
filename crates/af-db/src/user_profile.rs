use std::{fmt, time::Duration};

use af_domain::{Quota, UserId};
use base64::Engine as _;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseTransaction, DbBackend, DbErr,
    EntityTrait, IntoActiveModel, QueryFilter, QuerySelect, Set, SqlErr, TransactionTrait,
    sea_query::{Expr, LockType},
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};
use zeroize::Zeroizing;

use crate::{
    DatabasePool, EncryptedCredentialEnvelope,
    entity::users,
    identity_secret::{IdentitySecretError, hash_password},
};

/// 普通用户资料允许使用的用户名最大字节数。
pub const MAX_USER_PROFILE_USERNAME_BYTES: usize = 64;

/// 个人资料中可返回的通知偏好。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserNotificationPreferencesRecord {
    email_product_updates: bool,
    email_usage_alerts: bool,
    balance_alert_threshold: Option<Quota>,
}

impl UserNotificationPreferencesRecord {
    /// 组合已经通过持久化边界校验的通知偏好。
    #[must_use]
    pub const fn new(
        email_product_updates: bool,
        email_usage_alerts: bool,
        balance_alert_threshold: Option<Quota>,
    ) -> Self {
        Self {
            email_product_updates,
            email_usage_alerts,
            balance_alert_threshold,
        }
    }

    /// 是否接收产品更新邮件。
    #[must_use]
    pub const fn email_product_updates(self) -> bool {
        self.email_product_updates
    }

    /// 是否接收用量提醒邮件。
    #[must_use]
    pub const fn email_usage_alerts(self) -> bool {
        self.email_usage_alerts
    }

    /// 返回个人余额预警阈值；空值表示继承系统默认值。
    #[must_use]
    pub const fn balance_alert_threshold(self) -> Option<Quota> {
        self.balance_alert_threshold
    }
}

/// 当前登录用户的非敏感资料快照；不包含密码、令牌、余额或内部设置。
#[derive(Clone, PartialEq, Eq)]
pub struct UserProfileRecord {
    user_id: UserId,
    username: String,
    email: Option<String>,
    role: i16,
    notifications: UserNotificationPreferencesRecord,
}

impl UserProfileRecord {
    /// 返回稳定用户标识。
    #[must_use]
    pub const fn user_id(&self) -> UserId {
        self.user_id
    }

    /// 返回登录用户名。
    #[must_use]
    pub fn username(&self) -> &str {
        &self.username
    }

    /// 返回已保存邮箱。
    #[must_use]
    pub fn email(&self) -> Option<&str> {
        self.email.as_deref()
    }

    /// 返回数据库角色编码，由上层转换为会话角色。
    #[must_use]
    pub const fn role(&self) -> i16 {
        self.role
    }

    /// 返回通知偏好快照。
    #[must_use]
    pub const fn notifications(&self) -> UserNotificationPreferencesRecord {
        self.notifications
    }
}

impl fmt::Debug for UserProfileRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("UserProfileRecord(<redacted>)")
    }
}

/// 用户资料查询结果；不存在与已软删除用户统一视为未找到。
pub enum UserProfileLookupOutcome {
    /// 返回当前启用用户的资料快照。
    Found(UserProfileRecord),
    /// 用户不存在、被禁用或已软删除。
    NotFound,
}

impl fmt::Debug for UserProfileLookupOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Found(_) => formatter.write_str("UserProfileLookupOutcome::Found(<redacted>)"),
            Self::NotFound => formatter.write_str("UserProfileLookupOutcome::NotFound"),
        }
    }
}

/// 资料或通知偏好写入结果。
pub enum UserProfileMutationOutcome {
    /// 已写入并返回最新资料快照。
    Updated(UserProfileRecord),
    /// 用户不存在、被禁用或已软删除。
    NotFound,
    /// 用户名或邮箱与其他有效用户冲突。
    Conflict,
}

impl fmt::Debug for UserProfileMutationOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Updated(_) => {
                formatter.write_str("UserProfileMutationOutcome::Updated(<redacted>)")
            }
            Self::NotFound => formatter.write_str("UserProfileMutationOutcome::NotFound"),
            Self::Conflict => formatter.write_str("UserProfileMutationOutcome::Conflict"),
        }
    }
}

/// 修改密码的结果；错误密码不回显用户存在性。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserPasswordChangeOutcome {
    /// 密码已更新且会话版本已递增。
    Updated(UserId),
    /// 用户不存在、被禁用或已软删除。
    NotFound,
    /// 当前密码错误或账户没有可用密码。
    Rejected,
}

/// 用户 TOTP 配置写入结果；配置密文只在仓储边界内出现。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserTwoFactorMutationOutcome {
    /// 已写入并递增会话版本。
    Updated,
    /// 用户不存在、被禁用或已软删除。
    NotFound,
    /// 当前密码错误或账户没有密码。
    Rejected,
    /// 启用状态已经存在。
    AlreadyEnabled,
    /// 启用状态不存在。
    NotEnabled,
}

/// 用户资料仓储配置错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum UserProfileRepositoryConfigError {
    /// 零超时无法形成有效操作截止时间。
    #[error("用户资料操作超时必须大于零")]
    ZeroOperationTimeout,
}

/// 用户资料仓储内部错误；不携带用户名、邮箱或密码。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum UserProfileRepositoryError {
    /// 数据库查询或事务执行失败。
    #[error("用户资料数据库操作失败")]
    Query,
    /// 数据库操作超过硬截止时间。
    #[error("用户资料数据库操作超时")]
    Timeout,
    /// 持久化结果违反用户资料不变量。
    #[error("用户资料持久化状态损坏")]
    Invariant,
    /// 用户名与当前有效用户冲突。
    #[error("用户资料用户名冲突")]
    Conflict,
    /// 系统随机源或 Argon2id 输出不可用。
    #[error("用户资料密码哈希失败")]
    Entropy,
}

/// 当前登录用户资料、通知偏好和密码修改共用的数据库仓储。
#[derive(Clone)]
pub struct UserProfileRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl UserProfileRepository {
    /// 使用共享数据库连接池和有界操作截止时间构造仓储。
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, UserProfileRepositoryConfigError> {
        if operation_timeout.is_zero() {
            return Err(UserProfileRepositoryConfigError::ZeroOperationTimeout);
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }

    /// 读取当前启用用户的资料快照。
    pub async fn get(
        &self,
        user_id: UserId,
    ) -> Result<UserProfileLookupOutcome, UserProfileRepositoryError> {
        match timeout(self.operation_timeout, self.get_inner(user_id)).await {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(UserProfileRepositoryError::Timeout)),
        }
    }

    /// 在用户边界内更新登录用户名。
    pub async fn update_username(
        &self,
        user_id: UserId,
        username: String,
    ) -> Result<UserProfileMutationOutcome, UserProfileRepositoryError> {
        match timeout(
            self.operation_timeout,
            self.update_username_inner(user_id, username),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(UserProfileRepositoryError::Timeout)),
        }
    }

    /// 在邮箱验证码已经由应用层消费后更新当前用户邮箱。
    pub async fn update_email(
        &self,
        user_id: UserId,
        email: String,
    ) -> Result<UserProfileMutationOutcome, UserProfileRepositoryError> {
        match timeout(
            self.operation_timeout,
            self.update_email_inner(user_id, email),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(UserProfileRepositoryError::Timeout)),
        }
    }

    /// 更新当前用户的通知偏好，不触碰密码、额度或会话版本。
    pub async fn update_notifications(
        &self,
        user_id: UserId,
        notifications: UserNotificationPreferencesRecord,
    ) -> Result<UserProfileMutationOutcome, UserProfileRepositoryError> {
        if notifications
            .balance_alert_threshold()
            .is_some_and(|threshold| threshold.is_zero())
        {
            return Err(record_internal_error(UserProfileRepositoryError::Invariant));
        }
        match timeout(
            self.operation_timeout,
            self.update_notifications_inner(user_id, notifications),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(UserProfileRepositoryError::Timeout)),
        }
    }

    /// 校验当前密码后更新 Argon2id 哈希，并递增会话版本撤销旧 JWT。
    pub async fn change_password(
        &self,
        user_id: UserId,
        current_password: &[u8],
        new_password: Zeroizing<String>,
    ) -> Result<UserPasswordChangeOutcome, UserProfileRepositoryError> {
        match timeout(
            self.operation_timeout,
            self.change_password_inner(user_id, current_password, new_password),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(UserProfileRepositoryError::Timeout)),
        }
    }

    /// 校验当前密码后写入 TOTP 密文，并撤销全部既有会话。
    pub async fn enable_two_factor(
        &self,
        user_id: UserId,
        current_password: &[u8],
        encrypted_secret: EncryptedCredentialEnvelope,
    ) -> Result<UserTwoFactorMutationOutcome, UserProfileRepositoryError> {
        match timeout(
            self.operation_timeout,
            self.enable_two_factor_inner(user_id, current_password, encrypted_secret),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(UserProfileRepositoryError::Timeout)),
        }
    }

    /// 校验当前密码后移除 TOTP 密文和剩余备份码，并撤销全部既有会话。
    pub async fn disable_two_factor(
        &self,
        user_id: UserId,
        current_password: &[u8],
    ) -> Result<UserTwoFactorMutationOutcome, UserProfileRepositoryError> {
        match timeout(
            self.operation_timeout,
            self.disable_two_factor_inner(user_id, current_password),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(UserProfileRepositoryError::Timeout)),
        }
    }

    /// 读取当前用户是否已经配置 TOTP；不会读取或解密 secret。
    pub async fn two_factor_enabled(
        &self,
        user_id: UserId,
    ) -> Result<Option<bool>, UserProfileRepositoryError> {
        match timeout(self.operation_timeout, async {
            let model = users::Entity::find_by_id(user_id.get())
                .filter(users::Column::Status.eq(1_i16))
                .filter(users::Column::DeletedAt.is_null())
                .one(self.pool.connection())
                .with_subscriber(NoSubscriber::default())
                .await
                .map_err(|_| record_internal_error(UserProfileRepositoryError::Query))?;
            Ok::<_, UserProfileRepositoryError>(model.map(|model| model.totp_secret.is_some()))
        })
        .await
        {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(UserProfileRepositoryError::Timeout)),
        }
    }

    async fn get_inner(
        &self,
        user_id: UserId,
    ) -> Result<UserProfileLookupOutcome, UserProfileRepositoryError> {
        let model = users::Entity::find_by_id(user_id.get())
            .filter(users::Column::Status.eq(1_i16))
            .filter(users::Column::DeletedAt.is_null())
            .one(self.pool.connection())
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| record_internal_error(UserProfileRepositoryError::Query))?;
        model.map_or(Ok(UserProfileLookupOutcome::NotFound), |model| {
            record_from_model(model).map(UserProfileLookupOutcome::Found)
        })
    }

    async fn update_username_inner(
        &self,
        user_id: UserId,
        username: String,
    ) -> Result<UserProfileMutationOutcome, UserProfileRepositoryError> {
        if !valid_username(&username) {
            return Err(record_internal_error(UserProfileRepositoryError::Invariant));
        }
        let transaction = self.begin_transaction().await?;
        let Some(existing) = lock_user(&transaction, user_id).await? else {
            commit_transaction(transaction).await?;
            return Ok(UserProfileMutationOutcome::NotFound);
        };
        if users::Entity::find()
            .filter(users::Column::Username.eq(&username))
            .filter(users::Column::Id.ne(user_id.get()))
            .filter(users::Column::DeletedAt.is_null())
            .one(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| record_internal_error(UserProfileRepositoryError::Query))?
            .is_some()
        {
            commit_transaction(transaction).await?;
            return Ok(UserProfileMutationOutcome::Conflict);
        }
        if existing.username == username {
            let record = record_from_model(existing)?;
            commit_transaction(transaction).await?;
            return Ok(UserProfileMutationOutcome::Updated(record));
        }
        let mut active = existing.into_active_model();
        active.username = Set(username);
        let saved = active
            .update(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(map_write_error)?;
        let record = record_from_model(saved)?;
        commit_transaction(transaction).await?;
        Ok(UserProfileMutationOutcome::Updated(record))
    }

    async fn update_email_inner(
        &self,
        user_id: UserId,
        email: String,
    ) -> Result<UserProfileMutationOutcome, UserProfileRepositoryError> {
        if !valid_email(&email) {
            return Err(record_internal_error(UserProfileRepositoryError::Invariant));
        }
        let transaction = self.begin_transaction().await?;
        let Some(existing) = lock_user(&transaction, user_id).await? else {
            commit_transaction(transaction).await?;
            return Ok(UserProfileMutationOutcome::NotFound);
        };
        if users::Entity::find()
            .filter(users::Column::Email.eq(&email))
            .filter(users::Column::Id.ne(user_id.get()))
            .filter(users::Column::DeletedAt.is_null())
            .one(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| record_internal_error(UserProfileRepositoryError::Query))?
            .is_some()
        {
            commit_transaction(transaction).await?;
            return Ok(UserProfileMutationOutcome::Conflict);
        }
        if existing.email.as_deref() == Some(email.as_str()) {
            let record = record_from_model(existing)?;
            commit_transaction(transaction).await?;
            return Ok(UserProfileMutationOutcome::Updated(record));
        }
        let mut active = existing.into_active_model();
        active.email = Set(Some(email));
        let saved = active
            .update(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(map_write_error)?;
        let record = record_from_model(saved)?;
        commit_transaction(transaction).await?;
        Ok(UserProfileMutationOutcome::Updated(record))
    }

    async fn update_notifications_inner(
        &self,
        user_id: UserId,
        notifications: UserNotificationPreferencesRecord,
    ) -> Result<UserProfileMutationOutcome, UserProfileRepositoryError> {
        let transaction = self.begin_transaction().await?;
        let Some(existing) = lock_user(&transaction, user_id).await? else {
            commit_transaction(transaction).await?;
            return Ok(UserProfileMutationOutcome::NotFound);
        };
        if existing.email_product_updates == notifications.email_product_updates()
            && existing.email_usage_alerts == notifications.email_usage_alerts()
            && existing.balance_alert_threshold
                == notifications.balance_alert_threshold().map(Quota::units)
        {
            let record = record_from_model(existing)?;
            commit_transaction(transaction).await?;
            return Ok(UserProfileMutationOutcome::Updated(record));
        }
        let mut active = existing.into_active_model();
        active.email_product_updates = Set(notifications.email_product_updates());
        active.email_usage_alerts = Set(notifications.email_usage_alerts());
        active.balance_alert_threshold =
            Set(notifications.balance_alert_threshold().map(Quota::units));
        let saved = active
            .update(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(map_write_error)?;
        let record = record_from_model(saved)?;
        commit_transaction(transaction).await?;
        Ok(UserProfileMutationOutcome::Updated(record))
    }

    async fn change_password_inner(
        &self,
        user_id: UserId,
        current_password: &[u8],
        new_password: Zeroizing<String>,
    ) -> Result<UserPasswordChangeOutcome, UserProfileRepositoryError> {
        let transaction = self.begin_transaction().await?;
        let Some(existing) = lock_user(&transaction, user_id).await? else {
            commit_transaction(transaction).await?;
            return Ok(UserPasswordChangeOutcome::NotFound);
        };
        let Some(password_hash) = existing.password_hash.as_ref() else {
            commit_transaction(transaction).await?;
            return Ok(UserPasswordChangeOutcome::Rejected);
        };
        if !password_hash.verify(current_password) {
            commit_transaction(transaction).await?;
            return Ok(UserPasswordChangeOutcome::Rejected);
        }
        let next_session_version = existing
            .session_version
            .checked_add(1)
            .ok_or_else(|| record_internal_error(UserProfileRepositoryError::Invariant))?;
        let password_hash = hash_password(&new_password).map_err(map_identity_secret_error)?;
        let mut active = existing.into_active_model();
        active.password_hash = Set(Some(password_hash));
        active.session_version = Set(next_session_version);
        active
            .update(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(map_write_error)?;
        commit_transaction(transaction).await?;
        Ok(UserPasswordChangeOutcome::Updated(user_id))
    }

    async fn enable_two_factor_inner(
        &self,
        user_id: UserId,
        current_password: &[u8],
        encrypted_secret: EncryptedCredentialEnvelope,
    ) -> Result<UserTwoFactorMutationOutcome, UserProfileRepositoryError> {
        let transaction = self.begin_transaction().await?;
        let Some(existing) = lock_user(&transaction, user_id).await? else {
            commit_transaction(transaction).await?;
            return Ok(UserTwoFactorMutationOutcome::NotFound);
        };
        let Some(password_hash) = existing.password_hash.as_ref() else {
            commit_transaction(transaction).await?;
            return Ok(UserTwoFactorMutationOutcome::Rejected);
        };
        if !password_hash.verify(current_password) {
            commit_transaction(transaction).await?;
            return Ok(UserTwoFactorMutationOutcome::Rejected);
        }
        if existing.totp_secret.is_some() {
            commit_transaction(transaction).await?;
            return Ok(UserTwoFactorMutationOutcome::AlreadyEnabled);
        }
        let next_session_version = existing
            .session_version
            .checked_add(1)
            .ok_or_else(|| record_internal_error(UserProfileRepositoryError::Invariant))?;
        let mut active = existing.into_active_model();
        active.totp_secret = Set(Some(encrypted_json(encrypted_secret)?));
        active.session_version = Set(next_session_version);
        active
            .update(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(map_write_error)?;
        commit_transaction(transaction).await?;
        Ok(UserTwoFactorMutationOutcome::Updated)
    }

    async fn disable_two_factor_inner(
        &self,
        user_id: UserId,
        current_password: &[u8],
    ) -> Result<UserTwoFactorMutationOutcome, UserProfileRepositoryError> {
        let transaction = self.begin_transaction().await?;
        let Some(existing) = lock_user(&transaction, user_id).await? else {
            commit_transaction(transaction).await?;
            return Ok(UserTwoFactorMutationOutcome::NotFound);
        };
        let Some(password_hash) = existing.password_hash.as_ref() else {
            commit_transaction(transaction).await?;
            return Ok(UserTwoFactorMutationOutcome::Rejected);
        };
        if !password_hash.verify(current_password) {
            commit_transaction(transaction).await?;
            return Ok(UserTwoFactorMutationOutcome::Rejected);
        }
        if existing.totp_secret.is_none() {
            commit_transaction(transaction).await?;
            return Ok(UserTwoFactorMutationOutcome::NotEnabled);
        }
        let next_session_version = existing
            .session_version
            .checked_add(1)
            .ok_or_else(|| record_internal_error(UserProfileRepositoryError::Invariant))?;
        let mut active = existing.into_active_model();
        active.totp_secret = Set(None);
        active.session_version = Set(next_session_version);
        active
            .update(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(map_write_error)?;
        commit_transaction(transaction).await?;
        Ok(UserTwoFactorMutationOutcome::Updated)
    }

    async fn begin_transaction(&self) -> Result<DatabaseTransaction, UserProfileRepositoryError> {
        self.pool
            .connection()
            .begin()
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| record_internal_error(UserProfileRepositoryError::Query))
    }
}

impl fmt::Debug for UserProfileRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("UserProfileRepository")
            .field("operation_timeout", &self.operation_timeout)
            .finish_non_exhaustive()
    }
}

async fn lock_user(
    transaction: &DatabaseTransaction,
    user_id: UserId,
) -> Result<Option<users::Model>, UserProfileRepositoryError> {
    if transaction.get_database_backend() == DbBackend::Sqlite {
        // SQLite 没有 FOR UPDATE，先用不改变审计值的写语句取得数据库写锁。
        let result = users::Entity::update_many()
            .filter(users::Column::Id.eq(user_id.get()))
            .filter(users::Column::Status.eq(1_i16))
            .filter(users::Column::DeletedAt.is_null())
            .col_expr(
                users::Column::UpdatedAt,
                Expr::col(users::Column::UpdatedAt).into(),
            )
            .exec(transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| record_internal_error(UserProfileRepositoryError::Query))?;
        if result.rows_affected > 1 {
            return Err(record_internal_error(UserProfileRepositoryError::Invariant));
        }
        if result.rows_affected == 0 {
            return Ok(None);
        }
    }

    let mut query = users::Entity::find_by_id(user_id.get())
        .filter(users::Column::Status.eq(1_i16))
        .filter(users::Column::DeletedAt.is_null());
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| record_internal_error(UserProfileRepositoryError::Query))
}

fn record_from_model(model: users::Model) -> Result<UserProfileRecord, UserProfileRepositoryError> {
    let balance_alert_threshold = model
        .balance_alert_threshold
        .map(Quota::new)
        .transpose()
        .map_err(|_| record_internal_error(UserProfileRepositoryError::Invariant))?;
    if !valid_username(&model.username)
        || model
            .email
            .as_deref()
            .is_some_and(|email| !valid_email(email))
        || !matches!(model.role, 0 | 1)
        || !matches!(model.status, 1 | 2)
        || model.session_version < 1
        || balance_alert_threshold.is_some_and(|threshold| threshold.is_zero())
    {
        return Err(record_internal_error(UserProfileRepositoryError::Invariant));
    }
    Ok(UserProfileRecord {
        user_id: UserId::new(model.id)
            .map_err(|_| record_internal_error(UserProfileRepositoryError::Invariant))?,
        username: model.username,
        email: model.email,
        role: model.role,
        notifications: UserNotificationPreferencesRecord::new(
            model.email_product_updates,
            model.email_usage_alerts,
            balance_alert_threshold,
        ),
    })
}

fn valid_username(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_USER_PROFILE_USERNAME_BYTES
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn valid_email(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 320
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

async fn commit_transaction(
    transaction: DatabaseTransaction,
) -> Result<(), UserProfileRepositoryError> {
    transaction
        .commit()
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| record_internal_error(UserProfileRepositoryError::Query))
}

fn map_identity_secret_error(error: IdentitySecretError) -> UserProfileRepositoryError {
    match error {
        IdentitySecretError::Entropy => UserProfileRepositoryError::Entropy,
        IdentitySecretError::InvalidHash => {
            record_internal_error(UserProfileRepositoryError::Invariant)
        }
    }
}

fn map_write_error(error: DbErr) -> UserProfileRepositoryError {
    if matches!(error.sql_err(), Some(SqlErr::UniqueConstraintViolation(_))) {
        return UserProfileRepositoryError::Conflict;
    }
    record_internal_error(UserProfileRepositoryError::Query)
}

fn encrypted_json(
    envelope: EncryptedCredentialEnvelope,
) -> Result<crate::entity::EncryptedJson, UserProfileRepositoryError> {
    crate::entity::EncryptedJson::from_envelope(serde_json::json!({
        "version": 1,
        "algorithm": "xchacha20poly1305",
        "key_id": envelope.key_id(),
        "nonce": base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(envelope.nonce()),
        "ciphertext": base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(envelope.ciphertext()),
    }))
    .map_err(|_| record_internal_error(UserProfileRepositoryError::Invariant))
}

fn record_internal_error(error: UserProfileRepositoryError) -> UserProfileRepositoryError {
    let error_kind = match error {
        UserProfileRepositoryError::Query => "user_profile_query",
        UserProfileRepositoryError::Timeout => "user_profile_timeout",
        UserProfileRepositoryError::Invariant => "user_profile_invariant",
        UserProfileRepositoryError::Conflict => "user_profile_conflict",
        UserProfileRepositoryError::Entropy => "user_profile_entropy",
    };
    tracing::error!(target: "af_db::user_profile", error_kind, "用户资料仓储发生内部错误");
    error
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use super::*;
    use crate::{
        DatabaseOptions, InitialSetupOutcome, InitialSetupRecord, InitialSetupRepository,
        MigrationOptions, UserSessionLookupOutcome, UserSessionRepository,
    };

    async fn fixture() -> Result<(DatabasePool, UserId), Box<dyn Error>> {
        let pool = crate::connect_and_migrate(
            &DatabaseOptions::new("sqlite::memory:")?,
            MigrationOptions::default(),
        )
        .await?;
        let InitialSetupOutcome::Initialized { user_id } =
            InitialSetupRepository::new(pool.clone(), Duration::from_secs(5))?
                .initialize(InitialSetupRecord::new(
                    "owner".to_owned(),
                    "old secure password".to_owned(),
                ))
                .await?
        else {
            panic!("测试数据库必须完成首次安装");
        };
        Ok((pool, user_id))
    }

    #[tokio::test]
    async fn profile_and_notifications_are_scoped_to_current_user() -> Result<(), Box<dyn Error>> {
        let (pool, user_id) = fixture().await?;
        let repository = UserProfileRepository::new(pool.clone(), Duration::from_secs(5))?;

        let UserProfileLookupOutcome::Found(profile) = repository.get(user_id).await? else {
            panic!("初始用户资料必须存在");
        };
        assert_eq!(profile.username(), "owner");
        assert!(profile.notifications().email_usage_alerts());

        let UserProfileMutationOutcome::Updated(profile) = repository
            .update_username(user_id, "owner-renamed".to_owned())
            .await?
        else {
            panic!("资料更新必须成功");
        };
        assert_eq!(profile.username(), "owner-renamed");

        let UserProfileMutationOutcome::Updated(profile) = repository
            .update_notifications(
                user_id,
                UserNotificationPreferencesRecord::new(true, false, Some(Quota::new(250)?)),
            )
            .await?
        else {
            panic!("通知偏好更新必须成功");
        };
        assert!(profile.notifications().email_product_updates());
        assert!(!profile.notifications().email_usage_alerts());
        assert_eq!(
            profile
                .notifications()
                .balance_alert_threshold()
                .map(Quota::units),
            Some(250)
        );

        assert!(matches!(
            repository.get(UserId::new(999).unwrap()).await?,
            UserProfileLookupOutcome::NotFound
        ));
        pool.close().await?;
        Ok(())
    }

    #[tokio::test]
    async fn password_change_increments_session_version_and_rejects_old_password()
    -> Result<(), Box<dyn Error>> {
        let (pool, user_id) = fixture().await?;
        let repository = UserProfileRepository::new(pool.clone(), Duration::from_secs(5))?;
        assert_eq!(
            repository
                .change_password(
                    user_id,
                    b"wrong password",
                    Zeroizing::new("new secure password".to_owned()),
                )
                .await?,
            UserPasswordChangeOutcome::Rejected
        );
        assert_eq!(
            repository
                .change_password(
                    user_id,
                    b"old secure password",
                    Zeroizing::new("new secure password".to_owned()),
                )
                .await?,
            UserPasswordChangeOutcome::Updated(user_id)
        );
        let sessions = UserSessionRepository::new(pool.clone(), Duration::from_secs(5))?;
        assert_eq!(
            sessions.login("owner", b"old secure password").await?,
            UserSessionLookupOutcome::Rejected
        );
        assert!(matches!(
            sessions.login("owner", b"new secure password").await?,
            UserSessionLookupOutcome::Authenticated { user_id: actual, .. } if actual == user_id
        ));
        pool.close().await?;
        Ok(())
    }

    #[test]
    fn profile_debug_does_not_expose_email() {
        let profile = UserProfileRecord {
            user_id: UserId::new(1).unwrap(),
            username: "owner".to_owned(),
            email: Some("private@example.com".to_owned()),
            role: 0,
            notifications: UserNotificationPreferencesRecord::new(false, true, None),
        };
        let debug = format!("{profile:?}");
        assert_eq!(debug, "UserProfileRecord(<redacted>)");
        assert!(!debug.contains("private@example.com"));
    }
}
