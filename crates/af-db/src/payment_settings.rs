use std::{fmt, time::Duration};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseTransaction, DbBackend, EntityTrait,
    QueryFilter, QuerySelect, Set, TransactionTrait,
    sea_query::{Expr, LockType},
};
use serde_json::json;
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};
use url::Url;

use crate::{
    DatabasePool, EncryptedCredentialEnvelope,
    entity::{EncryptedJson, SensitiveString, payment_settings},
};

const PAYMENT_SETTINGS_ID: i16 = 1;
const MAX_PUBLISHABLE_KEY_BYTES: usize = 512;
const MAX_GATEWAY_URL_BYTES: usize = 2_048;
const MAX_MERCHANT_ID_BYTES: usize = 128;

/// 支付密钥在管理员完整保存中的处理方式。
pub enum PaymentSecretUpdate {
    Keep,
    Replace(EncryptedCredentialEnvelope),
    Clear,
}

impl fmt::Debug for PaymentSecretUpdate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Keep => "PaymentSecretUpdate::Keep",
            Self::Replace(_) => "PaymentSecretUpdate::Replace(<已脱敏>)",
            Self::Clear => "PaymentSecretUpdate::Clear",
        })
    }
}

/// 已完成持久化校验的在线支付设置快照。
#[derive(Clone, Eq, PartialEq)]
pub struct PaymentSettingsRecord {
    initialized: bool,
    stripe_enabled: bool,
    stripe_publishable_key: Option<String>,
    stripe_secret_key: Option<EncryptedCredentialEnvelope>,
    stripe_webhook_secret: Option<EncryptedCredentialEnvelope>,
    stripe_signature_tolerance_seconds: u16,
    epay_enabled: bool,
    epay_gateway_url: Option<String>,
    epay_merchant_id: Option<String>,
    epay_merchant_key: Option<EncryptedCredentialEnvelope>,
    epay_alipay_enabled: bool,
    epay_wxpay_enabled: bool,
    epay_qr_enabled: bool,
    epay_refund_enabled: bool,
    refund_auto_submit_enabled: bool,
    epay_quota_per_cny: i64,
    version: i64,
}

impl PaymentSettingsRecord {
    #[must_use]
    pub const fn initialized(&self) -> bool {
        self.initialized
    }

    #[must_use]
    pub const fn stripe_enabled(&self) -> bool {
        self.stripe_enabled
    }

    #[must_use]
    pub fn stripe_publishable_key(&self) -> Option<&str> {
        self.stripe_publishable_key.as_deref()
    }

    #[must_use]
    pub const fn stripe_secret_key_configured(&self) -> bool {
        self.stripe_secret_key.is_some()
    }

    #[must_use]
    pub fn stripe_secret_key(&self) -> Option<&EncryptedCredentialEnvelope> {
        self.stripe_secret_key.as_ref()
    }

    #[must_use]
    pub const fn stripe_webhook_secret_configured(&self) -> bool {
        self.stripe_webhook_secret.is_some()
    }

    #[must_use]
    pub fn stripe_webhook_secret(&self) -> Option<&EncryptedCredentialEnvelope> {
        self.stripe_webhook_secret.as_ref()
    }

    #[must_use]
    pub const fn stripe_signature_tolerance_seconds(&self) -> u16 {
        self.stripe_signature_tolerance_seconds
    }

    #[must_use]
    pub const fn epay_enabled(&self) -> bool {
        self.epay_enabled
    }

    #[must_use]
    pub fn epay_gateway_url(&self) -> Option<&str> {
        self.epay_gateway_url.as_deref()
    }

    #[must_use]
    pub fn epay_merchant_id(&self) -> Option<&str> {
        self.epay_merchant_id.as_deref()
    }

    #[must_use]
    pub const fn epay_merchant_key_configured(&self) -> bool {
        self.epay_merchant_key.is_some()
    }

    #[must_use]
    pub fn epay_merchant_key(&self) -> Option<&EncryptedCredentialEnvelope> {
        self.epay_merchant_key.as_ref()
    }

    #[must_use]
    pub const fn epay_alipay_enabled(&self) -> bool {
        self.epay_alipay_enabled
    }

    #[must_use]
    pub const fn epay_wxpay_enabled(&self) -> bool {
        self.epay_wxpay_enabled
    }

    #[must_use]
    pub const fn epay_qr_enabled(&self) -> bool {
        self.epay_qr_enabled
    }

    #[must_use]
    pub const fn epay_refund_enabled(&self) -> bool {
        self.epay_refund_enabled
    }

