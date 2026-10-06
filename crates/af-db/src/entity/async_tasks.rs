//! 供应商无关异步任务状态机的持久化实体。

use std::str::FromStr;

use af_domain::{AsyncTaskId, AsyncTaskRequestId, MAX_MODEL_NAME_BYTES, Protocol, UpstreamTaskId};
use sea_orm::entity::prelude::*;

use super::SensitiveString;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "async_tasks")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(column_type = "Char(Some(32))")]
    pub task_key: SensitiveString,
    pub user_id: i64,
    pub token_id: i64,
    pub group_id: i64,
    #[sea_orm(column_type = "Char(Some(32))")]
    pub idempotency_key: SensitiveString,
    #[sea_orm(column_type = "String(StringLen::N(64))")]
    pub protocol: String,
    #[sea_orm(column_type = "String(StringLen::N(256))")]
    pub requested_model: SensitiveString,
    #[sea_orm(column_type = "String(StringLen::N(256))")]
    pub upstream_model: SensitiveString,
    pub channel_id: i64,
    pub credential_id: i64,
    #[sea_orm(column_type = "Char(Some(16))")]
    pub credential_revision: SensitiveString,
    #[sea_orm(column_type = "String(StringLen::N(512))")]
    pub upstream_task_id: SensitiveString,
    pub status: i16,
    pub progress_basis_points: i16,
    pub failure_kind: Option<i16>,
    pub version: i64,
    pub terminal_at: Option<TimeDateTimeWithTimeZone>,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::UserId",
        to = "super::users::Column::Id",
        on_update = "Cascade",
        on_delete = "Restrict"
    )]
    User,
    #[sea_orm(
        belongs_to = "super::tokens::Entity",
        from = "Column::TokenId",
        to = "super::tokens::Column::Id",
        on_update = "Cascade",
        on_delete = "Restrict"
    )]
    Token,
    #[sea_orm(
        belongs_to = "super::groups::Entity",
        from = "Column::GroupId",
        to = "super::groups::Column::Id",
        on_update = "Cascade",
        on_delete = "Restrict"
    )]
    Group,
    #[sea_orm(
        belongs_to = "super::channels::Entity",
        from = "Column::ChannelId",
        to = "super::channels::Column::Id",
        on_update = "Cascade",
        on_delete = "Restrict"
    )]
    Channel,
    #[sea_orm(
        belongs_to = "super::credentials::Entity",
        from = "Column::CredentialId",
        to = "super::credentials::Column::Id",
        on_update = "Cascade",
        on_delete = "Restrict"
    )]
    Credential,
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::User.def()
    }
}

impl Related<super::tokens::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Token.def()
    }
}

impl Related<super::groups::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Group.def()
    }
}

impl Related<super::channels::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Channel.def()
    }
}

impl Related<super::credentials::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Credential.def()
    }
}

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C>(self, _database: &C, insert: bool) -> Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        // 状态迁移必须经过带版本条件的仓储更新，禁止 ActiveModel 绕过 CAS。
        if !insert {
            return Err(DbErr::Custom("异步任务必须通过仓储更新".to_owned()));
        }
        let task_key = self.task_key.try_as_ref().map(SensitiveString::as_str);
        let idempotency_key = self
            .idempotency_key
            .try_as_ref()
            .map(SensitiveString::as_str);
        let protocol = self.protocol.try_as_ref().map(String::as_str);
        let requested_model = self
            .requested_model
            .try_as_ref()
            .map(SensitiveString::as_str);
        let upstream_model = self
            .upstream_model
            .try_as_ref()
            .map(SensitiveString::as_str);
        let credential_revision = self
            .credential_revision
            .try_as_ref()
            .map(SensitiveString::as_str);
        let upstream_task_id = self
            .upstream_task_id
            .try_as_ref()
            .map(SensitiveString::as_str);
        let status = self.status.try_as_ref().copied();
        let progress = self.progress_basis_points.try_as_ref().copied();
        let failure_kind = self.failure_kind.try_as_ref().copied().flatten();
        let terminal_at = self.terminal_at.try_as_ref().copied().flatten();
        let created_at = self.created_at.try_as_ref().copied();
        let updated_at = self.updated_at.try_as_ref().copied();
        let state_shape_valid = match status {
            Some(1..=3) => failure_kind.is_none() && terminal_at.is_none(),
            Some(4) => progress == Some(10_000) && failure_kind.is_none() && terminal_at.is_some(),
            Some(5) => progress == Some(0) && failure_kind.is_some() && terminal_at.is_some(),
            _ => false,
        };
        if task_key.is_none_or(|value| AsyncTaskId::from_persistence_key(value).is_err())
            || idempotency_key
                .is_none_or(|value| AsyncTaskRequestId::from_persistence_key(value).is_err())
            || protocol.is_none_or(|value| Protocol::from_str(value).is_err())
            || requested_model.is_none_or(|value| !valid_model_name(value))
            || upstream_model.is_none_or(|value| !valid_model_name(value))
            || credential_revision.is_none_or(|value| !valid_credential_revision(value))
            || upstream_task_id.is_none_or(|value| UpstreamTaskId::new(value).is_err())
            || [
                self.user_id.try_as_ref().copied(),
                self.token_id.try_as_ref().copied(),
                self.group_id.try_as_ref().copied(),
                self.channel_id.try_as_ref().copied(),
                self.credential_id.try_as_ref().copied(),
            ]
            .into_iter()
            .any(|value| value.is_none_or(|value| value <= 0))
            || progress.is_none_or(|value| !(0..=10_000).contains(&value))
            || failure_kind.is_some_and(|value| !(1..=4).contains(&value))
            || self.version.try_as_ref().copied() != Some(1)
            || created_at.is_none()
            || updated_at.is_none()
            || updated_at < created_at
            || terminal_at.is_some_and(|value| created_at.is_none_or(|created| value < created))
            || !state_shape_valid
        {
            return Err(DbErr::Custom("异步任务持久化字段无效".to_owned()));
        }
        Ok(self)
    }
}

fn valid_model_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_MODEL_NAME_BYTES
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn valid_credential_revision(value: &str) -> bool {
    value.len() == 16
        && value
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}
