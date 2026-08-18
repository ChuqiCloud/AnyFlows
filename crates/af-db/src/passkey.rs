use std::{fmt, time::Duration};

use af_domain::UserId;
use base64::Engine as _;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseTransaction, DbBackend, EntityTrait,
    IntoActiveModel, QueryFilter, QueryOrder, QuerySelect, Set, SqlErr, TransactionTrait,
    sea_query::{Expr, LockType},
};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool, EncryptedCredentialEnvelope,
    entity::{
        EncryptedJson, passkey_authentication_challenges, passkey_registration_challenges,
        passkeys, users,
    },
};

const MAX_DISPLAY_NAME_BYTES: usize = 128;
const MAX_CREDENTIAL_ID_BYTES: usize = 2_048;
const MAX_CHALLENGE_DIGEST_BYTES: usize = 64;

/// 当前用户可见的 Passkey 目录记录，不包含公钥、AAGUID 或注册状态。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PasskeyRecord {
    id: i64,
    display_name: String,
    created_at: crate::DatabaseTimestamp,
    last_used_at: Option<crate::DatabaseTimestamp>,
    revoked_at: Option<crate::DatabaseTimestamp>,
}

impl PasskeyRecord {
    /// 返回凭证在服务端的内部标识。
    #[must_use]
    pub const fn id(&self) -> i64 {
        self.id
    }

    /// 返回用户可编辑的展示名称。
    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    /// 返回凭证创建时间。
    #[must_use]
    pub const fn created_at(&self) -> crate::DatabaseTimestamp {
        self.created_at
    }

    /// 返回最近一次使用时间。
    #[must_use]
    pub const fn last_used_at(&self) -> Option<crate::DatabaseTimestamp> {
        self.last_used_at
    }

    /// 返回撤销时间；非空表示凭证已经不可用。
    #[must_use]
    pub const fn revoked_at(&self) -> Option<crate::DatabaseTimestamp> {
        self.revoked_at
    }
}

/// 注册挑战的短期状态；原始挑战和 WebAuthn 状态不会离开加密仓储边界。
#[derive(Debug)]
pub struct PasskeyRegistrationChallenge {
    state: EncryptedCredentialEnvelope,
}

/// 用户名查找得到的 Passkey 登录目标；仅返回活动凭证和会话版本快照。
#[derive(Debug)]
pub struct PasskeyAuthenticationTarget {
    user_id: UserId,
    session_version: i64,
    credentials: Vec<serde_json::Value>,
}

impl PasskeyAuthenticationTarget {
    /// 返回登录目标用户。
    #[must_use]
    pub const fn user_id(&self) -> UserId {
        self.user_id
    }

    /// 返回创建挑战时的会话版本。
    #[must_use]
    pub const fn session_version(&self) -> i64 {
        self.session_version
    }

    /// 返回供 WebAuthn 校验的活动凭证 JSON。
    #[must_use]
    pub fn credentials(&self) -> &[serde_json::Value] {
        &self.credentials
    }
}

/// Passkey 登录挑战的解密前元数据。
#[derive(Debug)]
pub struct PasskeyAuthenticationChallenge {
    user_id: UserId,
    session_version: i64,
    state: EncryptedCredentialEnvelope,
}

impl PasskeyAuthenticationChallenge {
    /// 返回挑战绑定的用户。
    #[must_use]
    pub const fn user_id(&self) -> UserId {
        self.user_id
    }

    /// 返回挑战绑定的会话版本。
    #[must_use]
    pub const fn session_version(&self) -> i64 {
        self.session_version
    }

    /// 返回待解密的 WebAuthn 状态封套。
    #[must_use]
    pub fn state(&self) -> &EncryptedCredentialEnvelope {
        &self.state
    }
}

/// 登录挑战消费结果；异常凭证会在同一事务内进入终态。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PasskeyAuthenticationOutcome {
    /// 凭证计数通过校验并已更新。
    Authenticated,
    /// 挑战或凭证状态不再允许登录。
    Rejected,
    /// 正数计数回退，凭证已标记异常并拒绝本次登录。
    AnomalyDetected,
}

impl PasskeyRegistrationChallenge {
    /// 返回待解密的注册状态封套。
    #[must_use]
    pub fn state(&self) -> &EncryptedCredentialEnvelope {
        &self.state
    }
}

/// 账户安全操作所需的最小因素快照。
pub struct PasskeySecurityFactors {
    password_hash: Option<crate::entity::PasswordHash>,
    totp_secret: Option<EncryptedCredentialEnvelope>,
}

impl fmt::Debug for PasskeySecurityFactors {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PasskeySecurityFactors(<redacted>)")
    }
}

impl PasskeySecurityFactors {
    /// 在仓储边界内校验当前密码，调用方不会接触 Argon2 哈希。
    #[must_use]
    pub fn verify_password(&self, password: &[u8]) -> bool {
        self.password_hash
            .as_ref()
            .is_some_and(|hash| hash.verify(password))
    }

    /// 返回 TOTP 密文封套，供上层完成 TOTP/备份码 step-up。
    #[must_use]
    pub fn totp_secret(&self) -> Option<&EncryptedCredentialEnvelope> {
        self.totp_secret.as_ref()
    }
}

