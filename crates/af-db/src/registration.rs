use std::{fmt, time::Duration};

use af_domain::GroupId;
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseTransaction, DbErr, EntityTrait, QueryFilter, QueryOrder,
    Set, TransactionTrait,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Expr, OnConflict, Query, UpdateStatement},
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool,
    entity::{SensitiveString, groups, options, registration_rate_limits},
};

const REGISTRATION_POLICY_KEY: &str = "system.registration.policy.v1";
const REGISTRATION_POLICY_VERSION: u8 = 1;
const MAX_REGISTRATION_POLICY_BYTES: usize = 2_048;

/// 缺省注册限流允许的窗口内尝试次数。
pub const DEFAULT_REGISTRATION_RATE_LIMIT_ATTEMPTS: u32 = 5;
/// 缺省注册限流窗口秒数。
pub const DEFAULT_REGISTRATION_RATE_LIMIT_WINDOW_SECONDS: u64 = 3_600;
/// 注册限流允许的最大窗口内尝试次数。
pub const MAX_REGISTRATION_RATE_LIMIT_ATTEMPTS: u32 = 100;
/// 注册限流允许的最短窗口秒数。
pub const MIN_REGISTRATION_RATE_LIMIT_WINDOW_SECONDS: u64 = 60;
/// 注册限流允许的最长窗口秒数。
pub const MAX_REGISTRATION_RATE_LIMIT_WINDOW_SECONDS: u64 = 86_400;

/// 可公开读取的注册能力状态，不暴露默认分组、额度或限流细节。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RegistrationStatusRecord {
    enabled: bool,
    email_required: bool,
}

impl RegistrationStatusRecord {
    /// 返回是否允许公开创建普通用户。
    #[must_use]
    pub const fn enabled(self) -> bool {
        self.enabled
    }

    /// 返回注册表单是否必须提交邮箱。
    #[must_use]
    pub const fn email_required(self) -> bool {
        self.email_required
    }
}

/// 已完成持久化校验的注册策略快照。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RegistrationPolicyRecord {
    enabled: bool,
    default_group_id: GroupId,
    initial_quota: i64,
    email_required: bool,
    rate_limit_attempts: u32,
    rate_limit_window_seconds: u64,
}

impl RegistrationPolicyRecord {
    /// 返回是否允许公开注册。
    #[must_use]
    pub const fn enabled(self) -> bool {
        self.enabled
    }

    /// 返回新用户绑定的有效默认分组。
    #[must_use]
    pub const fn default_group_id(self) -> GroupId {
        self.default_group_id
    }

    /// 返回新用户获得的非负初始额度。
    #[must_use]
    pub const fn initial_quota(self) -> i64 {
        self.initial_quota
    }

    /// 返回注册时是否必须填写邮箱。
    #[must_use]
    pub const fn email_required(self) -> bool {
        self.email_required
    }

    /// 返回单个固定窗口内允许的尝试次数。
    #[must_use]
    pub const fn rate_limit_attempts(self) -> u32 {
        self.rate_limit_attempts
    }

    /// 返回固定窗口秒数。
    #[must_use]
    pub const fn rate_limit_window_seconds(self) -> u64 {
        self.rate_limit_window_seconds
    }
}

/// 管理员写入注册策略时使用的完整记录。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RegistrationPolicyWriteRecord {
    enabled: bool,
    default_group_id: GroupId,
    initial_quota: i64,
    email_required: bool,
    rate_limit_attempts: u32,
    rate_limit_window_seconds: u64,
}

impl RegistrationPolicyWriteRecord {
    /// 组合已经由应用层完成字段校验的完整策略。
    #[allow(clippy::too_many_arguments, reason = "字段与注册策略契约一一对应")]
    #[must_use]
    pub const fn new(
        enabled: bool,
        default_group_id: GroupId,
        initial_quota: i64,
        email_required: bool,
        rate_limit_attempts: u32,
        rate_limit_window_seconds: u64,
    ) -> Self {
        Self {
            enabled,
            default_group_id,
            initial_quota,
            email_required,
            rate_limit_attempts,
            rate_limit_window_seconds,
        }
    }

    fn into_policy(self) -> RegistrationPolicyRecord {
        RegistrationPolicyRecord {
            enabled: self.enabled,
            default_group_id: self.default_group_id,
            initial_quota: self.initial_quota,
            email_required: self.email_required,
            rate_limit_attempts: self.rate_limit_attempts,
            rate_limit_window_seconds: self.rate_limit_window_seconds,
        }
    }
}

