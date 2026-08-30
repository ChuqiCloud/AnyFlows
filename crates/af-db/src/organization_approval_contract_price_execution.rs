use std::fmt;

use af_domain::{
    OrganizationApprovalDecisionKind, OrganizationApprovalRequestId,
    OrganizationApprovalResourceType, OrganizationContractPriceId, OrganizationId,
    OrganizationMembershipId, OrganizationPermission, Protocol, UserId,
};
use rust_decimal::Decimal;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseTransaction, DbBackend, EntityTrait,
    QueryFilter, QuerySelect, Set, entity::prelude::TimeDateTimeWithTimeZone, sea_query::LockType,
};
use sha2::{Digest as _, Sha256};

use crate::{
    entity::organization_approval_contract_price_executions,
    organization::mutation,
    organization_approval_execution::{
        OrganizationApprovalExecutionRepositoryError, lock_decision, lock_request,
        require_current_member_permission,
    },
    organization_contract_price::{OrganizationContractPriceWrite, create_in_transaction},
};

const SUMMARY_PREFIX: &str = "contract-price-create-sha256:";

/// 合同价审批执行命令；所有价格和区间字段均来自申请时的完整快照。
#[derive(Clone, PartialEq)]
pub struct OrganizationApprovalContractPriceExecutionWrite {
    organization_id: OrganizationId,
    request_id: OrganizationApprovalRequestId,
    model: String,
    protocol: Protocol,
    prices: [Decimal; 5],
    effective_from: TimeDateTimeWithTimeZone,
    effective_until: Option<TimeDateTimeWithTimeZone>,
    expected_version: Option<i64>,
    executed_by_membership_id: OrganizationMembershipId,
    executed_by_user_id: UserId,
}

impl OrganizationApprovalContractPriceExecutionWrite {
    /// 构造合同价创建执行命令并校验时间与版本边界。
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        organization_id: OrganizationId,
        request_id: OrganizationApprovalRequestId,
        model: String,
        protocol: Protocol,
        prices: [Decimal; 5],
        effective_from: TimeDateTimeWithTimeZone,
        effective_until: Option<TimeDateTimeWithTimeZone>,
        expected_version: Option<i64>,
        executed_by_membership_id: OrganizationMembershipId,
        executed_by_user_id: UserId,
    ) -> Result<Self, OrganizationApprovalExecutionRepositoryError> {
        if model.is_empty()
            || model.len() > 256
            || model.trim() != model
            || model.chars().any(char::is_control)
            || prices.iter().any(|price| price.is_sign_negative())
            || effective_from.unix_timestamp() < 0
            || effective_until.is_some_and(|until| until <= effective_from)
            || expected_version.is_some_and(|version| !(0..i64::MAX).contains(&version))
        {
            return Err(OrganizationApprovalExecutionRepositoryError::InvalidInput);
        }
        Ok(Self {
            organization_id,
            request_id,
            model,
            protocol,
            prices,
            effective_from,
            effective_until,
            expected_version,
            executed_by_membership_id,
            executed_by_user_id,
        })
    }

    /// 返回创建审批申请时必须保存的规范化目标摘要。
    #[must_use]
    pub fn resource_summary(&self) -> String {
        organization_approval_contract_price_resource_summary(
            self.organization_id,
            &self.model,
            self.protocol,
            self.prices,
            self.effective_from,
            self.effective_until,
            self.expected_version,
        )
    }
}

/// 已追加的合同价审批执行事实。
#[derive(Clone, Debug, PartialEq)]
pub struct OrganizationApprovalContractPriceExecutionRecord {
    id: i64,
    organization_id: OrganizationId,
    request_id: OrganizationApprovalRequestId,
    decision_id: i64,
    contract_price_id: OrganizationContractPriceId,
    model: String,
    protocol: Protocol,
    prices: [Decimal; 5],
    effective_from: i64,
    effective_until: Option<i64>,
    expected_version: Option<i64>,
    applied_version: i64,
    executed_by_membership_id: OrganizationMembershipId,
    executed_at: i64,
}

