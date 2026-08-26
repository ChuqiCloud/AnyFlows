use std::{fmt, time::Duration};

use af_domain::{
    OrganizationId, OrganizationScimResourceSet, OrganizationScimToken,
    OrganizationScimTokenDigest, OrganizationScimTokenError, OrganizationScimTokenId,
    OrganizationScimTokenKey, OrganizationScimTokenStatus,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QuerySelect, Set, TransactionTrait,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Expr, LockType},
};
use thiserror::Error;
use tokio::time::timeout;

use crate::{
    DatabasePool,
    entity::{SensitiveString, organization_scim_tokens, organizations},
};

const MAX_SCIM_TOKEN_NAME_BYTES: usize = 128;

/// 企业 SCIM 令牌待签发字段；不接受外部令牌原文或摘要。
pub struct OrganizationScimTokenCreateRecord {
    organization_id: OrganizationId,
    name: String,
    resource_scope: OrganizationScimResourceSet,
}

impl OrganizationScimTokenCreateRecord {
    /// 组装令牌签发请求，名称和资源范围在仓储边界再次校验。
    #[must_use]
    pub fn new(
        organization_id: OrganizationId,
        name: String,
        resource_scope: OrganizationScimResourceSet,
    ) -> Self {
        Self {
            organization_id,
            name,
            resource_scope,
        }
    }
}

impl fmt::Debug for OrganizationScimTokenCreateRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationScimTokenCreateRecord")
            .field("organization_id", &self.organization_id)
            .field("name", &self.name)
            .field("resource_scope", &self.resource_scope)
            .finish()
    }
}

/// 令牌签发结果；明文令牌只允许由调用方在创建响应中消费一次。
pub struct OrganizationScimTokenIssued {
    record: OrganizationScimTokenRecord,
    token: OrganizationScimToken,
}

impl OrganizationScimTokenIssued {
    #[must_use]
    pub fn into_parts(self) -> (OrganizationScimTokenRecord, OrganizationScimToken) {
        (self.record, self.token)
    }
}

impl fmt::Debug for OrganizationScimTokenIssued {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationScimTokenIssued(<redacted>)")
    }
}

/// 企业 SCIM 令牌的脱敏持久化投影。
pub struct OrganizationScimTokenRecord {
    id: OrganizationScimTokenId,
    organization_id: OrganizationId,
    token_key: OrganizationScimTokenKey,
    token_prefix: String,
    name: String,
    resource_scope: OrganizationScimResourceSet,
    status: OrganizationScimTokenStatus,
    version: i64,
    created_at: i64,
    updated_at: i64,
    revoked_at: Option<i64>,
}

impl OrganizationScimTokenRecord {
    #[must_use]
    pub const fn id(&self) -> OrganizationScimTokenId {
        self.id
    }
    #[must_use]
    pub const fn organization_id(&self) -> OrganizationId {
        self.organization_id
    }
    #[must_use]
    pub fn token_key(&self) -> &OrganizationScimTokenKey {
        &self.token_key
    }
    #[must_use]
    pub fn token_prefix(&self) -> &str {
        &self.token_prefix
    }
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
    #[must_use]
    pub const fn resource_scope(&self) -> OrganizationScimResourceSet {
        self.resource_scope
    }
    #[must_use]
    pub const fn status(&self) -> OrganizationScimTokenStatus {
        self.status
    }
    #[must_use]
    pub const fn version(&self) -> i64 {
        self.version
    }
    #[must_use]
    pub const fn created_at(&self) -> i64 {
        self.created_at
    }
    #[must_use]
    pub const fn updated_at(&self) -> i64 {
        self.updated_at
    }
    #[must_use]
    pub const fn revoked_at(&self) -> Option<i64> {
        self.revoked_at
    }
}

impl fmt::Debug for OrganizationScimTokenRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationScimTokenRecord")
            .field("id", &self.id)
            .field("organization_id", &self.organization_id)
            .field("token_key", &self.token_key)
            .field("token_prefix", &self.token_prefix)
            .field("name", &self.name)
            .field("resource_scope", &self.resource_scope)
            .field("status", &self.status)
            .field("version", &self.version)
            .finish_non_exhaustive()
    }
}

/// SCIM 令牌存储错误；不向边界泄露令牌、摘要或跨企业命中事实。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationScimTokenRepositoryError {
    #[error("企业 SCIM 令牌输入无效")]
    InvalidInput,
    #[error("企业 SCIM 令牌企业范围不可用")]
    Restricted,
    #[error("企业 SCIM 令牌不存在")]
    NotFound,
    #[error("企业 SCIM 令牌版本冲突")]
    Conflict,
    #[error("企业 SCIM 令牌数据库操作失败")]
    Query,
    #[error("企业 SCIM 令牌操作超时")]
    Timeout,
    #[error("企业 SCIM 令牌持久化状态损坏")]
    Invariant,
}

