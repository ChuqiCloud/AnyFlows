//! 企业审批决定到管理审计的固定投影；不承载资源正文或用户填写理由。

use af_domain::{
    OrganizationApprovalDecision, OrganizationApprovalDecisionKind, OrganizationId,
    OrganizationMembershipStatus, OrganizationRole, UserId,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabasePool, DatabaseTransaction, EntityTrait, QueryFilter,
    QuerySelect, Set, entity::prelude::TimeDateTimeWithTimeZone,
};
use sha2::{Digest, Sha256};

use crate::{
    entity::{
        organization_approval_decisions, organization_approval_requests, organization_audit_logs,
        organization_memberships,
    },
    organization_approval::{
        OrganizationApprovalDecisionRecord, OrganizationApprovalRepositoryError,
    },
    organization_audit::{OrganizationAuditOutcome, OrganizationAuditWrite},
};

const AUDIT_OPERATION: &str = "organization.approval.decision.project";
const AUDIT_TARGET_TYPE: &str = "approval_decision";
const AUDIT_PERMISSION: &str = "organization.approvals.decide";
const AUDIT_PLATFORM_REASON: &str = "approval_timeout_scheduler";
const AUDIT_REQUEST_PREFIX: &str = "approval-decision:";
const FINGERPRINT_DOMAIN: &[u8] = b"anyflows.organization-approval.audit.v1";

/// 在审批决定事务内追加对应的企业审计事实。
pub(crate) async fn insert_decision_audit(
    transaction: &DatabaseTransaction,
    request: &organization_approval_requests::Model,
    decision: &OrganizationApprovalDecision,
    decision_model: &organization_approval_decisions::Model,
) -> Result<(), OrganizationApprovalRepositoryError> {
    let projection = Projection::new(transaction, request, decision, decision_model).await?;
    let existing = organization_audit_logs::Entity::find()
        .filter(
            organization_audit_logs::Column::OrganizationId.eq(projection.organization_id.get()),
        )
        .filter(organization_audit_logs::Column::RequestId.eq(&projection.request_id))
        .filter(organization_audit_logs::Column::Operation.eq(AUDIT_OPERATION))
        .one(transaction)
        .await
        .map_err(|_| OrganizationApprovalRepositoryError::Query)?;
    if let Some(existing) = existing {
        return projection
            .matches(&existing)
            .then_some(())
            .ok_or(OrganizationApprovalRepositoryError::Conflict);
    }

    let write = projection.to_write()?;
    organization_audit_logs::ActiveModel {
        organization_id: Set(write.organization_id().get()),
        actor_user_id: Set(write.actor_user_id().get()),
        actor_role: Set(write.actor_role().map(OrganizationRole::code)),
        permission_code: Set(write.permission_code().to_owned()),
        operation: Set(write.operation().to_owned()),
        target_type: Set(write.target_type().to_owned()),
        target_identifier: Set(write.target_identifier().to_owned()),
        outcome: Set(write.outcome().code()),
        platform_proxy: Set(write.platform_proxy()),
        platform_reason: Set(write.platform_reason().map(str::to_owned)),
        request_id: Set(write.request_id().to_owned()),
        client_fingerprint: Set(projection.client_fingerprint.clone()),
        change_summary: Set(write.change_summary().map(str::to_owned)),
        created_at: Set(decision_model.created_at),
        ..Default::default()
    }
    .insert(transaction)
    .await
    .map_err(|_| OrganizationApprovalRepositoryError::Query)?;
    Ok(())
}

/// 重放已有决定时校验审计投影，防止损坏或跨企业事实被静默接受。
pub(crate) async fn validate_decision_audit(
    pool: &DatabasePool,
    existing: &OrganizationApprovalDecisionRecord,
) -> Result<(), OrganizationApprovalRepositoryError> {
    let request = organization_approval_requests::Entity::find_by_id(existing.request_id().get())
        .filter(
            organization_approval_requests::Column::OrganizationId
                .eq(existing.organization_id().get()),
        )
        .one(pool.connection())
        .await
        .map_err(|_| OrganizationApprovalRepositoryError::Query)?
        .ok_or(OrganizationApprovalRepositoryError::Invariant)?;
    let projection = Projection::from_record(&request, existing)?;
    let audit = organization_audit_logs::Entity::find()
        .filter(
            organization_audit_logs::Column::OrganizationId.eq(projection.organization_id.get()),
        )
        .filter(organization_audit_logs::Column::RequestId.eq(&projection.request_id))
        .filter(organization_audit_logs::Column::Operation.eq(AUDIT_OPERATION))
        .one(pool.connection())
        .await
        .map_err(|_| OrganizationApprovalRepositoryError::Query)?
        .ok_or(OrganizationApprovalRepositoryError::Invariant)?;
    projection
        .matches(&audit)
        .then_some(())
        .ok_or(OrganizationApprovalRepositoryError::Invariant)
}

