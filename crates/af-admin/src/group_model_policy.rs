//! 管理员可配置的分组模型白名单。

use std::{future::Future, pin::Pin};

use af_db::{GroupModelPolicyRecord, GroupModelPolicyRepository, GroupModelPolicyRepositoryError};
use af_domain::GroupId;
use thiserror::Error;

use crate::{SessionPrincipal, SessionRole};

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
    ) -> GroupModelPolicyFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            self.repository
                .replace(group_id, allowed_models)
                .await
                .map_err(map_error)
        })
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
