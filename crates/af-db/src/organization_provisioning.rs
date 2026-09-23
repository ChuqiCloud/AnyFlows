use std::{fmt, future::Future, time::Duration};

use af_domain::{
    MAX_ORGANIZATION_PROVISIONING_DECISION_REASON_BYTES, OrganizationId, OrganizationKey,
    OrganizationMembershipStatus, OrganizationProvisioningCreateOutcome,
    OrganizationProvisioningDecisionKind, OrganizationProvisioningDecisionOutcome,
    OrganizationProvisioningIdempotencyKey, OrganizationProvisioningMaterial,
    OrganizationProvisioningRecord, OrganizationProvisioningRequest,
    OrganizationProvisioningRequestId, OrganizationProvisioningRequestKey,
    OrganizationProvisioningStatus, OrganizationRole, OrganizationSsoEnforcementPolicy,
    OrganizationStatus, PlatformPermission, UserId,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, DatabaseTransaction,
    DbBackend, EntityTrait, IntoActiveModel, QueryFilter, QueryOrder, QuerySelect, Set,
    TransactionTrait,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Expr, LockType},
};
use sha2::{Digest as _, Sha256};
use thiserror::Error;
use tokio::time::timeout;

use crate::{
    DatabasePool, OrganizationCreate, OrganizationEntitlementWrite, OrganizationRepositoryError,
    entity::{
        SensitiveString, organization_entitlements, organization_memberships,
        organization_provisioning_materials, organization_provisioning_requests,
        organization_sso_policies, organization_wallets, organizations, platform_audit_logs, users,
    },
    organization::mutation,
};

const ACTIVE_USER_STATUS: i16 = 1;
const ACTIVE_SLOT: i16 = 1;
const ADMIN_USER_ROLE: i16 = 1;
const PLATFORM_AUDIT_ROUTE: &str =
    "/api/management/organization-provisioning/{request_key}/decision";
const PLATFORM_AUDIT_RESOURCE: &str = "organization_provisioning_request";

/// 待写入的企业开通材料；正文仅在本次受控写入事务中暂存。
pub struct OrganizationProvisioningMaterialWrite {
    pub kind: String,
    pub object_reference: String,
    pub file_name: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub content_bytes: Vec<u8>,
}

/// 平台管理员以申请版本 CAS 提交的企业开通决定。
#[derive(Clone)]
pub struct OrganizationProvisioningDecisionWrite {
    request_key: OrganizationProvisioningRequestKey,
    expected_version: i64,
    reviewer_user_id: UserId,
    decision: OrganizationProvisioningDecisionKind,
    decision_reason: Option<String>,
    entitlement: Option<OrganizationEntitlementWrite>,
}

async fn decide_request_inner(
    connection: &DatabaseConnection,
    write: &OrganizationProvisioningDecisionWrite,
) -> Result<OrganizationProvisioningDecisionOutcome, OrganizationProvisioningRepositoryError> {
    let transaction = connection.begin().await.map_err(query_error)?;
    let result = decide_request_transaction(&transaction, write).await;
    match result {
        Ok(outcome) => {
            transaction
                .commit()
                .await
                .map_err(|_| OrganizationProvisioningRepositoryError::OutcomeUnknown)?;
            Ok(outcome)
        }
        Err(error) => {
            transaction
                .rollback()
                .await
                .map_err(|_| OrganizationProvisioningRepositoryError::OutcomeUnknown)?;
            Err(error)
        }
    }
}

