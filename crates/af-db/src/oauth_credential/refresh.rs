use std::fmt;

use af_domain::{ChannelId, CredentialId, CredentialKind, Status};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect, sea_query::Expr};
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use super::{
    OAuthCredentialRepository, OAuthCredentialRepositoryError, encrypted_json, is_valid_provider,
    query_error, record_internal_error,
};
use crate::{
    EncryptedCredentialEnvelope,
    entity::{EncryptedJson, channels, credentials},
};

/// 单次后台扫描允许返回的最大 OAuth 凭据数。
pub const MAX_OAUTH_REFRESH_CANDIDATES: usize = 256;

/// OAuth 刷新结果的条件写入结论。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OAuthCredentialRefreshUpdateOutcome {
    /// 新 token 密文、到期投影与版本已经原子提交。
    Updated,
    /// 上游调用期间凭据版本或密文已经变化，旧结果不得覆盖新事实。
    Stale,
    /// 目标不存在、已删除、不属于原渠道或不再是 OAuth 凭据。
    TargetNotFound,
    /// 目标已经切换到其他 OAuth provider。
    ProviderMismatch,
}

/// 旧 OAuth 密文到期投影的条件回填结论。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OAuthCredentialExpirationProjectionUpdateOutcome {
    /// 密文中的到期时间已经写入派生投影。
    Updated,
    /// 回填期间凭据版本、密文或投影已经变化。
    Stale,
    /// 目标不存在、已删除、不属于原渠道或不再是 OAuth 凭据。
    TargetNotFound,
    /// 目标已经切换到其他 OAuth provider。
    ProviderMismatch,
}

/// 数据库冻结的 OAuth 刷新候选；原密文只用于解密和后续 CAS。
pub struct OAuthRefreshCandidateRecord {
    channel_id: ChannelId,
    credential_id: CredentialId,
    provider: String,
    expected_revision: i64,
    expires_at_epoch_seconds: i64,
    expected_secret: EncryptedJson,
    envelope: EncryptedCredentialEnvelope,
}

impl OAuthRefreshCandidateRecord {
    /// 返回候选所属渠道。
    #[must_use]
    pub const fn channel_id(&self) -> ChannelId {
        self.channel_id
    }

    /// 返回候选凭据标识。
    #[must_use]
    pub const fn credential_id(&self) -> CredentialId {
        self.credential_id
    }

    /// 返回规范化 Provider 标识。
    #[must_use]
    pub fn provider(&self) -> &str {
        &self.provider
    }

    /// 返回候选读取时观察到的单调 OAuth 版本。
    #[must_use]
    pub const fn expected_revision(&self) -> i64 {
        self.expected_revision
    }

    /// 返回候选读取时观察到的绝对到期时间。
    #[must_use]
    pub const fn expires_at_epoch_seconds(&self) -> i64 {
        self.expires_at_epoch_seconds
    }

    /// 返回待解密的原密文封套；调用方不得记录其字段。
    #[must_use]
    pub const fn envelope(&self) -> &EncryptedCredentialEnvelope {
        &self.envelope
    }
}

impl fmt::Debug for OAuthRefreshCandidateRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OAuthRefreshCandidateRecord")
            .field("channel_id", &self.channel_id)
            .field("credential_id", &self.credential_id)
            .field("provider", &self.provider)
            .field("expected_revision", &self.expected_revision)
            .field("expires_at_epoch_seconds", &self.expires_at_epoch_seconds)
            .field("expected_secret", &"<已脱敏>")
            .finish()
    }
}

/// 缺少到期投影的旧 OAuth 凭据；密文只允许在应用层受控解密。
pub struct OAuthExpirationProjectionCandidateRecord {
    channel_id: ChannelId,
    credential_id: CredentialId,
    provider: String,
    expected_revision: i64,
    expected_secret: EncryptedJson,
    envelope: EncryptedCredentialEnvelope,
}

impl OAuthExpirationProjectionCandidateRecord {
    /// 返回候选所属渠道。
    #[must_use]
    pub const fn channel_id(&self) -> ChannelId {
        self.channel_id
    }

    /// 返回候选凭据标识。
    #[must_use]
    pub const fn credential_id(&self) -> CredentialId {
        self.credential_id
    }

    /// 返回规范化 Provider 标识。
    #[must_use]
    pub fn provider(&self) -> &str {
        &self.provider
    }

    /// 返回候选读取时观察到的单调 OAuth 版本。
    #[must_use]
    pub const fn expected_revision(&self) -> i64 {
        self.expected_revision
    }

