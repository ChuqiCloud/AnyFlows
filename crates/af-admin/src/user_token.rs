use std::{fmt, future::Future, pin::Pin};

use af_db::{
    UserTokenCreateRecord, UserTokenDeleteOutcome, UserTokenLookupOutcome,
    UserTokenMutationOutcome, UserTokenRepository, UserTokenRepositoryError,
};
use af_domain::TokenId;

use crate::{IssuedApiKey, SessionPrincipal};

mod types;

pub use types::{
    DEFAULT_USER_TOKEN_PAGE_SIZE, IssuedUserToken, MAX_USER_TOKENS_PER_USER, UserToken,
    UserTokenError, UserTokenListQuery, UserTokenPage, UserTokenStatus, UserTokenWriteCommand,
};

/// 用户 Key 列表调用的对象安全 Future。
pub type UserTokenListFuture<'a> =
    Pin<Box<dyn Future<Output = Result<UserTokenPage, UserTokenError>> + Send + 'a>>;
/// 用户 Key 详情调用的对象安全 Future。
pub type UserTokenGetFuture<'a> =
    Pin<Box<dyn Future<Output = Result<UserToken, UserTokenError>> + Send + 'a>>;
/// 用户 Key 签发调用的对象安全 Future。
pub type UserTokenCreateFuture<'a> =
    Pin<Box<dyn Future<Output = Result<IssuedUserToken, UserTokenError>> + Send + 'a>>;
/// 用户 Key 更新调用的对象安全 Future。
pub type UserTokenUpdateFuture<'a> =
    Pin<Box<dyn Future<Output = Result<UserToken, UserTokenError>> + Send + 'a>>;
/// 用户 Key 删除调用的对象安全 Future。
pub type UserTokenDeleteFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), UserTokenError>> + Send + 'a>>;

/// 当前登录用户自助管理 API Key 的应用端口。
pub trait UserTokenService: Send + Sync {
    /// 读取当前用户的一页 Key。
    fn list(
        &self,
        principal: SessionPrincipal,
        query: UserTokenListQuery,
    ) -> UserTokenListFuture<'_>;

    /// 读取当前用户拥有的一个 Key。
    fn get(&self, principal: SessionPrincipal, token_id: TokenId) -> UserTokenGetFuture<'_>;

    /// 为当前用户签发 Key，完整明文只随本次结果返回。
    fn create(
        &self,
        principal: SessionPrincipal,
        command: UserTokenWriteCommand,
    ) -> UserTokenCreateFuture<'_>;

    /// 更新当前用户可控字段，不覆盖管理员配置。
    fn update(
        &self,
        principal: SessionPrincipal,
        token_id: TokenId,
        command: UserTokenWriteCommand,
    ) -> UserTokenUpdateFuture<'_>;

    /// 软删除当前用户拥有的 Key。
    fn delete(&self, principal: SessionPrincipal, token_id: TokenId) -> UserTokenDeleteFuture<'_>;
}

/// 使用所有者范围数据库仓储实现用户 Key 服务。
pub struct DatabaseUserTokenService {
    repository: UserTokenRepository,
}

impl DatabaseUserTokenService {
    /// 绑定已经配置截止时间和容量边界的仓储。
    #[must_use]
    pub const fn new(repository: UserTokenRepository) -> Self {
        Self { repository }
    }
}

impl UserTokenService for DatabaseUserTokenService {
    fn list(
        &self,
        principal: SessionPrincipal,
        query: UserTokenListQuery,
    ) -> UserTokenListFuture<'_> {
        Box::pin(async move {
            let page = self
                .repository
                .list(principal.user_id(), query.after(), query.limit())
                .await
                .map_err(map_repository_error)?;
            let (records, next_cursor) = page.into_parts();
            let tokens = records
                .into_iter()
                .map(UserToken::from_record)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(UserTokenPage::from_parts(tokens, next_cursor))
        })
    }

    fn get(&self, principal: SessionPrincipal, token_id: TokenId) -> UserTokenGetFuture<'_> {
        Box::pin(async move {
            match self
                .repository
                .get(principal.user_id(), token_id)
                .await
                .map_err(map_repository_error)?
            {
                UserTokenLookupOutcome::Found(record) => UserToken::from_record(*record),
                UserTokenLookupOutcome::NotFound => Err(UserTokenError::NotFound),
            }
        })
    }

    fn create(
        &self,
        principal: SessionPrincipal,
        command: UserTokenWriteCommand,
    ) -> UserTokenCreateFuture<'_> {
        Box::pin(async move {
            let issued = IssuedApiKey::generate().map_err(|_| UserTokenError::Internal)?;
            let record = UserTokenCreateRecord::new(
                principal.user_id(),
                issued.digest().as_str().to_owned(),
                issued.display_prefix().as_str().to_owned(),
                command.into_record(),
            );
            let token = self
                .repository
                .create(record)
                .await
                .map_err(map_repository_error)
                .and_then(UserToken::from_record)?;
            Ok(IssuedUserToken::from_parts(token, issued))
        })
    }

    fn update(
        &self,
        principal: SessionPrincipal,
        token_id: TokenId,
        command: UserTokenWriteCommand,
    ) -> UserTokenUpdateFuture<'_> {
        Box::pin(async move {
            match self
                .repository
                .update(principal.user_id(), token_id, command.into_record())
                .await
                .map_err(map_repository_error)?
            {
                UserTokenMutationOutcome::Mutated(record) => UserToken::from_record(*record),
                UserTokenMutationOutcome::NotFound => Err(UserTokenError::NotFound),
            }
        })
    }

    fn delete(&self, principal: SessionPrincipal, token_id: TokenId) -> UserTokenDeleteFuture<'_> {
        Box::pin(async move {
            match self
                .repository
                .delete(principal.user_id(), token_id)
                .await
                .map_err(map_repository_error)?
            {
                UserTokenDeleteOutcome::Deleted => Ok(()),
                UserTokenDeleteOutcome::NotFound => Err(UserTokenError::NotFound),
            }
        })
    }
}

impl fmt::Debug for DatabaseUserTokenService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseUserTokenService(<redacted>)")
    }
}

fn map_repository_error(error: UserTokenRepositoryError) -> UserTokenError {
    match error {
        UserTokenRepositoryError::InvalidInput => UserTokenError::InvalidInput,
        UserTokenRepositoryError::OwnerUnavailable => UserTokenError::InvalidSession,
        UserTokenRepositoryError::LimitReached => UserTokenError::LimitReached,
        UserTokenRepositoryError::Query
        | UserTokenRepositoryError::Timeout
        | UserTokenRepositoryError::Invariant => UserTokenError::Internal,
    }
}
