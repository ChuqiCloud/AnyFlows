use std::time::Duration;

use af_domain::{
    OrganizationId, OrganizationVerificationCase, OrganizationVerificationCaseId,
    OrganizationVerificationMaterial, OrganizationVerificationMaterialId,
    OrganizationVerificationStatus, UserId,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter,
    QueryOrder, QuerySelect, Set, TransactionTrait, entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::LockType,
};
use thiserror::Error;
use tokio::time::timeout;

use crate::{
    DatabasePool,
    entity::{
        SensitiveString, organization_verification_cases, organization_verification_materials,
        users,
    },
};

/// 待提交的企业认证材料；正文写入数据库受控二进制列，不进入普通查询投影。
pub struct OrganizationVerificationMaterialWrite {
    pub kind: String,
    pub object_reference: String,
    pub file_name: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub content_bytes: Vec<u8>,
}

/// 企业认证案件创建或补充写入。
pub struct OrganizationVerificationWrite {
    pub organization_id: OrganizationId,
    pub applicant_user_id: UserId,
    pub summary: String,
    pub materials: Vec<OrganizationVerificationMaterialWrite>,
    pub created_at: u64,
}

/// 企业认证案件及材料安全投影。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationVerificationBundle {
    case: OrganizationVerificationCase,
    materials: Vec<OrganizationVerificationMaterial>,
}

impl OrganizationVerificationBundle {
    #[must_use]
    pub fn case(&self) -> &OrganizationVerificationCase {
        &self.case
    }
    #[must_use]
    pub fn materials(&self) -> &[OrganizationVerificationMaterial] {
        &self.materials
    }
}

/// 认证案件仓储写入结果。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OrganizationVerificationWriteOutcome {
    Created(OrganizationVerificationBundle),
    Resubmitted(OrganizationVerificationBundle),
    Existing(OrganizationVerificationBundle),
}

