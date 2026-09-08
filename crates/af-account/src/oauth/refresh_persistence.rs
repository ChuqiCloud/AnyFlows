use std::{fmt, time::SystemTime};

use af_db::{
    OAuthCredentialExpirationProjectionUpdateOutcome, OAuthCredentialIdentityPatch,
    OAuthCredentialRefreshFailureKind, OAuthCredentialRefreshFailureUpdateOutcome,
    OAuthCredentialRefreshUpdateOutcome, OAuthCredentialRepository, OAuthCredentialRepositoryError,
    OAuthExpirationProjectionCandidateRecord, OAuthRefreshCandidateRecord,
};
use af_domain::{ChannelId, CredentialId};
use thiserror::Error;

use super::{
    OAuthRefreshFailureKind, OAuthRefreshedTokenSet, OAuthTokenRefreshRequest,
    UpstreamOAuthProvider, persistence::checked_expiration_epoch_seconds,
};
use crate::{CredentialDecryptor, CredentialEncryptor, PlainOAuthCredential};

/// 单批旧 OAuth 到期投影回填摘要。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OAuthExpirationProjectionBackfillBatch {
    scanned: usize,
    projected: usize,
    incomplete: usize,
    conflicted: usize,
    last_credential_id: Option<CredentialId>,
}

impl OAuthExpirationProjectionBackfillBatch {
    /// 返回本批读取的旧凭据数。
    #[must_use]
    pub const fn scanned(&self) -> usize {
        self.scanned
    }

    /// 返回成功写入到期投影的凭据数。
    #[must_use]
    pub const fn projected(&self) -> usize {
        self.projected
    }

    /// 返回缺少 refresh token 或密文到期时间、不能自动回填的凭据数。
    #[must_use]
    pub const fn incomplete(&self) -> usize {
        self.incomplete
    }

    /// 返回回填期间已删除或被并发修改的凭据数。
    #[must_use]
    pub const fn conflicted(&self) -> usize {
        self.conflicted
    }

    /// 返回本批最后扫描的凭据 ID，可作为下一批查询游标。
    #[must_use]
    pub const fn last_credential_id(&self) -> Option<CredentialId> {
        self.last_credential_id
    }
}

/// 已准备完成的 OAuth 刷新候选；拆分后请求交给 Provider，守卫保留到条件写回。
pub struct OAuthRefreshCandidate {
    guard: OAuthRefreshWriteGuard,
    request: OAuthTokenRefreshRequest,
}

impl OAuthRefreshCandidate {
    /// 返回候选所属渠道。
    #[must_use]
    pub const fn channel_id(&self) -> ChannelId {
        self.guard.channel_id()
    }

    /// 返回候选凭据标识。
    #[must_use]
    pub const fn credential_id(&self) -> CredentialId {
        self.guard.credential_id()
    }

    /// 返回候选 Provider。
    #[must_use]
    pub const fn provider(&self) -> UpstreamOAuthProvider {
        self.guard.provider()
    }

    /// 返回候选读取时观察到的绝对到期时间。
    #[must_use]
    pub const fn expires_at_epoch_seconds(&self) -> i64 {
        self.guard.expires_at_epoch_seconds()
    }

    /// 返回候选读取时观察到的 OAuth 版本。
    #[must_use]
    pub const fn expected_revision(&self) -> i64 {
        self.guard.expected_revision()
    }

    /// 按所有权拆分上游请求与写回守卫，避免敏感 token 或 CAS 事实被复制。
    #[must_use]
    pub fn into_parts(self) -> (OAuthRefreshWriteGuard, OAuthTokenRefreshRequest) {
        (self.guard, self.request)
    }
}

impl fmt::Debug for OAuthRefreshCandidate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OAuthRefreshCandidate")
            .field("guard", &self.guard)
            .field("request", &self.request)
            .finish()
    }
}

/// 上游刷新完成后的条件写回守卫；成功结果与失败状态都必须消费同一候选事实。
pub struct OAuthRefreshWriteGuard {
    record: OAuthRefreshCandidateRecord,
    provider: UpstreamOAuthProvider,
}

impl OAuthRefreshWriteGuard {
    /// 返回守卫所属渠道。
    #[must_use]
    pub const fn channel_id(&self) -> ChannelId {
        self.record.channel_id()
    }

    /// 返回守卫凭据标识。
    #[must_use]
    pub const fn credential_id(&self) -> CredentialId {
        self.record.credential_id()
    }

    /// 返回守卫 Provider。
    #[must_use]
    pub const fn provider(&self) -> UpstreamOAuthProvider {
        self.provider
    }

    /// 返回候选读取时观察到的 OAuth 版本。
    #[must_use]
    pub const fn expected_revision(&self) -> i64 {
        self.record.expected_revision()
    }

