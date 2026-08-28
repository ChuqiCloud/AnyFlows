use std::{future::Future, pin::Pin, sync::Arc};

pub use af_db::OrganizationServiceAccountRecord;
use af_db::{
    MAX_ORGANIZATION_SERVICE_ACCOUNT_PAGE_SIZE, OrganizationServiceAccountIssued as DbIssued,
    OrganizationServiceAccountPageRecord, OrganizationServiceAccountRepository,
    OrganizationServiceAccountRepositoryError,
};
use af_domain::{
    OrganizationEntitlementStatus, OrganizationId, OrganizationPermission,
    OrganizationServiceAccountKey, OrganizationServiceAccountLocator,
    OrganizationServiceAccountName, OrganizationServiceAccountRuntimeIdentity,
    OrganizationServiceAccountSecret, OrganizationServiceAccountStatus,
};
use thiserror::Error;

use crate::OrganizationContext;

/// 企业服务账号目录默认页大小。
pub const DEFAULT_ORGANIZATION_SERVICE_ACCOUNT_PAGE_SIZE: usize = 50;

/// 已校验的服务账号目录查询。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationServiceAccountListQuery {
    after: Option<OrganizationServiceAccountKey>,
    limit: usize,
}

impl OrganizationServiceAccountListQuery {
    /// 校验稳定游标和页大小。
    pub fn new(
        after: Option<OrganizationServiceAccountKey>,
        limit: usize,
    ) -> Result<Self, OrganizationServiceAccountServiceError> {
        if !(1..=MAX_ORGANIZATION_SERVICE_ACCOUNT_PAGE_SIZE).contains(&limit) {
            return Err(OrganizationServiceAccountServiceError::InvalidInput);
        }
        Ok(Self { after, limit })
    }

    #[must_use]
    pub const fn after(self) -> Option<OrganizationServiceAccountKey> {
        self.after
    }

    #[must_use]
    pub const fn limit(self) -> usize {
        self.limit
    }
}

impl Default for OrganizationServiceAccountListQuery {
    fn default() -> Self {
        Self {
            after: None,
            limit: DEFAULT_ORGANIZATION_SERVICE_ACCOUNT_PAGE_SIZE,
        }
    }
}

/// 创建服务账号主体的命令；主体不绑定自然人。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrganizationServiceAccountCreateCommand {
    name: String,
}

impl OrganizationServiceAccountCreateCommand {
    #[must_use]
    pub fn new(name: String) -> Self {
        Self { name }
    }
}

/// 按随机公开标识和主体版本 CAS 变更生命周期。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationServiceAccountStatusCommand {
    service_account_key: OrganizationServiceAccountKey,
    expected_version: i64,
    target: OrganizationServiceAccountStatus,
}

impl OrganizationServiceAccountStatusCommand {
    pub fn new(
        service_account_key: OrganizationServiceAccountKey,
        expected_version: i64,
        target: OrganizationServiceAccountStatus,
    ) -> Result<Self, OrganizationServiceAccountServiceError> {
        validate_version(expected_version)?;
        Ok(Self {
            service_account_key,
            expected_version,
            target,
        })
    }
}

/// 按随机公开标识和主体版本 CAS 原子轮换密钥。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrganizationServiceAccountRotateCommand {
    service_account_key: OrganizationServiceAccountKey,
    expected_version: i64,
}

impl OrganizationServiceAccountRotateCommand {
    pub fn new(
        service_account_key: OrganizationServiceAccountKey,
        expected_version: i64,
    ) -> Result<Self, OrganizationServiceAccountServiceError> {
        validate_version(expected_version)?;
        Ok(Self {
            service_account_key,
            expected_version,
        })
    }
}

/// 服务账号目录分页结果。
#[derive(Debug, PartialEq)]
pub struct OrganizationServiceAccountPage {
    records: Vec<OrganizationServiceAccountRecord>,
    next_cursor: Option<OrganizationServiceAccountKey>,
}

impl OrganizationServiceAccountPage {
    #[must_use]
    pub fn records(&self) -> &[OrganizationServiceAccountRecord] {
        &self.records
    }

