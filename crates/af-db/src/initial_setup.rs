use std::{fmt, time::Duration};

use af_domain::UserId;
use sea_orm::{
    ActiveModelTrait, ConnectionTrait, DatabaseTransaction, DbErr, EntityTrait, QuerySelect, Set,
    SqlErr, TransactionTrait,
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};
use zeroize::Zeroizing;

use crate::{
    DatabasePool,
    entity::{SensitiveString, groups, options, users},
    identity_secret::{IdentitySecretError, generate_aff_code, hash_password},
};

const SETUP_MARKER_KEY: &str = "system.initial_setup.completed";
const SETUP_MARKER_VALUE: &str = "1";

/// 首次安装状态；只要存在安装标记或任意历史用户，就永久视为已完成。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InitialSetupStatus {
    /// 当前数据库允许创建首个管理员。
    Required,
    /// 当前数据库已经完成或越过首次安装阶段。
    Complete,
}

/// 首次安装写入结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InitialSetupOutcome {
    /// 默认分组和首个管理员已经在同一事务内提交。
    Initialized { user_id: UserId },
    /// 数据库已经安装，或并发请求先完成了安装。
    AlreadyInitialized,
}

/// 已由应用层校验的首次安装管理员输入。
pub struct InitialSetupRecord {
    username: String,
    password: Zeroizing<String>,
}

impl InitialSetupRecord {
    /// 组装首次安装仓储写入记录。
    #[must_use]
    pub fn new(username: String, password: String) -> Self {
        Self {
            username,
            password: Zeroizing::new(password),
        }
    }
}

impl fmt::Debug for InitialSetupRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("InitialSetupRecord(<redacted>)")
    }
}

/// 首次安装仓储构造错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum InitialSetupRepositoryConfigError {
    /// 零超时无法形成有效的安装截止时间。
    #[error("首次安装数据库操作超时必须大于零")]
    ZeroOperationTimeout,
}

/// 首次安装仓储内部错误；不携带用户名、密码或数据库诊断。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum InitialSetupRepositoryError {
    /// 获取连接或执行查询失败。
    #[error("首次安装数据库操作失败")]
    Query,
    /// 数据库操作超过配置的硬截止时间。
    #[error("首次安装数据库操作超时")]
    Timeout,
    /// 持久化结果违反首次安装不变量。
    #[error("首次安装持久化状态损坏")]
    Invariant,
    /// 系统随机源不可用，无法安全生成密码盐或邀请码。
    #[error("首次安装随机源不可用")]
    Entropy,
}

/// 首次安装状态读取与一次性原子写入仓储。
#[derive(Clone)]
pub struct InitialSetupRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl InitialSetupRepository {
    /// 使用共享数据库连接池和单次操作截止时间构造仓储。
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, InitialSetupRepositoryConfigError> {
        if operation_timeout.is_zero() {
            return Err(InitialSetupRepositoryConfigError::ZeroOperationTimeout);
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }

    /// 读取失败关闭的首次安装状态。
    pub async fn status(&self) -> Result<InitialSetupStatus, InitialSetupRepositoryError> {
        match timeout(self.operation_timeout, self.status_inner()).await {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(InitialSetupRepositoryError::Timeout)),
        }
    }

