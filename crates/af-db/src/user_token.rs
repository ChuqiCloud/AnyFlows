use std::{fmt, time::Duration};

use af_domain::{TokenId, UserId};
use tokio::time::timeout;

use crate::DatabasePool;

mod mutation;
mod read;
mod types;

pub use types::{
    MAX_USER_TOKEN_PAGE_SIZE, MAX_USER_TOKENS_PER_USER, UserTokenCreateRecord,
    UserTokenDeleteOutcome, UserTokenLookupOutcome, UserTokenMutationOutcome, UserTokenPageRecord,
    UserTokenRecord, UserTokenRepositoryConfigError, UserTokenRepositoryError,
    UserTokenWriteRecord,
};

/// 普通用户 API Key 的所有者范围仓储。
#[derive(Clone)]
pub struct UserTokenRepository {
    pub(super) pool: DatabasePool,
    pub(super) lookup_timeout: Duration,
}

impl UserTokenRepository {
    /// 使用共享连接池和单次操作截止时间构造仓储。
    pub fn new(
        pool: DatabasePool,
        lookup_timeout: Duration,
    ) -> Result<Self, UserTokenRepositoryConfigError> {
        if lookup_timeout.is_zero() {
            return Err(UserTokenRepositoryConfigError::ZeroLookupTimeout);
        }
        Ok(Self {
            pool,
            lookup_timeout,
        })
    }

    /// 按所有者和单调 ID 游标读取一页未软删除 Key。
    pub async fn list(
        &self,
        owner_user_id: UserId,
        after: Option<TokenId>,
        limit: usize,
    ) -> Result<UserTokenPageRecord, UserTokenRepositoryError> {
        if !(1..=MAX_USER_TOKEN_PAGE_SIZE).contains(&limit) {
            return Err(UserTokenRepositoryError::InvalidInput);
        }
        match timeout(
            self.lookup_timeout,
            read::list(self, owner_user_id, after, limit),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(UserTokenRepositoryError::Timeout)),
        }
    }

    /// 按所有者和稳定 ID 读取一个未软删除 Key。
    pub async fn get(
        &self,
        owner_user_id: UserId,
        token_id: TokenId,
    ) -> Result<UserTokenLookupOutcome, UserTokenRepositoryError> {
        match timeout(
            self.lookup_timeout,
            read::get(self, owner_user_id, token_id),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(UserTokenRepositoryError::Timeout)),
        }
    }

    /// 为当前所有者签发一个 Key，并原子执行容量检查。
    pub async fn create(
        &self,
        record: UserTokenCreateRecord,
    ) -> Result<UserTokenRecord, UserTokenRepositoryError> {
        match timeout(self.lookup_timeout, mutation::create(self, record)).await {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(UserTokenRepositoryError::Timeout)),
        }
    }

    /// 更新当前所有者可控字段，保留管理字段、密钥和累计使用状态。
    pub async fn update(
        &self,
        owner_user_id: UserId,
        token_id: TokenId,
        record: UserTokenWriteRecord,
    ) -> Result<UserTokenMutationOutcome, UserTokenRepositoryError> {
        match timeout(
            self.lookup_timeout,
            mutation::update(self, owner_user_id, token_id, record),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(UserTokenRepositoryError::Timeout)),
        }
    }

    /// 软删除当前所有者的 Key，跨用户 ID 统一视为未找到。
    pub async fn delete(
        &self,
        owner_user_id: UserId,
        token_id: TokenId,
    ) -> Result<UserTokenDeleteOutcome, UserTokenRepositoryError> {
        match timeout(
            self.lookup_timeout,
            mutation::delete(self, owner_user_id, token_id),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(UserTokenRepositoryError::Timeout)),
        }
    }
}

impl fmt::Debug for UserTokenRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("UserTokenRepository")
            .field("lookup_timeout", &self.lookup_timeout)
            .finish_non_exhaustive()
    }
}

/// 记录闭合错误类别，禁止令牌标识、白名单和密钥材料进入日志。
pub(super) fn record_internal_error(error: UserTokenRepositoryError) -> UserTokenRepositoryError {
    let error_kind = match error {
        UserTokenRepositoryError::Query => "user_token_query",
        UserTokenRepositoryError::Timeout => "user_token_timeout",
        UserTokenRepositoryError::Invariant => "user_token_invariant",
        UserTokenRepositoryError::InvalidInput
        | UserTokenRepositoryError::OwnerUnavailable
        | UserTokenRepositoryError::LimitReached => return error,
    };
    tracing::error!(target: "af_db::user_token", error_kind, "用户 API Key 仓储发生内部错误");
    error
}