/// 数据库原子登记一次公开注册尝试后的结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegistrationAttemptOutcome {
    /// 注册开关已关闭或尚未配置，不消耗限流次数。
    Disabled,
    /// 本次尝试已占用限流配额，并固定创建用户所需的策略快照。
    Allowed(RegistrationPolicyRecord),
    /// 当前 IP 指纹已耗尽窗口配额。
    RateLimited { retry_after_seconds: u64 },
}

/// 注册策略仓储构造错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RegistrationRepositoryConfigError {
    /// 零超时无法形成有效的数据库操作截止时间。
    #[error("注册策略数据库操作超时必须大于零")]
    ZeroOperationTimeout,
}

/// 注册策略仓储错误；不携带策略正文、IP 指纹或数据库诊断。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RegistrationRepositoryError {
    /// 查询、事务或写入失败。
    #[error("注册策略数据库操作失败")]
    Query,
    /// 数据库操作超过硬截止时间。
    #[error("注册策略数据库操作超时")]
    Timeout,
    /// 持久化策略或限流状态违反内部不变量。
    #[error("注册策略持久化状态损坏")]
    Invariant,
    /// 管理员提交的策略字段超出公开边界。
    #[error("注册策略字段无效")]
    InvalidPolicy,
    /// 策略引用的默认分组不存在或已软删除。
    #[error("注册策略默认分组无效")]
    InvalidReference,
}

/// 注册策略、公开状态和跨实例固定窗口限流仓储。
#[derive(Clone)]
pub struct RegistrationRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl RegistrationRepository {
    /// 使用共享连接池和单次操作截止时间构造仓储。
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, RegistrationRepositoryConfigError> {
        if operation_timeout.is_zero() {
            return Err(RegistrationRepositoryConfigError::ZeroOperationTimeout);
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }

    /// 读取最小公开注册状态；策略缺失时稳定返回关闭。
    pub async fn status(&self) -> Result<RegistrationStatusRecord, RegistrationRepositoryError> {
        match timeout(self.operation_timeout, self.status_inner()).await {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(RegistrationRepositoryError::Timeout)),
        }
    }