    /// 原子创建安装标记、默认分组和首个管理员。
    pub async fn initialize(
        &self,
        record: InitialSetupRecord,
    ) -> Result<InitialSetupOutcome, InitialSetupRepositoryError> {
        // 密码哈希和随机邀请码先于事务生成，避免持锁期间执行昂贵计算或访问系统随机源。
        let password_hash = hash_password(&record.password).map_err(map_secret_error)?;
        let aff_code = generate_aff_code().map_err(map_secret_error)?;
        let prepared = PreparedInitialSetup {
            username: record.username,
            password_hash,
            aff_code,
        };
        match timeout(self.operation_timeout, self.initialize_inner(prepared)).await {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(InitialSetupRepositoryError::Timeout)),
        }
    }

    async fn status_inner(&self) -> Result<InitialSetupStatus, InitialSetupRepositoryError> {
        let connection = self.pool.connection();
        if marker_exists(connection).await? || user_exists(connection).await? {
            Ok(InitialSetupStatus::Complete)
        } else {
            Ok(InitialSetupStatus::Required)
        }
    }

    async fn initialize_inner(
        &self,
        prepared: PreparedInitialSetup,
    ) -> Result<InitialSetupOutcome, InitialSetupRepositoryError> {
        let transaction = self
            .pool
            .connection()
            .begin()
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| record_internal_error(InitialSetupRepositoryError::Query))?;

        // 固定主键是跨数据库的原子竞争点；失败请求必须回滚后再返回稳定冲突。
        let marker = options::ActiveModel {
            key: Set(SETUP_MARKER_KEY.to_owned()),
            value: Set(SensitiveString::from(SETUP_MARKER_VALUE)),
            ..Default::default()
        }
        .insert(&transaction)
        .with_subscriber(NoSubscriber::default())
        .await;
        if let Err(error) = marker {
            if is_unique_violation(&error) {
                rollback_transaction(transaction).await?;
                return Ok(InitialSetupOutcome::AlreadyInitialized);
            }
            return Err(record_internal_error(InitialSetupRepositoryError::Query));
        }

        // 历史用户即使已软删除也代表系统曾安装，禁止通过清空有效用户重新打开 setup。
        if user_exists(&transaction).await? {
            rollback_transaction(transaction).await?;
            return Ok(InitialSetupOutcome::AlreadyInitialized);
        }

        let group = groups::ActiveModel {
            name: Set("default".to_owned()),
            display_name: Set("默认分组".to_owned()),
            ratio_micros: Set(1_000_000),
            peak_ratio_micros: Set(None),
            peak_start: Set(None),
            peak_end: Set(None),
            is_exclusive: Set(false),
            daily_limit: Set(None),
            weekly_limit: Set(None),
            monthly_limit: Set(None),
            rpm_limit: Set(None),
            fallback_group_id: Set(None),
            flags: Set(serde_json::json!({})),
            ..Default::default()
        }
        .insert(&transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| record_internal_error(InitialSetupRepositoryError::Query))?;

        let user = users::ActiveModel {
            username: Set(prepared.username),
            email: Set(None),
            password_hash: Set(Some(prepared.password_hash)),
            role: Set(1),
            status: Set(1),
            default_group_id: Set(group.id),
            quota: Set(0),
            aff_code: Set(prepared.aff_code),
            rpm_limit: Set(None),
            concurrency: Set(None),
            settings: Set(serde_json::json!({})),
            ..Default::default()
        }
        .insert(&transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| record_internal_error(InitialSetupRepositoryError::Query))?;
        let user_id = UserId::new(user.id)
            .map_err(|_| record_internal_error(InitialSetupRepositoryError::Invariant))?;

        transaction
            .commit()
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| record_internal_error(InitialSetupRepositoryError::Query))?;
        Ok(InitialSetupOutcome::Initialized { user_id })
    }
}

impl fmt::Debug for InitialSetupRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("InitialSetupRepository")
            .field("operation_timeout", &self.operation_timeout)
            .finish_non_exhaustive()
    }
}

struct PreparedInitialSetup {
    username: String,
    password_hash: crate::entity::PasswordHash,
    aff_code: String,
}

async fn marker_exists<C>(connection: &C) -> Result<bool, InitialSetupRepositoryError>
where
    C: ConnectionTrait,
{
    options::Entity::find_by_id(SETUP_MARKER_KEY)
        .one(connection)
        .with_subscriber(NoSubscriber::default())
        .await
        .map(|model| model.is_some())
        .map_err(|_| record_internal_error(InitialSetupRepositoryError::Query))
}

async fn user_exists<C>(connection: &C) -> Result<bool, InitialSetupRepositoryError>
where
    C: ConnectionTrait,
{
    users::Entity::find()
        .select_only()
        .column(users::Column::Id)
        .limit(1)
        .one(connection)
        .with_subscriber(NoSubscriber::default())
        .await
        .map(|model| model.is_some())
        .map_err(|_| record_internal_error(InitialSetupRepositoryError::Query))
}

async fn rollback_transaction(
    transaction: DatabaseTransaction,
) -> Result<(), InitialSetupRepositoryError> {
    transaction
        .rollback()
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| record_internal_error(InitialSetupRepositoryError::Query))
}

fn is_unique_violation(error: &DbErr) -> bool {
    matches!(error.sql_err(), Some(SqlErr::UniqueConstraintViolation(_)))
}

fn map_secret_error(error: IdentitySecretError) -> InitialSetupRepositoryError {
    match error {
        IdentitySecretError::Entropy => InitialSetupRepositoryError::Entropy,
        IdentitySecretError::InvalidHash => {
            record_internal_error(InitialSetupRepositoryError::Invariant)
        }
    }
}

/// 只记录闭合内部分类，避免数据库诊断或安装输入进入日志。
fn record_internal_error(error: InitialSetupRepositoryError) -> InitialSetupRepositoryError {
    tracing::error!(
        error_kind = match error {
            InitialSetupRepositoryError::Query => "initial_setup_query",
            InitialSetupRepositoryError::Timeout => "initial_setup_timeout",
            InitialSetupRepositoryError::Invariant => "initial_setup_invariant",
            InitialSetupRepositoryError::Entropy => "initial_setup_entropy",
        },
        "首次安装仓储操作失败"
    );
    error
}
