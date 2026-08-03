use std::{fmt, time::Duration};

use af_domain::{ChannelId, CredentialId, CredentialKind};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sea_orm::{
    ColumnTrait, EntityTrait, QueryFilter, QuerySelect,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Condition, Expr},
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool, EncryptedCredentialEnvelope,
    entity::{EncryptedJson, credentials},
};

const DEFAULT_OPERATION_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_OAUTH_PROVIDER_BYTES: usize = 64;

/// OAuth token 密文原子替换结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OAuthCredentialTokenUpdateOutcome {
    /// 已替换密文并固化 provider 身份。
    Updated,
    /// 目标不存在、已删除、不属于渠道或不是 OAuth 凭据。
    TargetNotFound,
    /// 凭据已经绑定其他 OAuth provider。
    ProviderMismatch,
}

/// OAuth 凭据密文仓储错误；不携带 provider、密文或底层数据库诊断。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum OAuthCredentialRepositoryError {
    /// 数据库操作截止时间不能为零。
    #[error("OAuth 凭据数据库操作超时必须大于零")]
    InvalidConfiguration,
    /// provider 不是规范化的内部标识。
    #[error("OAuth provider 标识无效")]
    InvalidProvider,
    /// 获取连接或执行查询失败。
    #[error("OAuth 凭据数据库操作失败")]
    Query,
    /// 数据库操作超过硬截止时间。
    #[error("OAuth 凭据数据库操作超时")]
    Timeout,
    /// 影响行数或持久化状态违反不变量。
    #[error("OAuth 凭据持久化状态损坏")]
    Invariant,
}

/// 只负责完整 OAuth token 密文替换的聚焦仓储。
#[derive(Clone)]
pub struct OAuthCredentialRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl OAuthCredentialRepository {
    /// 使用默认五秒截止时间创建仓储。
    #[must_use]
    pub fn new(pool: DatabasePool) -> Self {
        Self {
            pool,
            operation_timeout: DEFAULT_OPERATION_TIMEOUT,
        }
    }