    /// 返回待解密的原密文封套；调用方不得记录其字段。
    #[must_use]
    pub const fn envelope(&self) -> &EncryptedCredentialEnvelope {
        &self.envelope
    }
}

impl fmt::Debug for OAuthExpirationProjectionCandidateRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OAuthExpirationProjectionCandidateRecord")
            .field("channel_id", &self.channel_id)
            .field("credential_id", &self.credential_id)
            .field("provider", &self.provider)
            .field("expected_revision", &self.expected_revision)
            .field("expected_secret", &"<已脱敏>")
            .finish()
    }
}

impl OAuthCredentialRepository {
    /// 按凭据 ID 游标读取缺少到期投影的旧 OAuth 根凭据。
    pub async fn missing_expiration_projection_candidates(
        &self,
        after_credential_id: Option<CredentialId>,
        limit: usize,
    ) -> Result<Vec<OAuthExpirationProjectionCandidateRecord>, OAuthCredentialRepositoryError> {
        if limit == 0 || limit > MAX_OAUTH_REFRESH_CANDIDATES {
            return Err(OAuthCredentialRepositoryError::InvalidCandidateQuery);
        }
        let operation = self
            .missing_expiration_projection_candidates_inner(after_credential_id, limit)
            .with_subscriber(NoSubscriber::default());
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result.map_err(record_internal_error),
            Err(_) => Err(record_internal_error(
                OAuthCredentialRepositoryError::Timeout,
            )),
        }
    }

    async fn missing_expiration_projection_candidates_inner(
        &self,
        after_credential_id: Option<CredentialId>,
        limit: usize,
    ) -> Result<Vec<OAuthExpirationProjectionCandidateRecord>, OAuthCredentialRepositoryError> {
        let mut query = credentials::Entity::find()
            .find_also_related(channels::Entity)
            .filter(credentials::Column::Kind.eq(CredentialKind::Oauth.as_str()))
            .filter(credentials::Column::Status.eq(Status::Enabled.code()))
            .filter(credentials::Column::Schedulable.eq(true))
            .filter(credentials::Column::ParentId.is_null())
            .filter(credentials::Column::OauthProvider.is_not_null())
            .filter(credentials::Column::OauthRevision.gte(0_i64))
            .filter(credentials::Column::OauthRevision.lt(i64::MAX))
            .filter(credentials::Column::OauthExpiresAtEpochSeconds.is_null())
            .filter(credentials::Column::DeletedAt.is_null())
            .filter(channels::Column::Status.eq(Status::Enabled.code()))
            .filter(channels::Column::DeletedAt.is_null())
            .order_by_asc(credentials::Column::Id)
            .limit(
                u64::try_from(limit)
                    .map_err(|_| OAuthCredentialRepositoryError::InvalidCandidateQuery)?,
            );
        if let Some(after_credential_id) = after_credential_id {
            query = query.filter(credentials::Column::Id.gt(after_credential_id.get()));
        }
        query
            .all(self.pool.connection())
            .await
            .map_err(|_| query_error("oauth_expiration_projection_candidate_query"))?
            .into_iter()
            .map(|(credential, channel)| {
                expiration_projection_candidate_from_models(credential, channel)
            })
            .collect()
    }

    /// 按到期时间与凭据 ID 稳定读取启用根凭据，查询严格受批量上限约束。
    pub async fn due_refresh_candidates(
        &self,
        refresh_before_epoch_seconds: i64,
        limit: usize,
    ) -> Result<Vec<OAuthRefreshCandidateRecord>, OAuthCredentialRepositoryError> {
        if refresh_before_epoch_seconds < 0 || limit == 0 || limit > MAX_OAUTH_REFRESH_CANDIDATES {
            return Err(OAuthCredentialRepositoryError::InvalidCandidateQuery);
        }
        let operation = self
            .due_refresh_candidates_inner(refresh_before_epoch_seconds, limit)
            .with_subscriber(NoSubscriber::default());
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result.map_err(record_internal_error),
            Err(_) => Err(record_internal_error(
                OAuthCredentialRepositoryError::Timeout,
            )),
        }
    }

    async fn due_refresh_candidates_inner(
        &self,
        refresh_before_epoch_seconds: i64,
        limit: usize,
    ) -> Result<Vec<OAuthRefreshCandidateRecord>, OAuthCredentialRepositoryError> {
        let rows = credentials::Entity::find()
            .find_also_related(channels::Entity)
            .filter(credentials::Column::Kind.eq(CredentialKind::Oauth.as_str()))
            .filter(credentials::Column::Status.eq(Status::Enabled.code()))
            .filter(credentials::Column::Schedulable.eq(true))
            .filter(credentials::Column::ParentId.is_null())
            .filter(credentials::Column::OauthProvider.is_not_null())
            .filter(credentials::Column::OauthRevision.gte(0_i64))
            .filter(credentials::Column::OauthRevision.lt(i64::MAX))
            .filter(credentials::Column::OauthExpiresAtEpochSeconds.is_not_null())
            .filter(
                credentials::Column::OauthExpiresAtEpochSeconds.lte(refresh_before_epoch_seconds),
            )
            .filter(credentials::Column::DeletedAt.is_null())
            .filter(channels::Column::Status.eq(Status::Enabled.code()))
            .filter(channels::Column::DeletedAt.is_null())
            .order_by_asc(credentials::Column::OauthExpiresAtEpochSeconds)
            .order_by_asc(credentials::Column::Id)
            .limit(
                u64::try_from(limit)
                    .map_err(|_| OAuthCredentialRepositoryError::InvalidCandidateQuery)?,
            )
            .all(self.pool.connection())
            .await
            .map_err(|_| query_error("oauth_refresh_candidate_query"))?;
        rows.into_iter()
            .map(|(credential, channel)| candidate_from_models(credential, channel))
            .collect()
    }

    /// 仅在旧凭据事实与空投影仍未变化时写入密文中的绝对到期时间。
    pub async fn backfill_expiration_projection(
        &self,
        candidate: OAuthExpirationProjectionCandidateRecord,
        expires_at_epoch_seconds: i64,
    ) -> Result<OAuthCredentialExpirationProjectionUpdateOutcome, OAuthCredentialRepositoryError>
    {
        if expires_at_epoch_seconds < 0 {
            return Err(OAuthCredentialRepositoryError::InvalidExpiration);
        }
        let operation = self
            .backfill_expiration_projection_inner(candidate, expires_at_epoch_seconds)
            .with_subscriber(NoSubscriber::default());
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result.map_err(record_internal_error),
            Err(_) => Err(record_internal_error(
                OAuthCredentialRepositoryError::Timeout,
            )),
        }
    }

    async fn backfill_expiration_projection_inner(
        &self,
        candidate: OAuthExpirationProjectionCandidateRecord,
        expires_at_epoch_seconds: i64,
    ) -> Result<OAuthCredentialExpirationProjectionUpdateOutcome, OAuthCredentialRepositoryError>
    {
        let result = credentials::Entity::update_many()
            .filter(credentials::Column::Id.eq(candidate.credential_id.get()))
            .filter(credentials::Column::ChannelId.eq(candidate.channel_id.get()))
            .filter(credentials::Column::Kind.eq(CredentialKind::Oauth.as_str()))
            .filter(credentials::Column::DeletedAt.is_null())
            .filter(credentials::Column::OauthProvider.eq(candidate.provider.clone()))
            .filter(credentials::Column::OauthRevision.eq(candidate.expected_revision))
            .filter(credentials::Column::Secret.eq(candidate.expected_secret.clone()))
            .filter(credentials::Column::OauthExpiresAtEpochSeconds.is_null())
            .col_expr(
                credentials::Column::OauthExpiresAtEpochSeconds,
                Expr::value(Some(expires_at_epoch_seconds)),
            )
            .exec(self.pool.connection())
            .await
            .map_err(|_| query_error("oauth_expiration_projection_backfill"))?;
        match result.rows_affected {
            1 => Ok(OAuthCredentialExpirationProjectionUpdateOutcome::Updated),
            0 => {
                self.classify_expiration_projection_cas_miss(&candidate)
                    .await
            }
            _ => Err(OAuthCredentialRepositoryError::Invariant),
        }
    }

    async fn classify_expiration_projection_cas_miss(
        &self,
        candidate: &OAuthExpirationProjectionCandidateRecord,
    ) -> Result<OAuthCredentialExpirationProjectionUpdateOutcome, OAuthCredentialRepositoryError>
    {
        let target = credentials::Entity::find_by_id(candidate.credential_id.get())
            .filter(credentials::Column::ChannelId.eq(candidate.channel_id.get()))
            .filter(credentials::Column::DeletedAt.is_null())
            .one(self.pool.connection())
            .await
            .map_err(|_| query_error("oauth_expiration_projection_classify"))?;
        let Some(target) = target else {
            return Ok(OAuthCredentialExpirationProjectionUpdateOutcome::TargetNotFound);
        };
        if target.kind != CredentialKind::Oauth.as_str() {
            return Ok(OAuthCredentialExpirationProjectionUpdateOutcome::TargetNotFound);
        }
        if target.oauth_provider.as_deref() != Some(candidate.provider.as_str()) {
            return Ok(OAuthCredentialExpirationProjectionUpdateOutcome::ProviderMismatch);
        }
        if target.oauth_revision != candidate.expected_revision
            || target.secret != candidate.expected_secret
            || target.oauth_expires_at_epoch_seconds.is_some()
        {
            return Ok(OAuthCredentialExpirationProjectionUpdateOutcome::Stale);
        }
        Err(OAuthCredentialRepositoryError::Invariant)
    }

    /// 仅在候选读取到的 Provider、版本和原密文仍未变化时提交刷新结果。
    pub async fn replace_refreshed_token_secret(
        &self,
        candidate: OAuthRefreshCandidateRecord,
        expires_at_epoch_seconds: i64,
        envelope: EncryptedCredentialEnvelope,
    ) -> Result<OAuthCredentialRefreshUpdateOutcome, OAuthCredentialRepositoryError> {
        if expires_at_epoch_seconds < 0 {
            return Err(OAuthCredentialRepositoryError::InvalidExpiration);
        }
        let encrypted = encrypted_json(envelope)?;
        let operation = self
            .replace_refreshed_token_secret_inner(candidate, expires_at_epoch_seconds, encrypted)
            .with_subscriber(NoSubscriber::default());
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result.map_err(record_internal_error),
            Err(_) => Err(record_internal_error(
                OAuthCredentialRepositoryError::Timeout,
            )),
        }
    }

    async fn replace_refreshed_token_secret_inner(
        &self,
        candidate: OAuthRefreshCandidateRecord,
        expires_at_epoch_seconds: i64,
        encrypted: EncryptedJson,
    ) -> Result<OAuthCredentialRefreshUpdateOutcome, OAuthCredentialRepositoryError> {
        let now = sea_orm::entity::prelude::TimeDateTimeWithTimeZone::now_utc();
        let result = credentials::Entity::update_many()
            .filter(credentials::Column::Id.eq(candidate.credential_id.get()))
            .filter(credentials::Column::ChannelId.eq(candidate.channel_id.get()))
            .filter(credentials::Column::Kind.eq(CredentialKind::Oauth.as_str()))
            .filter(credentials::Column::DeletedAt.is_null())
            .filter(credentials::Column::OauthProvider.eq(candidate.provider.clone()))
            .filter(credentials::Column::OauthRevision.eq(candidate.expected_revision))
            .filter(credentials::Column::OauthRevision.lt(i64::MAX))
            .filter(credentials::Column::Secret.eq(candidate.expected_secret.clone()))
            .col_expr(credentials::Column::Secret, Expr::value(encrypted))
            .col_expr(
                credentials::Column::OauthExpiresAtEpochSeconds,
                Expr::value(Some(expires_at_epoch_seconds)),
            )
            .col_expr(
                credentials::Column::OauthRevision,
                Expr::col(credentials::Column::OauthRevision).add(1_i64),
            )
            .col_expr(credentials::Column::UpdatedAt, Expr::value(now))
            .exec(self.pool.connection())
            .await
            .map_err(|_| query_error("oauth_refresh_token_update"))?;
        match result.rows_affected {
            1 => Ok(OAuthCredentialRefreshUpdateOutcome::Updated),
            0 => self.classify_refresh_cas_miss(&candidate).await,
            _ => Err(OAuthCredentialRepositoryError::Invariant),
        }
    }

    async fn classify_refresh_cas_miss(
        &self,
        candidate: &OAuthRefreshCandidateRecord,
    ) -> Result<OAuthCredentialRefreshUpdateOutcome, OAuthCredentialRepositoryError> {
        let target = credentials::Entity::find_by_id(candidate.credential_id.get())
            .filter(credentials::Column::ChannelId.eq(candidate.channel_id.get()))
            .filter(credentials::Column::DeletedAt.is_null())
            .one(self.pool.connection())
            .await
            .map_err(|_| query_error("oauth_refresh_token_classify"))?;
        let Some(target) = target else {
            return Ok(OAuthCredentialRefreshUpdateOutcome::TargetNotFound);
        };
        if target.kind != CredentialKind::Oauth.as_str() {
            return Ok(OAuthCredentialRefreshUpdateOutcome::TargetNotFound);
        }
        if target.oauth_provider.as_deref() != Some(candidate.provider.as_str()) {
            return Ok(OAuthCredentialRefreshUpdateOutcome::ProviderMismatch);
        }
        if target.oauth_revision != candidate.expected_revision
            || target.secret != candidate.expected_secret
        {
            return Ok(OAuthCredentialRefreshUpdateOutcome::Stale);
        }
        Err(OAuthCredentialRepositoryError::Invariant)
    }
}