    /// 读取管理员完整策略；尚未配置时预选首个有效分组但保持关闭。
    pub async fn policy(&self) -> Result<RegistrationPolicyRecord, RegistrationRepositoryError> {
        match timeout(self.operation_timeout, self.policy_inner()).await {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(RegistrationRepositoryError::Timeout)),
        }
    }

    /// 原子校验默认分组并覆盖完整注册策略。
    pub async fn update_policy(
        &self,
        record: RegistrationPolicyWriteRecord,
    ) -> Result<RegistrationPolicyRecord, RegistrationRepositoryError> {
        match timeout(self.operation_timeout, self.update_policy_inner(record)).await {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(RegistrationRepositoryError::Timeout)),
        }
    }

    /// 以带密钥的 IP 摘要原子占用一次固定窗口尝试配额。
    pub async fn claim_attempt(
        &self,
        ip_fingerprint: [u8; 32],
        attempted_at: u64,
    ) -> Result<RegistrationAttemptOutcome, RegistrationRepositoryError> {
        match timeout(
            self.operation_timeout,
            self.claim_attempt_inner(ip_fingerprint, attempted_at),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(RegistrationRepositoryError::Timeout)),
        }
    }

    async fn status_inner(&self) -> Result<RegistrationStatusRecord, RegistrationRepositoryError> {
        let policy = load_stored_policy(self.pool.connection()).await?;
        let Some(policy) = policy else {
            return Ok(RegistrationStatusRecord {
                enabled: false,
                email_required: false,
            });
        };
        if policy.enabled
            && !active_group_exists(self.pool.connection(), policy.default_group_id).await?
        {
            return Err(record_internal_error(
                RegistrationRepositoryError::Invariant,
            ));
        }
        Ok(RegistrationStatusRecord {
            enabled: policy.enabled,
            email_required: policy.email_required,
        })
    }

    async fn policy_inner(&self) -> Result<RegistrationPolicyRecord, RegistrationRepositoryError> {
        if let Some(policy) = load_stored_policy(self.pool.connection()).await? {
            if !active_group_exists(self.pool.connection(), policy.default_group_id).await? {
                return Err(record_internal_error(
                    RegistrationRepositoryError::Invariant,
                ));
            }
            return Ok(policy);
        }
        let default_group_id = first_active_group(self.pool.connection()).await?;
        Ok(RegistrationPolicyRecord {
            enabled: false,
            default_group_id,
            initial_quota: 0,
            email_required: false,
            rate_limit_attempts: DEFAULT_REGISTRATION_RATE_LIMIT_ATTEMPTS,
            rate_limit_window_seconds: DEFAULT_REGISTRATION_RATE_LIMIT_WINDOW_SECONDS,
        })
    }

    async fn update_policy_inner(
        &self,
        record: RegistrationPolicyWriteRecord,
    ) -> Result<RegistrationPolicyRecord, RegistrationRepositoryError> {
        let policy = record.into_policy();
        validate_policy(policy).map_err(|_| RegistrationRepositoryError::InvalidPolicy)?;
        let transaction = begin_transaction(&self.pool).await?;
        if !active_group_exists(&transaction, policy.default_group_id).await? {
            return Err(RegistrationRepositoryError::InvalidReference);
        }
        let value = serialize_policy(policy)?;
        let now = TimeDateTimeWithTimeZone::now_utc();
        options::Entity::insert(options::ActiveModel {
            key: Set(REGISTRATION_POLICY_KEY.to_owned()),
            value: Set(SensitiveString::from(value)),
            created_at: Set(now),
            updated_at: Set(now),
        })
        .on_conflict(
            OnConflict::column(options::Column::Key)
                .update_columns([options::Column::Value, options::Column::UpdatedAt])
                .to_owned(),
        )
        .exec_without_returning(&transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|error| record_query_error("registration_policy_upsert", error))?;
        commit_transaction(transaction).await?;
        Ok(policy)
    }

    async fn claim_attempt_inner(
        &self,
        ip_fingerprint: [u8; 32],
        attempted_at: u64,
    ) -> Result<RegistrationAttemptOutcome, RegistrationRepositoryError> {
        let transaction = begin_transaction(&self.pool).await?;
        let Some(policy) = load_stored_policy(&transaction).await? else {
            commit_transaction(transaction).await?;
            return Ok(RegistrationAttemptOutcome::Disabled);
        };
        if !policy.enabled {
            commit_transaction(transaction).await?;
            return Ok(RegistrationAttemptOutcome::Disabled);
        }
        if !active_group_exists(&transaction, policy.default_group_id).await? {
            return Err(record_internal_error(
                RegistrationRepositoryError::Invariant,
            ));
        }

        let window_seconds = policy.rate_limit_window_seconds;
        let window_started_at = attempted_at
            .checked_div(window_seconds)
            .and_then(|window| window.checked_mul(window_seconds))
            .ok_or_else(|| record_internal_error(RegistrationRepositoryError::Invariant))?;
        let window_started_at = i64::try_from(window_started_at)
            .map_err(|_| record_internal_error(RegistrationRepositoryError::Invariant))?;
        let attempted_at_i64 = i64::try_from(attempted_at)
            .map_err(|_| record_internal_error(RegistrationRepositoryError::Invariant))?;
        let now = TimeDateTimeWithTimeZone::from_unix_timestamp(attempted_at_i64)
            .map_err(|_| record_internal_error(RegistrationRepositoryError::Invariant))?;
        let fingerprint = encode_lower_hex(&ip_fingerprint);

        // 先幂等建立零计数行，再用条件更新完成唯一的原子放行判定。
        registration_rate_limits::Entity::insert(registration_rate_limits::ActiveModel {
            ip_fingerprint: Set(SensitiveString::from(fingerprint.as_str())),
            window_started_at: Set(window_started_at),
            attempts: Set(0),
            created_at: Set(now),
            updated_at: Set(now),
        })
        .on_conflict(registration_attempt_on_conflict())
        .exec_without_returning(&transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|error| record_query_error("registration_attempt_insert", error))?;

        let max_attempts = i32::try_from(policy.rate_limit_attempts)
            .map_err(|_| record_internal_error(RegistrationRepositoryError::Invariant))?;
        let update = claim_attempt_update(&fingerprint, window_started_at, max_attempts, now);
        let result = transaction
            .execute(transaction.get_database_backend().build(&update))
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|error| record_query_error("registration_attempt_update", error))?;
        transaction
            .commit()
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|error| record_query_error("registration_attempt_commit", error))?;

        match result.rows_affected() {
            1 => Ok(RegistrationAttemptOutcome::Allowed(policy)),
            0 => {
                let window_end = u64::try_from(window_started_at)
                    .ok()
                    .and_then(|start| start.checked_add(window_seconds))
                    .ok_or_else(|| record_internal_error(RegistrationRepositoryError::Invariant))?;
                Ok(RegistrationAttemptOutcome::RateLimited {
                    retry_after_seconds: window_end.saturating_sub(attempted_at).max(1),
                })
            }
            _ => Err(record_internal_error(
                RegistrationRepositoryError::Invariant,
            )),
        }
    }
}