    /// 使用显式非零截止时间创建仓储。
    pub fn with_operation_timeout(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, OAuthCredentialRepositoryError> {
        if operation_timeout.is_zero() {
            return Err(OAuthCredentialRepositoryError::InvalidConfiguration);
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }

    /// 原子替换已有 OAuth 凭据的 token 密文，不改变启停和调度运行字段。
    pub async fn replace_token_secret(
        &self,
        channel_id: ChannelId,
        credential_id: CredentialId,
        provider: &str,
        envelope: EncryptedCredentialEnvelope,
    ) -> Result<OAuthCredentialTokenUpdateOutcome, OAuthCredentialRepositoryError> {
        if !is_valid_provider(provider) {
            return Err(OAuthCredentialRepositoryError::InvalidProvider);
        }
        let encrypted = encrypted_json(envelope)?;
        let operation = self
            .replace_token_secret_inner(channel_id, credential_id, provider, encrypted)
            .with_subscriber(NoSubscriber::default());
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result.map_err(record_internal_error),
            Err(_) => Err(record_internal_error(
                OAuthCredentialRepositoryError::Timeout,
            )),
        }
    }

    async fn replace_token_secret_inner(
        &self,
        channel_id: ChannelId,
        credential_id: CredentialId,
        provider: &str,
        encrypted: EncryptedJson,
    ) -> Result<OAuthCredentialTokenUpdateOutcome, OAuthCredentialRepositoryError> {
        let now = TimeDateTimeWithTimeZone::now_utc();
        let provider_filter = Condition::any()
            .add(credentials::Column::OauthProvider.is_null())
            .add(credentials::Column::OauthProvider.eq(provider));
        let result = credentials::Entity::update_many()
            .filter(credentials::Column::Id.eq(credential_id.get()))
            .filter(credentials::Column::ChannelId.eq(channel_id.get()))
            .filter(credentials::Column::Kind.eq(CredentialKind::Oauth.as_str()))
            .filter(credentials::Column::DeletedAt.is_null())
            .filter(provider_filter)
            .filter(credentials::Column::OauthRevision.gte(0_i64))
            .filter(credentials::Column::OauthRevision.lt(i64::MAX))
            .col_expr(credentials::Column::Secret, Expr::value(encrypted))
            .col_expr(
                credentials::Column::OauthProvider,
                Expr::value(Some(provider.to_owned())),
            )
            .col_expr(
                credentials::Column::OauthRevision,
                Expr::col(credentials::Column::OauthRevision).add(1_i64),
            )
            .col_expr(credentials::Column::UpdatedAt, Expr::value(now))
            .exec(self.pool.connection())
            .await
            .map_err(|_| query_error("oauth_credential_update"))?;
        match result.rows_affected {
            1 => Ok(OAuthCredentialTokenUpdateOutcome::Updated),
            0 => {
                self.classify_rejected_target(channel_id, credential_id, provider)
                    .await
            }
            _ => Err(OAuthCredentialRepositoryError::Invariant),
        }
    }

    async fn classify_rejected_target(
        &self,
        channel_id: ChannelId,
        credential_id: CredentialId,
        provider: &str,
    ) -> Result<OAuthCredentialTokenUpdateOutcome, OAuthCredentialRepositoryError> {
        let target = credentials::Entity::find_by_id(credential_id.get())
            .select_only()
            .column(credentials::Column::Kind)
            .column(credentials::Column::OauthProvider)
            .filter(credentials::Column::ChannelId.eq(channel_id.get()))
            .filter(credentials::Column::DeletedAt.is_null())
            .into_tuple::<(String, Option<String>)>()
            .one(self.pool.connection())
            .await
            .map_err(|_| query_error("oauth_credential_classify"))?;
        let Some(target) = target else {
            return Ok(OAuthCredentialTokenUpdateOutcome::TargetNotFound);
        };
        let (kind, current_provider) = target;
        if kind != CredentialKind::Oauth.as_str() {
            return Ok(OAuthCredentialTokenUpdateOutcome::TargetNotFound);
        }
        if current_provider
            .as_deref()
            .is_some_and(|current| current != provider)
        {
            return Ok(OAuthCredentialTokenUpdateOutcome::ProviderMismatch);
        }
        Err(OAuthCredentialRepositoryError::Invariant)
    }
}

impl fmt::Debug for OAuthCredentialRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OAuthCredentialRepository")
            .field("operation_timeout", &self.operation_timeout)
            .finish_non_exhaustive()
    }
}

fn encrypted_json(
    envelope: EncryptedCredentialEnvelope,
) -> Result<EncryptedJson, OAuthCredentialRepositoryError> {
    EncryptedJson::from_envelope(serde_json::json!({
        "version": 1,
        "algorithm": "xchacha20poly1305",
        "key_id": envelope.key_id(),
        "nonce": URL_SAFE_NO_PAD.encode(envelope.nonce()),
        "ciphertext": URL_SAFE_NO_PAD.encode(envelope.ciphertext()),
    }))
    .map_err(|_| OAuthCredentialRepositoryError::Invariant)
}

fn is_valid_provider(provider: &str) -> bool {
    !provider.is_empty()
        && provider.len() <= MAX_OAUTH_PROVIDER_BYTES
        && provider.trim() == provider
        && provider.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
        })
}

fn query_error(operation: &'static str) -> OAuthCredentialRepositoryError {
    tracing::error!(
        target: "af_db::oauth_credential",
        error_kind = operation,
        "OAuth 凭据数据库操作失败"
    );
    OAuthCredentialRepositoryError::Query
}

fn record_internal_error(error: OAuthCredentialRepositoryError) -> OAuthCredentialRepositoryError {
    let error_kind = match error {
        OAuthCredentialRepositoryError::InvalidConfiguration
        | OAuthCredentialRepositoryError::InvalidProvider => return error,
        OAuthCredentialRepositoryError::Query => "oauth_credential_query",
        OAuthCredentialRepositoryError::Timeout => "oauth_credential_timeout",
        OAuthCredentialRepositoryError::Invariant => "oauth_credential_invariant",
    };
    tracing::error!(
        target: "af_db::oauth_credential",
        error_kind,
        "OAuth 凭据仓储发生内部错误"
    );
    error
}