/// Passkey 数据与一次性注册挑战的持久化仓储。
#[derive(Clone)]
pub struct PasskeyRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl PasskeyRepository {
    /// 创建 Passkey 仓储；超时由统一认证配置传入。
    #[must_use]
    pub const fn new(pool: DatabasePool, operation_timeout: Duration) -> Self {
        Self {
            pool,
            operation_timeout,
        }
    }

    /// 查询当前用户的凭证目录，默认按创建时间倒序返回。
    pub async fn list(
        &self,
        user_id: UserId,
    ) -> Result<Vec<PasskeyRecord>, PasskeyRepositoryError> {
        self.run(self.list_inner(user_id)).await
    }

    /// 读取当前用户活动凭证的内部 ID，仅供 WebAuthn 排除列表使用。
    pub async fn active_credential_ids(
        &self,
        user_id: UserId,
    ) -> Result<Vec<String>, PasskeyRepositoryError> {
        self.run(async move {
            let models = passkeys::Entity::find()
                .filter(passkeys::Column::UserId.eq(user_id.get()))
                .filter(passkeys::Column::RevokedAt.is_null())
                .filter(passkeys::Column::AnomalyAt.is_null())
                .all(self.pool.connection())
                .with_subscriber(NoSubscriber::default())
                .await
                .map_err(|_| record_internal_error(PasskeyRepositoryError::Query))?;
            models
                .into_iter()
                .map(|model| {
                    validate_credential_id(&model.credential_id)?;
                    Ok(model.credential_id)
                })
                .collect()
        })
        .await
    }

    /// 按用户名查找可用于免密登录的活动凭证；未知、停用、TOTP 强制和空目录统一为空。
    pub async fn authentication_target(
        &self,
        username: &str,
    ) -> Result<Option<PasskeyAuthenticationTarget>, PasskeyRepositoryError> {
        self.run(self.authentication_target_inner(username.to_owned()))
            .await
    }

    /// 按用户 ID 读取当前仍可验证的凭证，供登录完成阶段避免信任客户端标识。
    pub async fn authentication_target_for_user(
        &self,
        user_id: UserId,
    ) -> Result<Option<PasskeyAuthenticationTarget>, PasskeyRepositoryError> {
        self.run(self.authentication_target_for_user_inner(user_id))
            .await
    }

    /// 原子替换用户未消费的 Passkey 登录挑战。
    pub async fn replace_authentication_challenge(
        &self,
        user_id: UserId,
        session_version: i64,
        challenge_digest: String,
        state: EncryptedCredentialEnvelope,
        expires_at: crate::DatabaseTimestamp,
        created_at: crate::DatabaseTimestamp,
    ) -> Result<(), PasskeyRepositoryError> {
        self.run(self.replace_authentication_challenge_inner(
            user_id,
            session_version,
            challenge_digest,
            state,
            expires_at,
            created_at,
        ))
        .await
    }

    /// 读取未消费且未过期的 Passkey 登录挑战。
    pub async fn load_authentication_challenge(
        &self,
        challenge_digest: &str,
        now: crate::DatabaseTimestamp,
    ) -> Result<PasskeyAuthenticationChallenge, PasskeyRepositoryError> {
        self.run(self.load_authentication_challenge_inner(challenge_digest, now))
            .await
    }

    /// 在事务内消费挑战、检查会话版本与凭证状态，并更新计数和最近使用时间。
    pub async fn consume_authentication_challenge(
        &self,
        challenge_digest: &str,
        user_id: UserId,
        session_version: i64,
        credential_id: String,
        passkey: serde_json::Value,
        observed_sign_count: i64,
        now: crate::DatabaseTimestamp,
    ) -> Result<PasskeyAuthenticationOutcome, PasskeyRepositoryError> {
        self.run(self.consume_authentication_challenge_inner(
            challenge_digest,
            user_id,
            session_version,
            credential_id,
            passkey,
            observed_sign_count,
            now,
        ))
        .await
    }

    /// 在签名已通过 WebAuthn 校验但计数回退时消费挑战并标记凭证异常。
    pub async fn consume_authentication_anomaly(
        &self,
        challenge_digest: &str,
        user_id: UserId,
        session_version: i64,
        credential_id: String,
        now: crate::DatabaseTimestamp,
    ) -> Result<PasskeyAuthenticationOutcome, PasskeyRepositoryError> {
        self.run(self.consume_authentication_anomaly_inner(
            challenge_digest,
            user_id,
            session_version,
            credential_id,
            now,
        ))
        .await
    }

    /// 原子替换当前用户未消费的注册挑战。
    pub async fn replace_registration_challenge(
        &self,
        user_id: UserId,
        challenge_digest: String,
        state: EncryptedCredentialEnvelope,
        expires_at: crate::DatabaseTimestamp,
        created_at: crate::DatabaseTimestamp,
    ) -> Result<(), PasskeyRepositoryError> {
        self.run(self.replace_registration_challenge_inner(
            user_id,
            challenge_digest,
            state,
            expires_at,
            created_at,
        ))
        .await
    }

    /// 读取未消费且未过期的注册状态；失败不会消费挑战。
    pub async fn load_registration_challenge(
        &self,
        user_id: UserId,
        challenge_digest: &str,
        now: crate::DatabaseTimestamp,
    ) -> Result<PasskeyRegistrationChallenge, PasskeyRepositoryError> {
        self.run(self.load_registration_challenge_inner(user_id, challenge_digest, now))
            .await
    }