struct Projection {
    organization_id: OrganizationId,
    actor_user_id: UserId,
    actor_role: Option<OrganizationRole>,
    platform_reason: Option<&'static str>,
    request_id: String,
    target_identifier: String,
    outcome: OrganizationAuditOutcome,
    client_fingerprint: String,
    change_summary: String,
    created_at: TimeDateTimeWithTimeZone,
}

impl Projection {
    async fn new(
        transaction: &DatabaseTransaction,
        request: &organization_approval_requests::Model,
        decision: &OrganizationApprovalDecision,
        decision_model: &organization_approval_decisions::Model,
    ) -> Result<Self, OrganizationApprovalRepositoryError> {
        let actor_user_id = match decision.decided_by_user_id() {
            Some(user_id) => user_id,
            None => UserId::new(request.requested_by_user_id)
                .map_err(|_| OrganizationApprovalRepositoryError::Invariant)?,
        };
        let actor_role = if decision.decided_by_user_id().is_some() {
            Some(active_member_role(transaction, decision.organization_id(), actor_user_id).await?)
        } else {
            None
        };
        Self::from_parts(
            request,
            decision.organization_id(),
            actor_user_id,
            actor_role,
            decision.kind(),
            decision.reason().is_some(),
            decision.decision_key().persistence_key(),
            decision_model.created_at,
        )
    }

