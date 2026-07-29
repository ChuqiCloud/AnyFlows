use std::fmt;

use af_db::MAX_PLAYGROUND_SHARE_SNAPSHOT_BYTES;
use serde::{Deserialize, Serialize};

/// 单份分享允许保存的模型会话上限。
pub const MAX_PLAYGROUND_SHARE_SESSIONS: usize = 4;
/// 全部模型会话合计允许保存的消息上限。
pub const MAX_PLAYGROUND_SHARE_MESSAGES: usize = 256;
/// 单条可见消息允许保存的 UTF-8 字节上限。
pub const MAX_PLAYGROUND_SHARE_MESSAGE_BYTES: usize = 64 * 1024;
const PLAYGROUND_SHARE_SNAPSHOT_VERSION: u8 = 1;

mod validation;

use validation::{has_complete_round_trips, valid_model_name, validate_sessions};

/// 分享快照允许公开呈现的消息角色。
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaygroundShareMessageRole {
    /// 用户输入。
    User,
    /// 已完成的模型输出。
    Assistant,
}

/// 分享快照中的一条可见文本消息。
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlaygroundShareMessage {
    role: PlaygroundShareMessageRole,
    content: String,
}

impl PlaygroundShareMessage {
    /// 构造一条有界用户或助手文本消息。
    pub fn new(
        role: PlaygroundShareMessageRole,
        content: String,
    ) -> Result<Self, PlaygroundShareInputError> {
        if content.is_empty()
            || content.len() > MAX_PLAYGROUND_SHARE_MESSAGE_BYTES
            || content.contains('\0')
        {
            return Err(PlaygroundShareInputError::InvalidMessage);
        }
        Ok(Self { role, content })
    }

    /// 返回公开消息角色。
    #[must_use]
    pub const fn role(&self) -> PlaygroundShareMessageRole {
        self.role
    }

    /// 返回公开消息正文。
    #[must_use]
    pub fn content(&self) -> &str {
        &self.content
    }

    /// 消费消息并返回角色与正文，供协议响应避免复制大文本。
    #[must_use]
    pub fn into_parts(self) -> (PlaygroundShareMessageRole, String) {
        (self.role, self.content)
    }
}

impl fmt::Debug for PlaygroundShareMessage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PlaygroundShareMessage")
            .field("role", &self.role)
            .field("content_bytes", &self.content.len())
            .finish()
    }
}

/// 单个模型对应的一组完整用户/助手往返。
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlaygroundShareSession {
    model: String,
    messages: Vec<PlaygroundShareMessage>,
}

impl PlaygroundShareSession {
    /// 校验模型名和完整交替往返后构造分享会话。
    pub fn new(
        model: String,
        messages: Vec<PlaygroundShareMessage>,
    ) -> Result<Self, PlaygroundShareInputError> {
        if !valid_model_name(&model) || !has_complete_round_trips(&messages) {
            return Err(PlaygroundShareInputError::InvalidSession);
        }
        Ok(Self { model, messages })
    }

    /// 返回公开的 Canonical 模型名。
    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    /// 返回该模型已完成的可见消息。
    #[must_use]
    pub fn messages(&self) -> &[PlaygroundShareMessage] {
        &self.messages
    }

    /// 消费会话并返回模型名与消息，供协议响应避免复制快照。
    #[must_use]
    pub fn into_parts(self) -> (String, Vec<PlaygroundShareMessage>) {
        (self.model, self.messages)
    }
}

impl fmt::Debug for PlaygroundShareSession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PlaygroundShareSession")
            .field("message_count", &self.messages.len())
            .finish_non_exhaustive()
    }
}

/// 分享有效期的闭合集合。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlaygroundShareTtl {
    /// 一天。
    OneDay,
    /// 七天。
    SevenDays,
    /// 三十天。
    ThirtyDays,
}

impl PlaygroundShareTtl {
    /// 从公开的天数值解析闭合有效期。
    pub const fn from_days(days: u16) -> Result<Self, PlaygroundShareInputError> {
        match days {
            1 => Ok(Self::OneDay),
            7 => Ok(Self::SevenDays),
            30 => Ok(Self::ThirtyDays),
            _ => Err(PlaygroundShareInputError::InvalidTtl),
        }
    }