    /// 在事务内再次锁定并消费挑战，同时插入新凭证，防止验证重放。
    pub async fn consume_registration_challenge(
        &self,
        user_id: UserId,
        challenge_digest: &str,
        credential_id: String,
        passkey: serde_json::Value,
        display_name: String,
        sign_count: i64,
        now: crate::DatabaseTimestamp,
    ) -> Result<PasskeyRecord, PasskeyRepositoryError> {
        self.run(self.consume_registration_challenge_inner(
            user_id,
            challenge_digest,
            credential_id,
            passkey,
            display_name,
            sign_count,
            now,
        ))
        .await
    }

    /// 修改当前用户凭证名称，跨用户或已撤销凭证统一视为不存在。
    pub async fn rename(
        &self,
        user_id: UserId,
        passkey_id: i64,
        display_name: String,
    ) -> Result<Option<PasskeyRecord>, PasskeyRepositoryError> {
        self.run(self.rename_inner(user_id, passkey_id, display_name))
            .await
    }

    /// 读取密码和 TOTP 密文快照，敏感值不会实现明文调试输出。
    pub async fn security_factors(
        &self,
        user_id: UserId,
    ) -> Result<Option<PasskeySecurityFactors>, PasskeyRepositoryError> {
        self.run(self.security_factors_inner(user_id)).await
    }

    /// 原子消费一个备份码替换密文。
    pub async fn consume_totp_backup_code(
        &self,
        user_id: UserId,
        expected: EncryptedCredentialEnvelope,
        replacement: EncryptedCredentialEnvelope,
    ) -> Result<bool, PasskeyRepositoryError> {
        self.run(self.consume_totp_backup_code_inner(user_id, expected, replacement))
            .await
    }

    /// 校验当前密码后撤销凭证，并立即递增用户会话版本。
    pub async fn revoke(
        &self,
        user_id: UserId,
        passkey_id: i64,
        current_password: &[u8],
        now: crate::DatabaseTimestamp,
    ) -> Result<PasskeyRevokeOutcome, PasskeyRepositoryError> {
        self.run(self.revoke_inner(user_id, passkey_id, current_password, now))
            .await
    }

