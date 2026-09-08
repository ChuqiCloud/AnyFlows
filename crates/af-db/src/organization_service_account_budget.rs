use std::{fmt, time::Duration};

use af_domain::{
    OrganizationBudgetPolicyId, OrganizationDepartmentId, OrganizationId,
    OrganizationServiceAccountBudgetBinding, OrganizationServiceAccountBudgetBindingError,
    OrganizationServiceAccountBudgetScope, OrganizationServiceAccountId,
    OrganizationServiceAccountStatus, OrganizationTeamId,
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseTransaction, DbBackend, DbErr,
    EntityTrait, QueryFilter, QuerySelect, Set, SqlErr, TransactionTrait,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Expr, LockType},
};
use thiserror::Error;
use tokio::time::timeout;

use crate::{
    DatabasePool,
    entity::{
        organization_budget_policies, organization_service_account_budget_bindings,
        organization_service_accounts, organization_teams, organizations,
    },
};

/// 服务账号预算绑定仓储构造错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationServiceAccountBudgetBindingRepositoryConfigError {
    #[error("服务账号预算绑定仓储超时必须大于零")]
    ZeroOperationTimeout,
}

/// 服务账号预算绑定仓储错误；不携带主体标识、策略内容或额度。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationServiceAccountBudgetBindingRepositoryError {
    #[error("服务账号预算绑定输入无效")]
    InvalidInput,
    #[error("服务账号预算绑定不存在")]
    NotFound,
    #[error("企业服务账号不可用")]
    AccountUnavailable,
    #[error("企业预算策略不可用")]
    PolicyUnavailable,
    #[error("企业预算团队不可用")]
    TeamUnavailable,
    #[error("服务账号预算绑定版本冲突")]
    Conflict,
    #[error("服务账号预算绑定数据库操作失败")]
    Query,
    #[error("服务账号预算绑定操作超时")]
    Timeout,
    #[error("服务账号预算绑定持久化状态损坏")]
    Invariant,
}

/// 服务账号企业预算绑定的持久化边界。
#[derive(Clone)]
pub struct OrganizationServiceAccountBudgetBindingRepository {
    pool: DatabasePool,
    operation_timeout: Duration,
}