    /// 返回用于计算过期时间的秒数。
    #[must_use]
    pub const fn seconds(self) -> u64 {
        match self {
            Self::OneDay => 24 * 60 * 60,
            Self::SevenDays => 7 * 24 * 60 * 60,
            Self::ThirtyDays => 30 * 24 * 60 * 60,
        }
    }
}

/// 已通过公开边界校验的分享创建命令。
pub struct PlaygroundShareCreateCommand {
    ttl: PlaygroundShareTtl,
    snapshot: StoredPlaygroundShareSnapshot,
}

impl PlaygroundShareCreateCommand {
    /// 校验 TTL、会话数量、模型唯一性、消息总量和最终 JSON 容量。
    pub fn new(
        ttl_days: u16,
        sessions: Vec<PlaygroundShareSession>,
    ) -> Result<Self, PlaygroundShareInputError> {
        let ttl = PlaygroundShareTtl::from_days(ttl_days)?;
        let snapshot = validated_stored_snapshot(sessions)?;
        Ok(Self { ttl, snapshot })
    }

    pub(crate) const fn ttl(&self) -> PlaygroundShareTtl {
        self.ttl
    }

    pub(crate) fn into_snapshot(self) -> Result<serde_json::Value, PlaygroundShareInputError> {
        serde_json::to_value(self.snapshot).map_err(|_| PlaygroundShareInputError::InvalidSnapshot)
    }
}

pub(crate) fn snapshot_from_sessions(
    sessions: Vec<PlaygroundShareSession>,
) -> Result<serde_json::Value, PlaygroundShareInputError> {
    serde_json::to_value(validated_stored_snapshot(sessions)?)
        .map_err(|_| PlaygroundShareInputError::InvalidSnapshot)
}

fn validated_stored_snapshot(
    sessions: Vec<PlaygroundShareSession>,
) -> Result<StoredPlaygroundShareSnapshot, PlaygroundShareInputError> {
    validate_sessions(&sessions)?;
    let snapshot = StoredPlaygroundShareSnapshot {
        version: PLAYGROUND_SHARE_SNAPSHOT_VERSION,
        sessions,
    };
    let bytes =
        serde_json::to_vec(&snapshot).map_err(|_| PlaygroundShareInputError::InvalidSnapshot)?;
    if bytes.len() > MAX_PLAYGROUND_SHARE_SNAPSHOT_BYTES {
        return Err(PlaygroundShareInputError::SnapshotTooLarge);
    }
    Ok(snapshot)
}

impl fmt::Debug for PlaygroundShareCreateCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PlaygroundShareCreateCommand")
            .field("ttl", &self.ttl)
            .field("session_count", &self.snapshot.sessions.len())
            .finish()
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StoredPlaygroundShareSnapshot {
    version: u8,
    sessions: Vec<PlaygroundShareSession>,
}

pub(crate) fn sessions_from_snapshot(
    snapshot: serde_json::Value,
) -> Result<Vec<PlaygroundShareSession>, PlaygroundShareInputError> {
    let stored: StoredPlaygroundShareSnapshot =
        serde_json::from_value(snapshot).map_err(|_| PlaygroundShareInputError::InvalidSnapshot)?;
    if stored.version != PLAYGROUND_SHARE_SNAPSHOT_VERSION {
        return Err(PlaygroundShareInputError::InvalidSnapshot);
    }
    validate_sessions(&stored.sessions)?;
    Ok(stored.sessions)
}

/// 分享快照输入错误；不携带模型名或消息正文。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlaygroundShareInputError {
    /// TTL 不属于闭合集合。
    InvalidTtl,
    /// 单条消息为空、超长或包含空字符。
    InvalidMessage,
    /// 模型名或用户/助手往返结构无效。
    InvalidSession,
    /// 会话数量、模型唯一性或消息总量无效。
    InvalidSnapshot,
    /// 最终序列化快照超过持久化上限。
    SnapshotTooLarge,
}
