use std::{fmt, future::Future, pin::Pin};

use af_db::{
    AdminChannelLookupOutcome, AdminChannelRepository, AdminChannelRepositoryError,
    AdminCredentialLookupOutcome, AdminCredentialPageOutcome,
};
use af_domain::{ChannelId, CredentialId};
use thiserror::Error;

use crate::{
    AdminChannel, AdminChannelListQuery, AdminChannelPage, AdminCredential,
    AdminCredentialListQuery, AdminCredentialPage, SessionPrincipal, SessionRole,
};

/// 管理渠道与凭据读取失败分类。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AdminChannelReadError {
    /// 游标或页大小不满足公开边界。
    #[error("管理渠道分页参数无效")]
    InvalidPagination,
    /// 当前会话不是管理员。
    #[error("管理渠道读取权限不足")]
    Forbidden,
    /// 渠道不存在或已经软删除。
    #[error("管理渠道不存在")]
    ChannelNotFound,
    /// 凭据不存在、已软删除或不属于指定渠道。
    #[error("管理渠道凭据不存在")]
    CredentialNotFound,
    /// 数据库失败或持久化状态损坏。
    #[error("管理渠道读取内部失败")]
    Internal,
}

/// 管理渠道列表调用的对象安全 Future。
pub type AdminChannelListFuture<'a> =
    Pin<Box<dyn Future<Output = Result<AdminChannelPage, AdminChannelReadError>> + Send + 'a>>;
/// 管理渠道详情调用的对象安全 Future。
pub type AdminChannelGetFuture<'a> =
    Pin<Box<dyn Future<Output = Result<AdminChannel, AdminChannelReadError>> + Send + 'a>>;
/// 管理凭据列表调用的对象安全 Future。
pub type AdminCredentialListFuture<'a> =
    Pin<Box<dyn Future<Output = Result<AdminCredentialPage, AdminChannelReadError>> + Send + 'a>>;
/// 管理凭据详情调用的对象安全 Future。
pub type AdminCredentialGetFuture<'a> =
    Pin<Box<dyn Future<Output = Result<AdminCredential, AdminChannelReadError>> + Send + 'a>>;

/// 管理渠道及其凭据的只读应用端口；角色校验必须在进入仓储前完成。
pub trait AdminChannelReader: Send + Sync {
    /// 读取一页渠道。
    fn list_channels<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: AdminChannelListQuery,
    ) -> AdminChannelListFuture<'a>;
    /// 按渠道 ID 读取详情。
    fn get_channel<'a>(
        &'a self,
        principal: SessionPrincipal,
        channel_id: ChannelId,
    ) -> AdminChannelGetFuture<'a>;
    /// 读取指定渠道下的一页凭据元数据。
    fn list_credentials<'a>(
        &'a self,
        principal: SessionPrincipal,
        channel_id: ChannelId,
        query: AdminCredentialListQuery,
    ) -> AdminCredentialListFuture<'a>;
    /// 读取指定渠道下的单个凭据元数据。
    fn get_credential<'a>(
        &'a self,
        principal: SessionPrincipal,
        channel_id: ChannelId,
        credential_id: CredentialId,
    ) -> AdminCredentialGetFuture<'a>;
}

/// 使用数据库仓储实现管理员渠道与凭据读取。
pub struct DatabaseAdminChannelReader {
    repository: AdminChannelRepository,
}

impl DatabaseAdminChannelReader {
    /// 绑定已配置查询截止时间的渠道仓储。
    #[must_use]
    pub const fn new(repository: AdminChannelRepository) -> Self {
        Self { repository }
    }
}

impl AdminChannelReader for DatabaseAdminChannelReader {
    fn list_channels<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: AdminChannelListQuery,
    ) -> AdminChannelListFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            let page = self
                .repository
                .list(query.after(), query.limit())
                .await
                .map_err(map_repository_error)?;
            let (records, next_cursor) = page.into_parts();
            Ok(AdminChannelPage::from_parts(
                records.into_iter().map(AdminChannel::from_record).collect(),
                next_cursor,
            ))
        })
    }

    fn get_channel<'a>(
        &'a self,
        principal: SessionPrincipal,
        channel_id: ChannelId,
    ) -> AdminChannelGetFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            match self
                .repository
                .get(channel_id)
                .await
                .map_err(map_repository_error)?
            {
                AdminChannelLookupOutcome::Found(record) => Ok(AdminChannel::from_record(*record)),
                AdminChannelLookupOutcome::NotFound => Err(AdminChannelReadError::ChannelNotFound),
            }
        })
    }

    fn list_credentials<'a>(
        &'a self,
        principal: SessionPrincipal,
        channel_id: ChannelId,
        query: AdminCredentialListQuery,
    ) -> AdminCredentialListFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            match self
                .repository
                .list_credentials(channel_id, query.after(), query.limit())
                .await
                .map_err(map_repository_error)?
            {
                AdminCredentialPageOutcome::Found(page) => {
                    let (records, next_cursor) = page.into_parts();
                    let credentials = records
                        .into_iter()
                        .map(AdminCredential::from_record)
                        .collect::<Result<Vec<_>, _>>()?;
                    Ok(AdminCredentialPage::from_parts(credentials, next_cursor))
                }
                AdminCredentialPageOutcome::ChannelNotFound => {
                    Err(AdminChannelReadError::ChannelNotFound)
                }
            }
        })
    }

    fn get_credential<'a>(
        &'a self,
        principal: SessionPrincipal,
        channel_id: ChannelId,
        credential_id: CredentialId,
    ) -> AdminCredentialGetFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            match self
                .repository
                .get_credential(channel_id, credential_id)
                .await
                .map_err(map_repository_error)?
            {
                AdminCredentialLookupOutcome::Found(record) => {
                    AdminCredential::from_record(*record)
                }
                AdminCredentialLookupOutcome::ChannelNotFound => {
                    Err(AdminChannelReadError::ChannelNotFound)
                }
                AdminCredentialLookupOutcome::CredentialNotFound => {
                    Err(AdminChannelReadError::CredentialNotFound)
                }
            }
        })
    }
}

impl fmt::Debug for DatabaseAdminChannelReader {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseAdminChannelReader(<redacted>)")
    }
}

fn require_admin(principal: SessionPrincipal) -> Result<(), AdminChannelReadError> {
    (principal.role() == SessionRole::Admin)
        .then_some(())
        .ok_or(AdminChannelReadError::Forbidden)
}

fn map_repository_error(error: AdminChannelRepositoryError) -> AdminChannelReadError {
    let _ = error;
    AdminChannelReadError::Internal
}

#[cfg(test)]
mod tests {
    use super::*;
    use af_domain::UserId;

    #[test]
    fn pagination_and_role_boundaries_are_closed() {
        assert_eq!(AdminChannelListQuery::default().limit(), 50);
        assert_eq!(AdminCredentialListQuery::default().limit(), 50);
        assert_eq!(
            AdminChannelListQuery::new(None, 0),
            Err(AdminChannelReadError::InvalidPagination)
        );
        assert_eq!(
            AdminCredentialListQuery::new(None, af_db::MAX_ADMIN_CHANNEL_PAGE_SIZE + 1),
            Err(AdminChannelReadError::InvalidPagination),
        );
        let principal = SessionPrincipal::new(UserId::new(1).unwrap(), SessionRole::User);
        assert_eq!(
            require_admin(principal),
            Err(AdminChannelReadError::Forbidden)
        );
    }
}
