use std::{fmt, future::Future, pin::Pin};

use af_db::{
    PlaygroundConversationDeleteOutcome, PlaygroundConversationRecord,
    PlaygroundConversationRepository, PlaygroundConversationRepositoryError,
    PlaygroundConversationSummaryRecord, PlaygroundConversationWrite,
};
use thiserror::Error;

use crate::{
    PlaygroundShareSession, SessionPrincipal,
    playground_share_snapshot::{sessions_from_snapshot, snapshot_from_sessions},
};

const MAX_PLAYGROUND_CONVERSATION_TITLE_CHARS: usize = 80;

/// 客户端生成、服务端重新校验的 128 位私有会话标识。
#[derive(Clone, Eq, PartialEq)]
pub struct PlaygroundConversationId(String);

impl PlaygroundConversationId {
    /// 解析非全零的 32 位小写十六进制标识。
    pub fn parse_owned(value: String) -> Result<Self, PlaygroundConversationInputError> {
        if value.len() == 32
            && value
                .bytes()
                .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
            && value.bytes().any(|byte| byte != b'0')
        {
            return Ok(Self(value));
        }
        Err(PlaygroundConversationInputError::InvalidId)
    }

    /// 返回所有者 API 路径使用的规范化文本。
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn into_string(self) -> String {
        self.0
    }
}

impl fmt::Debug for PlaygroundConversationId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PlaygroundConversationId(<redacted>)")
    }
}

/// 已校验会话结构并派生摘要的保存命令。
pub struct PlaygroundConversationSaveCommand {
    conversation_id: PlaygroundConversationId,
    expected_revision: Option<i64>,
    title: String,
    models: Vec<String>,
    snapshot: serde_json::Value,
}

impl PlaygroundConversationSaveCommand {
    /// 校验标识、revision 和完整往返，并由服务端派生标题与模型摘要。
    pub fn new(
        conversation_id: String,
        expected_revision: Option<i64>,
        sessions: Vec<PlaygroundShareSession>,
    ) -> Result<Self, PlaygroundConversationInputError> {
        let conversation_id = PlaygroundConversationId::parse_owned(conversation_id)?;
        if expected_revision.is_some_and(|revision| revision <= 0) {
            return Err(PlaygroundConversationInputError::InvalidRevision);
        }
        let (title, models) = derive_summary(&sessions)?;
        let snapshot = snapshot_from_sessions(sessions)
            .map_err(|_| PlaygroundConversationInputError::InvalidSnapshot)?;
        Ok(Self {
            conversation_id,
            expected_revision,
            title,
            models,
            snapshot,
        })
    }

    fn into_write(self) -> PlaygroundConversationWrite {
        PlaygroundConversationWrite::new(
            self.conversation_id.into_string(),
            self.title,
            self.models,
            self.snapshot,
            self.expected_revision,
        )
    }
}

impl fmt::Debug for PlaygroundConversationSaveCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PlaygroundConversationSaveCommand")
            .field("expected_revision", &self.expected_revision)
            .field("model_count", &self.models.len())
            .finish_non_exhaustive()
    }
}

/// 所有者可恢复的一份完整 Playground 会话。
pub struct PlaygroundConversation {
    conversation_id: PlaygroundConversationId,
    title: String,
    sessions: Vec<PlaygroundShareSession>,
    revision: i64,
    created_at: i64,
    updated_at: i64,
}

impl PlaygroundConversation {
    /// 消费完整会话并返回协议层响应字段。
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        PlaygroundConversationId,
        String,
        Vec<PlaygroundShareSession>,
        i64,
        i64,
        i64,
    ) {
        (
            self.conversation_id,
            self.title,
            self.sessions,
            self.revision,
            self.created_at,
            self.updated_at,
        )
    }
}

impl fmt::Debug for PlaygroundConversation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PlaygroundConversation")
            .field("session_count", &self.sessions.len())
            .field("revision", &self.revision)
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .finish_non_exhaustive()
    }
}

/// 历史列表使用的无正文摘要。
pub struct PlaygroundConversationSummary {
    conversation_id: PlaygroundConversationId,
    title: String,
    models: Vec<String>,
    revision: i64,
    created_at: i64,
    updated_at: i64,
}

impl PlaygroundConversationSummary {
    /// 消费摘要并返回协议层列表字段。
    #[must_use]
    pub fn into_parts(self) -> (PlaygroundConversationId, String, Vec<String>, i64, i64, i64) {
        (
            self.conversation_id,
            self.title,
            self.models,
            self.revision,
            self.created_at,
            self.updated_at,
        )
    }
}