async fn decide_request_transaction(
    transaction: &DatabaseTransaction,
    write: &OrganizationProvisioningDecisionWrite,
) -> Result<OrganizationProvisioningDecisionOutcome, OrganizationProvisioningRepositoryError> {
    let model = lock_request_for_decision(transaction, write.request_key).await?;
    let current = to_record(model.clone())?;
    if current.status() != OrganizationProvisioningStatus::Pending {
        verify_existing_decision(transaction, write, &current).await?;
        return Ok(OrganizationProvisioningDecisionOutcome::Existing(current));
    }
    if model.version != write.expected_version {
        return Err(OrganizationProvisioningRepositoryError::VersionConflict);
    }
    ensure_active_admin(transaction, write.reviewer_user_id).await?;

    let now = TimeDateTimeWithTimeZone::now_utc();
    let decided_at = if now < model.created_at {
        model.created_at
    } else {
        now
    };
    let (status, organization_id, organization_key) = match write.decision {
        OrganizationProvisioningDecisionKind::Approve => {
            let entitlement = write
                .entitlement
                .clone()
                .ok_or(OrganizationProvisioningRepositoryError::InvalidInput)?;
            let created = mutation::create_in_transaction(
                transaction,
                OrganizationCreate::new(
                    current.organization_name().to_owned(),
                    current.organization_slug().to_owned(),
                    current.applicant_user_id(),
                    entitlement,
                ),
            )
            .await
            .map_err(map_organization_error)?;
            let (organization, owner) = created.into_parts();
            if owner.user_id() != current.applicant_user_id()
                || owner.role() != OrganizationRole::Owner
                || owner.status() != OrganizationMembershipStatus::Active
            {
                return Err(OrganizationProvisioningRepositoryError::Invariant);
            }
            (
                OrganizationProvisioningStatus::Approved,
                Some(organization.id()),
                Some(organization.organization_key().persistence_key()),
            )
        }
        OrganizationProvisioningDecisionKind::Reject => {
            (OrganizationProvisioningStatus::Rejected, None, None)
        }
    };

    let next_version = model
        .version
        .checked_add(1)
        .ok_or(OrganizationProvisioningRepositoryError::Invariant)?;
    let mut active = model.into_active_model();
    active.status = Set(status.code());
    active.active_slot = Set(None);
    active.version = Set(next_version);
    active.reviewer_user_id = Set(Some(write.reviewer_user_id.get()));
    active.decision_reason = Set(write.decision_reason.as_deref().map(SensitiveString::from));
    active.organization_id = Set(organization_id.map(OrganizationId::get));
    active.updated_at = Set(decided_at);
    active.decided_at = Set(Some(decided_at));
    let updated = active.update(transaction).await.map_err(query_error)?;
    let record = to_record(updated)?;
    insert_platform_audit(
        transaction,
        write,
        &record,
        organization_key.as_deref(),
        decided_at,
    )
    .await?;
    Ok(OrganizationProvisioningDecisionOutcome::Applied(record))
}

async fn lock_request_for_decision(
    transaction: &DatabaseTransaction,
    request_key: OrganizationProvisioningRequestKey,
) -> Result<organization_provisioning_requests::Model, OrganizationProvisioningRepositoryError> {
    let key = request_key.persistence_key();
    if transaction.get_database_backend() == DbBackend::Sqlite {
        let result = organization_provisioning_requests::Entity::update_many()
            .filter(organization_provisioning_requests::Column::RequestKey.eq(&key))
            .col_expr(
                organization_provisioning_requests::Column::Version,
                Expr::col(organization_provisioning_requests::Column::Version).into(),
            )
            .exec(transaction)
            .await
            .map_err(query_error)?;
        if result.rows_affected != 1 {
            return Err(OrganizationProvisioningRepositoryError::NotFound);
        }
    }
    let mut query = organization_provisioning_requests::Entity::find()
        .filter(organization_provisioning_requests::Column::RequestKey.eq(key));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(query_error)?
        .ok_or(OrganizationProvisioningRepositoryError::NotFound)
}

async fn ensure_active_admin(
    transaction: &DatabaseTransaction,
    reviewer_user_id: UserId,
) -> Result<(), OrganizationProvisioningRepositoryError> {
    let reviewer = mutation::lock_active_user_model(transaction, reviewer_user_id)
        .await
        .map_err(map_organization_error)?;
    if reviewer.role != ADMIN_USER_ROLE {
        return Err(OrganizationProvisioningRepositoryError::Forbidden);
    }
    Ok(())
}