impl OrganizationServiceAccountBudgetBindingRepository {
    /// 使用共享连接池和非零硬截止时间构造仓储。
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, OrganizationServiceAccountBudgetBindingRepositoryConfigError> {
        if operation_timeout.is_zero() {
            return Err(
                OrganizationServiceAccountBudgetBindingRepositoryConfigError::ZeroOperationTimeout,
            );
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }

    /// 按服务账号版本 CAS 设置企业或团队预算策略绑定。
    ///
    /// `None` 表示仅允许创建，`Some(version)` 表示仅允许更新该版本；不存在绑定时，
    /// 带版本的更新以及并发创建均返回冲突。
    pub async fn set_binding(
        &self,
        organization_id: OrganizationId,
        service_account_id: OrganizationServiceAccountId,
        policy_id: OrganizationBudgetPolicyId,
        expected_version: Option<i64>,
    ) -> Result<
        OrganizationServiceAccountBudgetBinding,
        OrganizationServiceAccountBudgetBindingRepositoryError,
    > {
        validate_expected_version(expected_version)?;
        timeout(
            self.operation_timeout,
            self.set_binding_inner(
                organization_id,
                service_account_id,
                policy_id,
                expected_version,
            ),
        )
        .await
        .map_err(|_| OrganizationServiceAccountBudgetBindingRepositoryError::Timeout)?
    }

    /// 解析运行时服务账号的当前预算绑定；主体、团队或策略失效时失败关闭。
    pub async fn resolve_binding(
        &self,
        organization_id: OrganizationId,
        service_account_id: OrganizationServiceAccountId,
    ) -> Result<
        OrganizationServiceAccountBudgetBinding,
        OrganizationServiceAccountBudgetBindingRepositoryError,
    > {
        timeout(
            self.operation_timeout,
            self.resolve_binding_inner(organization_id, service_account_id),
        )
        .await
        .map_err(|_| OrganizationServiceAccountBudgetBindingRepositoryError::Timeout)?
    }

    async fn set_binding_inner(
        &self,
        organization_id: OrganizationId,
        service_account_id: OrganizationServiceAccountId,
        policy_id: OrganizationBudgetPolicyId,
        expected_version: Option<i64>,
    ) -> Result<
        OrganizationServiceAccountBudgetBinding,
        OrganizationServiceAccountBudgetBindingRepositoryError,
    > {
        let transaction = self.pool.connection().begin().await.map_err(query_error)?;
        let result = set_binding_in_transaction(
            &transaction,
            organization_id,
            service_account_id,
            policy_id,
            expected_version,
        )
        .await;
        match result {
            Ok(record) => {
                commit(transaction).await?;
                Ok(record)
            }
            Err(error) => {
                rollback(transaction).await?;
                Err(error)
            }
        }
    }

    async fn resolve_binding_inner(
        &self,
        organization_id: OrganizationId,
        service_account_id: OrganizationServiceAccountId,
    ) -> Result<
        OrganizationServiceAccountBudgetBinding,
        OrganizationServiceAccountBudgetBindingRepositoryError,
    > {
        let transaction = self.pool.connection().begin().await.map_err(query_error)?;
        lock_organization(&transaction, organization_id).await?;
        let account = lock_account(&transaction, organization_id, service_account_id).await?;
        validate_account(&account)?;
        let binding = lock_binding(&transaction, organization_id, service_account_id)
            .await?
            .ok_or(OrganizationServiceAccountBudgetBindingRepositoryError::NotFound)?;
        let policy = lock_policy(
            &transaction,
            organization_id,
            OrganizationBudgetPolicyId::new(binding.policy_id).map_err(|_| invariant())?,
        )
        .await?;
        let scope = validate_policy(
            &transaction,
            &policy,
            organization_id,
            TimeDateTimeWithTimeZone::now_utc(),
        )
        .await?;
        let record = binding_record_with_policy(binding, scope, policy.version)?;
        commit(transaction).await?;
        Ok(record)
    }
}

/// 在调用方事务内按企业 → 服务账号 → 策略 → 绑定的固定顺序更新预算绑定。
pub(crate) async fn set_binding_in_transaction(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    service_account_id: OrganizationServiceAccountId,
    policy_id: OrganizationBudgetPolicyId,
    expected_version: Option<i64>,
) -> Result<
    OrganizationServiceAccountBudgetBinding,
    OrganizationServiceAccountBudgetBindingRepositoryError,
> {
    set_binding_in_transaction_with_account_version(
        transaction,
        organization_id,
        service_account_id,
        policy_id,
        expected_version,
        None,
        None,
    )
    .await
}

/// 在调用方事务内额外复验服务账号主体版本，再更新预算绑定。
pub(crate) async fn set_binding_in_transaction_with_account_version(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    service_account_id: OrganizationServiceAccountId,
    policy_id: OrganizationBudgetPolicyId,
    expected_version: Option<i64>,
    expected_account_version: Option<i64>,
    expected_department_id: Option<Option<OrganizationDepartmentId>>,
) -> Result<
    OrganizationServiceAccountBudgetBinding,
    OrganizationServiceAccountBudgetBindingRepositoryError,
> {
    lock_organization(transaction, organization_id).await?;
    let account = lock_account(transaction, organization_id, service_account_id).await?;
    validate_account(&account)?;
    if expected_account_version.is_some_and(|version| version != account.version) {
        return Err(OrganizationServiceAccountBudgetBindingRepositoryError::Conflict);
    }
    if expected_department_id.is_some_and(|department_id| {
        department_id.map(OrganizationDepartmentId::get) != account.organization_department_id
    }) {
        return Err(OrganizationServiceAccountBudgetBindingRepositoryError::Conflict);
    }
    let policy = lock_policy(transaction, organization_id, policy_id).await?;
    let now = TimeDateTimeWithTimeZone::now_utc();
    let scope = validate_policy(transaction, &policy, organization_id, now).await?;
    let current = lock_binding(transaction, organization_id, service_account_id).await?;

    match current {
        Some(current) => {
            if expected_version != Some(current.version) {
                return Err(OrganizationServiceAccountBudgetBindingRepositoryError::Conflict);
            }
            let current_binding = binding_record(current.clone())?;
            if current_binding.policy_id() == policy_id {
                return binding_record_with_policy(current, scope, policy.version);
            }
            let version = current.version.checked_add(1).ok_or_else(invariant)?;
            let result = organization_service_account_budget_bindings::Entity::update_many()
                .filter(organization_service_account_budget_bindings::Column::Id.eq(current.id))
                .filter(
                    organization_service_account_budget_bindings::Column::OrganizationId
                        .eq(organization_id.get()),
                )
                .filter(
                    organization_service_account_budget_bindings::Column::ServiceAccountId
                        .eq(service_account_id.get()),
                )
                .filter(
                    organization_service_account_budget_bindings::Column::Version
                        .eq(current.version),
                )
                .col_expr(
                    organization_service_account_budget_bindings::Column::PolicyId,
                    Expr::value(policy_id.get()),
                )
                .col_expr(
                    organization_service_account_budget_bindings::Column::PolicyVersion,
                    Expr::value(policy.version),
                )
                .col_expr(
                    organization_service_account_budget_bindings::Column::ScopeKind,
                    Expr::value(scope.code()),
                )
                .col_expr(
                    organization_service_account_budget_bindings::Column::TeamId,
                    Expr::value(scope.team_id().map(OrganizationTeamId::get)),
                )
                .col_expr(
                    organization_service_account_budget_bindings::Column::Version,
                    Expr::value(version),
                )
                .col_expr(
                    organization_service_account_budget_bindings::Column::UpdatedAt,
                    Expr::value(now),
                )
                .exec(transaction)
                .await
                .map_err(query_error)?;
            if result.rows_affected != 1 {
                return Err(OrganizationServiceAccountBudgetBindingRepositoryError::Conflict);
            }
            let updated =
                organization_service_account_budget_bindings::Entity::find_by_id(current.id)
                    .filter(
                        organization_service_account_budget_bindings::Column::OrganizationId
                            .eq(organization_id.get()),
                    )
                    .one(transaction)
                    .await
                    .map_err(query_error)?
                    .ok_or_else(invariant)?;
            binding_record_with_policy(updated, scope, policy.version)
        }
        None => {
            if expected_version.is_some() {
                return Err(OrganizationServiceAccountBudgetBindingRepositoryError::Conflict);
            }
            let model = organization_service_account_budget_bindings::ActiveModel {
                organization_id: Set(organization_id.get()),
                service_account_id: Set(service_account_id.get()),
                policy_id: Set(policy_id.get()),
                policy_version: Set(policy.version),
                scope_kind: Set(scope.code()),
                team_id: Set(scope.team_id().map(OrganizationTeamId::get)),
                version: Set(1),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            }
            .insert(transaction)
            .await
            .map_err(|error| {
                if is_unique_violation(&error) {
                    OrganizationServiceAccountBudgetBindingRepositoryError::Conflict
                } else {
                    query_error(error)
                }
            })?;
            binding_record_with_policy(model, scope, policy.version)
        }
    }
}

impl fmt::Debug for OrganizationServiceAccountBudgetBindingRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OrganizationServiceAccountBudgetBindingRepository")
            .field("operation_timeout", &self.operation_timeout)
            .finish_non_exhaustive()
    }
}

