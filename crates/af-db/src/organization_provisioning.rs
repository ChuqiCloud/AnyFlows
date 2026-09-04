use std::{fmt, future::Future, time::Duration};

use af_domain::{
    OrganizationId, OrganizationProvisioningCreateOutcome, OrganizationProvisioningIdempotencyKey,
    OrganizationProvisioningRecord, OrganizationProvisioningRequest,
    OrganizationProvisioningRequestId, OrganizationProvisioningRequestKey,
    OrganizationProvisioningStatus, UserId,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, Set,
    entity::prelude::TimeDateTimeWithTimeZone,
};
use thiserror::Error;
use tokio::time::timeout;

use crate::{
    DatabasePool,
    entity::{SensitiveString, organization_provisioning_requests, users},
};

const ACTIVE_USER_STATUS: i16 = 1;
const ACTIVE_SLOT: i16 = 1;

/// 平台范围的企业开通申请事实仓储。
///
/// 本切片只创建和读取待审批事实，不执行审批、企业创建或商业授权发放。
#[derive(Clone)]
pub struct OrganizationProvisioningRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl OrganizationProvisioningRepository {
    /// 使用共享连接池和非零数据库操作截止时间构造仓储。
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, OrganizationProvisioningRepositoryConfigError> {
        if operation_timeout.is_zero() {
            return Err(OrganizationProvisioningRepositoryConfigError::ZeroOperationTimeout);
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }

    /// 幂等创建当前用户的企业开通申请。
    ///
    /// 同一用户和同一企业规范标识分别最多存在一个活动申请；写入超时只返回结果未知，
    /// 调用方必须使用原幂等键恢复，不能生成新申请。
    pub async fn create_request(
        &self,
        request: OrganizationProvisioningRequest,
    ) -> Result<OrganizationProvisioningCreateOutcome, OrganizationProvisioningRepositoryError>
    {
        match timeout(
            self.operation_timeout,
            create_request_inner(self.pool.connection(), &request),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(OrganizationProvisioningRepositoryError::OutcomeUnknown),
        }
    }

    /// 按当前用户和公开申请键读取事实；跨用户申请统一不可见。
    pub async fn get_request(
        &self,
        applicant_user_id: UserId,
        request_key: OrganizationProvisioningRequestKey,
    ) -> Result<Option<OrganizationProvisioningRecord>, OrganizationProvisioningRepositoryError>
    {
        self.run_read(load_by_request_key(
            self.pool.connection(),
            applicant_user_id,
            request_key,
        ))
        .await
    }

    /// 按当前用户和幂等键读取事实，供提交结果未知时恢复。
    pub async fn get_by_idempotency_key(
        &self,
        applicant_user_id: UserId,
        idempotency_key: OrganizationProvisioningIdempotencyKey,
    ) -> Result<Option<OrganizationProvisioningRecord>, OrganizationProvisioningRepositoryError>
    {
        self.run_read(load_by_idempotency_key(
            self.pool.connection(),
            applicant_user_id,
            idempotency_key,
        ))
        .await
    }

    /// 读取当前用户唯一的活动申请。
    pub async fn get_active_request(
        &self,
        applicant_user_id: UserId,
    ) -> Result<Option<OrganizationProvisioningRecord>, OrganizationProvisioningRepositoryError>
    {
        self.run_read(load_active_by_applicant(
            self.pool.connection(),
            applicant_user_id,
        ))
        .await
    }

    async fn run_read<T>(
        &self,
        operation: impl Future<Output = Result<T, OrganizationProvisioningRepositoryError>>,
    ) -> Result<T, OrganizationProvisioningRepositoryError> {
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result,
            Err(_) => Err(OrganizationProvisioningRepositoryError::Timeout),
        }
    }
}

impl fmt::Debug for OrganizationProvisioningRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationProvisioningRepository")
            .field("operation_timeout", &self.operation_timeout)
            .finish_non_exhaustive()
    }
}