    /// 返回候选读取时观察到的绝对到期时间。
    #[must_use]
    pub const fn expires_at_epoch_seconds(&self) -> i64 {
        self.record.expires_at_epoch_seconds()
    }
}

impl fmt::Debug for OAuthRefreshWriteGuard {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OAuthRefreshWriteGuard")
            .field("channel_id", &self.channel_id())
            .field("credential_id", &self.credential_id())
            .field("provider", &self.provider)
            .field("expected_revision", &self.expected_revision())
            .field("expires_at_epoch_seconds", &self.expires_at_epoch_seconds())
            .field("expected_secret", &"<已脱敏>")
            .finish()
    }
}

/// OAuth 刷新结果持久化结论。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OAuthRefreshPersistenceOutcome {
    /// 新 token 集合已经加密并原子写入。
    Stored,
    /// 上游调用期间凭据已经变化，旧刷新结果被安全丢弃。
    Stale,
    /// 目标不存在、已删除、不属于原渠道或不再是 OAuth 凭据。
    TargetNotFound,
    /// 目标凭据已经切换到其他 Provider。
    ProviderMismatch,
}

/// OAuth 刷新失败状态的持久化结论。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OAuthRefreshFailurePersistenceOutcome {
    /// 临时冷却或自动停用状态已经写入。
    Recorded,
    /// 上游调用期间凭据事实或运行状态已经变化，旧失败被安全丢弃。
    Stale,
    /// 目标不存在、已删除、不属于原渠道或不再是 OAuth 凭据。
    TargetNotFound,
    /// 目标凭据已经切换到其他 Provider。
    ProviderMismatch,
}

/// OAuth 刷新候选准备与状态持久化错误；不携带 token、scope、密文或底层诊断。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum OAuthRefreshPersistenceError {
    /// 到期阈值或批量上限违反仓储边界。
    #[error("OAuth 刷新候选查询参数无效")]
    InvalidCandidateQuery,
    /// Provider、密文或到期投影与候选事实不一致。
    #[error("OAuth 刷新候选无效")]
    InvalidCandidate,
    /// 到期候选没有 refresh token，不能发起刷新。
    #[error("OAuth 刷新候选缺少 refresh token")]
    MissingRefreshToken,
    /// 刷新结果 Provider 与候选不一致。
    #[error("OAuth 刷新结果 Provider 不匹配")]
    InvalidResultProvider,
    /// 相对有效期缺失或不能安全转换为绝对 Unix 时间。
    #[error("OAuth 刷新结果到期时间无效")]
    InvalidExpiration,
    /// 刷新结果违反加密明文不变量。
    #[error("OAuth 刷新结果无效")]
    InvalidTokenSet,
    /// 刷新结果无法形成有效密文封套。
    #[error("OAuth 刷新结果加密失败")]
    Encryption,
    /// 数据库操作失败。
    #[error("OAuth 刷新持久化失败")]
    RepositoryUnavailable,
    /// 数据库操作超过硬截止时间。
    #[error("OAuth 刷新持久化超时")]
    RepositoryTimeout,
    /// 内部数据库状态违反不变量。
    #[error("OAuth 刷新持久化状态损坏")]
    Invariant,
}

/// 组合候选解密、敏感所有权转移、成功结果加密与失败状态 CAS 的应用边界。
pub struct OAuthRefreshPersistenceService {
    encryptor: CredentialEncryptor,
    decryptor: CredentialDecryptor,
    repository: OAuthCredentialRepository,
}

impl OAuthRefreshPersistenceService {
    /// 组合启动期凭据加解密器与聚焦数据库仓储。
    #[must_use]
    pub const fn new(
        encryptor: CredentialEncryptor,
        decryptor: CredentialDecryptor,
        repository: OAuthCredentialRepository,
    ) -> Self {
        Self {
            encryptor,
            decryptor,
            repository,
        }
    }

    /// 有界解密旧凭据并回填可索引到期时间；调用方应按返回游标扫完整轮。
    pub async fn backfill_missing_expiration_projections(
        &self,
        after_credential_id: Option<CredentialId>,
        limit: usize,
    ) -> Result<OAuthExpirationProjectionBackfillBatch, OAuthRefreshPersistenceError> {
        let records = self
            .repository
            .missing_expiration_projection_candidates(after_credential_id, limit)
            .await
            .map_err(map_repository_error)?;
        let mut batch = OAuthExpirationProjectionBackfillBatch {
            scanned: records.len(),
            projected: 0,
            incomplete: 0,
            conflicted: 0,
            last_credential_id: None,
        };
        for record in records {
            batch.last_credential_id = Some(record.credential_id());
            let Some(expires_at_epoch_seconds) = self.read_legacy_expiration(&record)? else {
                batch.incomplete += 1;
                continue;
            };
            let outcome = self
                .repository
                .backfill_expiration_projection(record, expires_at_epoch_seconds)
                .await
                .map_err(map_repository_error)?;
            match outcome {
                OAuthCredentialExpirationProjectionUpdateOutcome::Updated => {
                    batch.projected += 1;
                }
                OAuthCredentialExpirationProjectionUpdateOutcome::Stale
                | OAuthCredentialExpirationProjectionUpdateOutcome::TargetNotFound
                | OAuthCredentialExpirationProjectionUpdateOutcome::ProviderMismatch => {
                    batch.conflicted += 1;
                }
            }
        }
        Ok(batch)
    }

