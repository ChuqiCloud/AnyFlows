use std::{fmt, time::Duration};

use af_domain::{
    OrganizationScimAuditFacts, OrganizationScimAuditResource, OrganizationScimResourceKind,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DbErr, EntityTrait, QueryFilter, Set, SqlErr,
    entity::prelude::TimeDateTimeWithTimeZone,
};
use thiserror::Error;
use tokio::time::timeout;

use crate::{DatabasePool, entity::organization_scim_audit_logs};

/// SCIM 脱敏审计事实的幂等写入结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrganizationScimAuditWriteOutcome {
    /// 首次追加成功。
    Applied,
    /// 相同企业、请求指纹和事件类型已经保存了完全相同的事实。
    Existing,
}

/// SCIM 脱敏审计仓储错误，不携带数据库或请求原文。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationScimAuditRepositoryError {
    #[error("SCIM 审计事实查询失败")]
    Query,
    #[error("SCIM 审计事实写入超时")]
    Timeout,
    #[error("SCIM 审计事实幂等冲突")]
    Conflict,
    #[error("SCIM 审计事实持久化状态损坏")]
    Invariant,
}

/// 只追加 SCIM 脱敏审计事实的仓储；本阶段不提供查询接口。
#[derive(Clone)]
pub struct OrganizationScimAuditRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl OrganizationScimAuditRepository {
    /// 创建带超时保护的 SCIM 审计仓储。
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, OrganizationScimAuditRepositoryError> {
        if operation_timeout.is_zero() {
            return Err(OrganizationScimAuditRepositoryError::Invariant);
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }

    /// 追加一条已由领域层校验过的脱敏事实，重复写入只返回 Existing。
    pub async fn record(
        &self,
        facts: &OrganizationScimAuditFacts,
    ) -> Result<OrganizationScimAuditWriteOutcome, OrganizationScimAuditRepositoryError> {
        timeout(self.operation_timeout, self.record_inner(facts))
            .await
            .map_err(|_| OrganizationScimAuditRepositoryError::Timeout)?
    }

    async fn record_inner(
        &self,
        facts: &OrganizationScimAuditFacts,
    ) -> Result<OrganizationScimAuditWriteOutcome, OrganizationScimAuditRepositoryError> {
        let (resource_kind, resource_digest, resource_version) = facts
            .resource()
            .map(resource_columns)
            .transpose()?
            .map(|(kind, digest, version)| (Some(kind), Some(digest), Some(version)))
            .unwrap_or((None, None, None));
        let retry_after_seconds = facts
            .retry_after_seconds()
            .map(i32::try_from)
            .transpose()
            .map_err(|_| OrganizationScimAuditRepositoryError::Invariant)?;
        let model = organization_scim_audit_logs::ActiveModel {
            organization_id: Set(facts.organization_id().get()),
            token_key: Set(facts.token_key().map(|value| value.as_str().to_owned())),
            event_type: Set(facts.event_type().code().to_owned()),
            result: Set(facts.result().code().to_owned()),
            reason: Set(facts.reason().map(|value| value.code().to_owned())),
            resource_kind: Set(resource_kind),
            resource_digest: Set(resource_digest),
            resource_version: Set(resource_version),
            request_fingerprint: Set(hex_fingerprint(*facts.request_fingerprint().as_bytes())),
            retry_after_seconds: Set(retry_after_seconds),
            created_at: Set(TimeDateTimeWithTimeZone::now_utc()),
            ..Default::default()
        };
        match model.insert(self.pool.connection()).await {
            Ok(_) => Ok(OrganizationScimAuditWriteOutcome::Applied),
            Err(error) if is_unique_violation(&error) => self.classify_existing(facts).await,
            Err(_) => Err(internal_error(OrganizationScimAuditRepositoryError::Query)),
        }
    }

    async fn classify_existing(
        &self,
        facts: &OrganizationScimAuditFacts,
    ) -> Result<OrganizationScimAuditWriteOutcome, OrganizationScimAuditRepositoryError> {
        let fingerprint = hex_fingerprint(*facts.request_fingerprint().as_bytes());
        let existing = organization_scim_audit_logs::Entity::find()
            .filter(
                organization_scim_audit_logs::Column::OrganizationId
                    .eq(facts.organization_id().get()),
            )
            .filter(organization_scim_audit_logs::Column::RequestFingerprint.eq(&fingerprint))
            .filter(organization_scim_audit_logs::Column::EventType.eq(facts.event_type().code()))
            .one(self.pool.connection())
            .await
            .map_err(|_| internal_error(OrganizationScimAuditRepositoryError::Query))?
            .ok_or_else(|| internal_error(OrganizationScimAuditRepositoryError::Invariant))?;
        if matches_facts(&existing, facts) {
            Ok(OrganizationScimAuditWriteOutcome::Existing)
        } else {
            Err(OrganizationScimAuditRepositoryError::Conflict)
        }
    }
}

impl fmt::Debug for OrganizationScimAuditRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationScimAuditRepository(<redacted>)")
    }
}

fn resource_columns(
    resource: &OrganizationScimAuditResource,
) -> Result<(i16, String, i64), OrganizationScimAuditRepositoryError> {
    Ok((
        match resource.kind() {
            OrganizationScimResourceKind::Users => 1,
            OrganizationScimResourceKind::Groups => 2,
        },
        resource.digest().persistence_key(),
        i64::try_from(resource.version().get())
            .map_err(|_| OrganizationScimAuditRepositoryError::Invariant)?,
    ))
}

fn matches_facts(
    model: &organization_scim_audit_logs::Model,
    facts: &OrganizationScimAuditFacts,
) -> bool {
    let (resource_kind, resource_digest, resource_version) = facts
        .resource()
        .and_then(|resource| resource_columns(resource).ok())
        .map(|(kind, digest, version)| (Some(kind), Some(digest), Some(version)))
        .unwrap_or((None, None, None));
    model.organization_id == facts.organization_id().get()
        && model.token_key.as_deref() == facts.token_key().map(|value| value.as_str())
        && model.event_type == facts.event_type().code()
        && model.result == facts.result().code()
        && model.reason.as_deref() == facts.reason().map(|value| value.code())
        && model.resource_kind == resource_kind
        && model.resource_digest == resource_digest
        && model.resource_version == resource_version
        && model.request_fingerprint == hex_fingerprint(*facts.request_fingerprint().as_bytes())
        && model.retry_after_seconds
            == facts
                .retry_after_seconds()
                .and_then(|value| i32::try_from(value).ok())
}

fn hex_fingerprint(value: [u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(64);
    for byte in value {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

fn is_unique_violation(error: &DbErr) -> bool {
    matches!(error.sql_err(), Some(SqlErr::UniqueConstraintViolation(_)))
}

fn internal_error(
    error: OrganizationScimAuditRepositoryError,
) -> OrganizationScimAuditRepositoryError {
    let kind = match error {
        OrganizationScimAuditRepositoryError::Query => "organization_scim_audit_query",
        OrganizationScimAuditRepositoryError::Timeout => "organization_scim_audit_timeout",
        OrganizationScimAuditRepositoryError::Conflict => return error,
        OrganizationScimAuditRepositoryError::Invariant => "organization_scim_audit_invariant",
    };
    tracing::error!(target: "af_db::organization_scim_audit", error_kind = kind, "SCIM 审计仓储发生内部错误");
    error
}