async fn create_request_inner(
    connection: &DatabaseConnection,
    request: &OrganizationProvisioningRequest,
) -> Result<OrganizationProvisioningCreateOutcome, OrganizationProvisioningRepositoryError> {
    if let Some(existing) = find_by_idempotency_key(
        connection,
        request.applicant_user_id(),
        request.idempotency_key(),
    )
    .await?
    {
        return existing_outcome(existing, request);
    }
    if let Some(existing) = find_by_request_key(connection, request.request_key()).await? {
        return existing_outcome(existing, request);
    }
    ensure_active_applicant(connection, request.applicant_user_id()).await?;
    if let Some(existing) =
        find_active_by_applicant(connection, request.applicant_user_id()).await?
    {
        return active_applicant_outcome(existing, request);
    }
    if let Some(existing) = find_active_by_slug(connection, request.organization_slug()).await? {
        return active_slug_outcome(existing, request);
    }

    let created_at = to_database_time(request.created_at())?;
    let model = organization_provisioning_requests::ActiveModel {
        request_key: Set(request.request_key().persistence_key()),
        idempotency_key: Set(request.idempotency_key().persistence_key()),
        applicant_user_id: Set(request.applicant_user_id().get()),
        organization_name: Set(request.organization_name().to_owned()),
        organization_slug: Set(request.organization_slug().to_owned()),
        business_reason: Set(SensitiveString::from(request.business_reason())),
        status: Set(OrganizationProvisioningStatus::Pending.code()),
        active_slot: Set(Some(ACTIVE_SLOT)),
        version: Set(1),
        reviewer_user_id: Set(None),
        decision_reason: Set(None),
        organization_id: Set(None),
        created_at: Set(created_at),
        updated_at: Set(created_at),
        decided_at: Set(None),
        ..Default::default()
    };

    match model.insert(connection).await {
        Ok(model) => Ok(OrganizationProvisioningCreateOutcome::Created(to_record(
            model,
        )?)),
        Err(_) => recover_create_conflict(connection, request).await,
    }
}

/// 并发唯一冲突发生后按全部稳定事实回读，只恢复完全一致的原申请。
async fn recover_create_conflict(
    connection: &DatabaseConnection,
    request: &OrganizationProvisioningRequest,
) -> Result<OrganizationProvisioningCreateOutcome, OrganizationProvisioningRepositoryError> {
    if let Some(existing) = find_by_idempotency_key(
        connection,
        request.applicant_user_id(),
        request.idempotency_key(),
    )
    .await?
    {
        return existing_outcome(existing, request);
    }
    if let Some(existing) = find_by_request_key(connection, request.request_key()).await? {
        return existing_outcome(existing, request);
    }
    if let Some(existing) =
        find_active_by_applicant(connection, request.applicant_user_id()).await?
    {
        return active_applicant_outcome(existing, request);
    }
    if let Some(existing) = find_active_by_slug(connection, request.organization_slug()).await? {
        return active_slug_outcome(existing, request);
    }
    Err(OrganizationProvisioningRepositoryError::Query)
}

fn existing_outcome(
    model: organization_provisioning_requests::Model,
    request: &OrganizationProvisioningRequest,
) -> Result<OrganizationProvisioningCreateOutcome, OrganizationProvisioningRepositoryError> {
    let record = to_record(model)?;
    if record.matches_create(request) {
        Ok(OrganizationProvisioningCreateOutcome::Existing(record))
    } else {
        Err(OrganizationProvisioningRepositoryError::Conflict)
    }
}

fn active_applicant_outcome(
    model: organization_provisioning_requests::Model,
    request: &OrganizationProvisioningRequest,
) -> Result<OrganizationProvisioningCreateOutcome, OrganizationProvisioningRepositoryError> {
    let record = to_record(model)?;
    if record.matches_create(request) {
        Ok(OrganizationProvisioningCreateOutcome::Existing(record))
    } else {
        Err(OrganizationProvisioningRepositoryError::ActiveApplicantConflict)
    }
}

fn active_slug_outcome(
    model: organization_provisioning_requests::Model,
    request: &OrganizationProvisioningRequest,
) -> Result<OrganizationProvisioningCreateOutcome, OrganizationProvisioningRepositoryError> {
    let record = to_record(model)?;
    if record.matches_create(request) {
        Ok(OrganizationProvisioningCreateOutcome::Existing(record))
    } else {
        Err(OrganizationProvisioningRepositoryError::ActiveSlugConflict)
    }
}