async fn insert_platform_audit(
    transaction: &DatabaseTransaction,
    write: &OrganizationProvisioningDecisionWrite,
    record: &OrganizationProvisioningRecord,
    organization_key: Option<&str>,
    created_at: TimeDateTimeWithTimeZone,
) -> Result<(), OrganizationProvisioningRepositoryError> {
    let audit = audit_fact(write, record, organization_key)?;
    platform_audit_logs::ActiveModel {
        operator_user_id: Set(write.reviewer_user_id.get()),
        permission_code: Set(PlatformPermission::OrganizationsManage.code().to_owned()),
        route: Set(PLATFORM_AUDIT_ROUTE.to_owned()),
        operation: Set(audit.operation),
        resource: Set(PLATFORM_AUDIT_RESOURCE.to_owned()),
        resource_id: Set(Some(write.request_key.persistence_key())),
        outcome: Set(1),
        before_value: Set(Some(audit.before_value)),
        after_value: Set(Some(audit.after_value)),
        audit_info: Set(Some(audit.audit_info)),
        request_id: Set(audit.request_id),
        created_at: Set(created_at),
        ..Default::default()
    }
    .insert(transaction)
    .await
    .map_err(query_error)?;
    Ok(())
}

struct ProvisioningAuditFact {
    operation: String,
    request_id: String,
    before_value: String,
    after_value: String,
    audit_info: String,
}

fn audit_fact(
    write: &OrganizationProvisioningDecisionWrite,
    record: &OrganizationProvisioningRecord,
    organization_key: Option<&str>,
) -> Result<ProvisioningAuditFact, OrganizationProvisioningRepositoryError> {
    let operation = match write.decision {
        OrganizationProvisioningDecisionKind::Approve => "organization.provisioning.approve",
        OrganizationProvisioningDecisionKind::Reject => "organization.provisioning.reject",
    }
    .to_owned();
    let request_id = format!(
        "organization-provisioning:{}",
        write.request_key.persistence_key()
    );
    let before_value = serde_json::json!({
        "status": "pending",
        "version": write.expected_version,
    })
    .to_string();
    let status = match record.status() {
        OrganizationProvisioningStatus::Approved => "approved",
        OrganizationProvisioningStatus::Rejected => "rejected",
        OrganizationProvisioningStatus::Pending => {
            return Err(OrganizationProvisioningRepositoryError::Invariant);
        }
    };
    let after_value = serde_json::json!({
        "organization_key": organization_key,
        "status": status,
        "version": record.version(),
    })
    .to_string();
    let audit_info = serde_json::json!({
        "decision_reason_recorded": write.decision_reason.is_some(),
        "entitlement_fingerprint": write.entitlement.as_ref().map(entitlement_fingerprint),
    })
    .to_string();
    Ok(ProvisioningAuditFact {
        operation,
        request_id,
        before_value,
        after_value,
        audit_info,
    })
}

fn entitlement_fingerprint(write: &OrganizationEntitlementWrite) -> String {
    let snapshot = write.snapshot();
    let payload = serde_json::json!({
        "audit_retention_days": snapshot.audit_retention_days(),
        "custom_roles_enabled": snapshot.custom_roles_enabled(),
        "external_reference": write.external_reference(),
        "grace_until": snapshot.grace_until(),
        "organization_key_limit": snapshot.capacity().organization_key_limit(),
        "seat_limit": snapshot.capacity().seat_limit(),
        "source": write.source(),
        "sso_enabled": snapshot.sso_enabled(),
        "team_limit": snapshot.capacity().team_limit(),
        "team_management_enabled": snapshot.team_management_enabled(),
        "valid_from": snapshot.valid_from(),
        "valid_until": snapshot.valid_until(),
    })
    .to_string();
    let digest = Sha256::digest(payload.as_bytes());
    let mut encoded = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut encoded, "{byte:02x}");
    }
    encoded
}

