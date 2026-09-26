use std::time::Duration;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, sea_query::Expr};
use serde_json::json;
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool, EncryptedCredentialEnvelope,
    entity::{EncryptedJson, account_verification_settings as settings},
};

#[derive(Clone)]
pub struct AccountVerificationSettingsRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

#[derive(Debug, Error)]
pub enum AccountVerificationSettingsError {
    #[error("实名认证设置无效")]
    Invalid,
    #[error("实名认证设置版本已变化")]
    Conflict,
    #[error("实名认证设置暂不可用")]
    Internal,
}

#[derive(Clone)]
pub struct AccountVerificationSettingsRecord {
    pub initialized: bool,
    pub manual_enabled: bool,
    pub individual_manual_enabled: bool,
    pub enterprise_manual_enabled: bool,
    pub individual_reason_required: bool,
    pub enterprise_reason_required: bool,
    pub enabled: bool,
    pub app_id: Option<String>,
    pub credentials: Option<EncryptedCredentialEnvelope>,
    pub gateway_url: String,
    pub biz_code: String,
    pub timeout_secs: u64,
    pub version: i64,
}

impl AccountVerificationSettingsRepository {
    pub fn new(pool: DatabasePool, operation_timeout: Duration) -> Self {
        Self {
            pool,
            operation_timeout,
        }
    }

    pub async fn settings(
        &self,
    ) -> Result<AccountVerificationSettingsRecord, AccountVerificationSettingsError> {
        timeout(self.operation_timeout, async {
            let model = settings::Entity::find_by_id(1_i16)
                .one(self.pool.connection())
                .with_subscriber(NoSubscriber::default())
                .await
                .map_err(|_| AccountVerificationSettingsError::Internal)?
                .ok_or(AccountVerificationSettingsError::Internal)?;
            record_from_model(model)
        })
        .await
        .map_err(|_| AccountVerificationSettingsError::Internal)?
    }

    pub async fn update(
        &self,
        record: AccountVerificationSettingsRecord,
        expected_version: i64,
    ) -> Result<AccountVerificationSettingsRecord, AccountVerificationSettingsError> {
        if expected_version < 1
            || record.version != expected_version
            || record.gateway_url.len() > 2048
            || record.biz_code.len() > 64
            || record
                .app_id
                .as_ref()
                .is_some_and(|value| value.len() > 128)
            || !(1..=30).contains(&record.timeout_secs)
            || (record.enabled && (record.app_id.is_none() || record.credentials.is_none()))
        {
            return Err(AccountVerificationSettingsError::Invalid);
        }
        timeout(self.operation_timeout, async {
            let credentials = record.credentials.map(encrypted_json).transpose()?;
            let result = settings::Entity::update_many()
                .col_expr(settings::Column::Initialized, Expr::value(true))
                .col_expr(
                    settings::Column::ManualEnabled,
                    Expr::value(record.manual_enabled),
                )
                .col_expr(
                    settings::Column::IndividualManualEnabled,
                    Expr::value(record.individual_manual_enabled),
                )
                .col_expr(
                    settings::Column::EnterpriseManualEnabled,
                    Expr::value(record.enterprise_manual_enabled),
                )
                .col_expr(
                    settings::Column::IndividualReasonRequired,
                    Expr::value(record.individual_reason_required),
                )
                .col_expr(
                    settings::Column::EnterpriseReasonRequired,
                    Expr::value(record.enterprise_reason_required),
                )
                .col_expr(settings::Column::AlipayEnabled, Expr::value(record.enabled))
                .col_expr(settings::Column::AlipayAppId, Expr::value(record.app_id))
                .col_expr(
                    settings::Column::AlipayCredentials,
                    Expr::value(credentials),
                )
                .col_expr(
                    settings::Column::AlipayGatewayUrl,
                    Expr::value(record.gateway_url),
                )
                .col_expr(
                    settings::Column::AlipayBizCode,
                    Expr::value(record.biz_code),
                )
                .col_expr(
                    settings::Column::AlipayTimeoutSecs,
                    Expr::value(record.timeout_secs as i32),
                )
                .col_expr(settings::Column::Version, Expr::value(expected_version + 1))
                .col_expr(
                    settings::Column::UpdatedAt,
                    Expr::value(sea_orm::entity::prelude::TimeDateTimeWithTimeZone::now_utc()),
                )
                .filter(settings::Column::Id.eq(1_i16))
                .filter(settings::Column::Version.eq(expected_version))
                .exec(self.pool.connection())
                .with_subscriber(NoSubscriber::default())
                .await
                .map_err(|_| AccountVerificationSettingsError::Internal)?;
            if result.rows_affected != 1 {
                return Err(AccountVerificationSettingsError::Conflict);
            }
            self.settings().await
        })
        .await
        .map_err(|_| AccountVerificationSettingsError::Internal)?
    }
}

fn record_from_model(
    model: settings::Model,
) -> Result<AccountVerificationSettingsRecord, AccountVerificationSettingsError> {
    Ok(AccountVerificationSettingsRecord {
        initialized: model.initialized,
        manual_enabled: model.manual_enabled,
        individual_manual_enabled: model.individual_manual_enabled,
        enterprise_manual_enabled: model.enterprise_manual_enabled,
        individual_reason_required: model.individual_reason_required,
        enterprise_reason_required: model.enterprise_reason_required,
        enabled: model.alipay_enabled,
        app_id: model.alipay_app_id.map(|value| value.as_str().to_owned()),
        credentials: model
            .alipay_credentials
            .map(envelope_from_json)
            .transpose()?,
        gateway_url: model.alipay_gateway_url.as_str().to_owned(),
        biz_code: model.alipay_biz_code,
        timeout_secs: u64::try_from(model.alipay_timeout_secs)
            .map_err(|_| AccountVerificationSettingsError::Internal)?,
        version: model.version,
    })
}

fn encrypted_json(
    envelope: EncryptedCredentialEnvelope,
) -> Result<EncryptedJson, AccountVerificationSettingsError> {
    EncryptedJson::from_envelope(json!({
        "version": 1, "algorithm": "xchacha20poly1305", "key_id": envelope.key_id(),
        "nonce": URL_SAFE_NO_PAD.encode(envelope.nonce()), "ciphertext": URL_SAFE_NO_PAD.encode(envelope.ciphertext()),
    })).map_err(|_| AccountVerificationSettingsError::Internal)
}

fn envelope_from_json(
    secret: EncryptedJson,
) -> Result<EncryptedCredentialEnvelope, AccountVerificationSettingsError> {
    let (key_id, nonce, ciphertext) = secret
        .envelope_parts()
        .map_err(|_| AccountVerificationSettingsError::Internal)?;
    EncryptedCredentialEnvelope::new(key_id, nonce, ciphertext)
        .map_err(|_| AccountVerificationSettingsError::Internal)
}