fn registration_attempt_on_conflict() -> OnConflict {
    OnConflict::column(registration_rate_limits::Column::IpFingerprint)
        // SeaQuery 的无参数 do_nothing 会生成 MySQL 不支持的 ON DUPLICATE KEY IGNORE。
        .do_nothing_on([registration_rate_limits::Column::IpFingerprint])
        .to_owned()
}

fn claim_attempt_update(
    fingerprint: &str,
    window_started_at: i64,
    max_attempts: i32,
    now: TimeDateTimeWithTimeZone,
) -> UpdateStatement {
    Query::update()
        .table(registration_rate_limits::Entity)
        // MySQL 按从左到右执行赋值，因此必须先基于旧窗口计算次数，再覆盖窗口起点。
        .value(
            registration_rate_limits::Column::Attempts,
            Expr::case(
                Expr::col(registration_rate_limits::Column::WindowStartedAt)
                    .ne(window_started_at),
                1_i32,
            )
            .finally(Expr::col(registration_rate_limits::Column::Attempts).add(1_i32)),
        )
        .value(
            registration_rate_limits::Column::WindowStartedAt,
            window_started_at,
        )
        .value(registration_rate_limits::Column::UpdatedAt, now)
        .and_where(Expr::col(registration_rate_limits::Column::IpFingerprint).eq(fingerprint))
        .and_where(
            Expr::col(registration_rate_limits::Column::WindowStartedAt)
                .ne(window_started_at)
                .or(Expr::col(registration_rate_limits::Column::Attempts).lt(max_attempts)),
        )
        .to_owned()
}

impl fmt::Debug for RegistrationRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RegistrationRepository")
            .field("operation_timeout", &self.operation_timeout)
            .finish_non_exhaustive()
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StoredRegistrationPolicy {
    version: u8,
    enabled: bool,
    default_group_id: i64,
    initial_quota: i64,
    email_required: bool,
    rate_limit_attempts: u32,
    rate_limit_window_seconds: u64,
}

async fn begin_transaction(
    pool: &DatabasePool,
) -> Result<DatabaseTransaction, RegistrationRepositoryError> {
    pool.connection()
        .begin()
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| record_internal_error(RegistrationRepositoryError::Query))
}

async fn commit_transaction(
    transaction: DatabaseTransaction,
) -> Result<(), RegistrationRepositoryError> {
    transaction
        .commit()
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| record_internal_error(RegistrationRepositoryError::Query))
}

async fn load_stored_policy<C>(
    connection: &C,
) -> Result<Option<RegistrationPolicyRecord>, RegistrationRepositoryError>
where
    C: ConnectionTrait,
{
    let model = options::Entity::find_by_id(REGISTRATION_POLICY_KEY)
        .one(connection)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| record_internal_error(RegistrationRepositoryError::Query))?;
    model
        .map(|model| parse_policy(model.value.as_str()))
        .transpose()
}

fn parse_policy(value: &str) -> Result<RegistrationPolicyRecord, RegistrationRepositoryError> {
    if value.len() > MAX_REGISTRATION_POLICY_BYTES {
        return Err(record_internal_error(
            RegistrationRepositoryError::Invariant,
        ));
    }
    let stored = serde_json::from_str::<StoredRegistrationPolicy>(value)
        .map_err(|_| record_internal_error(RegistrationRepositoryError::Invariant))?;
    if stored.version != REGISTRATION_POLICY_VERSION {
        return Err(record_internal_error(
            RegistrationRepositoryError::Invariant,
        ));
    }
    let policy = RegistrationPolicyRecord {
        enabled: stored.enabled,
        default_group_id: GroupId::new(stored.default_group_id)
            .map_err(|_| record_internal_error(RegistrationRepositoryError::Invariant))?,
        initial_quota: stored.initial_quota,
        email_required: stored.email_required,
        rate_limit_attempts: stored.rate_limit_attempts,
        rate_limit_window_seconds: stored.rate_limit_window_seconds,
    };
    validate_policy(policy)
        .map(|()| policy)
        .map_err(|_| record_internal_error(RegistrationRepositoryError::Invariant))
}

fn serialize_policy(
    policy: RegistrationPolicyRecord,
) -> Result<String, RegistrationRepositoryError> {
    let stored = StoredRegistrationPolicy {
        version: REGISTRATION_POLICY_VERSION,
        enabled: policy.enabled,
        default_group_id: policy.default_group_id.get(),
        initial_quota: policy.initial_quota,
        email_required: policy.email_required,
        rate_limit_attempts: policy.rate_limit_attempts,
        rate_limit_window_seconds: policy.rate_limit_window_seconds,
    };
    let value = serde_json::to_string(&stored)
        .map_err(|_| record_internal_error(RegistrationRepositoryError::Invariant))?;
    if value.len() > MAX_REGISTRATION_POLICY_BYTES {
        return Err(record_internal_error(
            RegistrationRepositoryError::Invariant,
        ));
    }
    Ok(value)
}