impl OrganizationApprovalContractPriceExecutionRecord {
    #[must_use]
    pub const fn id(&self) -> i64 {
        self.id
    }
    #[must_use]
    pub const fn organization_id(&self) -> OrganizationId {
        self.organization_id
    }
    #[must_use]
    pub const fn request_id(&self) -> OrganizationApprovalRequestId {
        self.request_id
    }
    #[must_use]
    pub const fn decision_id(&self) -> i64 {
        self.decision_id
    }
    #[must_use]
    pub const fn contract_price_id(&self) -> OrganizationContractPriceId {
        self.contract_price_id
    }
    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }
    #[must_use]
    pub const fn protocol(&self) -> Protocol {
        self.protocol
    }
    #[must_use]
    pub const fn prices(&self) -> [Decimal; 5] {
        self.prices
    }
    #[must_use]
    pub const fn effective_from(&self) -> i64 {
        self.effective_from
    }
    #[must_use]
    pub const fn effective_until(&self) -> Option<i64> {
        self.effective_until
    }
    #[must_use]
    pub const fn expected_version(&self) -> Option<i64> {
        self.expected_version
    }
    #[must_use]
    pub const fn applied_version(&self) -> i64 {
        self.applied_version
    }
    #[must_use]
    pub const fn executed_by_membership_id(&self) -> OrganizationMembershipId {
        self.executed_by_membership_id
    }
    #[must_use]
    pub const fn executed_at(&self) -> i64 {
        self.executed_at
    }
}

/// 合同价审批执行结果；Existing 表示命令未被再次应用。
#[derive(Clone, Debug, PartialEq)]
pub enum OrganizationApprovalContractPriceExecutionOutcome {
    Applied(OrganizationApprovalContractPriceExecutionRecord),
    Existing(OrganizationApprovalContractPriceExecutionRecord),
}

/// 在同一事务内复验审批事实、企业边界、权限和合同价版本，并追加创建事实。
pub(crate) async fn execute_contract_price_transaction(
    transaction: &DatabaseTransaction,
    write: OrganizationApprovalContractPriceExecutionWrite,
) -> Result<
    OrganizationApprovalContractPriceExecutionOutcome,
    OrganizationApprovalExecutionRepositoryError,