async fn verify_existing_decision(
    transaction: &DatabaseTransaction,
    write: &OrganizationProvisioningDecisionWrite,
    record: &OrganizationProvisioningRecord,
) -> Result<(), OrganizationProvisioningRepositoryError> {
    let expected_status = match write.decision {
        OrganizationProvisioningDecisionKind::Approve => OrganizationProvisioningStatus::Approved,
        OrganizationProvisioningDecisionKind::Reject => OrganizationProvisioningStatus::Rejected,
    };
    let expected_version = write
        .expected_version
        .checked_add(1)
        .ok_or(OrganizationProvisioningRepositoryError::InvalidInput)?;
    if record.status() != expected_status
        || record.version()
            != u64::try_from(expected_version)
                .map_err(|_| OrganizationProvisioningRepositoryError::Invariant)?
        || record.reviewer_user_id() != Some(write.reviewer_user_id)
        || record.decision_reason() != write.decision_reason.as_deref()
    {
        return Err(OrganizationProvisioningRepositoryError::Conflict);
    }

    let organization_key = if let Some(organization_id) = record.organization_id() {
        let organization = organizations::Entity::find_by_id(organization_id.get())
            .one(transaction)
            .await
            .map_err(query_error)?
            .ok_or(OrganizationProvisioningRepositoryError::Invariant)?;
        OrganizationKey::from_persistence_key(&organization.organization_key)
            .map_err(|_| OrganizationProvisioningRepositoryError::Invariant)?;
        OrganizationStatus::try_from(organization.status)
            .map_err(|_| OrganizationProvisioningRepositoryError::Invariant)?;
        if organization.created_by_user_id != record.applicant_user_id().get() {
            return Err(OrganizationProvisioningRepositoryError::Invariant);
        }
        if organization.name != record.organization_name()
            || organization.slug != record.organization_slug()
        {
            return Err(OrganizationProvisioningRepositoryError::Invariant);
        }
        verify_provisioned_relations(transaction, organization_id, record.applicant_user_id())
            .await?;
        Some(organization.organization_key)
    } else {
        None
    };
    let audit = audit_fact(write, record, organization_key.as_deref())?;
    let stored = platform_audit_logs::Entity::find()
        .filter(platform_audit_logs::Column::RequestId.eq(&audit.request_id))
        .filter(platform_audit_logs::Column::Route.eq(PLATFORM_AUDIT_ROUTE))
        .filter(platform_audit_logs::Column::Operation.eq(&audit.operation))
        .one(transaction)
        .await
        .map_err(query_error)?
        .ok_or(OrganizationProvisioningRepositoryError::Invariant)?;
    if stored.operator_user_id != write.reviewer_user_id.get()
        || stored.permission_code != PlatformPermission::OrganizationsManage.code()
        || stored.resource != PLATFORM_AUDIT_RESOURCE
        || stored.resource_id.as_deref() != Some(write.request_key.persistence_key().as_str())
        || stored.outcome != 1
        || stored.before_value.as_deref() != Some(audit.before_value.as_str())
        || stored.after_value.as_deref() != Some(audit.after_value.as_str())
        || stored.audit_info.as_deref() != Some(audit.audit_info.as_str())
    {
        return Err(OrganizationProvisioningRepositoryError::Invariant);
    }
    Ok(())
}