async fn ensure_active_applicant(
    connection: &DatabaseConnection,
    applicant_user_id: UserId,
) -> Result<(), OrganizationProvisioningRepositoryError> {
    let applicant = users::Entity::find_by_id(applicant_user_id.get())
        .one(connection)
        .await
        .map_err(query_error)?
        .filter(|model| model.status == ACTIVE_USER_STATUS && model.deleted_at.is_none());
    if applicant.is_none() {
        return Err(OrganizationProvisioningRepositoryError::ApplicantUnavailable);
    }
    Ok(())
}

async fn load_by_request_key(
    connection: &DatabaseConnection,
    applicant_user_id: UserId,
    request_key: OrganizationProvisioningRequestKey,
) -> Result<Option<OrganizationProvisioningRecord>, OrganizationProvisioningRepositoryError> {
    organization_provisioning_requests::Entity::find()
        .filter(
            organization_provisioning_requests::Column::ApplicantUserId.eq(applicant_user_id.get()),
        )
        .filter(
            organization_provisioning_requests::Column::RequestKey
                .eq(request_key.persistence_key()),
        )
        .one(connection)
        .await
        .map_err(query_error)?
        .map(to_record)
        .transpose()
}

async fn load_by_idempotency_key(
    connection: &DatabaseConnection,
    applicant_user_id: UserId,
    idempotency_key: OrganizationProvisioningIdempotencyKey,
) -> Result<Option<OrganizationProvisioningRecord>, OrganizationProvisioningRepositoryError> {
    find_by_idempotency_key(connection, applicant_user_id, idempotency_key)
        .await?
        .map(to_record)
        .transpose()
}

async fn load_active_by_applicant(
    connection: &DatabaseConnection,
    applicant_user_id: UserId,
) -> Result<Option<OrganizationProvisioningRecord>, OrganizationProvisioningRepositoryError> {
    find_active_by_applicant(connection, applicant_user_id)
        .await?
        .map(to_record)
        .transpose()
}

async fn find_by_idempotency_key(
    connection: &DatabaseConnection,
    applicant_user_id: UserId,
    idempotency_key: OrganizationProvisioningIdempotencyKey,
) -> Result<
    Option<organization_provisioning_requests::Model>,
    OrganizationProvisioningRepositoryError,
> {
    organization_provisioning_requests::Entity::find()
        .filter(
            organization_provisioning_requests::Column::ApplicantUserId.eq(applicant_user_id.get()),
        )
        .filter(
            organization_provisioning_requests::Column::IdempotencyKey
                .eq(idempotency_key.persistence_key()),
        )
        .one(connection)
        .await
        .map_err(query_error)
}

async fn find_by_request_key(
    connection: &DatabaseConnection,
    request_key: OrganizationProvisioningRequestKey,
) -> Result<
    Option<organization_provisioning_requests::Model>,
    OrganizationProvisioningRepositoryError,
> {
    organization_provisioning_requests::Entity::find()
        .filter(
            organization_provisioning_requests::Column::RequestKey
                .eq(request_key.persistence_key()),
        )
        .one(connection)
        .await
        .map_err(query_error)
}

async fn find_active_by_applicant(
    connection: &DatabaseConnection,
    applicant_user_id: UserId,
) -> Result<
    Option<organization_provisioning_requests::Model>,
    OrganizationProvisioningRepositoryError,
> {
    organization_provisioning_requests::Entity::find()
        .filter(
            organization_provisioning_requests::Column::ApplicantUserId.eq(applicant_user_id.get()),
        )
        .filter(organization_provisioning_requests::Column::ActiveSlot.eq(ACTIVE_SLOT))
        .order_by_desc(organization_provisioning_requests::Column::Id)
        .one(connection)
        .await
        .map_err(query_error)
}

async fn find_active_by_slug(
    connection: &DatabaseConnection,
    organization_slug: &str,
) -> Result<
    Option<organization_provisioning_requests::Model>,
    OrganizationProvisioningRepositoryError,
> {
    organization_provisioning_requests::Entity::find()
        .filter(organization_provisioning_requests::Column::OrganizationSlug.eq(organization_slug))
        .filter(organization_provisioning_requests::Column::ActiveSlot.eq(ACTIVE_SLOT))
        .order_by_desc(organization_provisioning_requests::Column::Id)
        .one(connection)
        .await
        .map_err(query_error)
}