    #[must_use]
    pub const fn next_cursor(&self) -> Option<OrganizationServiceAccountKey> {
        self.next_cursor
    }
}

/// 创建或轮换时一次性携带的密钥明文；调试输出不会泄露密钥。
pub struct OrganizationServiceAccountIssued {
    account: OrganizationServiceAccountRecord,
    key_key: String,
    key_prefix: String,
    secret: String,
}

impl OrganizationServiceAccountIssued {
    #[must_use]
    pub fn account(&self) -> &OrganizationServiceAccountRecord {
        &self.account
    }

    #[must_use]
    pub fn key_key(&self) -> &str {
        &self.key_key
    }

    #[must_use]
    pub fn key_prefix(&self) -> &str {
        &self.key_prefix
    }

    /// 拆出一次性密钥明文；调用方不得持久化或写入日志。
    #[must_use]
    pub fn into_parts(self) -> (OrganizationServiceAccountRecord, String, String, String) {
        (self.account, self.key_key, self.key_prefix, self.secret)
    }
}

impl std::fmt::Debug for OrganizationServiceAccountIssued {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("OrganizationServiceAccountIssued(<redacted>)")
    }
}

/// 企业服务账号管理错误；错误不携带主体名称或密钥材料。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationServiceAccountServiceError {
    #[error("企业服务账号请求无效")]
    InvalidInput,
    #[error("当前企业角色无权管理服务账号")]
    Forbidden,
    #[error("企业服务账号不存在")]
    NotFound,
    #[error("企业服务账号版本冲突")]
    Conflict,
    #[error("企业服务账号状态转换无效")]
    InvalidTransition,
    #[error("企业商业授权不允许服务账号")]
    EntitlementRestricted,
    #[error("企业服务账号服务内部错误")]
    Internal,
}

pub type OrganizationServiceAccountPageFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<
                    OrganizationServiceAccountPage,
                    OrganizationServiceAccountServiceError,
                >,
            > + Send
            + 'a,
    >,
>;
pub type OrganizationServiceAccountFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<
                    OrganizationServiceAccountRecord,
                    OrganizationServiceAccountServiceError,
                >,
            > + Send
            + 'a,
    >,
>;
pub type OrganizationServiceAccountIssuedFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<
                    OrganizationServiceAccountIssued,
                    OrganizationServiceAccountServiceError,
                >,
            > + Send
            + 'a,
    >,
>;

/// 运行时服务账号密钥认证的失败分类；不携带主体、摘要或密钥材料。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrganizationServiceAccountRuntimeAuthenticationError {
    #[error("服务账号凭据无效")]
    InvalidCredentials,
    #[error("服务账号认证服务内部错误")]
    Internal,
}

pub type OrganizationServiceAccountRuntimeAuthenticationFuture<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<
                    OrganizationServiceAccountRuntimeIdentity,
                    OrganizationServiceAccountRuntimeAuthenticationError,
                >,
            > + Send
            + 'a,
    >,
>;

/// 企业范围运行时服务账号认证端口；不创建管理会话或成员身份。
pub trait OrganizationServiceAccountRuntimeAuthenticator: Send + Sync {
    fn authenticate<'a>(
        &'a self,
        organization_id: OrganizationId,
        secret: OrganizationServiceAccountSecret,
    ) -> OrganizationServiceAccountRuntimeAuthenticationFuture<'a>;
}

/// 企业服务账号管理端口；与运行时认证端口保持分离，预算绑定留在后续切片。
pub trait OrganizationServiceAccountService: Send + Sync {
    fn list<'a>(
        &'a self,
        context: OrganizationContext,
        query: OrganizationServiceAccountListQuery,
    ) -> OrganizationServiceAccountPageFuture<'a>;

    fn get<'a>(
        &'a self,
        context: OrganizationContext,
        service_account_key: OrganizationServiceAccountKey,
    ) -> OrganizationServiceAccountFuture<'a>;

    fn create<'a>(
        &'a self,
        context: OrganizationContext,
        command: OrganizationServiceAccountCreateCommand,
    ) -> OrganizationServiceAccountIssuedFuture<'a>;

    fn set_status<'a>(
        &'a self,
        context: OrganizationContext,
        command: OrganizationServiceAccountStatusCommand,
    ) -> OrganizationServiceAccountFuture<'a>;

    fn rotate_key<'a>(
        &'a self,
        context: OrganizationContext,
        command: OrganizationServiceAccountRotateCommand,
    ) -> OrganizationServiceAccountIssuedFuture<'a>;
}

