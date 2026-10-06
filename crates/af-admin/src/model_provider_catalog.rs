use std::{fmt, future::Future, pin::Pin};

use af_db::{
    ModelProviderCatalogRecord, ModelProviderCatalogRepository,
    ModelProviderCatalogRepositoryError, ModelProviderCatalogWriteRecord,
};
use af_domain::PlatformPermission;
use thiserror::Error;

use crate::{PlatformPolicy, SessionPrincipal};

/// 管理端与模型/渠道选择器共用的厂商目录投影。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminModelProvider {
    provider_key: String,
    display_name: String,
    logo: Option<String>,
    aliases: Vec<String>,
    enabled: bool,
    sort_order: i32,
    version: i64,
}

impl AdminModelProvider {
    fn from_record(record: &ModelProviderCatalogRecord) -> Self {
        Self {
            provider_key: record.provider_key().to_owned(),
            display_name: record.display_name().to_owned(),
            logo: record.logo().map(str::to_owned),
            aliases: record.aliases().to_vec(),
            enabled: record.enabled(),
            sort_order: record.sort_order(),
            version: record.version(),
        }
    }

    pub fn provider_key(&self) -> &str {
        &self.provider_key
    }
    pub fn display_name(&self) -> &str {
        &self.display_name
    }
    pub fn logo(&self) -> Option<&str> {
        self.logo.as_deref()
    }
    pub fn aliases(&self) -> &[String] {
        &self.aliases
    }
    pub const fn enabled(&self) -> bool {
        self.enabled
    }
    pub const fn sort_order(&self) -> i32 {
        self.sort_order
    }
    pub const fn version(&self) -> i64 {
        self.version
    }
}

pub struct AdminModelProviderCommand {
    pub expected_version: i64,
    pub display_name: String,
    pub logo: Option<String>,
    pub aliases: Vec<String>,
    pub enabled: bool,
    pub sort_order: i32,
}

impl fmt::Debug for AdminModelProviderCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminModelProviderCommand(<validated>)")
    }
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum AdminModelProviderError {
    #[error("模型厂商目录输入无效")]
    InvalidInput,
    #[error("模型厂商目录权限不足")]
    Forbidden,
    #[error("模型厂商目录不存在")]
    NotFound,
    #[error("模型厂商目录版本冲突")]
    Conflict,
    #[error("模型厂商目录服务内部失败")]
    Internal,
}

pub type AdminModelProviderListFuture<'a> = Pin<
    Box<dyn Future<Output = Result<Vec<AdminModelProvider>, AdminModelProviderError>> + Send + 'a>,
>;
pub type AdminModelProviderGetFuture<'a> =
    Pin<Box<dyn Future<Output = Result<AdminModelProvider, AdminModelProviderError>> + Send + 'a>>;
pub type AdminModelProviderWriteFuture<'a> =
    Pin<Box<dyn Future<Output = Result<AdminModelProvider, AdminModelProviderError>> + Send + 'a>>;
pub type AdminModelProviderDeleteFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), AdminModelProviderError>> + Send + 'a>>;

pub trait AdminModelProviderCatalogService: Send + Sync {
    fn list_public(&self) -> AdminModelProviderListFuture<'_>;
    fn list(&self, principal: SessionPrincipal) -> AdminModelProviderListFuture<'_>;
    fn get(
        &self,
        principal: SessionPrincipal,
        provider_key: String,
    ) -> AdminModelProviderGetFuture<'_>;
    fn save(
        &self,
        principal: SessionPrincipal,
        provider_key: String,
        command: AdminModelProviderCommand,
    ) -> AdminModelProviderWriteFuture<'_>;
    fn delete(
        &self,
        principal: SessionPrincipal,
        provider_key: String,
        expected_version: i64,
    ) -> AdminModelProviderDeleteFuture<'_>;
}

pub struct DatabaseAdminModelProviderCatalogService {
    repository: ModelProviderCatalogRepository,
}

impl DatabaseAdminModelProviderCatalogService {
    #[must_use]
    pub const fn new(repository: ModelProviderCatalogRepository) -> Self {
        Self { repository }
    }
}

impl AdminModelProviderCatalogService for DatabaseAdminModelProviderCatalogService {
    fn list_public(&self) -> AdminModelProviderListFuture<'_> {
        Box::pin(async move {
            self.repository
                .providers()
                .await
                // 返回完整的覆盖目录，让前端能够同步处理管理员停用的内置厂商。
                // 是否展示由客户端依据 enabled 字段决定。
                .map(|records| records.iter().map(AdminModelProvider::from_record).collect())
                .map_err(map_repository_error)
        })
    }

    fn list(&self, principal: SessionPrincipal) -> AdminModelProviderListFuture<'_> {
        Box::pin(async move {
            require_permission(principal)?;
            self.repository
                .providers()
                .await
                .map(|records| {
                    records
                        .iter()
                        .map(AdminModelProvider::from_record)
                        .collect()
                })
                .map_err(map_repository_error)
        })
    }

    fn get(
        &self,
        principal: SessionPrincipal,
        provider_key: String,
    ) -> AdminModelProviderGetFuture<'_> {
        Box::pin(async move {
            require_permission(principal)?;
            self.repository
                .provider(&provider_key)
                .await
                .map_err(map_repository_error)?
                .as_ref()
                .map(AdminModelProvider::from_record)
                .ok_or(AdminModelProviderError::NotFound)
        })
    }

    fn save(
        &self,
        principal: SessionPrincipal,
        provider_key: String,
        command: AdminModelProviderCommand,
    ) -> AdminModelProviderWriteFuture<'_> {
        Box::pin(async move {
            require_permission(principal)?;
            self.repository
                .save_provider(
                    &provider_key,
                    ModelProviderCatalogWriteRecord {
                        expected_version: command.expected_version,
                        display_name: command.display_name,
                        logo: command.logo,
                        aliases: command.aliases,
                        enabled: command.enabled,
                        sort_order: command.sort_order,
                    },
                )
                .await
                .map(|record| AdminModelProvider::from_record(&record))
                .map_err(map_repository_error)
        })
    }

    fn delete(
        &self,
        principal: SessionPrincipal,
        provider_key: String,
        expected_version: i64,
    ) -> AdminModelProviderDeleteFuture<'_> {
        Box::pin(async move {
            require_permission(principal)?;
            self.repository
                .delete_provider(&provider_key, expected_version)
                .await
                .map_err(map_repository_error)
        })
    }
}

impl fmt::Debug for DatabaseAdminModelProviderCatalogService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseAdminModelProviderCatalogService")
    }
}

fn require_permission(principal: SessionPrincipal) -> Result<(), AdminModelProviderError> {
    PlatformPolicy::allows(principal, PlatformPermission::ModelProvidersManage)
        .then_some(())
        .ok_or(AdminModelProviderError::Forbidden)
}

fn map_repository_error(error: ModelProviderCatalogRepositoryError) -> AdminModelProviderError {
    match error {
        ModelProviderCatalogRepositoryError::InvalidInput => AdminModelProviderError::InvalidInput,
        ModelProviderCatalogRepositoryError::ConcurrentUpdate => AdminModelProviderError::Conflict,
        ModelProviderCatalogRepositoryError::Query
        | ModelProviderCatalogRepositoryError::Timeout
        | ModelProviderCatalogRepositoryError::Invariant => AdminModelProviderError::Internal,
    }
}