    async fn run<T>(
        &self,
        operation: impl std::future::Future<Output = Result<T, PasskeyRepositoryError>>,
    ) -> Result<T, PasskeyRepositoryError> {
        if self.operation_timeout.is_zero() {
            return Err(record_internal_error(
                PasskeyRepositoryError::InvalidConfiguration,
            ));
        }
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(PasskeyRepositoryError::Timeout)),
        }
    }

    async fn list_inner(
        &self,
        user_id: UserId,
    ) -> Result<Vec<PasskeyRecord>, PasskeyRepositoryError> {
        let models = passkeys::Entity::find()
            .filter(passkeys::Column::UserId.eq(user_id.get()))
            .order_by_desc(passkeys::Column::CreatedAt)
            .all(self.pool.connection())
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| record_internal_error(PasskeyRepositoryError::Query))?;
        models.into_iter().map(record_from_model).collect()
    }

    async fn authentication_target_inner(
        &self,
        username: String,
    ) -> Result<Option<PasskeyAuthenticationTarget>, PasskeyRepositoryError> {
        if !valid_username(&username) {
            return Ok(None);
        }
        let Some(user) = users::Entity::find()
            .filter(users::Column::Username.eq(username))
            .filter(users::Column::Status.eq(1_i16))
            .filter(users::Column::DeletedAt.is_null())
            .filter(users::Column::TotpSecret.is_null())
            .one(self.pool.connection())
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| record_internal_error(PasskeyRepositoryError::Query))?
        else {
            return Ok(None);
        };
        if user.session_version < 1 {
            return Err(record_internal_error(PasskeyRepositoryError::Invariant));
        }
        self.authentication_target_from_user(user).await
    }

    async fn authentication_target_for_user_inner(
        &self,
        user_id: UserId,
    ) -> Result<Option<PasskeyAuthenticationTarget>, PasskeyRepositoryError> {
        let Some(user) = users::Entity::find_by_id(user_id.get())
            .filter(users::Column::Status.eq(1_i16))
            .filter(users::Column::DeletedAt.is_null())
            .filter(users::Column::TotpSecret.is_null())
            .one(self.pool.connection())
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| record_internal_error(PasskeyRepositoryError::Query))?
        else {
            return Ok(None);
        };
        self.authentication_target_from_user(user).await
    }

    async fn authentication_target_from_user(
        &self,
        user: users::Model,
    ) -> Result<Option<PasskeyAuthenticationTarget>, PasskeyRepositoryError> {
        if user.session_version < 1 {
            return Err(record_internal_error(PasskeyRepositoryError::Invariant));
        }
        let models = passkeys::Entity::find()
            .filter(passkeys::Column::UserId.eq(user.id))
            .filter(passkeys::Column::RevokedAt.is_null())
            .filter(passkeys::Column::AnomalyAt.is_null())
            .order_by_asc(passkeys::Column::Id)
            .all(self.pool.connection())
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| record_internal_error(PasskeyRepositoryError::Query))?;
        if models.is_empty() {
            return Ok(None);
        }
        let credentials = models
            .into_iter()
            .map(|model| {
                validate_credential_id(&model.credential_id)?;
                if model.sign_count < 0 || !model.passkey.is_object() {
                    return Err(record_internal_error(PasskeyRepositoryError::Invariant));
                }
                Ok(model.passkey)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Some(PasskeyAuthenticationTarget {
            user_id: UserId::new(user.id)
                .map_err(|_| record_internal_error(PasskeyRepositoryError::Invariant))?,
            session_version: user.session_version,
            credentials,
        }))
    }

    async fn replace_authentication_challenge_inner(
        &self,
        user_id: UserId,
        session_version: i64,
        challenge_digest: String,
        state: EncryptedCredentialEnvelope,
        expires_at: crate::DatabaseTimestamp,
        created_at: crate::DatabaseTimestamp,
    ) -> Result<(), PasskeyRepositoryError> {
        validate_digest(&challenge_digest)?;
        if session_version < 1 || expires_at <= created_at {
            return Err(record_internal_error(PasskeyRepositoryError::Invariant));
        }
        let transaction = self.begin_transaction().await?;
        let Some(user) = lock_user(&transaction, user_id).await? else {
            rollback_transaction(transaction).await?;
            return Err(PasskeyRepositoryError::NotFound);
        };
        if user.session_version != session_version || user.totp_secret.is_some() {
            rollback_transaction(transaction).await?;
            return Err(PasskeyRepositoryError::Conflict);
        }
        passkey_authentication_challenges::Entity::delete_many()
            .filter(passkey_authentication_challenges::Column::UserId.eq(user_id.get()))
            .filter(passkey_authentication_challenges::Column::ConsumedAt.is_null())
            .exec(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| record_internal_error(PasskeyRepositoryError::Query))?;
        let active = passkey_authentication_challenges::ActiveModel {
            user_id: Set(user_id.get()),
            challenge_digest: Set(challenge_digest),
            authentication_state: Set(encrypted_json(state)?),
            session_version: Set(session_version),
            expires_at: Set(expires_at),
            consumed_at: Set(None),
            created_at: Set(created_at),
            ..Default::default()
        };
        active
            .insert(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(map_write_error)?;
        commit_transaction(transaction).await
    }

    async fn load_authentication_challenge_inner(
        &self,
        challenge_digest: &str,
        now: crate::DatabaseTimestamp,
    ) -> Result<PasskeyAuthenticationChallenge, PasskeyRepositoryError> {
        validate_digest(challenge_digest)?;
        let Some(model) = passkey_authentication_challenges::Entity::find()
            .filter(passkey_authentication_challenges::Column::ChallengeDigest.eq(challenge_digest))
            .filter(passkey_authentication_challenges::Column::ConsumedAt.is_null())
            .one(self.pool.connection())
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| record_internal_error(PasskeyRepositoryError::Query))?
        else {
            return Err(PasskeyRepositoryError::NotFound);
        };
        if model.expires_at <= now {
            return Err(PasskeyRepositoryError::Expired);
        }
        let user_id = UserId::new(model.user_id)
            .map_err(|_| record_internal_error(PasskeyRepositoryError::Invariant))?;
        if model.session_version < 1 {
            return Err(record_internal_error(PasskeyRepositoryError::Invariant));
        }
        Ok(PasskeyAuthenticationChallenge {
            user_id,
            session_version: model.session_version,
            state: envelope_from_json(model.authentication_state)?,
        })
    }

    async fn consume_authentication_challenge_inner(
        &self,
        challenge_digest: &str,
        user_id: UserId,
        session_version: i64,
        credential_id: String,
        passkey: serde_json::Value,
        observed_sign_count: i64,
        now: crate::DatabaseTimestamp,
    ) -> Result<PasskeyAuthenticationOutcome, PasskeyRepositoryError> {
        validate_digest(challenge_digest)?;
        validate_credential_id(&credential_id)?;
        if session_version < 1 || observed_sign_count < 0 || !passkey.is_object() {
            return Err(record_internal_error(PasskeyRepositoryError::Invariant));
        }
        let transaction = self.begin_transaction().await?;
        let Some(user) = lock_user_any(&transaction, user_id).await? else {
            rollback_transaction(transaction).await?;
            return Ok(PasskeyAuthenticationOutcome::Rejected);
        };
        let Some(challenge) = lock_authentication_challenge(&transaction, challenge_digest).await?
        else {
            rollback_transaction(transaction).await?;
            return Err(PasskeyRepositoryError::NotFound);
        };
        if challenge.consumed_at.is_some() {
            rollback_transaction(transaction).await?;
            return Err(PasskeyRepositoryError::Consumed);
        }
        if challenge.expires_at <= now {
            rollback_transaction(transaction).await?;
            return Err(PasskeyRepositoryError::Expired);
        }
        let challenge_user_id = challenge.user_id;
        let challenge_session_version = challenge.session_version;
        let mut consumed = challenge.into_active_model();
        consumed.consumed_at = Set(Some(now));
        consumed
            .update(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(map_write_error)?;
        let valid_version = challenge_user_id == user_id.get()
            && challenge_session_version == session_version
            && user.session_version == session_version
            && user.status == 1
            && user.deleted_at.is_none()
            && user.totp_secret.is_none();
        let Some(credential) =
            lock_passkey_by_digest(&transaction, user_id, &credential_id_digest(&credential_id))
                .await?
        else {
            commit_transaction(transaction).await?;
            return Ok(PasskeyAuthenticationOutcome::Rejected);
        };
        if !valid_version || credential.revoked_at.is_some() || credential.anomaly_at.is_some() {
            commit_transaction(transaction).await?;
            return Ok(PasskeyAuthenticationOutcome::Rejected);
        }
        if credential.sign_count > 0 && observed_sign_count <= credential.sign_count {
            let mut anomaly = credential.into_active_model();
            anomaly.anomaly_at = Set(Some(now));
            anomaly
                .update(&transaction)
                .with_subscriber(NoSubscriber::default())
                .await
                .map_err(map_write_error)?;
            commit_transaction(transaction).await?;
            return Ok(PasskeyAuthenticationOutcome::AnomalyDetected);
        }
        let mut updated = credential.into_active_model();
        updated.passkey = Set(passkey);
        updated.sign_count = Set(observed_sign_count);
        updated.last_used_at = Set(Some(now));
        updated
            .update(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(map_write_error)?;
        commit_transaction(transaction).await?;
        Ok(PasskeyAuthenticationOutcome::Authenticated)
    }

    async fn consume_authentication_anomaly_inner(
        &self,
        challenge_digest: &str,
        user_id: UserId,
        session_version: i64,
        credential_id: String,
        now: crate::DatabaseTimestamp,
    ) -> Result<PasskeyAuthenticationOutcome, PasskeyRepositoryError> {
        validate_digest(challenge_digest)?;
        validate_credential_id(&credential_id)?;
        if session_version < 1 {
            return Err(record_internal_error(PasskeyRepositoryError::Invariant));
        }
        let transaction = self.begin_transaction().await?;
        // 所有认证消费统一按用户、挑战、凭证加锁，避免成功与异常路径形成反向锁序。
        let Some(user) = lock_user_any(&transaction, user_id).await? else {
            rollback_transaction(transaction).await?;
            return Ok(PasskeyAuthenticationOutcome::Rejected);
        };
        let Some(challenge) = lock_authentication_challenge(&transaction, challenge_digest).await?
        else {
            rollback_transaction(transaction).await?;
            return Err(PasskeyRepositoryError::NotFound);
        };
        if challenge.consumed_at.is_some() {
            rollback_transaction(transaction).await?;
            return Err(PasskeyRepositoryError::Consumed);
        }
        if challenge.expires_at <= now {
            rollback_transaction(transaction).await?;
            return Err(PasskeyRepositoryError::Expired);
        }
        let challenge_user_id = challenge.user_id;
        let challenge_session_version = challenge.session_version;
        let mut consumed = challenge.into_active_model();
        consumed.consumed_at = Set(Some(now));
        consumed
            .update(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(map_write_error)?;

        // 账户状态也要消耗挑战，避免停用后重新启用时复用旧的有效响应。
        let valid_version = challenge_user_id == user_id.get()
            && challenge_session_version == session_version
            && user.session_version == session_version
            && user.status == 1
            && user.deleted_at.is_none()
            && user.totp_secret.is_none();
        let Some(credential) =
            lock_passkey_by_digest(&transaction, user_id, &credential_id_digest(&credential_id))
                .await?
        else {
            commit_transaction(transaction).await?;
            return Ok(PasskeyAuthenticationOutcome::Rejected);
        };
        if !valid_version || credential.revoked_at.is_some() || credential.anomaly_at.is_some() {
            commit_transaction(transaction).await?;
            return Ok(PasskeyAuthenticationOutcome::Rejected);
        }
        let mut anomaly = credential.into_active_model();
        anomaly.anomaly_at = Set(Some(now));
        anomaly
            .update(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(map_write_error)?;
        commit_transaction(transaction).await?;
        Ok(PasskeyAuthenticationOutcome::AnomalyDetected)
    }

    async fn replace_registration_challenge_inner(
        &self,
        user_id: UserId,
        challenge_digest: String,
        state: EncryptedCredentialEnvelope,
        expires_at: crate::DatabaseTimestamp,
        created_at: crate::DatabaseTimestamp,
    ) -> Result<(), PasskeyRepositoryError> {
        validate_digest(&challenge_digest)?;
        if expires_at <= created_at {
            return Err(record_internal_error(PasskeyRepositoryError::Invariant));
        }
        let transaction = self.begin_transaction().await?;
        if lock_user(&transaction, user_id).await?.is_none() {
            rollback_transaction(transaction).await?;
            return Err(PasskeyRepositoryError::NotFound);
        }
        passkey_registration_challenges::Entity::delete_many()
            .filter(passkey_registration_challenges::Column::UserId.eq(user_id.get()))
            .filter(passkey_registration_challenges::Column::ConsumedAt.is_null())
            .exec(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| record_internal_error(PasskeyRepositoryError::Query))?;
        let active = passkey_registration_challenges::ActiveModel {
            user_id: Set(user_id.get()),
            challenge_digest: Set(challenge_digest),
            registration_state: Set(encrypted_json(state)?),
            expires_at: Set(expires_at),
            consumed_at: Set(None),
            created_at: Set(created_at),
            ..Default::default()
        };
        active
            .insert(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(map_write_error)?;
        commit_transaction(transaction).await
    }

    async fn load_registration_challenge_inner(
        &self,
        user_id: UserId,
        challenge_digest: &str,
        now: crate::DatabaseTimestamp,
    ) -> Result<PasskeyRegistrationChallenge, PasskeyRepositoryError> {
        validate_digest(challenge_digest)?;
        let Some(model) = passkey_registration_challenges::Entity::find()
            .filter(passkey_registration_challenges::Column::UserId.eq(user_id.get()))
            .filter(passkey_registration_challenges::Column::ChallengeDigest.eq(challenge_digest))
            .filter(passkey_registration_challenges::Column::ConsumedAt.is_null())
            .one(self.pool.connection())
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| record_internal_error(PasskeyRepositoryError::Query))?
        else {
            return Err(PasskeyRepositoryError::NotFound);
        };
        if model.expires_at <= now {
            return Err(PasskeyRepositoryError::Expired);
        }
        Ok(PasskeyRegistrationChallenge {
            state: envelope_from_json(model.registration_state)?,
        })
    }

    async fn consume_registration_challenge_inner(
        &self,
        user_id: UserId,
        challenge_digest: &str,
        credential_id: String,
        passkey: serde_json::Value,
        display_name: String,
        sign_count: i64,
        now: crate::DatabaseTimestamp,
    ) -> Result<PasskeyRecord, PasskeyRepositoryError> {
        validate_digest(challenge_digest)?;
        validate_credential_id(&credential_id)?;
        validate_display_name(&display_name)?;
        if sign_count < 0 || !passkey.is_object() {
            return Err(record_internal_error(PasskeyRepositoryError::Invariant));
        }
        let transaction = self.begin_transaction().await?;
        if lock_user(&transaction, user_id).await?.is_none() {
            rollback_transaction(transaction).await?;
            return Err(PasskeyRepositoryError::NotFound);
        }
        let Some(challenge) = lock_challenge(&transaction, user_id, challenge_digest).await? else {
            rollback_transaction(transaction).await?;
            return Err(PasskeyRepositoryError::NotFound);
        };
        if challenge.consumed_at.is_some() {
            rollback_transaction(transaction).await?;
            return Err(PasskeyRepositoryError::Consumed);
        }
        if challenge.expires_at <= now {
            rollback_transaction(transaction).await?;
            return Err(PasskeyRepositoryError::Expired);
        }
        let mut consumed = challenge.into_active_model();
        consumed.consumed_at = Set(Some(now));
        consumed
            .update(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(map_write_error)?;
        let credential_id_digest = credential_id_digest(&credential_id);
        let created = passkeys::ActiveModel {
            user_id: Set(user_id.get()),
            credential_id: Set(credential_id),
            credential_id_digest: Set(credential_id_digest),
            passkey: Set(passkey),
            display_name: Set(display_name),
            created_at: Set(now),
            last_used_at: Set(None),
            revoked_at: Set(None),
            sign_count: Set(sign_count),
            anomaly_at: Set(None),
            ..Default::default()
        };
        let saved = created
            .insert(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(map_write_error)?;
        let record = record_from_model(saved)?;
        commit_transaction(transaction).await?;
        Ok(record)
    }

    async fn rename_inner(
        &self,
        user_id: UserId,
        passkey_id: i64,
        display_name: String,
    ) -> Result<Option<PasskeyRecord>, PasskeyRepositoryError> {
        if passkey_id <= 0 {
            return Err(record_internal_error(PasskeyRepositoryError::Invariant));
        }
        validate_display_name(&display_name)?;
        let transaction = self.begin_transaction().await?;
        let Some(model) = lock_passkey(&transaction, user_id, passkey_id).await? else {
            rollback_transaction(transaction).await?;
            return Ok(None);
        };
        if model.revoked_at.is_some() {
            rollback_transaction(transaction).await?;
            return Ok(None);
        }
        let mut active = model.into_active_model();
        active.display_name = Set(display_name);
        let saved = active
            .update(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(map_write_error)?;
        let record = record_from_model(saved)?;
        commit_transaction(transaction).await?;
        Ok(Some(record))
    }

    async fn security_factors_inner(
        &self,
        user_id: UserId,
    ) -> Result<Option<PasskeySecurityFactors>, PasskeyRepositoryError> {
        let Some(model) = users::Entity::find_by_id(user_id.get())
            .filter(users::Column::Status.eq(1_i16))
            .filter(users::Column::DeletedAt.is_null())
            .one(self.pool.connection())
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| record_internal_error(PasskeyRepositoryError::Query))?
        else {
            return Ok(None);
        };
        Ok(Some(PasskeySecurityFactors {
            password_hash: model.password_hash,
            totp_secret: model.totp_secret.map(envelope_from_json).transpose()?,
        }))
    }

    async fn consume_totp_backup_code_inner(
        &self,
        user_id: UserId,
        expected: EncryptedCredentialEnvelope,
        replacement: EncryptedCredentialEnvelope,
    ) -> Result<bool, PasskeyRepositoryError> {
        let expected_json = encrypted_json(expected)?;
        let replacement_json = encrypted_json(replacement)?;
        let result = users::Entity::update_many()
            .filter(users::Column::Id.eq(user_id.get()))
            .filter(users::Column::Status.eq(1_i16))
            .filter(users::Column::DeletedAt.is_null())
            .filter(users::Column::TotpSecret.eq(expected_json))
            .col_expr(users::Column::TotpSecret, Expr::value(replacement_json))
            .exec(self.pool.connection())
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| record_internal_error(PasskeyRepositoryError::Query))?;
        match result.rows_affected {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(record_internal_error(PasskeyRepositoryError::Invariant)),
        }
    }

    async fn revoke_inner(
        &self,
        user_id: UserId,
        passkey_id: i64,
        current_password: &[u8],
        now: crate::DatabaseTimestamp,
    ) -> Result<PasskeyRevokeOutcome, PasskeyRepositoryError> {
        if passkey_id <= 0 || current_password.is_empty() {
            return Err(record_internal_error(PasskeyRepositoryError::Invariant));
        }
        let transaction = self.begin_transaction().await?;
        let Some(user) = lock_user(&transaction, user_id).await? else {
            rollback_transaction(transaction).await?;
            return Ok(PasskeyRevokeOutcome::NotFound);
        };
        let Some(password_hash) = user.password_hash.as_ref() else {
            rollback_transaction(transaction).await?;
            return Ok(PasskeyRevokeOutcome::PasswordRejected);
        };
        if !password_hash.verify(current_password) {
            rollback_transaction(transaction).await?;
            return Ok(PasskeyRevokeOutcome::PasswordRejected);
        }
        let Some(passkey) = lock_passkey(&transaction, user_id, passkey_id).await? else {
            rollback_transaction(transaction).await?;
            return Ok(PasskeyRevokeOutcome::NotFound);
        };
        if passkey.revoked_at.is_some() {
            rollback_transaction(transaction).await?;
            return Ok(PasskeyRevokeOutcome::AlreadyRevoked);
        }
        let mut passkey = passkey.into_active_model();
        passkey.revoked_at = Set(Some(now));
        passkey
            .update(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(map_write_error)?;
        let next_version = user
            .session_version
            .checked_add(1)
            .ok_or_else(|| record_internal_error(PasskeyRepositoryError::Invariant))?;
        let mut user = user.into_active_model();
        user.session_version = Set(next_version);
        user.update(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(map_write_error)?;
        commit_transaction(transaction).await?;
        Ok(PasskeyRevokeOutcome::Revoked)
    }

    async fn begin_transaction(&self) -> Result<DatabaseTransaction, PasskeyRepositoryError> {
        self.pool
            .connection()
            .begin()
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| record_internal_error(PasskeyRepositoryError::Query))
    }
}

impl fmt::Debug for PasskeyRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PasskeyRepository")
            .field("operation_timeout", &self.operation_timeout)
            .finish_non_exhaustive()
    }
}

/// 凭证撤销结果；失败原因不泄露跨用户的凭证存在性。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PasskeyRevokeOutcome {
    /// 已撤销并使旧会话失效。
    Revoked,
    /// 用户或凭证不存在。
    NotFound,
    /// 当前密码错误或账户没有密码。
    PasswordRejected,
    /// 凭证已经处于终态。
    AlreadyRevoked,
}

/// Passkey 仓储内部错误分类。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum PasskeyRepositoryError {
    #[error("Passkey 仓储配置无效")]
    InvalidConfiguration,
    #[error("Passkey 数据库操作失败")]
    Query,
    #[error("Passkey 数据库操作超时")]
    Timeout,
    #[error("Passkey 持久化状态损坏")]
    Invariant,
    #[error("Passkey 凭证冲突")]
    Conflict,
    #[error("Passkey 挑战不存在")]
    NotFound,
    #[error("Passkey 挑战已过期")]
    Expired,
    #[error("Passkey 挑战已消费")]
    Consumed,
}

async fn lock_user(
    transaction: &DatabaseTransaction,
    user_id: UserId,
) -> Result<Option<users::Model>, PasskeyRepositoryError> {
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
        .map_err(|_| record_internal_error(PasskeyRepositoryError::Query))
}

async fn lock_user_any(
    transaction: &DatabaseTransaction,
    user_id: UserId,
) -> Result<Option<users::Model>, PasskeyRepositoryError> {
    let mut query = users::Entity::find_by_id(user_id.get());
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| record_internal_error(PasskeyRepositoryError::Query))
}