#[derive(Clone)]
pub struct OrganizationScimTokenRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl OrganizationScimTokenRepository {
    /// 创建带截止时间的 SCIM 令牌仓储。
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, OrganizationScimTokenRepositoryError> {
        if operation_timeout.is_zero() {
            return Err(OrganizationScimTokenRepositoryError::InvalidInput);
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }

    /// 在活动企业内签发令牌并只将摘要写入数据库。
    pub async fn create(
        &self,
        request: OrganizationScimTokenCreateRecord,
    ) -> Result<OrganizationScimTokenIssued, OrganizationScimTokenRepositoryError> {
        timeout(self.operation_timeout, self.create_inner(request))
            .await
            .map_err(|_| OrganizationScimTokenRepositoryError::Timeout)?
    }

    /// 按企业范围校验令牌；撤销或跨企业令牌统一不可见。
    pub async fn authenticate_for_organization(
        &self,
        organization_id: OrganizationId,
        token: OrganizationScimToken,
    ) -> Result<OrganizationScimTokenRecord, OrganizationScimTokenRepositoryError> {
        timeout(
            self.operation_timeout,
            self.authenticate_inner(organization_id, token.digest()),
        )
        .await
        .map_err(|_| OrganizationScimTokenRepositoryError::Timeout)?
    }

    /// 按企业和版本原子撤销令牌；重复撤销不恢复令牌或版本。
    pub async fn revoke(
        &self,
        organization_id: OrganizationId,
        token_id: OrganizationScimTokenId,
        expected_version: i64,
    ) -> Result<OrganizationScimTokenRecord, OrganizationScimTokenRepositoryError> {
        if expected_version <= 0 {
            return Err(OrganizationScimTokenRepositoryError::InvalidInput);
        }
        timeout(
            self.operation_timeout,
            self.revoke_inner(organization_id, token_id, expected_version),
        )
        .await
        .map_err(|_| OrganizationScimTokenRepositoryError::Timeout)?
    }

    async fn create_inner(
        &self,
        request: OrganizationScimTokenCreateRecord,
    ) -> Result<OrganizationScimTokenIssued, OrganizationScimTokenRepositoryError> {
        if request.name.trim() != request.name
            || request.name.is_empty()
            || request.name.len() > MAX_SCIM_TOKEN_NAME_BYTES
            || request.name.chars().any(char::is_control)
        {
            return Err(OrganizationScimTokenRepositoryError::InvalidInput);
        }
        let organization_id = request.organization_id;
        let transaction = self
            .pool
            .connection()
            .begin()
            .await
            .map_err(|_| OrganizationScimTokenRepositoryError::Query)?;
        let organization = organizations::Entity::find_by_id(organization_id.get())
            .lock(LockType::Update)
            .one(&transaction)
            .await
            .map_err(|_| OrganizationScimTokenRepositoryError::Query)?
            .ok_or(OrganizationScimTokenRepositoryError::Restricted)?;
        if organization.status != 1 {
            return Err(OrganizationScimTokenRepositoryError::Restricted);
        }

        let token = OrganizationScimToken::generate()
            .map_err(|_| OrganizationScimTokenRepositoryError::Query)?;
        let digest = token.digest();
        let token_key = OrganizationScimTokenKey::generate()
            .map_err(|_| OrganizationScimTokenRepositoryError::Query)?;
        let now = TimeDateTimeWithTimeZone::now_utc();
        let model = organization_scim_tokens::ActiveModel {
            organization_id: Set(organization_id.get()),
            token_key: Set(token_key.as_str().to_owned()),
            token_prefix: Set(token.prefix().to_owned()),
            token_digest: Set(SensitiveString::from(digest.as_str())),
            name: Set(request.name),
            resource_scope: Set(request.resource_scope.code()),
            status: Set(OrganizationScimTokenStatus::Active.code()),
            version: Set(1),
            created_at: Set(now),
            updated_at: Set(now),
            revoked_at: Set(None),
            ..Default::default()
        };
        let inserted = model
            .insert(&transaction)
            .await
            .map_err(|_| OrganizationScimTokenRepositoryError::Query)?;
        let record = record_from_model(inserted)?;
        transaction
            .commit()
            .await
            .map_err(|_| OrganizationScimTokenRepositoryError::Query)?;
        Ok(OrganizationScimTokenIssued { record, token })
    }

    async fn authenticate_inner(
        &self,
        organization_id: OrganizationId,
        digest: OrganizationScimTokenDigest,
    ) -> Result<OrganizationScimTokenRecord, OrganizationScimTokenRepositoryError> {
        let model = organization_scim_tokens::Entity::find()
            .filter(organization_scim_tokens::Column::OrganizationId.eq(organization_id.get()))
            .filter(organization_scim_tokens::Column::TokenDigest.eq(digest.as_str()))
            .filter(
                organization_scim_tokens::Column::Status
                    .eq(OrganizationScimTokenStatus::Active.code()),
            )
            .one(self.pool.connection())
            .await
            .map_err(|_| OrganizationScimTokenRepositoryError::Query)?
            .ok_or(OrganizationScimTokenRepositoryError::NotFound)?;
        record_from_model(model)
    }

    async fn revoke_inner(
        &self,
        organization_id: OrganizationId,
        token_id: OrganizationScimTokenId,
        expected_version: i64,
    ) -> Result<OrganizationScimTokenRecord, OrganizationScimTokenRepositoryError> {
        let now = TimeDateTimeWithTimeZone::now_utc();
        let result = organization_scim_tokens::Entity::update_many()
            .col_expr(
                organization_scim_tokens::Column::Status,
                Expr::value(OrganizationScimTokenStatus::Revoked.code()),
            )
            .col_expr(
                organization_scim_tokens::Column::Version,
                Expr::col(organization_scim_tokens::Column::Version).add(1_i64),
            )
            .col_expr(
                organization_scim_tokens::Column::UpdatedAt,
                Expr::value(now),
            )
            .col_expr(
                organization_scim_tokens::Column::RevokedAt,
                Expr::value(now),
            )
            .filter(organization_scim_tokens::Column::Id.eq(token_id.get()))
            .filter(organization_scim_tokens::Column::OrganizationId.eq(organization_id.get()))
            .filter(organization_scim_tokens::Column::Version.eq(expected_version))
            .filter(
                organization_scim_tokens::Column::Status
                    .eq(OrganizationScimTokenStatus::Active.code()),
            )
            .exec(self.pool.connection())
            .await
            .map_err(|_| OrganizationScimTokenRepositoryError::Query)?;
        if result.rows_affected == 0 {
            return Err(OrganizationScimTokenRepositoryError::Conflict);
        }
        let model = organization_scim_tokens::Entity::find_by_id(token_id.get())
            .filter(organization_scim_tokens::Column::OrganizationId.eq(organization_id.get()))
            .one(self.pool.connection())
            .await
            .map_err(|_| OrganizationScimTokenRepositoryError::Query)?
            .ok_or(OrganizationScimTokenRepositoryError::NotFound)?;
        record_from_model(model)
    }
}

fn record_from_model(
    model: organization_scim_tokens::Model,
) -> Result<OrganizationScimTokenRecord, OrganizationScimTokenRepositoryError> {
    let id = OrganizationScimTokenId::new(model.id)
        .map_err(|_| OrganizationScimTokenRepositoryError::Invariant)?;
    let organization_id = OrganizationId::new(model.organization_id)
        .map_err(|_| OrganizationScimTokenRepositoryError::Invariant)?;
    let token_key = OrganizationScimTokenKey::new(model.token_key)
        .map_err(|_| OrganizationScimTokenRepositoryError::Invariant)?;
    OrganizationScimTokenDigest::from_persistence_key(model.token_digest.as_str())
        .map_err(|_| OrganizationScimTokenRepositoryError::Invariant)?;
    let resource_scope = OrganizationScimResourceSet::from_code(model.resource_scope)
        .map_err(|_| OrganizationScimTokenRepositoryError::Invariant)?;
    let status = OrganizationScimTokenStatus::from_code(model.status)
        .map_err(|_| OrganizationScimTokenRepositoryError::Invariant)?;
    if model.token_prefix != "scim_"
        || model.name.trim() != model.name
        || model.name.is_empty()
        || model.name.len() > MAX_SCIM_TOKEN_NAME_BYTES
        || model.version <= 0
        || (status == OrganizationScimTokenStatus::Active && model.revoked_at.is_some())
        || (status == OrganizationScimTokenStatus::Revoked && model.revoked_at.is_none())
    {
        return Err(OrganizationScimTokenRepositoryError::Invariant);
    }
    Ok(OrganizationScimTokenRecord {
        id,
        organization_id,
        token_key,
        token_prefix: model.token_prefix,
        name: model.name,
        resource_scope,
        status,
        version: model.version,
        created_at: model.created_at.unix_timestamp(),
        updated_at: model.updated_at.unix_timestamp(),
        revoked_at: model.revoked_at.map(|value| value.unix_timestamp()),
    })
}

impl From<OrganizationScimTokenError> for OrganizationScimTokenRepositoryError {
    fn from(_: OrganizationScimTokenError) -> Self {
        Self::Invariant
    }
}
