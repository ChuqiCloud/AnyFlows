use std::{collections::BTreeSet, future::Future, pin::Pin};

use af_db::{
    GroupKeyAccessError, GroupKeyAccessRepository, MAX_GROUP_KEY_ALLOWED_USERS,
    MAX_USER_TOKEN_GROUP_PAGE_SIZE,
};
use af_domain::{GroupId, UserId};
use thiserror::Error;

use crate::{SessionPrincipal, SessionRole};

pub use af_db::{
    GroupKeyPolicyRecord as GroupKeyPolicy, GroupKeySelectionMode,
    UserTokenGroupPageRecord as UserTokenGroupPage, UserTokenGroupRecord as UserTokenGroup,
};

/// 已校验并去重的分组 Key 策略写命令。
#[derive(Debug)]
pub struct GroupKeyPolicyCommand {
    selection_mode: GroupKeySelectionMode,
    allowed_user_ids: Vec<UserId>,
}

impl GroupKeyPolicyCommand {
    /// 校验授权上限与模式约束，并按用户 ID 排序去重。
    pub fn new(
        selection_mode: GroupKeySelectionMode,
        allowed_user_ids: Vec<UserId>,
    ) -> Result<Self, GroupKeyPolicyError> {
        if allowed_user_ids.len() > MAX_GROUP_KEY_ALLOWED_USERS
            || (selection_mode != GroupKeySelectionMode::SelectedUsers
                && !allowed_user_ids.is_empty())
        {
            return Err(GroupKeyPolicyError::InvalidInput);
        }
        Ok(Self {
            selection_mode,
            allowed_user_ids: allowed_user_ids
                .into_iter()
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
        })
    }
}

/// 用户自选分组的有界 ID 游标查询。
#[derive(Clone, Copy, Debug)]
pub struct UserTokenGroupListQuery {
    after: Option<GroupId>,
    limit: usize,
}

impl UserTokenGroupListQuery {
    /// 校验页大小；游标必须为有效分组 ID。
    pub fn new(after: Option<GroupId>, limit: usize) -> Result<Self, GroupKeyPolicyError> {
        if !(1..=MAX_USER_TOKEN_GROUP_PAGE_SIZE).contains(&limit) {
            return Err(GroupKeyPolicyError::InvalidInput);
        }
        Ok(Self { after, limit })
    }
}

impl Default for UserTokenGroupListQuery {
    fn default() -> Self {
        Self {
            after: None,
            limit: 50,
        }
    }
}

/// 策略应用端口可公开的错误分类。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum GroupKeyPolicyError {
    /// 输入超出界限或授权用户无效。
    #[error("分组 Key 策略输入无效")]
    InvalidInput,
    /// 当前会话不是管理员。
    #[error("分组 Key 策略权限不足")]
    Forbidden,
    /// 分组不存在或已删除。
    #[error("分组不存在")]
    NotFound,
    /// 当前用户已失效。
    #[error("登录会话已失效")]
    OwnerUnavailable,
    /// 数据库操作失败。
    #[error("分组 Key 策略内部失败")]
    Internal,
}

/// 策略查询和替换的对象安全 Future。
pub type GroupKeyPolicyFuture<'a> =
    Pin<Box<dyn Future<Output = Result<GroupKeyPolicy, GroupKeyPolicyError>> + Send + 'a>>;
/// 用户可选分组列表的对象安全 Future。
pub type UserTokenGroupListFuture<'a> =
    Pin<Box<dyn Future<Output = Result<UserTokenGroupPage, GroupKeyPolicyError>> + Send + 'a>>;

/// 个人 Key 的分组选择策略应用端口。
pub trait GroupKeyAccessService: Send + Sync {
    /// 管理员读取策略。
    fn get_policy<'a>(
        &'a self,
        principal: SessionPrincipal,
        group_id: GroupId,
    ) -> GroupKeyPolicyFuture<'a>;
    /// 管理员完整替换策略与授权。
    fn update_policy<'a>(
        &'a self,
        principal: SessionPrincipal,
        group_id: GroupId,
        command: GroupKeyPolicyCommand,
    ) -> GroupKeyPolicyFuture<'a>;
    /// 当前登录用户读取可选分组。
    fn list_user_groups<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: UserTokenGroupListQuery,
    ) -> UserTokenGroupListFuture<'a>;
}

/// 使用数据库实现分组自选策略；管理员校验在访问仓储前完成。
pub struct DatabaseGroupKeyAccessService {
    repository: GroupKeyAccessRepository,
}

impl DatabaseGroupKeyAccessService {
    /// 绑定已配置截止时间的仓储。
    #[must_use]
    pub const fn new(repository: GroupKeyAccessRepository) -> Self {
        Self { repository }
    }
}

impl GroupKeyAccessService for DatabaseGroupKeyAccessService {
    fn get_policy<'a>(
        &'a self,
        principal: SessionPrincipal,
        group_id: GroupId,
    ) -> GroupKeyPolicyFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            self.repository
                .get_policy(group_id)
                .await
                .map_err(map_error)
        })
    }

    fn update_policy<'a>(
        &'a self,
        principal: SessionPrincipal,
        group_id: GroupId,
        command: GroupKeyPolicyCommand,
    ) -> GroupKeyPolicyFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            self.repository
                .update_policy(group_id, command.selection_mode, command.allowed_user_ids)
                .await
                .map_err(map_error)
        })
    }

    fn list_user_groups<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: UserTokenGroupListQuery,
    ) -> UserTokenGroupListFuture<'a> {
        Box::pin(async move {
            self.repository
                .list_user_groups(principal.user_id(), query.after, query.limit)
                .await
                .map_err(map_error)
        })
    }
}

fn require_admin(principal: SessionPrincipal) -> Result<(), GroupKeyPolicyError> {
    if principal.role() != SessionRole::Admin {
        return Err(GroupKeyPolicyError::Forbidden);
    }
    Ok(())
}

fn map_error(error: GroupKeyAccessError) -> GroupKeyPolicyError {
    match error {
        GroupKeyAccessError::InvalidInput => GroupKeyPolicyError::InvalidInput,
        GroupKeyAccessError::NotFound => GroupKeyPolicyError::NotFound,
        GroupKeyAccessError::OwnerUnavailable => GroupKeyPolicyError::OwnerUnavailable,
        GroupKeyAccessError::Internal => GroupKeyPolicyError::Internal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_rejects_ambiguous_grants_and_deduplicates_selected_users() {
        let user = UserId::new(1).unwrap();
        for mode in [
            GroupKeySelectionMode::AllUsers,
            GroupKeySelectionMode::DefaultOnly,
        ] {
            assert_eq!(
                GroupKeyPolicyCommand::new(mode, vec![user]).unwrap_err(),
                GroupKeyPolicyError::InvalidInput
            );
        }
        let command =
            GroupKeyPolicyCommand::new(GroupKeySelectionMode::SelectedUsers, vec![user, user])
                .unwrap();
        assert_eq!(command.allowed_user_ids, [user]);
        assert!(
            GroupKeyPolicyCommand::new(GroupKeySelectionMode::SelectedUsers, vec![user; 1_001])
                .is_err()
        );
    }

    #[test]
    fn policy_admin_boundary_and_pagination_fail_closed() {
        let user = SessionPrincipal::new(UserId::new(1).unwrap(), SessionRole::User);
        assert_eq!(require_admin(user), Err(GroupKeyPolicyError::Forbidden));
        for limit in [0, 101] {
            assert!(UserTokenGroupListQuery::new(None, limit).is_err());
        }
    }
}