fn validate_policy(policy: RegistrationPolicyRecord) -> Result<(), ()> {
    if policy.initial_quota < 0
        || !(1..=MAX_REGISTRATION_RATE_LIMIT_ATTEMPTS).contains(&policy.rate_limit_attempts)
        || !(MIN_REGISTRATION_RATE_LIMIT_WINDOW_SECONDS
            ..=MAX_REGISTRATION_RATE_LIMIT_WINDOW_SECONDS)
            .contains(&policy.rate_limit_window_seconds)
    {
        return Err(());
    }
    Ok(())
}

async fn active_group_exists<C>(
    connection: &C,
    group_id: GroupId,
) -> Result<bool, RegistrationRepositoryError>
where
    C: ConnectionTrait,
{
    groups::Entity::find_by_id(group_id.get())
        .filter(groups::Column::DeletedAt.is_null())
        .one(connection)
        .with_subscriber(NoSubscriber::default())
        .await
        .map(|group| group.is_some())
        .map_err(|_| record_internal_error(RegistrationRepositoryError::Query))
}

async fn first_active_group<C>(connection: &C) -> Result<GroupId, RegistrationRepositoryError>
where
    C: ConnectionTrait,
{
    let group = groups::Entity::find()
        .filter(groups::Column::DeletedAt.is_null())
        .order_by_asc(groups::Column::Id)
        .one(connection)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| record_internal_error(RegistrationRepositoryError::Query))?
        .ok_or_else(|| record_internal_error(RegistrationRepositoryError::Invariant))?;
    GroupId::new(group.id)
        .map_err(|_| record_internal_error(RegistrationRepositoryError::Invariant))
}

fn encode_lower_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

fn record_internal_error(error: RegistrationRepositoryError) -> RegistrationRepositoryError {
    if matches!(
        error,
        RegistrationRepositoryError::Query
            | RegistrationRepositoryError::Timeout
            | RegistrationRepositoryError::Invariant
    ) {
        tracing::error!(
            error_kind = match error {
                RegistrationRepositoryError::Query => "registration_query",
                RegistrationRepositoryError::Timeout => "registration_timeout",
                RegistrationRepositoryError::Invariant => "registration_invariant",
                RegistrationRepositoryError::InvalidPolicy => "registration_invalid_policy",
                RegistrationRepositoryError::InvalidReference => {
                    "registration_invalid_reference"
                }
            },
            "注册策略仓储操作失败"
        );
    }
    error
}

fn record_query_error(operation: &'static str, error: DbErr) -> RegistrationRepositoryError {
    #[cfg(test)]
    eprintln!("注册数据库测试失败：operation={operation} error={error}");
    #[cfg(not(test))]
    let _ = error;
    tracing::error!(
        error_kind = "registration_query",
        operation,
        "注册策略仓储数据库操作失败"
    );
    RegistrationRepositoryError::Query
}

#[cfg(test)]
mod sql_tests {
    use sea_orm::sea_query::MysqlQueryBuilder;

    use super::*;

    #[test]
    fn mysql_attempt_insert_uses_supported_duplicate_key_noop() {
        let statement = Query::insert()
            .into_table(registration_rate_limits::Entity)
            .columns([registration_rate_limits::Column::IpFingerprint])
            .values_panic(["11".repeat(32).into()])
            .on_conflict(registration_attempt_on_conflict())
            .to_owned()
            .to_string(MysqlQueryBuilder);

        assert!(
            statement.contains("ON DUPLICATE KEY UPDATE `ip_fingerprint` = `ip_fingerprint`"),
            "{statement}"
        );
        assert!(!statement.contains("IGNORE"), "{statement}");
    }

    #[test]
    fn mysql_attempt_update_keeps_counter_assignment_before_window_assignment() {
        let statement = claim_attempt_update(
            &"11".repeat(32),
            120,
            5,
            TimeDateTimeWithTimeZone::from_unix_timestamp(121).unwrap(),
        )
        .to_string(MysqlQueryBuilder);

        let attempts = statement.find("`attempts` =").unwrap();
        let window = statement.find(", `window_started_at` =").unwrap();
        assert!(attempts < window, "{statement}");
        assert!(statement.contains("`attempts` < 5"), "{statement}");
    }
}