/// 使用数据库仓储实现企业范围、固定角色和商业授权边界。
pub struct DatabaseOrganizationServiceAccountService {
    repository: Arc<OrganizationServiceAccountRepository>,
}

impl DatabaseOrganizationServiceAccountService {
    #[must_use]
    pub fn new(repository: Arc<OrganizationServiceAccountRepository>) -> Self {
        Self { repository }
    }
}

impl OrganizationServiceAccountService for DatabaseOrganizationServiceAccountService {
    fn list<'a>(
        &'a self,
        context: OrganizationContext,
        query: OrganizationServiceAccountListQuery,
    ) -> OrganizationServiceAccountPageFuture<'a> {
        Box::pin(async move {
            require_read_access(&context)?;
            let page = self
                .repository
                .list(context.organization_id(), query.after(), query.limit())
                .await
                .map_err(map_repository_error)?;
            Ok(page_from_record(page))
        })
    }

    fn get<'a>(
        &'a self,
        context: OrganizationContext,
        service_account_key: OrganizationServiceAccountKey,
    ) -> OrganizationServiceAccountFuture<'a> {
        Box::pin(async move {
            require_read_access(&context)?;
            self.repository
                .get_by_public_key(context.organization_id(), service_account_key)
                .await
                .map_err(map_repository_error)
        })
    }

    fn create<'a>(
        &'a self,
        context: OrganizationContext,
        command: OrganizationServiceAccountCreateCommand,
    ) -> OrganizationServiceAccountIssuedFuture<'a> {
        Box::pin(async move {
            require_manage_access(&context)?;
            let name = OrganizationServiceAccountName::new(command.name)
                .map_err(|_| OrganizationServiceAccountServiceError::InvalidInput)?;
            let issued = self
                .repository
                .create(context.organization_id(), name)
                .await
                .map_err(map_repository_error)?;
            Ok(issued_from_db(issued))
        })
    }

    fn set_status<'a>(
        &'a self,
        context: OrganizationContext,
        command: OrganizationServiceAccountStatusCommand,
    ) -> OrganizationServiceAccountFuture<'a> {
        Box::pin(async move {
            require_manage_access(&context)?;
            self.repository
                .set_status(
                    OrganizationServiceAccountLocator::new(
                        context.organization_id(),
                        self.repository
                            .get_by_public_key(
                                context.organization_id(),
                                command.service_account_key,
                            )
                            .await
                            .map_err(map_repository_error)?
                            .locator()
                            .service_account_id(),
                    ),
                    command.expected_version,
                    command.target,
                )
                .await
                .map_err(map_repository_error)
        })
    }

    fn rotate_key<'a>(
        &'a self,
        context: OrganizationContext,
        command: OrganizationServiceAccountRotateCommand,
    ) -> OrganizationServiceAccountIssuedFuture<'a> {
        Box::pin(async move {
            require_manage_access(&context)?;
            let issued = self
                .repository
                .rotate_key(
                    OrganizationServiceAccountLocator::new(
                        context.organization_id(),
                        self.repository
                            .get_by_public_key(
                                context.organization_id(),
                                command.service_account_key,
                            )
                            .await
                            .map_err(map_repository_error)?
                            .locator()
                            .service_account_id(),
                    ),
                    command.expected_version,
                )
                .await
                .map_err(map_repository_error)?;
            Ok(issued_from_db(issued))
        })
    }
}