    #[must_use]
    pub const fn refund_auto_submit_enabled(&self) -> bool {
        self.refund_auto_submit_enabled
    }

    #[must_use]
    pub const fn epay_quota_per_cny(&self) -> i64 {
        self.epay_quota_per_cny
    }

    #[must_use]
    pub const fn version(&self) -> i64 {
        self.version
    }
}

impl fmt::Debug for PaymentSettingsRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PaymentSettingsRecord")
            .field("initialized", &self.initialized)
            .field("stripe_enabled", &self.stripe_enabled)
            .field(
                "stripe_publishable_key",
                &self.stripe_publishable_key.as_ref().map(|_| "<已脱敏>"),
            )
            .field(
                "stripe_secret_key_configured",
                &self.stripe_secret_key_configured(),
            )
            .field(
                "stripe_webhook_secret_configured",
                &self.stripe_webhook_secret_configured(),
            )
            .field(
                "stripe_signature_tolerance_seconds",
                &self.stripe_signature_tolerance_seconds,
            )
            .field("epay_enabled", &self.epay_enabled)
            .field(
                "epay_gateway_url",
                &self.epay_gateway_url.as_ref().map(|_| "<已脱敏>"),
            )
            .field(
                "epay_merchant_id",
                &self.epay_merchant_id.as_ref().map(|_| "<已脱敏>"),
            )
            .field(
                "epay_merchant_key_configured",
                &self.epay_merchant_key_configured(),
            )
            .field("epay_alipay_enabled", &self.epay_alipay_enabled)
            .field("epay_wxpay_enabled", &self.epay_wxpay_enabled)
            .field("epay_qr_enabled", &self.epay_qr_enabled)
            .field("epay_refund_enabled", &self.epay_refund_enabled)
            .field(
                "refund_auto_submit_enabled",
                &self.refund_auto_submit_enabled,
            )
            .field("epay_quota_per_cny", &self.epay_quota_per_cny)
            .field("version", &self.version)
            .finish()
    }
}

/// 管理员完整覆盖在线支付设置时使用的写入记录。
pub struct PaymentSettingsWriteRecord {
    expected_version: i64,
    stripe_enabled: bool,
    stripe_publishable_key: Option<String>,
    stripe_secret_key: PaymentSecretUpdate,
    stripe_webhook_secret: PaymentSecretUpdate,
    stripe_signature_tolerance_seconds: u16,
    epay_enabled: bool,
    epay_gateway_url: Option<String>,
    epay_merchant_id: Option<String>,
    epay_merchant_key: PaymentSecretUpdate,
    epay_alipay_enabled: bool,
    epay_wxpay_enabled: bool,
    epay_qr_enabled: bool,
    epay_refund_enabled: bool,
    refund_auto_submit_enabled: bool,
    epay_quota_per_cny: i64,
}

impl PaymentSettingsWriteRecord {
    #[allow(clippy::too_many_arguments, reason = "字段与支付设置契约一一对应")]
    #[must_use]
    pub fn new(
        expected_version: i64,
        stripe_enabled: bool,
        stripe_publishable_key: Option<String>,
        stripe_secret_key: PaymentSecretUpdate,
        stripe_webhook_secret: PaymentSecretUpdate,
        stripe_signature_tolerance_seconds: u16,
        epay_enabled: bool,
        epay_gateway_url: Option<String>,
        epay_merchant_id: Option<String>,
        epay_merchant_key: PaymentSecretUpdate,
        epay_alipay_enabled: bool,
        epay_wxpay_enabled: bool,
        epay_qr_enabled: bool,
        epay_refund_enabled: bool,
        refund_auto_submit_enabled: bool,
        epay_quota_per_cny: i64,
    ) -> Self {
        Self {
            expected_version,
            stripe_enabled,
            stripe_publishable_key,
            stripe_secret_key,
            stripe_webhook_secret,
            stripe_signature_tolerance_seconds,
            epay_enabled,
            epay_gateway_url,
            epay_merchant_id,
            epay_merchant_key,
            epay_alipay_enabled,
            epay_wxpay_enabled,
            epay_qr_enabled,
            epay_refund_enabled,
            refund_auto_submit_enabled,
            epay_quota_per_cny,
        }
    }