fn candidate_from_models(
    credential: credentials::Model,
    channel: Option<channels::Model>,
) -> Result<OAuthRefreshCandidateRecord, OAuthCredentialRepositoryError> {
    let channel = channel.ok_or(OAuthCredentialRepositoryError::Invariant)?;
    let channel_id = ChannelId::new(credential.channel_id)
        .map_err(|_| OAuthCredentialRepositoryError::Invariant)?;
    let credential_id =
        CredentialId::new(credential.id).map_err(|_| OAuthCredentialRepositoryError::Invariant)?;
    let provider = credential
        .oauth_provider
        .ok_or(OAuthCredentialRepositoryError::Invariant)?;
    let expires_at_epoch_seconds = credential
        .oauth_expires_at_epoch_seconds
        .ok_or(OAuthCredentialRepositoryError::Invariant)?;
    if credential.kind != CredentialKind::Oauth.as_str()
        || credential.status != Status::Enabled.code()
        || !credential.schedulable
        || credential.parent_id.is_some()
        || credential.deleted_at.is_some()
        || credential.oauth_revision < 0
        || credential.oauth_revision == i64::MAX
        || expires_at_epoch_seconds < 0
        || channel.status != Status::Enabled.code()
        || channel.deleted_at.is_some()
        || !is_valid_provider(&provider)
    {
        return Err(OAuthCredentialRepositoryError::Invariant);
    }
    let (key_id, nonce, ciphertext) = credential
        .secret
        .envelope_parts()
        .map_err(|_| OAuthCredentialRepositoryError::Invariant)?;
    let envelope = EncryptedCredentialEnvelope::new(key_id, nonce, ciphertext)
        .map_err(|_| OAuthCredentialRepositoryError::Invariant)?;
    Ok(OAuthRefreshCandidateRecord {
        channel_id,
        credential_id,
        provider,
        expected_revision: credential.oauth_revision,
        expires_at_epoch_seconds,
        expected_secret: credential.secret,
        envelope,
    })
}