impl fmt::Debug for PlaygroundConversationSummary {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PlaygroundConversationSummary")
            .field("model_count", &self.models.len())
            .field("revision", &self.revision)
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .finish_non_exhaustive()
    }
}

/// 私有会话输入错误；不携带标识、标题、模型或正文。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum PlaygroundConversationInputError {
    /// 会话标识不是规范化的非零 128 位文本。
    #[error("Playground 会话标识无效")]
    InvalidId,
    /// revision 不是空值或正整数。
    #[error("Playground 会话版本无效")]
    InvalidRevision,
    /// 模型、消息往返或快照容量无效。
    #[error("Playground 会话快照无效")]
    InvalidSnapshot,
}

/// 私有会话应用服务的稳定错误分类。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum PlaygroundConversationError {
    /// 标识、revision、模型、消息或容量不满足公开契约。
    #[error("Playground 会话参数无效")]
    InvalidInput,
    /// 当前登录用户在写入前已经失效。
    #[error("Playground 会话登录状态无效")]
    InvalidSession,
    /// 当前用户的历史数量已经达到上限。
    #[error("Playground 会话历史已达上限")]
    LimitReached,
    /// 会话标识未知或不属于当前用户。
    #[error("Playground 会话不存在")]
    NotFound,
    /// revision 已被其他请求推进或初始标识携带不同内容。
    #[error("Playground 会话版本冲突")]
    Conflict,
    /// 数据库或持久化快照发生内部故障。
    #[error("Playground 会话内部失败")]
    Internal,
}

/// 保存私有会话 Future。
pub type PlaygroundConversationSaveFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<PlaygroundConversation, PlaygroundConversationError>>
            + Send
            + 'a,
    >,
>;
/// 列出私有会话摘要 Future。
pub type PlaygroundConversationListFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<Vec<PlaygroundConversationSummary>, PlaygroundConversationError>>
            + Send
            + 'a,
    >,
>;
/// 读取完整私有会话 Future。
pub type PlaygroundConversationReadFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<PlaygroundConversation, PlaygroundConversationError>>
            + Send
            + 'a,
    >,
>;
/// 删除私有会话 Future。
pub type PlaygroundConversationDeleteFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), PlaygroundConversationError>> + Send + 'a>>;

/// Playground 私有会话历史应用端口。
pub trait PlaygroundConversationService: Send + Sync {
    /// 幂等创建或按 revision 更新当前用户的会话。
    fn save(
        &self,
        principal: SessionPrincipal,
        command: PlaygroundConversationSaveCommand,
    ) -> PlaygroundConversationSaveFuture<'_>;

    /// 列出当前用户的全部有界摘要。
    fn list(&self, principal: SessionPrincipal) -> PlaygroundConversationListFuture<'_>;

    /// 读取当前用户拥有的一份完整会话。
    fn read(
        &self,
        principal: SessionPrincipal,
        conversation_id: String,
    ) -> PlaygroundConversationReadFuture<'_>;

    /// 删除当前用户拥有的一份会话。
    fn delete(
        &self,
        principal: SessionPrincipal,
        conversation_id: String,
    ) -> PlaygroundConversationDeleteFuture<'_>;
}

/// 使用数据库仓储实现的 Playground 私有历史服务。
pub struct DatabasePlaygroundConversationService {
    repository: PlaygroundConversationRepository,
}

impl DatabasePlaygroundConversationService {
    /// 绑定已配置截止时间与容量边界的会话仓储。
    #[must_use]
    pub const fn new(repository: PlaygroundConversationRepository) -> Self {
        Self { repository }
    }
}

impl PlaygroundConversationService for DatabasePlaygroundConversationService {
    fn save(
        &self,
        principal: SessionPrincipal,
        command: PlaygroundConversationSaveCommand,
    ) -> PlaygroundConversationSaveFuture<'_> {
        Box::pin(async move {
            let record = self
                .repository
                .save(principal.user_id(), command.into_write())
                .await
                .map_err(map_repository_error)?;
            conversation_from_record(record)
        })
    }

    fn list(&self, principal: SessionPrincipal) -> PlaygroundConversationListFuture<'_> {
        Box::pin(async move {
            self.repository
                .list(principal.user_id())
                .await
                .map_err(map_repository_error)?
                .into_iter()
                .map(summary_from_record)
                .collect()
        })
    }

    fn read(
        &self,
        principal: SessionPrincipal,
        conversation_id: String,
    ) -> PlaygroundConversationReadFuture<'_> {
        Box::pin(async move {
            let conversation_id = PlaygroundConversationId::parse_owned(conversation_id)
                .map_err(|_| PlaygroundConversationError::NotFound)?;
            let record = self
                .repository
                .find(principal.user_id(), conversation_id.as_str())
                .await
                .map_err(map_repository_error)?
                .ok_or(PlaygroundConversationError::NotFound)?;
            conversation_from_record(record)
        })
    }

    fn delete(
        &self,
        principal: SessionPrincipal,
        conversation_id: String,
    ) -> PlaygroundConversationDeleteFuture<'_> {
        Box::pin(async move {
            let conversation_id = PlaygroundConversationId::parse_owned(conversation_id)
                .map_err(|_| PlaygroundConversationError::NotFound)?;
            match self
                .repository
                .delete(principal.user_id(), conversation_id.as_str())
                .await
                .map_err(map_repository_error)?
            {
                PlaygroundConversationDeleteOutcome::Deleted => Ok(()),
                PlaygroundConversationDeleteOutcome::NotFound => {
                    Err(PlaygroundConversationError::NotFound)
                }
            }
        })
    }
}