    /// 读取并解密有界到期候选；任一候选损坏时整批失败关闭。
    pub async fn due_candidates(
        &self,
        refresh_before_epoch_seconds: i64,
        limit: usize,
    ) -> Result<Vec<OAuthRefreshCandidate>, OAuthRefreshPersistenceError> {
        self.repository
            .due_refresh_candidates(refresh_before_epoch_seconds, limit)
            .await
            .map_err(map_repository_error)?
            .into_iter()
            .map(|record| self.prepare_candidate(record))
            .collect()
    }

    /// 加密刷新结果，并仅在守卫观察到的凭据事实仍未变化时原子写回。
    pub async fn persist(
        &self,
        guard: OAuthRefreshWriteGuard,
        token_set: OAuthRefreshedTokenSet,
    ) -> Result<OAuthRefreshPersistenceOutcome, OAuthRefreshPersistenceError> {
        self.persist_at(guard, token_set, SystemTime::now()).await
    }

    /// 消费候选守卫，并以同一组 CAS 事实原子写入刷新失败状态。
    pub async fn record_failure(
        &self,
        guard: OAuthRefreshWriteGuard,
        failure_kind: OAuthRefreshFailureKind,
    ) -> Result<OAuthRefreshFailurePersistenceOutcome, OAuthRefreshPersistenceError> {
        let repository_kind = match failure_kind {
            OAuthRefreshFailureKind::Transient => OAuthCredentialRefreshFailureKind::Transient,
            OAuthRefreshFailureKind::Revoked => OAuthCredentialRefreshFailureKind::Revoked,
        };
        let outcome = self
            .repository
            .record_refresh_failure(guard.record, repository_kind)
            .await
            .map_err(map_repository_error)?;
        Ok(match outcome {
            OAuthCredentialRefreshFailureUpdateOutcome::Recorded => {
                OAuthRefreshFailurePersistenceOutcome::Recorded
            }
            OAuthCredentialRefreshFailureUpdateOutcome::Stale => {
                OAuthRefreshFailurePersistenceOutcome::Stale
            }
            OAuthCredentialRefreshFailureUpdateOutcome::TargetNotFound => {
                OAuthRefreshFailurePersistenceOutcome::TargetNotFound
            }
            OAuthCredentialRefreshFailureUpdateOutcome::ProviderMismatch => {
                OAuthRefreshFailurePersistenceOutcome::ProviderMismatch
            }
        })
    }

    fn prepare_candidate(
        &self,
        record: OAuthRefreshCandidateRecord,
    ) -> Result<OAuthRefreshCandidate, OAuthRefreshPersistenceError> {
        let provider = parse_provider(record.provider())
            .ok_or(OAuthRefreshPersistenceError::InvalidCandidate)?;
        let decrypted = self
            .decryptor
            .decrypt_oauth_envelope(
                record.channel_id(),
                record.credential_id().get(),
                record.envelope(),
            )
            .map_err(|_| OAuthRefreshPersistenceError::InvalidCandidate)?;
        let (refresh_token, scope, encrypted_expiration) = decrypted.into_refresh_material();
        if encrypted_expiration != Some(record.expires_at_epoch_seconds()) {
            return Err(OAuthRefreshPersistenceError::InvalidCandidate);
        }
        let refresh_token =
            refresh_token.ok_or(OAuthRefreshPersistenceError::MissingRefreshToken)?;
        let request = OAuthTokenRefreshRequest::from_zeroizing(provider, refresh_token, scope)
            .map_err(|_| OAuthRefreshPersistenceError::InvalidCandidate)?;
        Ok(OAuthRefreshCandidate {
            guard: OAuthRefreshWriteGuard { record, provider },
            request,
        })
    }