fn expiration_projection_candidate_from_models(
    credential: credentials::Model,
    channel: Option<channels::Model>,
) -> Result<OAuthExpirationProjectionCandidateRecord, OAuthCredentialRepositoryError> {
    let channel = channel.ok_or(OAuthCredentialRepositoryError::Invariant)?;
    let channel_id = ChannelId::new(credential.channel_id)
        .map_err(|_| OAuthCredentialRepositoryError::Invariant)?;
    let credential_id =
        CredentialId::new(credential.id).map_err(|_| OAuthCredentialRepositoryError::Invariant)?;
    let provider = credential
        .oauth_provider
        .ok_or(OAuthCredentialRepositoryError::Invariant)?;
    if credential.kind != CredentialKind::Oauth.as_str()
        || credential.status != Status::Enabled.code()
        || !credential.schedulable
        || credential.parent_id.is_some()
        || credential.deleted_at.is_some()
        || credential.oauth_revision < 0
        || credential.oauth_revision == i64::MAX
        || credential.oauth_expires_at_epoch_seconds.is_some()
        || channel.status != Status::Enabled.code()
        || channel.deleted_at.is_some()
        || !is_valid_provider(&provider)
    {
        return Err(OAuthCredentialRepositoryError::Invariant);
    }
    let (key_id, nonce, ciphertext) = credential
        .secret
        .envelope_parts()
        .map_err(|_| OAuthCredentialRepositoryError::Invariant)?;
    let envelope = EncryptedCredentialEnvelope::new(key_id, nonce, ciphertext)
        .map_err(|_| OAuthCredentialRepositoryError::Invariant)?;
    Ok(OAuthExpirationProjectionCandidateRecord {
        channel_id,
        credential_id,
        provider,
        expected_revision: credential.oauth_revision,
        expected_secret: credential.secret,
        envelope,
    })
}