/// 认证案件仓储；所有材料正文都不经过该边界。
#[derive(Clone)]
pub struct OrganizationVerificationRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl OrganizationVerificationRepository {
    pub async fn history(
        &self,
        case_id: OrganizationVerificationCaseId,
        before: Option<i64>,
    ) -> Result<Vec<serde_json::Value>, OrganizationVerificationRepositoryError> {
        use crate::entity::organization_verification_history as history;
        if before.is_some_and(|v| v <= 0) {
            return Err(OrganizationVerificationRepositoryError::InvalidInput);
        }
        timeout(self.operation_timeout, async {
            let mut query =
                history::Entity::find().filter(history::Column::CaseId.eq(case_id.get()));
            if let Some(before) = before {
                query = query.filter(history::Column::Version.lt(before));
            }
            query
                .order_by_desc(history::Column::Version)
                .limit(25)
                .all(self.pool.connection())
                .await
                .map_err(query_error)?
                .into_iter()
                .map(|row| {
                    serde_json::from_str(row.snapshot.as_str())
                        .map_err(|_| OrganizationVerificationRepositoryError::Invariant)
                })
                .collect()
        })
        .await
        .map_err(|_| OrganizationVerificationRepositoryError::Query)?
    }
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, OrganizationVerificationRepositoryConfigError> {
        if operation_timeout.is_zero() {
            return Err(OrganizationVerificationRepositoryConfigError::ZeroOperationTimeout);
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }

    /// 创建首个案件，或在补充/驳回状态下复用原案件重新提交。
    pub async fn create_or_resubmit(
        &self,
        write: OrganizationVerificationWrite,
    ) -> Result<OrganizationVerificationWriteOutcome, OrganizationVerificationRepositoryError> {
        timeout(
            self.operation_timeout,
            create_or_resubmit_inner(self.pool.connection(), write),
        )
        .await
        .map_err(|_| OrganizationVerificationRepositoryError::OutcomeUnknown)?
    }

    pub async fn get_by_organization(
        &self,
        organization_id: OrganizationId,
    ) -> Result<Option<OrganizationVerificationBundle>, OrganizationVerificationRepositoryError>
    {
        timeout(
            self.operation_timeout,
            load_by_organization(self.pool.connection(), organization_id),
        )
        .await
        .map_err(|_| OrganizationVerificationRepositoryError::Query)?
    }

    pub async fn get_by_id(
        &self,
        case_id: OrganizationVerificationCaseId,
    ) -> Result<Option<OrganizationVerificationBundle>, OrganizationVerificationRepositoryError>
    {
        timeout(
            self.operation_timeout,
            load_by_id(self.pool.connection(), case_id),
        )
        .await
        .map_err(|_| OrganizationVerificationRepositoryError::Query)?
    }

    pub async fn list(
        &self,
        status: Option<OrganizationVerificationStatus>,
        before: Option<i64>,
        limit: usize,
    ) -> Result<Vec<OrganizationVerificationBundle>, OrganizationVerificationRepositoryError> {
        timeout(
            self.operation_timeout,
            list_inner(self.pool.connection(), status, before, limit),
        )
        .await
        .map_err(|_| OrganizationVerificationRepositoryError::Query)?
    }

    /// 以版本 CAS 推进平台审核状态。
    pub async fn decide(
        &self,
        case_id: OrganizationVerificationCaseId,
        expected_version: i64,
        reviewer_user_id: UserId,
        status: OrganizationVerificationStatus,
        reason: Option<String>,
        decided_at: u64,
    ) -> Result<OrganizationVerificationBundle, OrganizationVerificationRepositoryError> {
        timeout(
            self.operation_timeout,
            decide_inner(
                self.pool.connection(),
                case_id,
                expected_version,
                reviewer_user_id,
                status,
                reason,
                decided_at,
            ),
        )
        .await
        .map_err(|_| OrganizationVerificationRepositoryError::OutcomeUnknown)?
    }

    /// 按案件边界读取材料正文，调用方必须先完成平台权限校验。
    pub async fn read_material_content(
        &self,
        case_id: OrganizationVerificationCaseId,
        material_id: OrganizationVerificationMaterialId,
    ) -> Result<Vec<u8>, OrganizationVerificationRepositoryError> {
        timeout(
            self.operation_timeout,
            read_material_content_inner(self.pool.connection(), case_id, material_id),
        )
        .await
        .map_err(|_| OrganizationVerificationRepositoryError::Query)?
    }
}

async fn create_or_resubmit_inner(
    connection: &DatabaseConnection,
    write: OrganizationVerificationWrite,
) -> Result<OrganizationVerificationWriteOutcome, OrganizationVerificationRepositoryError> {
    let transaction = connection.begin().await.map_err(query_error)?;
    let existing = organization_verification_cases::Entity::find()
        .filter(
            organization_verification_cases::Column::OrganizationId.eq(write.organization_id.get()),
        )
        .lock(LockType::Update)
        .one(&transaction)
        .await
        .map_err(query_error)?;
    if let Some(model) = existing {
        let current = to_bundle(&transaction, model.clone()).await?;
        match current.case().status() {
            OrganizationVerificationStatus::Submitted
            | OrganizationVerificationStatus::InReview => {
                transaction
                    .commit()
                    .await
                    .map_err(|_| OrganizationVerificationRepositoryError::OutcomeUnknown)?;
                return Ok(OrganizationVerificationWriteOutcome::Existing(current));
            }
            OrganizationVerificationStatus::NeedsSupplement
            | OrganizationVerificationStatus::Rejected => {
                save_history(&transaction, &model).await?;
                let next_version = model
                    .version
                    .checked_add(1)
                    .ok_or(OrganizationVerificationRepositoryError::InvalidInput)?;
                let active = organization_verification_cases::ActiveModel {
                    id: Set(model.id),
                    status: Set(OrganizationVerificationStatus::Submitted.code()),
                    version: Set(next_version),
                    summary: Set(SensitiveString::from(write.summary.clone())),
                    reviewer_user_id: Set(None),
                    review_reason: Set(None),
                    decided_at: Set(None),
                    updated_at: Set(to_database_time(write.created_at)?),
                    ..Default::default()
                };
                active.update(&transaction).await.map_err(query_error)?;
                let bundle =
                    append_materials(&transaction, model.id, write.materials, write.created_at)
                        .await?;
                transaction
                    .commit()
                    .await
                    .map_err(|_| OrganizationVerificationRepositoryError::OutcomeUnknown)?;
                return Ok(OrganizationVerificationWriteOutcome::Resubmitted(bundle));
            }
            OrganizationVerificationStatus::Approved => {
                return Err(OrganizationVerificationRepositoryError::AlreadyApproved);
            }
        }
    }

    ensure_active_user(&transaction, write.applicant_user_id).await?;
    let model = organization_verification_cases::ActiveModel {
        organization_id: Set(write.organization_id.get()),
        applicant_user_id: Set(write.applicant_user_id.get()),
        status: Set(OrganizationVerificationStatus::Submitted.code()),
        version: Set(1),
        summary: Set(SensitiveString::from(write.summary)),
        created_at: Set(to_database_time(write.created_at)?),
        updated_at: Set(to_database_time(write.created_at)?),
        ..Default::default()
    }
    .insert(&transaction)
    .await
    .map_err(query_error)?;
    let bundle =
        append_materials(&transaction, model.id, write.materials, write.created_at).await?;
    transaction
        .commit()
        .await
        .map_err(|_| OrganizationVerificationRepositoryError::OutcomeUnknown)?;
    Ok(OrganizationVerificationWriteOutcome::Created(bundle))
}

async fn decide_inner(
    connection: &DatabaseConnection,
    case_id: OrganizationVerificationCaseId,
    expected_version: i64,
    reviewer_user_id: UserId,
    status: OrganizationVerificationStatus,
    reason: Option<String>,
    decided_at: u64,
) -> Result<OrganizationVerificationBundle, OrganizationVerificationRepositoryError> {
    if expected_version <= 0
        || !matches!(
            status,
            OrganizationVerificationStatus::NeedsSupplement
                | OrganizationVerificationStatus::Approved
                | OrganizationVerificationStatus::Rejected
        )
    {
        return Err(OrganizationVerificationRepositoryError::InvalidInput);
    }
    let transaction = connection.begin().await.map_err(query_error)?;
    let model = organization_verification_cases::Entity::find_by_id(case_id.get())
        .lock(LockType::Update)
        .one(&transaction)
        .await
        .map_err(query_error)?
        .ok_or(OrganizationVerificationRepositoryError::NotFound)?;
    if model.version != expected_version || matches!(model.status, 4) {
        return Err(OrganizationVerificationRepositoryError::Conflict);
    }
    let next_version = expected_version
        .checked_add(1)
        .ok_or(OrganizationVerificationRepositoryError::InvalidInput)?;
    save_history(&transaction, &model).await?;
    let active = organization_verification_cases::ActiveModel {
        id: Set(model.id),
        status: Set(status.code()),
        version: Set(next_version),
        reviewer_user_id: Set(Some(reviewer_user_id.get())),
        review_reason: Set(reason.map(SensitiveString::from)),
        decided_at: Set(Some(to_database_time(decided_at)?)),
        updated_at: Set(to_database_time(decided_at)?),
        ..Default::default()
    };
    active.update(&transaction).await.map_err(query_error)?;
    let bundle = to_bundle(
        &transaction,
        organization_verification_cases::Entity::find_by_id(case_id.get())
            .one(&transaction)
            .await
            .map_err(query_error)?
            .ok_or(OrganizationVerificationRepositoryError::Invariant)?,
    )
    .await?;
    transaction
        .commit()
        .await
        .map_err(|_| OrganizationVerificationRepositoryError::OutcomeUnknown)?;
    Ok(bundle)
}

async fn save_history<C: ConnectionTrait>(
    connection: &C,
    model: &organization_verification_cases::Model,
) -> Result<(), OrganizationVerificationRepositoryError> {
    let material_ids = organization_verification_materials::Entity::find()
        .select_only()
        .column(organization_verification_materials::Column::Id)
        .filter(organization_verification_materials::Column::CaseId.eq(model.id))
        .into_tuple::<i64>()
        .all(connection)
        .await
        .map_err(query_error)?;
    let snapshot = serde_json::json!({
        "case_id":model.id,"version":model.version,"status":model.status,
        "summary":model.summary.as_str(),"review_reason":model.review_reason.as_ref().map(|r|r.as_str()),
        "reviewer_user_id":model.reviewer_user_id,"created_at":model.created_at.unix_timestamp(),
        "updated_at":model.updated_at.unix_timestamp(),"material_ids":material_ids,
    });
    crate::entity::organization_verification_history::ActiveModel {
        case_id: Set(model.id),
        version: Set(model.version),
        snapshot: Set(SensitiveString::from(snapshot.to_string())),
        ..Default::default()
    }
    .insert(connection)
    .await
    .map_err(query_error)?;
    Ok(())
}

async fn load_by_organization(
    connection: &DatabaseConnection,
    organization_id: OrganizationId,
) -> Result<Option<OrganizationVerificationBundle>, OrganizationVerificationRepositoryError> {
    let model = organization_verification_cases::Entity::find()
        .filter(organization_verification_cases::Column::OrganizationId.eq(organization_id.get()))
        .one(connection)
        .await
        .map_err(query_error)?;
    match model {
        Some(model) => to_bundle(connection, model).await.map(Some),
        None => Ok(None),
    }
}

async fn load_by_id(
    connection: &DatabaseConnection,
    case_id: OrganizationVerificationCaseId,
) -> Result<Option<OrganizationVerificationBundle>, OrganizationVerificationRepositoryError> {
    let model = organization_verification_cases::Entity::find_by_id(case_id.get())
        .one(connection)
        .await
        .map_err(query_error)?;
    match model {
        Some(model) => to_bundle(connection, model).await.map(Some),
        None => Ok(None),
    }
}

async fn list_inner(
    connection: &DatabaseConnection,
    status: Option<OrganizationVerificationStatus>,
    before: Option<i64>,
    limit: usize,
) -> Result<Vec<OrganizationVerificationBundle>, OrganizationVerificationRepositoryError> {
    if !(1..=100).contains(&limit) || before.is_some_and(|value| value <= 0) {
        return Err(OrganizationVerificationRepositoryError::InvalidInput);
    }
    let mut query = organization_verification_cases::Entity::find();
    if let Some(status) = status {
        query = query.filter(organization_verification_cases::Column::Status.eq(status.code()));
    }
    if let Some(before) = before {
        query = query.filter(organization_verification_cases::Column::Id.lt(before));
    }
    let models = query
        .order_by_desc(organization_verification_cases::Column::Id)
        .limit(limit as u64)
        .all(connection)
        .await
        .map_err(query_error)?;
    let mut bundles = Vec::with_capacity(models.len());
    for model in models {
        bundles.push(to_bundle(connection, model).await?);
    }
    Ok(bundles)
}

async fn append_materials<C: ConnectionTrait>(
    connection: &C,
    case_id: i64,
    writes: Vec<OrganizationVerificationMaterialWrite>,
    created_at: u64,
) -> Result<OrganizationVerificationBundle, OrganizationVerificationRepositoryError> {
    for write in writes {
        let material = organization_verification_materials::ActiveModel {
            case_id: Set(case_id),
            kind: Set(write.kind),
            object_reference: Set(SensitiveString::from(write.object_reference)),
            file_name: Set(SensitiveString::from(write.file_name)),
            content_type: Set(write.content_type),
            size_bytes: Set(write.size_bytes),
            content_bytes: Set(Some(write.content_bytes)),
            created_at: Set(to_database_time(created_at)?),
            ..Default::default()
        };
        material.insert(connection).await.map_err(query_error)?;
    }
    let case_model = organization_verification_cases::Entity::find_by_id(case_id)
        .one(connection)
        .await
        .map_err(query_error)?
        .ok_or(OrganizationVerificationRepositoryError::Invariant)?;
    to_bundle(connection, case_model).await
}

async fn to_bundle<C: ConnectionTrait>(
    connection: &C,
    model: organization_verification_cases::Model,
) -> Result<OrganizationVerificationBundle, OrganizationVerificationRepositoryError> {
    let case = OrganizationVerificationCase::from_persistence(
        OrganizationVerificationCaseId::new(model.id)
            .map_err(|_| OrganizationVerificationRepositoryError::Invariant)?,
        OrganizationId::new(model.organization_id)
            .map_err(|_| OrganizationVerificationRepositoryError::Invariant)?,
        UserId::new(model.applicant_user_id)
            .map_err(|_| OrganizationVerificationRepositoryError::Invariant)?,
        OrganizationVerificationStatus::try_from(model.status)
            .map_err(|_| OrganizationVerificationRepositoryError::Invariant)?,
        model.version,
        model.summary.as_str().to_owned(),
        model
            .reviewer_user_id
            .map(UserId::new)
            .transpose()
            .map_err(|_| OrganizationVerificationRepositoryError::Invariant)?,
        model.review_reason.map(|value| value.as_str().to_owned()),
        unix_seconds(model.created_at)?,
        unix_seconds(model.updated_at)?,
        model.decided_at.map(unix_seconds).transpose()?,
    )
    .map_err(|_| OrganizationVerificationRepositoryError::Invariant)?;
    let materials = organization_verification_materials::Entity::find().select_only().columns([organization_verification_materials::Column::Id, organization_verification_materials::Column::CaseId, organization_verification_materials::Column::Kind, organization_verification_materials::Column::ObjectReference, organization_verification_materials::Column::FileName, organization_verification_materials::Column::ContentType, organization_verification_materials::Column::SizeBytes, organization_verification_materials::Column::CreatedAt])
        .column_as(sea_orm::sea_query::Expr::col(organization_verification_materials::Column::ContentBytes).is_not_null(), "content_available")
        .filter(organization_verification_materials::Column::CaseId.eq(model.id))
        .order_by_asc(organization_verification_materials::Column::Id)
        .into_model::<MaterialMetadataRow>().all(connection)
        .await
        .map_err(query_error)?
        .into_iter()
        .map(to_material)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(OrganizationVerificationBundle { case, materials })
}

fn to_material(
    model: MaterialMetadataRow,
) -> Result<OrganizationVerificationMaterial, OrganizationVerificationRepositoryError> {
    OrganizationVerificationMaterial::new(
        OrganizationVerificationMaterialId::new(model.id)
            .map_err(|_| OrganizationVerificationRepositoryError::Invariant)?,
        OrganizationVerificationCaseId::new(model.case_id)
            .map_err(|_| OrganizationVerificationRepositoryError::Invariant)?,
        model.kind,
        model.object_reference.as_str().to_owned(),
        model.file_name.as_str().to_owned(),
        model.content_type,
        model.size_bytes,
        model.content_available,
        unix_seconds(model.created_at)?,
    )
    .map_err(|_| OrganizationVerificationRepositoryError::Invariant)
}

async fn read_material_content_inner(
    connection: &DatabaseConnection,
    case_id: OrganizationVerificationCaseId,
    material_id: OrganizationVerificationMaterialId,
) -> Result<Vec<u8>, OrganizationVerificationRepositoryError> {
    let material = organization_verification_materials::Entity::find_by_id(material_id.get())
        .filter(organization_verification_materials::Column::CaseId.eq(case_id.get()))
        .one(connection)
        .await
        .map_err(query_error)?
        .ok_or(OrganizationVerificationRepositoryError::MaterialNotFound)?;
    material
        .content_bytes
        .ok_or(OrganizationVerificationRepositoryError::MaterialContentUnavailable)
}

async fn ensure_active_user<C: ConnectionTrait>(
    connection: &C,
    user_id: UserId,
) -> Result<(), OrganizationVerificationRepositoryError> {
    let user = users::Entity::find_by_id(user_id.get())
        .one(connection)
        .await
        .map_err(query_error)?;
    if user.is_none_or(|value| value.deleted_at.is_some() || value.status != 1) {
        return Err(OrganizationVerificationRepositoryError::ApplicantUnavailable);
    }
    Ok(())
}

fn to_database_time(
    value: u64,
) -> Result<TimeDateTimeWithTimeZone, OrganizationVerificationRepositoryError> {
    TimeDateTimeWithTimeZone::from_unix_timestamp(
        i64::try_from(value).map_err(|_| OrganizationVerificationRepositoryError::InvalidInput)?,
    )
    .map_err(|_| OrganizationVerificationRepositoryError::InvalidInput)
}

fn unix_seconds(
    value: TimeDateTimeWithTimeZone,
) -> Result<u64, OrganizationVerificationRepositoryError> {
    u64::try_from(value.unix_timestamp())
        .map_err(|_| OrganizationVerificationRepositoryError::Invariant)
}

fn query_error(_: sea_orm::DbErr) -> OrganizationVerificationRepositoryError {
    OrganizationVerificationRepositoryError::Query
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationVerificationRepositoryConfigError {
    #[error("企业认证仓储操作超时必须大于零")]
    ZeroOperationTimeout,
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationVerificationRepositoryError {
    #[error("企业认证请求无效")]
    InvalidInput,
    #[error("企业认证案件不存在")]
    NotFound,
    #[error("企业认证案件冲突")]
    Conflict,
    #[error("企业认证已通过")]
    AlreadyApproved,
    #[error("企业认证申请人不可用")]
    ApplicantUnavailable,
    #[error("企业认证查询失败")]
    Query,
    #[error("企业认证状态不变量损坏")]
    Invariant,
    #[error("企业认证写入结果未知")]
    OutcomeUnknown,
    #[error("企业认证材料不存在")]
    MaterialNotFound,
    #[error("企业认证材料正文不可用")]
    MaterialContentUnavailable,
}

#[derive(sea_orm::FromQueryResult)]
struct MaterialMetadataRow {
    id: i64,
    case_id: i64,
    kind: String,
    object_reference: String,
    file_name: String,
    content_type: String,
    size_bytes: i64,
    content_available: bool,
    created_at: TimeDateTimeWithTimeZone,
}