impl fmt::Debug for DatabasePlaygroundConversationService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabasePlaygroundConversationService(<redacted>)")
    }
}

fn conversation_from_record(
    record: PlaygroundConversationRecord,
) -> Result<PlaygroundConversation, PlaygroundConversationError> {
    let (conversation_id, title, models, snapshot, revision, created_at, updated_at) =
        record.into_parts();
    let conversation_id = PlaygroundConversationId::parse_owned(conversation_id)
        .map_err(|_| PlaygroundConversationError::Internal)?;
    let sessions =
        sessions_from_snapshot(snapshot).map_err(|_| PlaygroundConversationError::Internal)?;
    let (derived_title, derived_models) =
        derive_summary(&sessions).map_err(|_| PlaygroundConversationError::Internal)?;
    if title != derived_title || models != derived_models || revision <= 0 {
        return Err(PlaygroundConversationError::Internal);
    }
    Ok(PlaygroundConversation {
        conversation_id,
        title,
        sessions,
        revision,
        created_at,
        updated_at,
    })
}

fn summary_from_record(
    record: PlaygroundConversationSummaryRecord,
) -> Result<PlaygroundConversationSummary, PlaygroundConversationError> {
    let (conversation_id, title, models, revision, created_at, updated_at) = record.into_parts();
    let conversation_id = PlaygroundConversationId::parse_owned(conversation_id)
        .map_err(|_| PlaygroundConversationError::Internal)?;
    if title.is_empty() || models.is_empty() || revision <= 0 {
        return Err(PlaygroundConversationError::Internal);
    }
    Ok(PlaygroundConversationSummary {
        conversation_id,
        title,
        models,
        revision,
        created_at,
        updated_at,
    })
}

fn derive_summary(
    sessions: &[PlaygroundShareSession],
) -> Result<(String, Vec<String>), PlaygroundConversationInputError> {
    let first = sessions
        .first()
        .ok_or(PlaygroundConversationInputError::InvalidSnapshot)?;
    let first_user_message = first
        .messages()
        .first()
        .ok_or(PlaygroundConversationInputError::InvalidSnapshot)?;
    let collapsed = first_user_message
        .content()
        .split(|character: char| character.is_whitespace() || character.is_control())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let title_source = if collapsed.is_empty() {
        first.model()
    } else {
        &collapsed
    };
    let title = title_source
        .chars()
        .take(MAX_PLAYGROUND_CONVERSATION_TITLE_CHARS)
        .collect::<String>();
    if title.is_empty() {
        return Err(PlaygroundConversationInputError::InvalidSnapshot);
    }
    let models = sessions
        .iter()
        .map(|session| session.model().to_owned())
        .collect();
    Ok((title, models))
}

fn map_repository_error(
    error: PlaygroundConversationRepositoryError,
) -> PlaygroundConversationError {
    match error {
        PlaygroundConversationRepositoryError::InvalidInput => {
            PlaygroundConversationError::InvalidInput
        }
        PlaygroundConversationRepositoryError::OwnerUnavailable => {
            PlaygroundConversationError::InvalidSession
        }
        PlaygroundConversationRepositoryError::LimitReached => {
            PlaygroundConversationError::LimitReached
        }
        PlaygroundConversationRepositoryError::NotFound => PlaygroundConversationError::NotFound,
        PlaygroundConversationRepositoryError::Conflict => PlaygroundConversationError::Conflict,
        PlaygroundConversationRepositoryError::Query
        | PlaygroundConversationRepositoryError::Timeout
        | PlaygroundConversationRepositoryError::Invariant => PlaygroundConversationError::Internal,
    }
}