async fn lock_account(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    service_account_id: OrganizationServiceAccountId,
) -> Result<
    organization_service_accounts::Model,
    OrganizationServiceAccountBudgetBindingRepositoryError,
> {
    let mut query = organization_service_accounts::Entity::find_by_id(service_account_id.get())
        .filter(organization_service_accounts::Column::OrganizationId.eq(organization_id.get()));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(query_error)?
        .ok_or(OrganizationServiceAccountBudgetBindingRepositoryError::AccountUnavailable)
}

async fn lock_organization(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
) -> Result<(), OrganizationServiceAccountBudgetBindingRepositoryError> {
    let mut query = organizations::Entity::find_by_id(organization_id.get())
        .filter(organizations::Column::Status.eq(1_i16));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(query_error)?
        .filter(|organization| organization.closed_at.is_none())
        .map(|_| ())
        .ok_or(OrganizationServiceAccountBudgetBindingRepositoryError::AccountUnavailable)
}

fn validate_account(
    account: &organization_service_accounts::Model,
) -> Result<(), OrganizationServiceAccountBudgetBindingRepositoryError> {
    if account.status != OrganizationServiceAccountStatus::Active.code()
        || account.version <= 0
        || account.revoked_at.is_some()
    {
        return Err(OrganizationServiceAccountBudgetBindingRepositoryError::AccountUnavailable);
    }
    Ok(())
}

async fn lock_policy(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    policy_id: OrganizationBudgetPolicyId,
) -> Result<
    organization_budget_policies::Model,
    OrganizationServiceAccountBudgetBindingRepositoryError,
> {
    let mut query = organization_budget_policies::Entity::find_by_id(policy_id.get())
        .filter(organization_budget_policies::Column::OrganizationId.eq(organization_id.get()));
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .await
        .map_err(query_error)?
        .ok_or(OrganizationServiceAccountBudgetBindingRepositoryError::PolicyUnavailable)
}

async fn validate_policy(
    transaction: &DatabaseTransaction,
    policy: &organization_budget_policies::Model,
    organization_id: OrganizationId,
    now: TimeDateTimeWithTimeZone,
) -> Result<
    OrganizationServiceAccountBudgetScope,
    OrganizationServiceAccountBudgetBindingRepositoryError,