> {
    let organization = mutation::lock_organization(transaction, write.organization_id)
        .await
        .map_err(map_organization_error)?;
    if organization.status != af_domain::OrganizationStatus::Active.code() {
        return Err(OrganizationApprovalExecutionRepositoryError::Forbidden);
    }
    let request = lock_request(transaction, write.organization_id, write.request_id).await?;
    if request.resource_type != OrganizationApprovalResourceType::ContractPrice.code() {
        return Err(OrganizationApprovalExecutionRepositoryError::ResourceTypeMismatch);
    }
    if request.resource_summary.as_str() != write.resource_summary() {
        return Err(OrganizationApprovalExecutionRepositoryError::Conflict);
    }
    let decision = lock_decision(transaction, write.organization_id, write.request_id).await?;
    if decision.decision_kind != OrganizationApprovalDecisionKind::Approved.code() {
        return Err(OrganizationApprovalExecutionRepositoryError::NotApproved);
    }
    let actor = mutation::lock_membership(
        transaction,
        write.organization_id,
        write.executed_by_membership_id,
    )
    .await
    .map_err(map_organization_error)?;
    require_current_member_permission(
        transaction,
        &actor,
        write.executed_by_user_id,
        OrganizationPermission::ContractPricesManage,
    )
    .await?;

    if let Some(existing) =
        find_execution(transaction, write.organization_id, write.request_id).await?
    {
        let record = execution_record(existing)?;
        return exact_replay(&record, &write, decision.id)
            .then_some(OrganizationApprovalContractPriceExecutionOutcome::Existing(
                record,
            ))
            .ok_or(OrganizationApprovalExecutionRepositoryError::Conflict);
    }
    if request.expires_at <= TimeDateTimeWithTimeZone::now_utc() {
        return Err(OrganizationApprovalExecutionRepositoryError::Expired);
    }

    let price = create_in_transaction(
        transaction,
        OrganizationContractPriceWrite::new(
            write.organization_id,
            write.model.clone(),
            write.protocol,
            write.prices,
            write.effective_from,
            write.effective_until,
            write.expected_version,
            write.executed_by_user_id,
        ),
    )
    .await
    .map_err(map_contract_price_error)?;
    let executed_at = TimeDateTimeWithTimeZone::now_utc();
    let model = organization_approval_contract_price_executions::ActiveModel {
        organization_id: Set(write.organization_id.get()),
        request_id: Set(write.request_id.get()),
        decision_id: Set(decision.id),
        contract_price_id: Set(price.id().get()),
        model: Set(write.model.into()),
        protocol: Set(write.protocol.as_str().to_owned()),
        input_price: Set(write.prices[0].into()),
        output_price: Set(write.prices[1].into()),
        cache_read_price: Set(write.prices[2].into()),
        cache_creation_5m_price: Set(write.prices[3].into()),
        cache_creation_1h_price: Set(write.prices[4].into()),
        effective_from: Set(write.effective_from),
        effective_until: Set(write.effective_until),
        expected_version: Set(write.expected_version),
        applied_version: Set(price.version()),
        executed_by_membership_id: Set(write.executed_by_membership_id.get()),
        executed_at: Set(executed_at),
        ..Default::default()
    }
    .insert(transaction)
    .await
    .map_err(query_error)?;
    Ok(OrganizationApprovalContractPriceExecutionOutcome::Applied(
        execution_record(model)?,
    ))
}

async fn find_execution(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    request_id: OrganizationApprovalRequestId,
) -> Result<
    Option<organization_approval_contract_price_executions::Model>,
    OrganizationApprovalExecutionRepositoryError,
> {
    let mut query = organization_approval_contract_price_executions::Entity::find()
        .filter(
            organization_approval_contract_price_executions::Column::OrganizationId
                .eq(organization_id.get()),
        )
        .filter(
            organization_approval_contract_price_executions::Column::RequestId.eq(request_id.get()),
        );
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query.one(transaction).await.map_err(query_error)
}

fn exact_replay(
    record: &OrganizationApprovalContractPriceExecutionRecord,
    write: &OrganizationApprovalContractPriceExecutionWrite,
    decision_id: i64,
) -> bool {
    record.organization_id == write.organization_id
        && record.request_id == write.request_id
        && record.decision_id == decision_id
        && record.model == write.model
        && record.protocol == write.protocol
        && record.prices == write.prices
        && record.effective_from == write.effective_from.unix_timestamp()
        && record.effective_until == write.effective_until.map(|value| value.unix_timestamp())
        && record.expected_version == write.expected_version
        && record.executed_by_membership_id == write.executed_by_membership_id
}

fn execution_record(
    model: organization_approval_contract_price_executions::Model,
) -> Result<
    OrganizationApprovalContractPriceExecutionRecord,
    OrganizationApprovalExecutionRepositoryError,
