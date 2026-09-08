use std::{fmt, time::Duration};

use af_domain::UserId;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{DatabasePool, entity::TokenHash};

mod mutation;
mod read;
mod types;

pub use types::{
    MAX_ACTIVE_PLAYGROUND_SHARES_PER_USER, MAX_PLAYGROUND_SHARE_SNAPSHOT_BYTES,
    PlaygroundShareCreatedRecord, PlaygroundShareRecord, PlaygroundShareRepositoryConfigError,
    PlaygroundShareRepositoryError, PlaygroundShareRevokeOutcome, PlaygroundShareWrite,
};

/// Playground 只读分享的创建、公开读取和所有者撤销仓储。
#[derive(Clone)]
pub struct PlaygroundShareRepository {
    pub(super) pool: DatabasePool,
    operation_timeout: Duration,
}

impl PlaygroundShareRepository {
    /// 使用共享数据库连接池和非零单次操作截止时间构造仓储。
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, PlaygroundShareRepositoryConfigError> {
        if operation_timeout.is_zero() {
            return Err(PlaygroundShareRepositoryConfigError::ZeroOperationTimeout);
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }

    /// 原子清理当前用户的失效记录、检查容量并创建不可变快照。
    pub async fn create(
        &self,
        write: PlaygroundShareWrite,
    ) -> Result<PlaygroundShareCreatedRecord, PlaygroundShareRepositoryError> {
        let operation = mutation::create(self, write).with_subscriber(NoSubscriber::default());
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(
                PlaygroundShareRepositoryError::Timeout,
            )),
        }
    }

    /// 按令牌摘要公开读取仍在有效期且未撤销的快照。
    pub async fn find_active(
        &self,
        token_hash: &str,
    ) -> Result<Option<PlaygroundShareRecord>, PlaygroundShareRepositoryError> {
        let Ok(token_hash) = TokenHash::parse(token_hash) else {
            return Ok(None);
        };
        let operation =
            read::find_active(self, token_hash).with_subscriber(NoSubscriber::default());
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(
                PlaygroundShareRepositoryError::Timeout,
            )),
        }
    }

    /// 仅允许创建者撤销仍有效的分享，其余状态统一返回未找到。
    pub async fn revoke(
        &self,
        owner_user_id: UserId,
        token_hash: &str,
    ) -> Result<PlaygroundShareRevokeOutcome, PlaygroundShareRepositoryError> {
        let Ok(token_hash) = TokenHash::parse(token_hash) else {
            return Ok(PlaygroundShareRevokeOutcome::NotFound);
        };
        let operation =
            read::revoke(self, owner_user_id, token_hash).with_subscriber(NoSubscriber::default());
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(
                PlaygroundShareRepositoryError::Timeout,
            )),
        }
    }
}

impl fmt::Debug for PlaygroundShareRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PlaygroundShareRepository")
            .field("operation_timeout", &self.operation_timeout)
            .finish_non_exhaustive()
    }
}

pub(super) fn validate_snapshot(
    snapshot: &serde_json::Value,
) -> Result<(), PlaygroundShareRepositoryError> {
    if !snapshot.is_object() {
        return Err(PlaygroundShareRepositoryError::InvalidInput);
    }
    let serialized =
        serde_json::to_vec(snapshot).map_err(|_| PlaygroundShareRepositoryError::InvalidInput)?;
    if serialized.len() > MAX_PLAYGROUND_SHARE_SNAPSHOT_BYTES {
        return Err(PlaygroundShareRepositoryError::InvalidInput);
    }
    Ok(())
}

/// 只记录闭合分类，避免分享所有者、令牌摘要和正文进入日志。
pub(super) fn record_internal_error(
    error: PlaygroundShareRepositoryError,
) -> PlaygroundShareRepositoryError {
    let error_kind = match error {
        PlaygroundShareRepositoryError::InvalidInput
        | PlaygroundShareRepositoryError::OwnerUnavailable
        | PlaygroundShareRepositoryError::LimitReached
        | PlaygroundShareRepositoryError::TokenConflict => return error,
        PlaygroundShareRepositoryError::Query => "playground_share_query",
        PlaygroundShareRepositoryError::Timeout => "playground_share_timeout",
        PlaygroundShareRepositoryError::Invariant => "playground_share_invariant",
    };
    tracing::error!(
        target: "af_db::playground_share",
        error_kind,
        "Playground 分享仓储发生内部错误"
    );
    error
}