fn to_record(
    model: organization_provisioning_requests::Model,
) -> Result<OrganizationProvisioningRecord, OrganizationProvisioningRepositoryError> {
    let status = OrganizationProvisioningStatus::try_from(model.status)
        .map_err(|_| OrganizationProvisioningRepositoryError::Invariant)?;
    let expected_active_slot =
        (status == OrganizationProvisioningStatus::Pending).then_some(ACTIVE_SLOT);
    if model.active_slot != expected_active_slot {
        return Err(OrganizationProvisioningRepositoryError::Invariant);
    }
    let request = OrganizationProvisioningRequest::new(
        OrganizationProvisioningRequestKey::from_persistence_key(&model.request_key)
            .map_err(|_| OrganizationProvisioningRepositoryError::Invariant)?,
        OrganizationProvisioningIdempotencyKey::from_persistence_key(&model.idempotency_key)
            .map_err(|_| OrganizationProvisioningRepositoryError::Invariant)?,
        UserId::new(model.applicant_user_id)
            .map_err(|_| OrganizationProvisioningRepositoryError::Invariant)?,
        model.organization_name,
        model.organization_slug,
        model.business_reason.as_str().to_owned(),
        unix_seconds(model.created_at)?,
    )
    .map_err(|_| OrganizationProvisioningRepositoryError::Invariant)?;
    let reviewer_user_id = model
        .reviewer_user_id
        .map(UserId::new)
        .transpose()
        .map_err(|_| OrganizationProvisioningRepositoryError::Invariant)?;
    let organization_id = model
        .organization_id
        .map(OrganizationId::new)
        .transpose()
        .map_err(|_| OrganizationProvisioningRepositoryError::Invariant)?;
    OrganizationProvisioningRecord::from_persistence(
        OrganizationProvisioningRequestId::new(model.id)
            .map_err(|_| OrganizationProvisioningRepositoryError::Invariant)?,
        request,
        status,
        u64::try_from(model.version)
            .map_err(|_| OrganizationProvisioningRepositoryError::Invariant)?,
        reviewer_user_id,
        model.decision_reason.map(|value| value.as_str().to_owned()),
        organization_id,
        unix_seconds(model.updated_at)?,
        model.decided_at.map(unix_seconds).transpose()?,
    )
    .map_err(|_| OrganizationProvisioningRepositoryError::Invariant)
}

fn to_database_time(
    value: u64,
) -> Result<TimeDateTimeWithTimeZone, OrganizationProvisioningRepositoryError> {
    let value =
        i64::try_from(value).map_err(|_| OrganizationProvisioningRepositoryError::Invariant)?;
    TimeDateTimeWithTimeZone::from_unix_timestamp(value)
        .map_err(|_| OrganizationProvisioningRepositoryError::Invariant)
}

fn unix_seconds(
    value: TimeDateTimeWithTimeZone,
) -> Result<u64, OrganizationProvisioningRepositoryError> {
    u64::try_from(value.unix_timestamp())
        .map_err(|_| OrganizationProvisioningRepositoryError::Invariant)
}

fn query_error(_: sea_orm::DbErr) -> OrganizationProvisioningRepositoryError {
    OrganizationProvisioningRepositoryError::Query
}

/// 企业开通申请仓储构造错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationProvisioningRepositoryConfigError {
    #[error("企业开通申请仓储超时必须大于零")]
    ZeroOperationTimeout,
}

/// 企业开通申请仓储错误；不携带企业资料或业务说明。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationProvisioningRepositoryError {
    #[error("企业开通申请事实冲突")]
    Conflict,
    #[error("当前用户已有活动企业开通申请")]
    ActiveApplicantConflict,
    #[error("企业规范标识已有活动开通申请")]
    ActiveSlugConflict,
    #[error("企业开通申请人不可用")]
    ApplicantUnavailable,
    #[error("企业开通申请数据库操作失败")]
    Query,
    #[error("企业开通申请写入结果未知")]
    OutcomeUnknown,
    #[error("企业开通申请数据库操作超时")]
    Timeout,
    #[error("企业开通申请持久化状态损坏")]
    Invariant,
}