> {
    if model.id <= 0
        || model.request_id <= 0
        || model.decision_id <= 0
        || model.organization_id <= 0
        || model.contract_price_id <= 0
        || model.model.as_str().is_empty()
        || model.model.as_str().len() > 256
        || model.model.as_str().trim() != model.model.as_str()
        || model.model.as_str().chars().any(char::is_control)
        || model.prices().iter().any(|price| price.is_sign_negative())
        || model
            .effective_until
            .is_some_and(|until| until <= model.effective_from)
        || model
            .expected_version
            .is_some_and(|version| !(0..i64::MAX).contains(&version))
        || model.applied_version <= 0
        || model
            .expected_version
            .is_some_and(|version| version.checked_add(1) != Some(model.applied_version))
        || model.expected_version.is_none() && model.applied_version != 1
        || model.executed_by_membership_id <= 0
        || model.effective_from.unix_timestamp() < 0
        || model.executed_at.unix_timestamp() < 0
    {
        return Err(invariant());
    }
    Ok(OrganizationApprovalContractPriceExecutionRecord {
        id: model.id,
        organization_id: OrganizationId::new(model.organization_id).map_err(|_| invariant())?,
        request_id: OrganizationApprovalRequestId::new(model.request_id)
            .map_err(|_| invariant())?,
        decision_id: model.decision_id,
        contract_price_id: OrganizationContractPriceId::new(model.contract_price_id)
            .map_err(|_| invariant())?,
        model: model.model.as_str().to_owned(),
        protocol: model
            .protocol
            .parse::<Protocol>()
            .map_err(|_| invariant())?,
        prices: [
            model.input_price.expose(),
            model.output_price.expose(),
            model.cache_read_price.expose(),
            model.cache_creation_5m_price.expose(),
            model.cache_creation_1h_price.expose(),
        ],
        effective_from: model.effective_from.unix_timestamp(),
        effective_until: model.effective_until.map(|value| value.unix_timestamp()),
        expected_version: model.expected_version,
        applied_version: model.applied_version,
        executed_by_membership_id: OrganizationMembershipId::new(model.executed_by_membership_id)
            .map_err(|_| invariant())?,
        executed_at: model.executed_at.unix_timestamp(),
    })
}

/// 为申请创建和执行复验生成同一份合同价目标摘要。
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn organization_approval_contract_price_resource_summary(
    organization_id: OrganizationId,
    model: &str,
    protocol: Protocol,
    prices: [Decimal; 5],
    effective_from: TimeDateTimeWithTimeZone,
    effective_until: Option<TimeDateTimeWithTimeZone>,
    expected_version: Option<i64>,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"anyflows:organization-approval:contract-price-create:v1\0");
    hasher.update(organization_id.get().to_be_bytes());
    hash_text(&mut hasher, model);
    hash_text(&mut hasher, protocol.as_str());
    for price in prices {
        hash_text(&mut hasher, &price.normalize().to_string());
    }
    hasher.update(effective_from.unix_timestamp().to_be_bytes());
    match effective_until {
        Some(value) => {
            hasher.update([1]);
            hasher.update(value.unix_timestamp().to_be_bytes());
        }
        None => hasher.update([0]),
    }
    match expected_version {
        Some(value) => {
            hasher.update([1]);
            hasher.update(value.to_be_bytes());
        }
        None => hasher.update([0]),
    }
    digest_summary(hasher.finalize())
}

fn hash_text(hasher: &mut Sha256, value: &str) {
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value.as_bytes());
}

fn digest_summary(digest: impl AsRef<[u8]>) -> String {
    let bytes = digest.as_ref();
    let mut summary = String::with_capacity(SUMMARY_PREFIX.len() + bytes.len() * 2);
    summary.push_str(SUMMARY_PREFIX);
    for byte in bytes {
        use std::fmt::Write as _;
        write!(&mut summary, "{byte:02x}").expect("写入 String 不会失败");
    }
    summary
}