async fn lock_passkey(
    transaction: &DatabaseTransaction,
    user_id: UserId,
    passkey_id: i64,
) -> Result<Option<passkeys::Model>, PasskeyRepositoryError> {
    let mut query =
        passkeys::Entity::find_by_id(passkey_id).filter(passkeys::Column::UserId.eq(user_id.get()));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| record_internal_error(PasskeyRepositoryError::Query))
}

async fn lock_challenge(
    transaction: &DatabaseTransaction,
    user_id: UserId,
    challenge_digest: &str,
) -> Result<Option<passkey_registration_challenges::Model>, PasskeyRepositoryError> {
    let mut query = passkey_registration_challenges::Entity::find()
        .filter(passkey_registration_challenges::Column::UserId.eq(user_id.get()))
        .filter(passkey_registration_challenges::Column::ChallengeDigest.eq(challenge_digest));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| record_internal_error(PasskeyRepositoryError::Query))
}

async fn lock_authentication_challenge(
    transaction: &DatabaseTransaction,
    challenge_digest: &str,
) -> Result<Option<passkey_authentication_challenges::Model>, PasskeyRepositoryError> {
    let mut query = passkey_authentication_challenges::Entity::find()
        .filter(passkey_authentication_challenges::Column::ChallengeDigest.eq(challenge_digest));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| record_internal_error(PasskeyRepositoryError::Query))
}