    /// 只从完整可刷新的旧密文提取非敏感到期时间。
    fn read_legacy_expiration(
        &self,
        record: &OAuthExpirationProjectionCandidateRecord,
    ) -> Result<Option<i64>, OAuthRefreshPersistenceError> {
        parse_provider(record.provider()).ok_or(OAuthRefreshPersistenceError::InvalidCandidate)?;
        let decrypted = self
            .decryptor
            .decrypt_oauth_envelope(
                record.channel_id(),
                record.credential_id().get(),
                record.envelope(),
            )
            .map_err(|_| OAuthRefreshPersistenceError::InvalidCandidate)?;
        let (refresh_token, scope, expires_at_epoch_seconds) = decrypted.into_refresh_material();
        let refreshable = refresh_token.is_some();
        // 回填只需要非敏感到期时间，数据库等待前释放 refresh token 与 scope。
        drop(refresh_token);
        drop(scope);
        if !refreshable {
            return Ok(None);
        }
        Ok(expires_at_epoch_seconds)
    }

    async fn persist_at(
        &self,
        guard: OAuthRefreshWriteGuard,
        token_set: OAuthRefreshedTokenSet,
        now: SystemTime,
    ) -> Result<OAuthRefreshPersistenceOutcome, OAuthRefreshPersistenceError> {
        let parts = token_set.into_parts();
        if parts.provider != guard.provider {
            return Err(OAuthRefreshPersistenceError::InvalidResultProvider);
        }
        let expires_in = parts
            .expires_in
            .ok_or(OAuthRefreshPersistenceError::InvalidExpiration)?;
        let expires_at_epoch_seconds = checked_expiration_epoch_seconds(now, expires_in)
            .map_err(|()| OAuthRefreshPersistenceError::InvalidExpiration)?;
        let (account_key, project_id) = parts.identity.into_parts();
        let identity = OAuthCredentialIdentityPatch::new(account_key, project_id)
            .map_err(|_| OAuthRefreshPersistenceError::InvalidTokenSet)?;
        let secret = PlainOAuthCredential::from_zeroizing(
            parts.access_token,
            Some(parts.refresh_token),
            Some(expires_at_epoch_seconds),
            parts.scope,
        )
        .map_err(|_| OAuthRefreshPersistenceError::InvalidTokenSet)?;
        let envelope = self
            .encryptor
            .encrypt_oauth(guard.channel_id(), guard.credential_id().get(), &secret)
            .map_err(|_| OAuthRefreshPersistenceError::Encryption)?;
        // 上游可能已经轮转 refresh token；数据库等待期间只保留新旧密文，不保留任何明文。
        drop(secret);
        let outcome = self
            .repository
            .replace_refreshed_token_secret(
                guard.record,
                expires_at_epoch_seconds,
                envelope,
                identity,
            )
            .await
            .map_err(map_repository_error)?;
        Ok(match outcome {
            OAuthCredentialRefreshUpdateOutcome::Updated => OAuthRefreshPersistenceOutcome::Stored,
            OAuthCredentialRefreshUpdateOutcome::Stale => OAuthRefreshPersistenceOutcome::Stale,
            OAuthCredentialRefreshUpdateOutcome::TargetNotFound => {
                OAuthRefreshPersistenceOutcome::TargetNotFound
            }
            OAuthCredentialRefreshUpdateOutcome::ProviderMismatch => {
                OAuthRefreshPersistenceOutcome::ProviderMismatch
            }
        })
    }
}

impl fmt::Debug for OAuthRefreshPersistenceService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OAuthRefreshPersistenceService")
            .field("encryptor", &self.encryptor)
            .field("decryptor", &self.decryptor)
            .field("repository", &self.repository)
            .finish()
    }
}

fn parse_provider(value: &str) -> Option<UpstreamOAuthProvider> {
    match value {
        "claude_code" => Some(UpstreamOAuthProvider::ClaudeCode),
        "codex" => Some(UpstreamOAuthProvider::Codex),
        "gemini" => Some(UpstreamOAuthProvider::Gemini),
        "antigravity" => Some(UpstreamOAuthProvider::Antigravity),
        _ => None,
    }
}

fn map_repository_error(error: OAuthCredentialRepositoryError) -> OAuthRefreshPersistenceError {
    match error {
        OAuthCredentialRepositoryError::InvalidCandidateQuery => {
            OAuthRefreshPersistenceError::InvalidCandidateQuery
        }
        OAuthCredentialRepositoryError::Query => {
            OAuthRefreshPersistenceError::RepositoryUnavailable
        }
        OAuthCredentialRepositoryError::Timeout => OAuthRefreshPersistenceError::RepositoryTimeout,
        OAuthCredentialRepositoryError::InvalidConfiguration
        | OAuthCredentialRepositoryError::InvalidProvider
        | OAuthCredentialRepositoryError::InvalidExpiration
        | OAuthCredentialRepositoryError::InvalidIdentity
        | OAuthCredentialRepositoryError::Invariant => OAuthRefreshPersistenceError::Invariant,
        _ => OAuthRefreshPersistenceError::Invariant,
    }
}

#[cfg(test)]
mod tests;