async fn verify_provisioned_relations(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    applicant_user_id: UserId,
) -> Result<(), OrganizationProvisioningRepositoryError> {
    let entitlement = organization_entitlements::Entity::find()
        .filter(organization_entitlements::Column::OrganizationId.eq(organization_id.get()))
        .one(transaction)
        .await
        .map_err(query_error)?;
    let wallet = organization_wallets::Entity::find()
        .filter(organization_wallets::Column::OrganizationId.eq(organization_id.get()))
        .one(transaction)
        .await
        .map_err(query_error)?;
    let sso_policy = organization_sso_policies::Entity::find()
        .filter(organization_sso_policies::Column::OrganizationId.eq(organization_id.get()))
        .one(transaction)
        .await
        .map_err(query_error)?;
    let membership = organization_memberships::Entity::find()
        .filter(organization_memberships::Column::OrganizationId.eq(organization_id.get()))
        .filter(organization_memberships::Column::UserId.eq(applicant_user_id.get()))
        .one(transaction)
        .await
        .map_err(query_error)?;
    if entitlement.is_none() || wallet.is_none() || sso_policy.is_none() || membership.is_none() {
        return Err(OrganizationProvisioningRepositoryError::Invariant);
    }
    let sso_policy = sso_policy.expect("存在性已检查");
    OrganizationSsoEnforcementPolicy::try_from(sso_policy.enforcement_policy)
        .map_err(|_| OrganizationProvisioningRepositoryError::Invariant)?;
    let membership = membership.expect("存在性已检查");
    OrganizationRole::try_from(membership.role)
        .map_err(|_| OrganizationProvisioningRepositoryError::Invariant)?;
    OrganizationMembershipStatus::try_from(membership.status)
        .map_err(|_| OrganizationProvisioningRepositoryError::Invariant)?;
    Ok(())
}

fn valid_decision_reason(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ORGANIZATION_PROVISIONING_DECISION_REASON_BYTES
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn map_organization_error(
    error: OrganizationRepositoryError,
) -> OrganizationProvisioningRepositoryError {
    match error {
        OrganizationRepositoryError::InvalidInput => {
            OrganizationProvisioningRepositoryError::InvalidInput
        }
        OrganizationRepositoryError::UserUnavailable => {
            OrganizationProvisioningRepositoryError::ApplicantUnavailable
        }
        OrganizationRepositoryError::Conflict => {
            OrganizationProvisioningRepositoryError::OrganizationSlugConflict
        }
        OrganizationRepositoryError::EntropyUnavailable => {
            OrganizationProvisioningRepositoryError::EntropyUnavailable
        }
        OrganizationRepositoryError::Query | OrganizationRepositoryError::Timeout => {
            OrganizationProvisioningRepositoryError::Query
        }
        OrganizationRepositoryError::NotFound
        | OrganizationRepositoryError::InvalidTransition
        | OrganizationRepositoryError::LastOwner
        | OrganizationRepositoryError::TeamHasActiveMembers
        | OrganizationRepositoryError::EntitlementRestricted
        | OrganizationRepositoryError::SeatLimitReached
        | OrganizationRepositoryError::InvitationExpired
        | OrganizationRepositoryError::InvitationNotPending
        | OrganizationRepositoryError::InvitationEmailMismatch
        | OrganizationRepositoryError::InvitationBatchConflict
        | OrganizationRepositoryError::TeamLimitReached
        | OrganizationRepositoryError::Invariant => {
            OrganizationProvisioningRepositoryError::Invariant
        }
    }
}

impl OrganizationProvisioningDecisionWrite {
    /// 校验决定形状；批准必须提供初始授权，拒绝必须提供理由。
    pub fn new(
        request_key: OrganizationProvisioningRequestKey,
        expected_version: u64,
        reviewer_user_id: UserId,
        decision: OrganizationProvisioningDecisionKind,
        decision_reason: Option<String>,
        entitlement: Option<OrganizationEntitlementWrite>,
    ) -> Result<Self, OrganizationProvisioningRepositoryError> {
        let expected_version = i64::try_from(expected_version)
            .map_err(|_| OrganizationProvisioningRepositoryError::InvalidInput)?;
        if expected_version <= 0 || expected_version == i64::MAX {
            return Err(OrganizationProvisioningRepositoryError::InvalidInput);
        }
        if decision_reason
            .as_deref()
            .is_some_and(|value| !valid_decision_reason(value))
            || matches!(decision, OrganizationProvisioningDecisionKind::Reject)
                && decision_reason.is_none()
            || matches!(decision, OrganizationProvisioningDecisionKind::Approve)
                && entitlement
                    .as_ref()
                    .is_none_or(|write| write.expected_version().is_some())
            || matches!(decision, OrganizationProvisioningDecisionKind::Reject)
                && entitlement.is_some()
        {
            return Err(OrganizationProvisioningRepositoryError::InvalidInput);
        }
        Ok(Self {
            request_key,
            expected_version,
            reviewer_user_id,
            decision,
            decision_reason,
            entitlement,
        })
    }
}

impl fmt::Debug for OrganizationProvisioningDecisionWrite {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationProvisioningDecisionWrite")
            .field("expected_version", &self.expected_version)
            .field("decision", &self.decision)
            .finish_non_exhaustive()
    }
}