async fn lock_passkey_by_digest(
    transaction: &DatabaseTransaction,
    user_id: UserId,
    credential_id_digest: &str,
) -> Result<Option<passkeys::Model>, PasskeyRepositoryError> {
    let mut query = passkeys::Entity::find()
        .filter(passkeys::Column::UserId.eq(user_id.get()))
        .filter(passkeys::Column::CredentialIdDigest.eq(credential_id_digest));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| record_internal_error(PasskeyRepositoryError::Query))
}

fn record_from_model(model: passkeys::Model) -> Result<PasskeyRecord, PasskeyRepositoryError> {
    if model.id <= 0
        || model.user_id <= 0
        || !valid_display_name(&model.display_name)
        || validate_credential_id(&model.credential_id).is_err()
        || model.sign_count < 0
    {
        return Err(record_internal_error(PasskeyRepositoryError::Invariant));
    }
    Ok(PasskeyRecord {
        id: model.id,
        display_name: model.display_name,
        created_at: model.created_at,
        last_used_at: model.last_used_at,
        revoked_at: model.revoked_at,
    })
}

fn valid_username(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn validate_display_name(value: &str) -> Result<(), PasskeyRepositoryError> {
    if valid_display_name(value) {
        Ok(())
    } else {
        Err(record_internal_error(PasskeyRepositoryError::Invariant))
    }
}

fn valid_display_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_DISPLAY_NAME_BYTES
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn validate_credential_id(value: &str) -> Result<(), PasskeyRepositoryError> {
    if value.is_empty()
        || value.len() > MAX_CREDENTIAL_ID_BYTES
        || value.trim() != value
        || value
            .bytes()
            .any(|byte| !byte.is_ascii_alphanumeric() && !matches!(byte, b'-' | b'_'))
    {
        return Err(record_internal_error(PasskeyRepositoryError::Invariant));
    }
    Ok(())
}

fn credential_id_digest(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

fn validate_digest(value: &str) -> Result<(), PasskeyRepositoryError> {
    if value.len() != MAX_CHALLENGE_DIGEST_BYTES
        || value
            .bytes()
            .any(|byte| !byte.is_ascii_hexdigit() || byte.is_ascii_uppercase())
    {
        return Err(record_internal_error(PasskeyRepositoryError::Invariant));
    }
    Ok(())
}

fn encrypted_json(
    envelope: EncryptedCredentialEnvelope,
) -> Result<EncryptedJson, PasskeyRepositoryError> {
    EncryptedJson::from_envelope(serde_json::json!({
        "version": 1,
        "algorithm": "xchacha20poly1305",
        "key_id": envelope.key_id(),
        "nonce": base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(envelope.nonce()),
        "ciphertext": base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(envelope.ciphertext()),
    }))
    .map_err(|_| record_internal_error(PasskeyRepositoryError::Invariant))
}

fn envelope_from_json(
    json: EncryptedJson,
) -> Result<EncryptedCredentialEnvelope, PasskeyRepositoryError> {
    let (key_id, nonce, ciphertext) = json
        .envelope_parts()
        .map_err(|_| record_internal_error(PasskeyRepositoryError::Invariant))?;
    EncryptedCredentialEnvelope::new(key_id, nonce, ciphertext)
        .map_err(|_| record_internal_error(PasskeyRepositoryError::Invariant))
}

async fn commit_transaction(
    transaction: DatabaseTransaction,
) -> Result<(), PasskeyRepositoryError> {
    transaction
        .commit()
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| record_internal_error(PasskeyRepositoryError::Query))
}

async fn rollback_transaction(
    transaction: DatabaseTransaction,
) -> Result<(), PasskeyRepositoryError> {
    transaction
        .rollback()
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| record_internal_error(PasskeyRepositoryError::Query))
}

fn map_write_error(error: sea_orm::DbErr) -> PasskeyRepositoryError {
    if matches!(error.sql_err(), Some(SqlErr::UniqueConstraintViolation(_))) {
        return PasskeyRepositoryError::Conflict;
    }
    record_internal_error(PasskeyRepositoryError::Query)
}

fn record_internal_error(error: PasskeyRepositoryError) -> PasskeyRepositoryError {
    let error_kind = match error {
        PasskeyRepositoryError::InvalidConfiguration => return error,
        PasskeyRepositoryError::Query => "passkey_query",
        PasskeyRepositoryError::Timeout => "passkey_timeout",
        PasskeyRepositoryError::Invariant => "passkey_invariant",
        PasskeyRepositoryError::Conflict => "passkey_conflict",
        PasskeyRepositoryError::NotFound => return error,
        PasskeyRepositoryError::Expired => return error,
        PasskeyRepositoryError::Consumed => return error,
    };
    tracing::error!(target: "af_db::passkey", error_kind, "Passkey 仓储发生内部错误");
    error
}