    fn validate(&self) -> Result<(), PaymentSettingsRepositoryError> {
        if self.expected_version < 1
            || !(30..=900).contains(&self.stripe_signature_tolerance_seconds)
            || self.epay_quota_per_cny <= 0
            || !valid_optional_publishable_key(self.stripe_publishable_key.as_deref())
            || !valid_optional_gateway_url(self.epay_gateway_url.as_deref())
            || !valid_optional_merchant_id(self.epay_merchant_id.as_deref())
            || (self.epay_enabled && !self.epay_alipay_enabled && !self.epay_wxpay_enabled)
            || (self.epay_refund_enabled && !self.epay_enabled)
            || (self.refund_auto_submit_enabled && !self.epay_refund_enabled)
        {
            return Err(PaymentSettingsRepositoryError::InvalidSettings);
        }
        Ok(())
    }
}

impl fmt::Debug for PaymentSettingsWriteRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PaymentSettingsWriteRecord")
            .field("expected_version", &self.expected_version)
            .field("stripe_enabled", &self.stripe_enabled)
            .field(
                "stripe_publishable_key",
                &self.stripe_publishable_key.as_ref().map(|_| "<已脱敏>"),
            )
            .field("stripe_secret_key", &self.stripe_secret_key)
            .field("stripe_webhook_secret", &self.stripe_webhook_secret)
            .field("epay_enabled", &self.epay_enabled)
            .field(
                "epay_gateway_url",
                &self.epay_gateway_url.as_ref().map(|_| "<已脱敏>"),
            )
            .field(
                "epay_merchant_id",
                &self.epay_merchant_id.as_ref().map(|_| "<已脱敏>"),
            )
            .field("epay_merchant_key", &self.epay_merchant_key)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum PaymentSettingsRepositoryConfigError {
    #[error("支付设置数据库操作超时必须大于零")]
    ZeroOperationTimeout,
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum PaymentSettingsRepositoryError {
    #[error("支付设置数据库操作失败")]
    Query,
    #[error("支付设置数据库操作超时")]
    Timeout,
    #[error("支付设置持久化状态损坏")]
    Invariant,
    #[error("支付设置字段无效")]
    InvalidSettings,
    #[error("支付设置版本已变化")]
    ConcurrentUpdate,
}

/// 在线支付设置的固定记录仓储。
#[derive(Clone)]
pub struct PaymentSettingsRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl PaymentSettingsRepository {
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, PaymentSettingsRepositoryConfigError> {
        if operation_timeout.is_zero() {
            return Err(PaymentSettingsRepositoryConfigError::ZeroOperationTimeout);
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }

    pub async fn settings(&self) -> Result<PaymentSettingsRecord, PaymentSettingsRepositoryError> {
        match timeout(self.operation_timeout, self.settings_inner()).await {
            Ok(result) => result,
            Err(_) => Err(internal(PaymentSettingsRepositoryError::Timeout)),
        }
    }

    pub async fn update(
        &self,
        record: PaymentSettingsWriteRecord,
    ) -> Result<PaymentSettingsRecord, PaymentSettingsRepositoryError> {
        match timeout(self.operation_timeout, self.update_inner(record)).await {
            Ok(result) => result,
            Err(_) => Err(internal(PaymentSettingsRepositoryError::Timeout)),
        }
    }

    pub async fn restore_if_version(
        &self,
        expected_version: i64,
        record: &PaymentSettingsRecord,
    ) -> Result<(), PaymentSettingsRepositoryError> {
        match timeout(
            self.operation_timeout,
            self.restore_if_version_inner(expected_version, record),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(internal(PaymentSettingsRepositoryError::Timeout)),
        }
    }

    async fn settings_inner(
        &self,
    ) -> Result<PaymentSettingsRecord, PaymentSettingsRepositoryError> {
        payment_settings::Entity::find_by_id(PAYMENT_SETTINGS_ID)
            .one(self.pool.connection())
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query("payment_settings_read"))?
            .ok_or_else(|| internal(PaymentSettingsRepositoryError::Invariant))
            .and_then(record_from_model)
    }

    async fn update_inner(
        &self,
        record: PaymentSettingsWriteRecord,
    ) -> Result<PaymentSettingsRecord, PaymentSettingsRepositoryError> {
        record.validate()?;
        let transaction = self
            .pool
            .connection()
            .begin()
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query("payment_settings_begin"))?;
        let existing = lock_settings(&transaction).await?;
        if existing.version != record.expected_version {
            return Err(PaymentSettingsRepositoryError::ConcurrentUpdate);
        }

        let stripe_secret_key =
            resolve_secret(existing.stripe_secret_key, record.stripe_secret_key)?;
        let stripe_webhook_secret =
            resolve_secret(existing.stripe_webhook_secret, record.stripe_webhook_secret)?;
        let epay_merchant_key =
            resolve_secret(existing.epay_merchant_key, record.epay_merchant_key)?;
        validate_resolved_shape(
            record.stripe_enabled,
            record.stripe_publishable_key.as_deref(),
            stripe_secret_key.as_ref(),
            stripe_webhook_secret.as_ref(),
            record.epay_enabled,
            record.epay_gateway_url.as_deref(),
            record.epay_merchant_id.as_deref(),
            epay_merchant_key.as_ref(),
            record.epay_alipay_enabled,
            record.epay_wxpay_enabled,
            record.epay_refund_enabled,
            record.refund_auto_submit_enabled,
        )?;

        let version = existing
            .version
            .checked_add(1)
            .ok_or_else(|| internal(PaymentSettingsRepositoryError::Invariant))?;
        let now = sea_orm::entity::prelude::TimeDateTimeWithTimeZone::now_utc();
        let saved = payment_settings::ActiveModel {
            id: Set(PAYMENT_SETTINGS_ID),
            initialized: Set(true),
            stripe_enabled: Set(record.stripe_enabled),
            stripe_publishable_key: Set(record.stripe_publishable_key.map(SensitiveString::from)),
            stripe_secret_key: Set(stripe_secret_key),
            stripe_webhook_secret: Set(stripe_webhook_secret),
            stripe_signature_tolerance_seconds: Set(i32::from(
                record.stripe_signature_tolerance_seconds,
            )),
            epay_enabled: Set(record.epay_enabled),
            epay_gateway_url: Set(record.epay_gateway_url.map(SensitiveString::from)),
            epay_merchant_id: Set(record.epay_merchant_id.map(SensitiveString::from)),
            epay_merchant_key: Set(epay_merchant_key),
            epay_alipay_enabled: Set(record.epay_alipay_enabled),
            epay_wxpay_enabled: Set(record.epay_wxpay_enabled),
            epay_qr_enabled: Set(record.epay_qr_enabled),
            epay_refund_enabled: Set(record.epay_refund_enabled),
            refund_auto_submit_enabled: Set(record.refund_auto_submit_enabled),
            epay_quota_per_cny: Set(record.epay_quota_per_cny),
            version: Set(version),
            created_at: Set(existing.created_at),
            updated_at: Set(now),
        }
        .update(&transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| query("payment_settings_write"))?;
        let saved = record_from_model(saved)?;
        transaction
            .commit()
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query("payment_settings_commit"))?;
        Ok(saved)
    }

    async fn restore_if_version_inner(
        &self,
        expected_version: i64,
        record: &PaymentSettingsRecord,
    ) -> Result<(), PaymentSettingsRepositoryError> {
        if expected_version < 1 || record.version < 1 {
            return Err(internal(PaymentSettingsRepositoryError::Invariant));
        }
        let transaction = self
            .pool
            .connection()
            .begin()
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query("payment_settings_restore_begin"))?;
        let existing = lock_settings(&transaction).await?;
        if existing.version != expected_version {
            return Err(PaymentSettingsRepositoryError::ConcurrentUpdate);
        }
        let version = expected_version
            .checked_add(1)
            .ok_or_else(|| internal(PaymentSettingsRepositoryError::Invariant))?;
        let now = sea_orm::entity::prelude::TimeDateTimeWithTimeZone::now_utc();
        payment_settings::ActiveModel {
            id: Set(PAYMENT_SETTINGS_ID),
            initialized: Set(record.initialized),
            stripe_enabled: Set(record.stripe_enabled),
            stripe_publishable_key: Set(record
                .stripe_publishable_key
                .clone()
                .map(SensitiveString::from)),
            stripe_secret_key: Set(record
                .stripe_secret_key
                .clone()
                .map(encrypted_json)
                .transpose()?),
            stripe_webhook_secret: Set(record
                .stripe_webhook_secret
                .clone()
                .map(encrypted_json)
                .transpose()?),
            stripe_signature_tolerance_seconds: Set(i32::from(
                record.stripe_signature_tolerance_seconds,
            )),
            epay_enabled: Set(record.epay_enabled),
            epay_gateway_url: Set(record.epay_gateway_url.clone().map(SensitiveString::from)),
            epay_merchant_id: Set(record.epay_merchant_id.clone().map(SensitiveString::from)),
            epay_merchant_key: Set(record
                .epay_merchant_key
                .clone()
                .map(encrypted_json)
                .transpose()?),
            epay_alipay_enabled: Set(record.epay_alipay_enabled),
            epay_wxpay_enabled: Set(record.epay_wxpay_enabled),
            epay_qr_enabled: Set(record.epay_qr_enabled),
            epay_refund_enabled: Set(record.epay_refund_enabled),
            refund_auto_submit_enabled: Set(record.refund_auto_submit_enabled),
            epay_quota_per_cny: Set(record.epay_quota_per_cny),
            version: Set(version),
            created_at: Set(existing.created_at),
            updated_at: Set(now),
        }
        .update(&transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| query("payment_settings_restore_write"))?;
        transaction
            .commit()
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query("payment_settings_restore_commit"))?;
        Ok(())
    }
}