/// 平台范围的企业开通申请事实仓储。
///
/// 创建、读取并原子审批企业开通申请，所有公开读取仍保持当前用户隔离。
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
        self.create_request_with_materials(request, Vec::new())
            .await
    }

    /// 幂等创建申请并在同一事务中保存真实材料正文。
    pub async fn create_request_with_materials(
        &self,
        request: OrganizationProvisioningRequest,
        materials: Vec<OrganizationProvisioningMaterialWrite>,
    ) -> Result<OrganizationProvisioningCreateOutcome, OrganizationProvisioningRepositoryError>
    {
        match timeout(
            self.operation_timeout,
            create_request_inner(self.pool.connection(), &request, materials),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(OrganizationProvisioningRepositoryError::OutcomeUnknown),
        }
    }

    /// 按申请键读取管理员可见的材料元数据，不返回正文。
    pub async fn list_materials(
        &self,
        request_key: OrganizationProvisioningRequestKey,
    ) -> Result<Vec<OrganizationProvisioningMaterial>, OrganizationProvisioningRepositoryError>
    {
        self.run_read(list_materials_inner(self.pool.connection(), request_key))
            .await
    }

    /// 按申请和材料边界读取正文；调用方必须先完成平台权限校验。
    pub async fn read_material_content(
        &self,
        request_key: OrganizationProvisioningRequestKey,
        material_id: i64,
    ) -> Result<Vec<u8>, OrganizationProvisioningRepositoryError> {
        self.run_read(read_material_content_inner(
            self.pool.connection(),
            request_key,
            material_id,
        ))
        .await
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

    /// 按平台范围读取开通申请，使用自增主键倒序游标避免泄漏跨用户筛选条件。
    pub async fn list_requests_self(
        &self,
        user_id: UserId,
        status: Option<OrganizationProvisioningStatus>,
        before: Option<i64>,
        limit: u64,
    ) -> Result<Vec<OrganizationProvisioningRecord>, OrganizationProvisioningRepositoryError> {
        self.run_read(async move {
            if !(1..=100).contains(&limit) || before.is_some_and(|value| value <= 0) {
                return Err(OrganizationProvisioningRepositoryError::InvalidInput);
            }
            let mut query = organization_provisioning_requests::Entity::find()
                .filter(
                    organization_provisioning_requests::Column::ApplicantUserId.eq(user_id.get()),
                )
                .order_by_desc(organization_provisioning_requests::Column::Id)
                .limit(limit);
            if let Some(before) = before {
                query = query.filter(organization_provisioning_requests::Column::Id.lt(before));
            }
            if let Some(status) = status {
                query = query
                    .filter(organization_provisioning_requests::Column::Status.eq(status.code()));
            }
            query
                .all(self.pool.connection())
                .await
                .map_err(query_error)?
                .into_iter()
                .map(to_record)
                .collect()
        })
        .await
    }

    pub async fn list_requests(
        &self,
        status: Option<OrganizationProvisioningStatus>,
        before: Option<i64>,
        limit: u64,
    ) -> Result<Vec<OrganizationProvisioningRecord>, OrganizationProvisioningRepositoryError> {
        self.run_read(async move {
            if !(1..=100).contains(&limit) || before.is_some_and(|value| value <= 0) {
                return Err(OrganizationProvisioningRepositoryError::InvalidInput);
            }
            let mut query = organization_provisioning_requests::Entity::find()
                .order_by_desc(organization_provisioning_requests::Column::Id)
                .limit(limit);
            if let Some(before) = before {
                query = query.filter(organization_provisioning_requests::Column::Id.lt(before));
            }
            if let Some(status) = status {
                query = query
                    .filter(organization_provisioning_requests::Column::Status.eq(status.code()));
            }
            query
                .all(self.pool.connection())
                .await
                .map_err(query_error)?
                .into_iter()
                .map(to_record)
                .collect()
        })
        .await
    }

    /// 按平台范围读取单条申请；普通用户必须使用带申请人边界的读取方法。
    pub async fn get_request_for_admin(
        &self,
        request_key: OrganizationProvisioningRequestKey,
    ) -> Result<Option<OrganizationProvisioningRecord>, OrganizationProvisioningRepositoryError>
    {
        self.run_read(async move {
            find_by_request_key(self.pool.connection(), request_key)
                .await?
                .map(to_record)
                .transpose()
        })
        .await
    }

    /// 以版本 CAS 批准或拒绝申请；写入超时后只能重放同一命令。
    pub async fn decide_request(
        &self,
        write: OrganizationProvisioningDecisionWrite,
    ) -> Result<OrganizationProvisioningDecisionOutcome, OrganizationProvisioningRepositoryError>
    {
        match timeout(
            self.operation_timeout,
            decide_request_inner(self.pool.connection(), &write),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(OrganizationProvisioningRepositoryError::OutcomeUnknown),
        }
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
    materials: Vec<OrganizationProvisioningMaterialWrite>,
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
    if !crate::account_verification::has_enterprise_verification(
        connection,
        request.applicant_user_id(),
    )
    .await
    .map_err(|_| OrganizationProvisioningRepositoryError::Query)?
    {
        return Err(OrganizationProvisioningRepositoryError::Forbidden);
    }
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

    let transaction = connection.begin().await.map_err(query_error)?;
    match model.insert(&transaction).await {
        Ok(model) => {
            let request_id = model.id;
            if let Err(error) =
                append_materials(&transaction, request_id, request.created_at(), materials).await
            {
                transaction
                    .rollback()
                    .await
                    .map_err(|_| OrganizationProvisioningRepositoryError::OutcomeUnknown)?;
                return Err(error);
            }
            transaction
                .commit()
                .await
                .map_err(|_| OrganizationProvisioningRepositoryError::OutcomeUnknown)?;
            Ok(OrganizationProvisioningCreateOutcome::Created(to_record(
                model,
            )?))
        }
        Err(_) => {
            transaction
                .rollback()
                .await
                .map_err(|_| OrganizationProvisioningRepositoryError::OutcomeUnknown)?;
            recover_create_conflict(connection, request).await
        }
    }
}

async fn append_materials(
    connection: &DatabaseTransaction,
    request_id: i64,
    created_at: u64,
    materials: Vec<OrganizationProvisioningMaterialWrite>,
) -> Result<(), OrganizationProvisioningRepositoryError> {
    let created_at = to_database_time(created_at)?;
    for material in materials {
        organization_provisioning_materials::ActiveModel {
            request_id: Set(request_id),
            kind: Set(material.kind),
            object_reference: Set(SensitiveString::from(material.object_reference)),
            file_name: Set(SensitiveString::from(material.file_name)),
            content_type: Set(material.content_type),
            size_bytes: Set(material.size_bytes),
            content_bytes: Set(Some(material.content_bytes)),
            created_at: Set(created_at),
            ..Default::default()
        }
        .insert(connection)
        .await
        .map_err(query_error)?;
    }
    Ok(())
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

async fn list_materials_inner(
    connection: &DatabaseConnection,
    request_key: OrganizationProvisioningRequestKey,
) -> Result<Vec<OrganizationProvisioningMaterial>, OrganizationProvisioningRepositoryError> {
    let request = find_by_request_key(connection, request_key)
        .await?
        .ok_or(OrganizationProvisioningRepositoryError::NotFound)?;
    organization_provisioning_materials::Entity::find().select_only().columns([organization_provisioning_materials::Column::Id, organization_provisioning_materials::Column::RequestId, organization_provisioning_materials::Column::Kind, organization_provisioning_materials::Column::ObjectReference, organization_provisioning_materials::Column::FileName, organization_provisioning_materials::Column::ContentType, organization_provisioning_materials::Column::SizeBytes, organization_provisioning_materials::Column::CreatedAt])
        .column_as(sea_orm::sea_query::Expr::col(organization_provisioning_materials::Column::ContentBytes).is_not_null(), "content_available")
        .filter(organization_provisioning_materials::Column::RequestId.eq(request.id))
        .order_by_asc(organization_provisioning_materials::Column::Id)
        .into_model::<MaterialMetadataRow>().all(connection)
        .await
        .map_err(query_error)?
        .into_iter()
        .map(|model| to_material(model, request_key))
        .collect()
}

async fn read_material_content_inner(
    connection: &DatabaseConnection,
    request_key: OrganizationProvisioningRequestKey,
    material_id: i64,
) -> Result<Vec<u8>, OrganizationProvisioningRepositoryError> {
    let request = find_by_request_key(connection, request_key)
        .await?
        .ok_or(OrganizationProvisioningRepositoryError::NotFound)?;
    let material = organization_provisioning_materials::Entity::find_by_id(material_id)
        .filter(organization_provisioning_materials::Column::RequestId.eq(request.id))
        .one(connection)
        .await
        .map_err(query_error)?
        .ok_or(OrganizationProvisioningRepositoryError::MaterialNotFound)?;
    material
        .content_bytes
        .ok_or(OrganizationProvisioningRepositoryError::MaterialContentUnavailable)
}

fn to_material(
    model: MaterialMetadataRow,
    request_key: OrganizationProvisioningRequestKey,
) -> Result<OrganizationProvisioningMaterial, OrganizationProvisioningRepositoryError> {
    OrganizationProvisioningMaterial::from_persistence(
        model.id,
        request_key,
        model.kind,
        model.object_reference.as_str().to_owned(),
        model.file_name.as_str().to_owned(),
        model.content_type,
        model.size_bytes,
        model.content_available,
        unix_seconds(model.created_at)?,
    )
    .map_err(|_| OrganizationProvisioningRepositoryError::Invariant)
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
    #[error("企业开通申请输入无效")]
    InvalidInput,
    #[error("企业开通申请不存在")]
    NotFound,
    #[error("企业开通审批权限不足")]
    Forbidden,
    #[error("企业开通申请版本冲突")]
    VersionConflict,
    #[error("企业开通申请事实冲突")]
    Conflict,
    #[error("当前用户已有活动企业开通申请")]
    ActiveApplicantConflict,
    #[error("企业规范标识已有活动开通申请")]
    ActiveSlugConflict,
    #[error("企业规范标识已被占用")]
    OrganizationSlugConflict,
    #[error("企业开通申请人不可用")]
    ApplicantUnavailable,
    #[error("企业公开标识生成失败")]
    EntropyUnavailable,
    #[error("企业开通申请数据库操作失败")]
    Query,
    #[error("企业开通申请写入结果未知")]
    OutcomeUnknown,
    #[error("企业开通申请数据库操作超时")]
    Timeout,
    #[error("企业开通申请持久化状态损坏")]
    Invariant,
    #[error("企业开通材料不存在")]
    MaterialNotFound,
    #[error("企业开通材料正文不可用")]
    MaterialContentUnavailable,
}

#[derive(sea_orm::FromQueryResult)]
struct MaterialMetadataRow {
    id: i64,
    kind: String,
    object_reference: String,
    file_name: String,
    content_type: String,
    size_bytes: i64,
    content_available: bool,
    created_at: TimeDateTimeWithTimeZone,
}
