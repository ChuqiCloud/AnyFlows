use af_admin::{
    IssuedPlaygroundShare, PlaygroundShareCreateCommand, PlaygroundShareInputError,
    PlaygroundShareMessage, PlaygroundShareMessageRole, PlaygroundShareSession,
    PlaygroundShareView, PresentedPlaygroundShareToken,
};
use serde::{Deserialize, Serialize};
use utoipa::{
    ToSchema,
    openapi::{
        RefOr,
        schema::{KnownFormat, ObjectBuilder, Schema, SchemaFormat, Type},
    },
};

/// 分享快照公开的闭合消息角色。
#[derive(Clone, Copy, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = PlaygroundShareMessageRole, rename_all = "snake_case")]
pub(crate) enum PlaygroundShareMessageRoleDto {
    User,
    Assistant,
}

#[derive(Deserialize, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = PlaygroundShareMessage)]
pub(crate) struct PlaygroundShareMessageDto {
    role: PlaygroundShareMessageRoleDto,
    #[schema(min_length = 1, max_length = 65536)]
    content: String,
}

#[derive(Deserialize, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = PlaygroundShareSession)]
pub(crate) struct PlaygroundShareSessionDto {
    #[schema(min_length = 1, max_length = 256)]
    model: String,
    #[schema(min_items = 2, max_items = 256)]
    messages: Vec<PlaygroundShareMessageDto>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = PlaygroundShareCreateRequest)]
pub(crate) struct PlaygroundShareCreateRequest {
    #[schema(schema_with = playground_share_ttl_days_schema)]
    ttl_days: u16,
    #[schema(min_items = 1, max_items = 4)]
    sessions: Vec<PlaygroundShareSessionDto>,
}

/// 让 OpenAPI 与应用层共享同一组闭合 TTL 数值。
fn playground_share_ttl_days_schema() -> RefOr<Schema> {
    ObjectBuilder::new()
        .schema_type(Type::Integer)
        .format(Some(SchemaFormat::KnownFormat(KnownFormat::Int32)))
        .enum_values(Some([1, 7, 30]))
        .build()
        .into()
}

impl PlaygroundShareCreateRequest {
    pub(super) fn into_command(
        self,
    ) -> Result<PlaygroundShareCreateCommand, PlaygroundShareInputError> {
        let sessions = self
            .sessions
            .into_iter()
            .map(PlaygroundShareSessionDto::into_application)
            .collect::<Result<Vec<_>, _>>()?;
        PlaygroundShareCreateCommand::new(self.ttl_days, sessions)
    }
}

impl PlaygroundShareSessionDto {
    pub(crate) fn into_application(
        self,
    ) -> Result<PlaygroundShareSession, PlaygroundShareInputError> {
        let messages = self
            .messages
            .into_iter()
            .map(PlaygroundShareMessageDto::into_application)
            .collect::<Result<Vec<_>, _>>()?;
        PlaygroundShareSession::new(self.model, messages)
    }

    pub(crate) fn from_application(session: PlaygroundShareSession) -> Self {
        let (model, messages) = session.into_parts();
        Self {
            model,
            messages: messages
                .into_iter()
                .map(PlaygroundShareMessageDto::from_application)
                .collect(),
        }
    }
}

impl PlaygroundShareMessageDto {
    fn into_application(self) -> Result<PlaygroundShareMessage, PlaygroundShareInputError> {
        PlaygroundShareMessage::new(self.role.into_application(), self.content)
    }

    fn from_application(message: PlaygroundShareMessage) -> Self {
        let (role, content) = message.into_parts();
        Self {
            role: PlaygroundShareMessageRoleDto::from_application(role),
            content,
        }
    }
}

impl PlaygroundShareMessageRoleDto {
    const fn into_application(self) -> PlaygroundShareMessageRole {
        match self {
            Self::User => PlaygroundShareMessageRole::User,
            Self::Assistant => PlaygroundShareMessageRole::Assistant,
        }
    }

    const fn from_application(role: PlaygroundShareMessageRole) -> Self {
        match role {
            PlaygroundShareMessageRole::User => Self::User,
            PlaygroundShareMessageRole::Assistant => Self::Assistant,
        }
    }
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = PlaygroundShareCreateResponse)]
pub(crate) struct PlaygroundShareCreateResponse {
    #[schema(value_type = String, min_length = 49, max_length = 49)]
    token: PresentedPlaygroundShareToken,
    #[schema(minimum = 1)]
    created_at: i64,
    #[schema(minimum = 1)]
    expires_at: i64,
}

impl PlaygroundShareCreateResponse {
    pub(super) fn from_issued(issued: IssuedPlaygroundShare) -> Self {
        let (token, created_at, expires_at) = issued.into_parts();
        Self {
            token,
            created_at,
            expires_at,
        }
    }
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = PlaygroundShareReadResponse)]
pub(crate) struct PlaygroundShareReadResponse {
    #[schema(min_items = 1, max_items = 4)]
    sessions: Vec<PlaygroundShareSessionDto>,
    #[schema(minimum = 1)]
    created_at: i64,
    #[schema(minimum = 1)]
    expires_at: i64,
}

impl PlaygroundShareReadResponse {
    pub(super) fn from_view(view: PlaygroundShareView) -> Self {
        let (sessions, created_at, expires_at) = view.into_parts();
        Self {
            sessions: sessions
                .into_iter()
                .map(PlaygroundShareSessionDto::from_application)
                .collect(),
            created_at,
            expires_at,
        }
    }
}
