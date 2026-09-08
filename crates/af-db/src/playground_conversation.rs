use std::{fmt, time::Duration};

use af_domain::UserId;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{DatabasePool, entity::PlaygroundConversationKey};

mod mutation;
mod read;
mod types;

pub use types::{
    MAX_PLAYGROUND_CONVERSATIONS_PER_USER, PlaygroundConversationDeleteOutcome,
    PlaygroundConversationRecord, PlaygroundConversationRepositoryConfigError,
    PlaygroundConversationRepositoryError, PlaygroundConversationSummaryRecord,
    PlaygroundConversationWrite,
};

/// Playground 私有会话历史的所有者范围仓储。
#[derive(Clone)]
pub struct PlaygroundConversationRepository {
    pub(super) pool: DatabasePool,
    operation_timeout: Duration,
}

impl PlaygroundConversationRepository {
    /// 使用共享连接池和非零单次操作截止时间构造仓储。
    pub fn new(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, PlaygroundConversationRepositoryConfigError> {
        if operation_timeout.is_zero() {
            return Err(PlaygroundConversationRepositoryConfigError::ZeroOperationTimeout);
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }

    /// 幂等创建或按 revision 更新当前用户的一份私有会话。
    pub async fn save(
        &self,
        owner_user_id: UserId,
        write: PlaygroundConversationWrite,
    ) -> Result<PlaygroundConversationRecord, PlaygroundConversationRepositoryError> {
        let operation =
            mutation::save(self, owner_user_id, write).with_subscriber(NoSubscriber::default());
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(
                PlaygroundConversationRepositoryError::Timeout,
            )),
        }
    }

    /// 按最近更新时间列出当前用户的全部有界摘要，不读取消息正文。
    pub async fn list(
        &self,
        owner_user_id: UserId,
    ) -> Result<Vec<PlaygroundConversationSummaryRecord>, PlaygroundConversationRepositoryError>
    {
        let operation = read::list(self, owner_user_id).with_subscriber(NoSubscriber::default());
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(
                PlaygroundConversationRepositoryError::Timeout,
            )),
        }
    }

    /// 仅在标识属于当前用户时返回完整会话。
    pub async fn find(
        &self,
        owner_user_id: UserId,
        conversation_id: &str,
    ) -> Result<Option<PlaygroundConversationRecord>, PlaygroundConversationRepositoryError> {
        let Ok(conversation_id) = PlaygroundConversationKey::parse(conversation_id) else {
            return Ok(None);
        };
        let operation = read::find(self, owner_user_id, conversation_id)
            .with_subscriber(NoSubscriber::default());
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(
                PlaygroundConversationRepositoryError::Timeout,
            )),
        }
    }

    /// 仅删除当前用户拥有的会话，其他状态统一为未找到。
    pub async fn delete(
        &self,
        owner_user_id: UserId,
        conversation_id: &str,
    ) -> Result<PlaygroundConversationDeleteOutcome, PlaygroundConversationRepositoryError> {
        let Ok(conversation_id) = PlaygroundConversationKey::parse(conversation_id) else {
            return Ok(PlaygroundConversationDeleteOutcome::NotFound);
        };
        let operation = read::delete(self, owner_user_id, conversation_id)
            .with_subscriber(NoSubscriber::default());
        match timeout(self.operation_timeout, operation).await {
            Ok(result) => result,
            Err(_) => Err(record_internal_error(
                PlaygroundConversationRepositoryError::Timeout,
            )),
        }
    }
}

impl fmt::Debug for PlaygroundConversationRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PlaygroundConversationRepository")
            .field("operation_timeout", &self.operation_timeout)
            .finish_non_exhaustive()
    }
}

pub(super) fn validate_title(title: &str) -> Result<(), PlaygroundConversationRepositoryError> {
    if title.is_empty()
        || title.chars().count() > 120
        || title.trim() != title
        || title.chars().any(char::is_control)
    {
        return Err(PlaygroundConversationRepositoryError::InvalidInput);
    }
    Ok(())
}

pub(super) fn validate_models(
    models: &[String],
) -> Result<(), PlaygroundConversationRepositoryError> {
    if models.is_empty() || models.len() > 4 {
        return Err(PlaygroundConversationRepositoryError::InvalidInput);
    }
    for (index, model) in models.iter().enumerate() {
        if model.is_empty()
            || model.len() > 256
            || model.trim() != model
            || model.contains('\0')
            || models[..index].contains(model)
        {
            return Err(PlaygroundConversationRepositoryError::InvalidInput);
        }
    }
    Ok(())
}

pub(super) fn validate_snapshot(
    snapshot: &serde_json::Value,
) -> Result<(), PlaygroundConversationRepositoryError> {
    if !snapshot.is_object() {
        return Err(PlaygroundConversationRepositoryError::InvalidInput);
    }
    let serialized = serde_json::to_vec(snapshot)
        .map_err(|_| PlaygroundConversationRepositoryError::InvalidInput)?;
    if serialized.len() > crate::MAX_PLAYGROUND_SHARE_SNAPSHOT_BYTES {
        return Err(PlaygroundConversationRepositoryError::InvalidInput);
    }
    Ok(())
}

/// 只记录闭合分类，避免所有者、会话标识、标题和正文进入日志。
pub(super) fn record_internal_error(
    error: PlaygroundConversationRepositoryError,
) -> PlaygroundConversationRepositoryError {
    let error_kind = match error {
        PlaygroundConversationRepositoryError::InvalidInput
        | PlaygroundConversationRepositoryError::OwnerUnavailable
        | PlaygroundConversationRepositoryError::LimitReached
        | PlaygroundConversationRepositoryError::NotFound
        | PlaygroundConversationRepositoryError::Conflict => return error,
        PlaygroundConversationRepositoryError::Query => "playground_conversation_query",
        PlaygroundConversationRepositoryError::Timeout => "playground_conversation_timeout",
        PlaygroundConversationRepositoryError::Invariant => "playground_conversation_invariant",
    };
    tracing::error!(
        target: "af_db::playground_conversation",
        error_kind,
        "Playground 会话历史仓储发生内部错误"
    );
    error
}