async fn lock_settings(
    transaction: &DatabaseTransaction,
) -> Result<payment_settings::Model, PaymentSettingsRepositoryError> {
    if transaction.get_database_backend() == DbBackend::Sqlite {
        let result = payment_settings::Entity::update_many()
            .filter(payment_settings::Column::Id.eq(PAYMENT_SETTINGS_ID))
            .col_expr(
                payment_settings::Column::Version,
                Expr::col(payment_settings::Column::Version).into(),
            )
            .exec(transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(|_| query("payment_settings_lock"))?;
        if result.rows_affected != 1 {
            return Err(internal(PaymentSettingsRepositoryError::Invariant));
        }
    }
    let mut query = payment_settings::Entity::find()
        .filter(payment_settings::Column::Id.eq(PAYMENT_SETTINGS_ID));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(|_| query_error("payment_settings_read_for_update"))?
        .ok_or_else(|| internal(PaymentSettingsRepositoryError::Invariant))
}

fn record_from_model(
    model: payment_settings::Model,
) -> Result<PaymentSettingsRecord, PaymentSettingsRepositoryError> {
    let stripe_publishable_key = model
        .stripe_publishable_key
        .map(|value| value.as_str().to_owned());
    let stripe_secret_key = model
        .stripe_secret_key
        .map(envelope_from_json)
        .transpose()?;
    let stripe_webhook_secret = model
        .stripe_webhook_secret
        .map(envelope_from_json)
        .transpose()?;
    let epay_gateway_url = model
        .epay_gateway_url
        .map(|value| value.as_str().to_owned());
    let epay_merchant_id = model
        .epay_merchant_id
        .map(|value| value.as_str().to_owned());
    let epay_merchant_key = model
        .epay_merchant_key
        .map(envelope_from_json)
        .transpose()?;
    let tolerance = u16::try_from(model.stripe_signature_tolerance_seconds)
        .map_err(|_| internal(PaymentSettingsRepositoryError::Invariant))?;
    validate_resolved_shape(
        model.stripe_enabled,
        stripe_publishable_key.as_deref(),
        stripe_secret_key.as_ref(),
        stripe_webhook_secret.as_ref(),
        model.epay_enabled,
        epay_gateway_url.as_deref(),
        epay_merchant_id.as_deref(),
        epay_merchant_key.as_ref(),
        model.epay_alipay_enabled,
        model.epay_wxpay_enabled,
        model.epay_refund_enabled,
        model.refund_auto_submit_enabled,
    )
    .map_err(|_| internal(PaymentSettingsRepositoryError::Invariant))?;
    if model.id != PAYMENT_SETTINGS_ID
        || model.version < 1
        || !(30..=900).contains(&tolerance)
        || model.epay_quota_per_cny <= 0
        || !valid_optional_publishable_key(stripe_publishable_key.as_deref())
        || !valid_optional_gateway_url(epay_gateway_url.as_deref())
        || !valid_optional_merchant_id(epay_merchant_id.as_deref())
    {
        return Err(internal(PaymentSettingsRepositoryError::Invariant));
    }
    Ok(PaymentSettingsRecord {
        initialized: model.initialized,
        stripe_enabled: model.stripe_enabled,
        stripe_publishable_key,
        stripe_secret_key,
        stripe_webhook_secret,
        stripe_signature_tolerance_seconds: tolerance,
        epay_enabled: model.epay_enabled,
        epay_gateway_url,
        epay_merchant_id,
        epay_merchant_key,
        epay_alipay_enabled: model.epay_alipay_enabled,
        epay_wxpay_enabled: model.epay_wxpay_enabled,
        epay_qr_enabled: model.epay_qr_enabled,
        epay_refund_enabled: model.epay_refund_enabled,
        refund_auto_submit_enabled: model.refund_auto_submit_enabled,
        epay_quota_per_cny: model.epay_quota_per_cny,
        version: model.version,
    })
}

fn resolve_secret(
    existing: Option<EncryptedJson>,
    update: PaymentSecretUpdate,
) -> Result<Option<EncryptedJson>, PaymentSettingsRepositoryError> {
    match update {
        PaymentSecretUpdate::Keep => Ok(existing),
        PaymentSecretUpdate::Replace(envelope) => encrypted_json(envelope).map(Some),
        PaymentSecretUpdate::Clear => Ok(None),
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "显式校验两个 Provider 的完整启用形态"
)]
fn validate_resolved_shape<T>(
    stripe_enabled: bool,
    stripe_publishable_key: Option<&str>,
    stripe_secret_key: Option<&T>,
    stripe_webhook_secret: Option<&T>,
    epay_enabled: bool,
    epay_gateway_url: Option<&str>,
    epay_merchant_id: Option<&str>,
    epay_merchant_key: Option<&T>,
    epay_alipay_enabled: bool,
    epay_wxpay_enabled: bool,
    epay_refund_enabled: bool,
    refund_auto_submit_enabled: bool,
) -> Result<(), PaymentSettingsRepositoryError> {
    if (stripe_enabled
        && (stripe_publishable_key.is_none()
            || stripe_secret_key.is_none()
            || stripe_webhook_secret.is_none()))
        || (epay_enabled
            && (epay_gateway_url.is_none()
                || epay_merchant_id.is_none()
                || epay_merchant_key.is_none()
                || (!epay_alipay_enabled && !epay_wxpay_enabled)))
        || (epay_refund_enabled && !epay_enabled)
        || (refund_auto_submit_enabled && !epay_refund_enabled)
    {
        return Err(PaymentSettingsRepositoryError::InvalidSettings);
    }
    Ok(())
}

fn valid_optional_publishable_key(value: Option<&str>) -> bool {
    value.is_none_or(|value| {
        !value.is_empty()
            && value.len() <= MAX_PUBLISHABLE_KEY_BYTES
            && value.bytes().all(|byte| (0x21..=0x7e).contains(&byte))
            && (value.starts_with("pk_test_") || value.starts_with("pk_live_"))
    })
}

fn valid_optional_gateway_url(value: Option<&str>) -> bool {
    value.is_none_or(|value| {
        if value.is_empty() || value.len() > MAX_GATEWAY_URL_BYTES || value.ends_with('/') {
            return false;
        }
        Url::parse(value).is_ok_and(|url| {
            url.scheme() == "https"
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none()
        })
    })
}

fn valid_optional_merchant_id(value: Option<&str>) -> bool {
    value.is_none_or(|value| {
        !value.is_empty()
            && value.len() <= MAX_MERCHANT_ID_BYTES
            && value.trim() == value
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
    })
}

fn encrypted_json(
    envelope: EncryptedCredentialEnvelope,
) -> Result<EncryptedJson, PaymentSettingsRepositoryError> {
    EncryptedJson::from_envelope(json!({
        "version": 1,
        "algorithm": "xchacha20poly1305",
        "key_id": envelope.key_id(),
        "nonce": URL_SAFE_NO_PAD.encode(envelope.nonce()),
        "ciphertext": URL_SAFE_NO_PAD.encode(envelope.ciphertext()),
    }))
    .map_err(|_| internal(PaymentSettingsRepositoryError::Invariant))
}

fn envelope_from_json(
    secret: EncryptedJson,
) -> Result<EncryptedCredentialEnvelope, PaymentSettingsRepositoryError> {
    let (key_id, nonce, ciphertext) = secret
        .envelope_parts()
        .map_err(|_| internal(PaymentSettingsRepositoryError::Invariant))?;
    EncryptedCredentialEnvelope::new(key_id, nonce, ciphertext)
        .map_err(|_| internal(PaymentSettingsRepositoryError::Invariant))
}

fn query(operation: &'static str) -> PaymentSettingsRepositoryError {
    query_error(operation)
}

fn query_error(operation: &'static str) -> PaymentSettingsRepositoryError {
    tracing::error!(
        target: "af_db::payment_settings",
        error_kind = operation,
        "支付设置数据库操作失败"
    );
    PaymentSettingsRepositoryError::Query
}

fn internal(error: PaymentSettingsRepositoryError) -> PaymentSettingsRepositoryError {
    tracing::error!(
        target: "af_db::payment_settings",
        error_kind = ?error,
        "支付设置内部状态无效"
    );
    error
}