fn map_contract_price_error(
    error: crate::OrganizationContractPriceRepositoryError,
) -> OrganizationApprovalExecutionRepositoryError {
    match error {
        crate::OrganizationContractPriceRepositoryError::InvalidInput => {
            OrganizationApprovalExecutionRepositoryError::InvalidInput
        }
        crate::OrganizationContractPriceRepositoryError::NotFound
        | crate::OrganizationContractPriceRepositoryError::OrganizationUnavailable => {
            OrganizationApprovalExecutionRepositoryError::NotFound
        }
        crate::OrganizationContractPriceRepositoryError::CreatorUnavailable => {
            OrganizationApprovalExecutionRepositoryError::Forbidden
        }
        crate::OrganizationContractPriceRepositoryError::IntervalConflict
        | crate::OrganizationContractPriceRepositoryError::Conflict => {
            OrganizationApprovalExecutionRepositoryError::Conflict
        }
        crate::OrganizationContractPriceRepositoryError::InvalidTransition => {
            OrganizationApprovalExecutionRepositoryError::InvalidTransition
        }
        crate::OrganizationContractPriceRepositoryError::Query
        | crate::OrganizationContractPriceRepositoryError::Timeout
        | crate::OrganizationContractPriceRepositoryError::Invariant => {
            OrganizationApprovalExecutionRepositoryError::Query
        }
    }
}

fn map_organization_error(
    error: crate::OrganizationRepositoryError,
) -> OrganizationApprovalExecutionRepositoryError {
    match error {
        crate::OrganizationRepositoryError::InvalidInput => {
            OrganizationApprovalExecutionRepositoryError::InvalidInput
        }
        crate::OrganizationRepositoryError::NotFound
        | crate::OrganizationRepositoryError::UserUnavailable => {
            OrganizationApprovalExecutionRepositoryError::NotFound
        }
        crate::OrganizationRepositoryError::Conflict => {
            OrganizationApprovalExecutionRepositoryError::Conflict
        }
        crate::OrganizationRepositoryError::InvalidTransition => {
            OrganizationApprovalExecutionRepositoryError::InvalidTransition
        }
        crate::OrganizationRepositoryError::LastOwner => {
            OrganizationApprovalExecutionRepositoryError::LastOwner
        }
        crate::OrganizationRepositoryError::EntitlementRestricted => {
            OrganizationApprovalExecutionRepositoryError::EntitlementRestricted
        }
        crate::OrganizationRepositoryError::SeatLimitReached => {
            OrganizationApprovalExecutionRepositoryError::SeatLimitReached
        }
        crate::OrganizationRepositoryError::Query => {
            OrganizationApprovalExecutionRepositoryError::Query
        }
        crate::OrganizationRepositoryError::Timeout => {
            OrganizationApprovalExecutionRepositoryError::Timeout
        }
        crate::OrganizationRepositoryError::Invariant
        | crate::OrganizationRepositoryError::TeamHasActiveMembers
        | crate::OrganizationRepositoryError::InvitationExpired
        | crate::OrganizationRepositoryError::InvitationNotPending
        | crate::OrganizationRepositoryError::InvitationEmailMismatch
        | crate::OrganizationRepositoryError::InvitationBatchConflict
        | crate::OrganizationRepositoryError::TeamLimitReached
        | crate::OrganizationRepositoryError::EntropyUnavailable => {
            OrganizationApprovalExecutionRepositoryError::Invariant
        }
    }
}

fn query_error(_: sea_orm::DbErr) -> OrganizationApprovalExecutionRepositoryError {
    OrganizationApprovalExecutionRepositoryError::Query
}

fn invariant() -> OrganizationApprovalExecutionRepositoryError {
    OrganizationApprovalExecutionRepositoryError::Invariant
}

impl fmt::Debug for OrganizationApprovalContractPriceExecutionWrite {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OrganizationApprovalContractPriceExecutionWrite(<redacted>)")
    }
}

trait PricesExpose {
    fn prices(&self) -> [Decimal; 5];
}

impl PricesExpose for organization_approval_contract_price_executions::Model {
    fn prices(&self) -> [Decimal; 5] {
        [
            self.input_price.expose(),
            self.output_price.expose(),
            self.cache_read_price.expose(),
            self.cache_creation_5m_price.expose(),
            self.cache_creation_1h_price.expose(),
        ]
    }
}