impl OrganizationServiceAccountRuntimeAuthenticator for DatabaseOrganizationServiceAccountService {
    fn authenticate<'a>(
        &'a self,
        organization_id: OrganizationId,
        secret: OrganizationServiceAccountSecret,
    ) -> OrganizationServiceAccountRuntimeAuthenticationFuture<'a> {
        Box::pin(async move {
            self.repository
                .authenticate_secret_for_organization(organization_id, secret)
                .await
                .map(|authentication| authentication.identity())
                .map_err(|error| match error {
                    OrganizationServiceAccountRepositoryError::InvalidInput
                    | OrganizationServiceAccountRepositoryError::NotFound
                    | OrganizationServiceAccountRepositoryError::Restricted => {
                        OrganizationServiceAccountRuntimeAuthenticationError::InvalidCredentials
                    }
                    OrganizationServiceAccountRepositoryError::Conflict
                    | OrganizationServiceAccountRepositoryError::Query
                    | OrganizationServiceAccountRepositoryError::Timeout
                    | OrganizationServiceAccountRepositoryError::Invariant => {
                        OrganizationServiceAccountRuntimeAuthenticationError::Internal
                    }
                })
        })
    }
}

impl std::fmt::Debug for DatabaseOrganizationServiceAccountService {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("DatabaseOrganizationServiceAccountService(<repository>)")
    }
}

fn require_read_access(
    context: &OrganizationContext,
) -> Result<(), OrganizationServiceAccountServiceError> {
    if !matches!(
        context.role(),
        af_domain::OrganizationRole::Owner | af_domain::OrganizationRole::Admin
    ) || !context.allows(OrganizationPermission::ServiceAccountsRead)
    {
        return Err(OrganizationServiceAccountServiceError::Forbidden);
    }
    if context.entitlement().status() != OrganizationEntitlementStatus::Active {
        return Err(OrganizationServiceAccountServiceError::EntitlementRestricted);
    }
    Ok(())
}

fn require_manage_access(
    context: &OrganizationContext,
) -> Result<(), OrganizationServiceAccountServiceError> {
    if !matches!(
        context.role(),
        af_domain::OrganizationRole::Owner | af_domain::OrganizationRole::Admin
    ) || !context.allows(OrganizationPermission::ServiceAccountsManage)
    {
        return Err(OrganizationServiceAccountServiceError::Forbidden);
    }
    if context.entitlement().status() != OrganizationEntitlementStatus::Active {
        return Err(OrganizationServiceAccountServiceError::EntitlementRestricted);
    }
    Ok(())
}

fn validate_version(version: i64) -> Result<(), OrganizationServiceAccountServiceError> {
    if version <= 0 {
        return Err(OrganizationServiceAccountServiceError::InvalidInput);
    }
    Ok(())
}

fn page_from_record(page: OrganizationServiceAccountPageRecord) -> OrganizationServiceAccountPage {
    OrganizationServiceAccountPage {
        records: page.items().to_vec(),
        next_cursor: page.next_cursor(),
    }
}

fn issued_from_db(issued: DbIssued) -> OrganizationServiceAccountIssued {
    let (account, key, secret) = issued.into_parts();
    OrganizationServiceAccountIssued {
        account,
        key_key: key.key_key().persistence_key(),
        key_prefix: key.key_prefix().to_owned(),
        secret: secret.as_str().to_owned(),
    }
}

fn map_repository_error(
    error: OrganizationServiceAccountRepositoryError,
) -> OrganizationServiceAccountServiceError {
    match error {
        OrganizationServiceAccountRepositoryError::InvalidInput => {
            OrganizationServiceAccountServiceError::InvalidInput
        }
        OrganizationServiceAccountRepositoryError::NotFound => {
            OrganizationServiceAccountServiceError::NotFound
        }
        OrganizationServiceAccountRepositoryError::Conflict => {
            OrganizationServiceAccountServiceError::Conflict
        }
        OrganizationServiceAccountRepositoryError::Restricted => {
            OrganizationServiceAccountServiceError::InvalidTransition
        }
        OrganizationServiceAccountRepositoryError::Query
        | OrganizationServiceAccountRepositoryError::Timeout
        | OrganizationServiceAccountRepositoryError::Invariant => {
            OrganizationServiceAccountServiceError::Internal
        }
    }
}