    fn from_record(
        request: &organization_approval_requests::Model,
        decision: &OrganizationApprovalDecisionRecord,
    ) -> Result<Self, OrganizationApprovalRepositoryError> {
        let actor_user_id = match decision.decided_by_user_id() {
            Some(user_id) => user_id,
            None => UserId::new(request.requested_by_user_id)
                .map_err(|_| OrganizationApprovalRepositoryError::Invariant)?,
        };
        Self::from_parts(
            request,
            decision.organization_id(),
            actor_user_id,
            decision
                .decided_by_user_id()
                .map(|_| OrganizationRole::Member),
            decision.kind(),
            decision.reason().is_some(),
            decision.decision_key().persistence_key(),
            decision.created_at(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn from_parts(
        request: &organization_approval_requests::Model,
        organization_id: OrganizationId,
        actor_user_id: UserId,
        actor_role: Option<OrganizationRole>,
        kind: OrganizationApprovalDecisionKind,
        reason_present: bool,
        decision_key: String,
        created_at: TimeDateTimeWithTimeZone,
    ) -> Result<Self, OrganizationApprovalRepositoryError> {
        if request.organization_id != organization_id.get()
            || request.id <= 0
            || request.request_key.is_empty()
        {
            return Err(OrganizationApprovalRepositoryError::Invariant);
        }
        let request_key = request.request_key.clone();
        let request_id = format!("{AUDIT_REQUEST_PREFIX}{decision_key}");
        let target_identifier = format!("{request_key}:{decision_key}");
        let result = decision_result(kind);
        let reason_code = reason_code(kind, reason_present);
        let change_summary = serde_json::json!({
            "event_type": "approval_decision",
            "result": result,
            "reason_code": reason_code,
            "request_key": request_key,
            "decision_key": decision_key,
        })
        .to_string();
        let mut digest = Sha256::new();
        digest.update(FINGERPRINT_DOMAIN);
        digest.update(organization_id.get().to_be_bytes());
        digest.update(decision_key.as_bytes());
        let mut fingerprint = [0_u8; 32];
        fingerprint.copy_from_slice(&digest.finalize());
        Ok(Self {
            organization_id,
            actor_user_id,
            actor_role,
            platform_reason: matches!(kind, OrganizationApprovalDecisionKind::TimedOut)
                .then_some(AUDIT_PLATFORM_REASON),
            request_id,
            target_identifier,
            outcome: audit_outcome(kind),
            client_fingerprint: fingerprint
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
            change_summary,
            created_at,
        })
    }

    fn to_write(&self) -> Result<OrganizationAuditWrite, OrganizationApprovalRepositoryError> {
        OrganizationAuditWrite::new(
            self.organization_id,
            self.actor_user_id,
            self.actor_role,
            AUDIT_PERMISSION.to_owned(),
            AUDIT_OPERATION.to_owned(),
            AUDIT_TARGET_TYPE.to_owned(),
            self.target_identifier.clone(),
            self.outcome,
            self.platform_reason.map(str::to_owned),
            self.request_id.clone(),
            decode_fingerprint(&self.client_fingerprint)?,
            Some(self.change_summary.clone()),
        )
        .map_err(|_| OrganizationApprovalRepositoryError::Invariant)
    }

    fn matches(&self, model: &organization_audit_logs::Model) -> bool {
        model.organization_id == self.organization_id.get()
            && model.actor_user_id == self.actor_user_id.get()
            && model.actor_role.is_some() == self.actor_role.is_some()
            && model.permission_code == AUDIT_PERMISSION
            && model.operation == AUDIT_OPERATION
            && model.target_type == AUDIT_TARGET_TYPE
            && model.target_identifier == self.target_identifier
            && model.outcome == self.outcome.code()
            && model.platform_proxy == self.actor_role.is_none()
            && model.platform_reason.as_deref() == self.platform_reason
            && model.request_id == self.request_id
            && model.client_fingerprint == self.client_fingerprint
            && model.change_summary.as_deref() == Some(self.change_summary.as_str())
            && model.created_at == self.created_at
    }
}

async fn active_member_role(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    user_id: UserId,
) -> Result<OrganizationRole, OrganizationApprovalRepositoryError> {
    let member = organization_memberships::Entity::find()
        .filter(organization_memberships::Column::OrganizationId.eq(organization_id.get()))
        .filter(organization_memberships::Column::UserId.eq(user_id.get()))
        .filter(
            organization_memberships::Column::Status
                .eq(OrganizationMembershipStatus::Active.code()),
        )
        .one(transaction)
        .await
        .map_err(|_| OrganizationApprovalRepositoryError::Query)?
        .ok_or(OrganizationApprovalRepositoryError::MemberUnavailable)?;
    OrganizationRole::try_from(member.role)
        .map_err(|_| OrganizationApprovalRepositoryError::Invariant)
}

fn decision_result(kind: OrganizationApprovalDecisionKind) -> &'static str {
    match kind {
        OrganizationApprovalDecisionKind::Approved => "approved",
        OrganizationApprovalDecisionKind::Rejected => "rejected",
        OrganizationApprovalDecisionKind::TimedOut => "timed_out",
    }
}

fn reason_code(kind: OrganizationApprovalDecisionKind, reason_present: bool) -> &'static str {
    match kind {
        OrganizationApprovalDecisionKind::TimedOut => "expired",
        OrganizationApprovalDecisionKind::Approved | OrganizationApprovalDecisionKind::Rejected => {
            if reason_present {
                "provided"
            } else {
                "none"
            }
        }
    }
}

fn audit_outcome(kind: OrganizationApprovalDecisionKind) -> OrganizationAuditOutcome {
    match kind {
        OrganizationApprovalDecisionKind::Approved => OrganizationAuditOutcome::Succeeded,
        OrganizationApprovalDecisionKind::Rejected | OrganizationApprovalDecisionKind::TimedOut => {
            OrganizationAuditOutcome::Failed
        }
    }
}

fn decode_fingerprint(value: &str) -> Result<[u8; 32], OrganizationApprovalRepositoryError> {
    if value.len() != 64 {
        return Err(OrganizationApprovalRepositoryError::Invariant);
    }
    let mut decoded = [0_u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let high = hex(pair[0]).ok_or(OrganizationApprovalRepositoryError::Invariant)?;
        let low = hex(pair[1]).ok_or(OrganizationApprovalRepositoryError::Invariant)?;
        decoded[index] = (high << 4) | low;
    }
    Ok(decoded)
}

fn hex(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        _ => None,
    }
}
