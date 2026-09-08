use af_admin::{
    PlaygroundConversation, PlaygroundConversationInputError, PlaygroundConversationSaveCommand,
    PlaygroundConversationSummary,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::playground_shares::PlaygroundShareSessionDto;

/// 私有会话的幂等创建或版本更新正文。
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = PlaygroundConversationSaveRequest)]
pub(crate) struct PlaygroundConversationSaveRequest {
    #[schema(minimum = 1, nullable = true)]
    revision: Option<i64>,
    #[schema(min_items = 1, max_items = 4)]
    sessions: Vec<PlaygroundShareSessionDto>,
}

impl PlaygroundConversationSaveRequest {
    pub(super) fn into_command(
        self,
        conversation_id: String,
    ) -> Result<PlaygroundConversationSaveCommand, PlaygroundConversationInputError> {
        let sessions = self
            .sessions
            .into_iter()
            .map(PlaygroundShareSessionDto::into_application)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| PlaygroundConversationInputError::InvalidSnapshot)?;
        PlaygroundConversationSaveCommand::new(conversation_id, self.revision, sessions)
    }
}

/// 历史抽屉中的单条无正文摘要。
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = PlaygroundConversationSummary)]
pub(crate) struct PlaygroundConversationSummaryDto {
    #[schema(min_length = 32, max_length = 32, pattern = "^[0-9a-f]{32}$")]
    conversation_id: String,
    #[schema(min_length = 1, max_length = 120)]
    title: String,
    #[schema(min_items = 1, max_items = 4)]
    models: Vec<String>,
    #[schema(minimum = 1)]
    revision: i64,
    #[schema(minimum = 1)]
    created_at: i64,
    #[schema(minimum = 1)]
    updated_at: i64,
}

impl PlaygroundConversationSummaryDto {
    fn from_application(summary: PlaygroundConversationSummary) -> Self {
        let (conversation_id, title, models, revision, created_at, updated_at) =
            summary.into_parts();
        Self {
            conversation_id: conversation_id.as_str().to_owned(),
            title,
            models,
            revision,
            created_at,
            updated_at,
        }
    }
}

/// 当前用户的全部有界历史摘要。
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = PlaygroundConversationListResponse)]
pub(crate) struct PlaygroundConversationListResponse {
    #[schema(max_items = 50)]
    conversations: Vec<PlaygroundConversationSummaryDto>,
}

impl PlaygroundConversationListResponse {
    pub(super) fn from_application(summaries: Vec<PlaygroundConversationSummary>) -> Self {
        Self {
            conversations: summaries
                .into_iter()
                .map(PlaygroundConversationSummaryDto::from_application)
                .collect(),
        }
    }
}

/// 保存或恢复时返回的完整私有会话。
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = PlaygroundConversationResponse)]
pub(crate) struct PlaygroundConversationResponse {
    #[schema(min_length = 32, max_length = 32, pattern = "^[0-9a-f]{32}$")]
    conversation_id: String,
    #[schema(min_length = 1, max_length = 120)]
    title: String,
    #[schema(min_items = 1, max_items = 4)]
    sessions: Vec<PlaygroundShareSessionDto>,
    #[schema(minimum = 1)]
    revision: i64,
    #[schema(minimum = 1)]
    created_at: i64,
    #[schema(minimum = 1)]
    updated_at: i64,
}

impl PlaygroundConversationResponse {
    pub(super) fn from_application(conversation: PlaygroundConversation) -> Self {
        let (conversation_id, title, sessions, revision, created_at, updated_at) =
            conversation.into_parts();
        Self {
            conversation_id: conversation_id.as_str().to_owned(),
            title,
            sessions: sessions
                .into_iter()
                .map(PlaygroundShareSessionDto::from_application)
                .collect(),
            revision,
            created_at,
            updated_at,
        }
    }
}