> {
    if policy.organization_id != organization_id.get()
        || policy.version <= 0
        || policy.effective_from > now
        || policy.effective_until.is_some_and(|until| until <= now)
    {
        return Err(OrganizationServiceAccountBudgetBindingRepositoryError::PolicyUnavailable);
    }
    if policy.scope_kind == 3 || policy.membership_id.is_some() {
        return Err(OrganizationServiceAccountBudgetBindingRepositoryError::PolicyUnavailable);
    }
    let team_id = policy
        .team_id
        .map(OrganizationTeamId::new)
        .transpose()
        .map_err(|_| invariant())?;
    let scope = OrganizationServiceAccountBudgetScope::from_persistence(policy.scope_kind, team_id)
        .map_err(|error| match error {
            OrganizationServiceAccountBudgetBindingError::InvalidScope
            | OrganizationServiceAccountBudgetBindingError::InvalidVersion
            | OrganizationServiceAccountBudgetBindingError::OrganizationMismatch => invariant(),
        })?;
    if let Some(team_id) = scope.team_id() {
        let team = organization_teams::Entity::find_by_id(team_id.get())
            .filter(organization_teams::Column::OrganizationId.eq(organization_id.get()))
            .one(transaction)
            .await
            .map_err(query_error)?
            .ok_or(OrganizationServiceAccountBudgetBindingRepositoryError::TeamUnavailable)?;
        if team.status != 1 || team.deleted_at.is_some() {
            return Err(OrganizationServiceAccountBudgetBindingRepositoryError::TeamUnavailable);
        }
    }
    Ok(scope)
}

async fn lock_binding(
    transaction: &DatabaseTransaction,
    organization_id: OrganizationId,
    service_account_id: OrganizationServiceAccountId,
) -> Result<
    Option<organization_service_account_budget_bindings::Model>,
    OrganizationServiceAccountBudgetBindingRepositoryError,
> {
    let mut query = organization_service_account_budget_bindings::Entity::find()
        .filter(
            organization_service_account_budget_bindings::Column::OrganizationId
                .eq(organization_id.get()),
        )
        .filter(
            organization_service_account_budget_bindings::Column::ServiceAccountId
                .eq(service_account_id.get()),
        );
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query.one(transaction).await.map_err(query_error)
}

fn binding_record(
    model: organization_service_account_budget_bindings::Model,
) -> Result<
    OrganizationServiceAccountBudgetBinding,
    OrganizationServiceAccountBudgetBindingRepositoryError,
> {
    let organization_id = OrganizationId::new(model.organization_id).map_err(|_| invariant())?;
    let service_account_id =
        OrganizationServiceAccountId::new(model.service_account_id).map_err(|_| invariant())?;
    let policy_id = OrganizationBudgetPolicyId::new(model.policy_id).map_err(|_| invariant())?;
    let team_id = model
        .team_id
        .map(OrganizationTeamId::new)
        .transpose()
        .map_err(|_| invariant())?;
    let scope = OrganizationServiceAccountBudgetScope::from_persistence(model.scope_kind, team_id)
        .map_err(|_| invariant())?;
    OrganizationServiceAccountBudgetBinding::new(
        organization_id,
        service_account_id,
        policy_id,
        model.policy_version,
        scope,
        model.version,
    )
    .map_err(|_| invariant())
}

fn binding_record_with_policy(
    model: organization_service_account_budget_bindings::Model,
    policy_scope: OrganizationServiceAccountBudgetScope,
    policy_version: i64,
) -> Result<
    OrganizationServiceAccountBudgetBinding,
    OrganizationServiceAccountBudgetBindingRepositoryError,
> {
    let record = binding_record(model)?;
    if record.scope() != policy_scope || record.policy_version() != policy_version {
        return Err(OrganizationServiceAccountBudgetBindingRepositoryError::PolicyUnavailable);
    }
    Ok(record)
}

fn validate_expected_version(
    expected_version: Option<i64>,
) -> Result<(), OrganizationServiceAccountBudgetBindingRepositoryError> {
    if expected_version.is_some_and(|version| version <= 0 || version == i64::MAX) {
        return Err(OrganizationServiceAccountBudgetBindingRepositoryError::InvalidInput);
    }
    Ok(())
}

async fn commit(
    transaction: DatabaseTransaction,
) -> Result<(), OrganizationServiceAccountBudgetBindingRepositoryError> {
    transaction.commit().await.map_err(query_error)
}

async fn rollback(
    transaction: DatabaseTransaction,
) -> Result<(), OrganizationServiceAccountBudgetBindingRepositoryError> {
    transaction.rollback().await.map_err(query_error)
}

fn is_unique_violation(error: &DbErr) -> bool {
    matches!(error.sql_err(), Some(SqlErr::UniqueConstraintViolation(_)))
}

fn query_error(_error: DbErr) -> OrganizationServiceAccountBudgetBindingRepositoryError {
    OrganizationServiceAccountBudgetBindingRepositoryError::Query
}

fn invariant() -> OrganizationServiceAccountBudgetBindingRepositoryError {
    OrganizationServiceAccountBudgetBindingRepositoryError::Invariant
}
