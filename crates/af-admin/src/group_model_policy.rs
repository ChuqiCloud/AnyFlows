//! 管理员可配置的分组模型白名单。

use std::{fmt, future::Future, pin::Pin, sync::Arc};

use af_db::{
    GroupModelPolicyRecord, GroupModelPolicyRepository, GroupModelPolicyRepositoryError,
    GroupModelRatioOverridePolicyRecord,
};
use af_domain::GroupId;
use thiserror::Error;

use crate::{GroupPricingRuntimeRefresher, SessionPrincipal, SessionRole};

/// 分组模型策略应用端口错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum GroupModelPolicyError {
    /// 白名单不符合模型名称或容量契约。
    #[error("分组模型白名单无效")]
    InvalidInput,
    /// 当前会话不是管理员。
    #[error("分组模型策略权限不足")]
    Forbidden,
    /// 分组不存在或已删除。
    #[error("分组不存在")]
    NotFound,
    /// 数据库操作失败。
    #[error("分组模型策略内部失败")]
    Internal,
    /// 数据库已写入但运行时计费快照刷新失败。
    #[error("分组模型策略运行时刷新失败")]
    RuntimeRefreshFailed,
}

/// 分组模型策略操作的对象安全 Future。
pub type GroupModelPolicyFuture<'a> = Pin<
    Box<dyn Future<Output = Result<GroupModelPolicyRecord, GroupModelPolicyError>> + Send + 'a>,
>;

/// 分组模型访问策略的应用端口。
pub trait GroupModelPolicyService: Send + Sync {
    /// 管理员读取当前策略；`None` 表示不限模型。
    fn get<'a>(
        &'a self,
        principal: SessionPrincipal,
        group_id: GroupId,
    ) -> GroupModelPolicyFuture<'a>;

    /// 管理员整体替换模型白名单；`None` 清除限制。
    fn replace<'a>(
        &'a self,
        principal: SessionPrincipal,
        group_id: GroupId,
        allowed_models: Option<Vec<String>>,
        ratio_overrides: Option<Vec<GroupModelRatioOverridePolicyRecord>>,
    ) -> GroupModelPolicyFuture<'a>;
}

/// 基于数据库仓储的分组模型策略应用服务。
pub struct DatabaseGroupModelPolicyService {
    repository: GroupModelPolicyRepository,
}

impl DatabaseGroupModelPolicyService {
    /// 绑定已配置截止时间的仓储。
    #[must_use]
    pub const fn new(repository: GroupModelPolicyRepository) -> Self {
        Self { repository }
    }
}

impl GroupModelPolicyService for DatabaseGroupModelPolicyService {
    fn get<'a>(
        &'a self,
        principal: SessionPrincipal,
        group_id: GroupId,
    ) -> GroupModelPolicyFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            self.repository.get(group_id).await.map_err(map_error)
        })
    }

    fn replace<'a>(
        &'a self,
        principal: SessionPrincipal,
        group_id: GroupId,
        allowed_models: Option<Vec<String>>,
        ratio_overrides: Option<Vec<GroupModelRatioOverridePolicyRecord>>,
    ) -> GroupModelPolicyFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            self.repository
                .replace_with_ratio_overrides(group_id, allowed_models, ratio_overrides)
                .await
                .map_err(map_error)
        })
    }
}

/// 在模型策略持久化成功后刷新运行时分组计费快照的装饰器。
pub struct RuntimeRefreshingGroupModelPolicyService {
    service: Arc<dyn GroupModelPolicyService>,
    refresher: Arc<dyn GroupPricingRuntimeRefresher>,
}

impl RuntimeRefreshingGroupModelPolicyService {
    /// 包装基础策略服务，保持数据库事务与运行时快照发布职责分离。
    #[must_use]
    pub fn new(
        service: Arc<dyn GroupModelPolicyService>,
        refresher: Arc<dyn GroupPricingRuntimeRefresher>,
    ) -> Self {
        Self { service, refresher }
    }
}

impl GroupModelPolicyService for RuntimeRefreshingGroupModelPolicyService {
    fn get<'a>(
        &'a self,
        principal: SessionPrincipal,
        group_id: GroupId,
    ) -> GroupModelPolicyFuture<'a> {
        self.service.get(principal, group_id)
    }

    fn replace<'a>(
        &'a self,
        principal: SessionPrincipal,
        group_id: GroupId,
        allowed_models: Option<Vec<String>>,
        ratio_overrides: Option<Vec<GroupModelRatioOverridePolicyRecord>>,
    ) -> GroupModelPolicyFuture<'a> {
        Box::pin(async move {
            let policy = self
                .service
                .replace(principal, group_id, allowed_models, ratio_overrides)
                .await?;
            self.refresher
                .refresh()
                .await
                .map_err(|_| GroupModelPolicyError::RuntimeRefreshFailed)?;
            Ok(policy)
        })
    }
}

impl fmt::Debug for RuntimeRefreshingGroupModelPolicyService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RuntimeRefreshingGroupModelPolicyService(<受控>)")
    }
}

fn require_admin(principal: SessionPrincipal) -> Result<(), GroupModelPolicyError> {
    if principal.role() != SessionRole::Admin {
        return Err(GroupModelPolicyError::Forbidden);
    }
    Ok(())
}

fn map_error(error: GroupModelPolicyRepositoryError) -> GroupModelPolicyError {
    match error {
        GroupModelPolicyRepositoryError::InvalidInput => GroupModelPolicyError::InvalidInput,
        GroupModelPolicyRepositoryError::NotFound => GroupModelPolicyError::NotFound,
        GroupModelPolicyRepositoryError::Internal => GroupModelPolicyError::Internal,
    }
}
