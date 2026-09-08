//! 异步任务上游提交 claim 与恢复绑定的持久化实体。

use std::str::FromStr;

use af_domain::{
    AsyncTaskId, AsyncTaskRequestFingerprint, AsyncTaskRequestId, MAX_MODEL_NAME_BYTES, Protocol,
};
use sea_orm::entity::prelude::*;

use super::SensitiveString;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "async_task_submission_claims")]
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
    #[sea_orm(column_type = "Char(Some(64))")]
    pub request_fingerprint: SensitiveString,
    pub state: i16,
    #[sea_orm(column_type = "Char(Some(32))")]
    pub attempt_key: Option<SensitiveString>,
    pub target_group_id: Option<i64>,
    #[sea_orm(column_type = "String(StringLen::N(256))")]
    pub upstream_model: Option<SensitiveString>,
    pub channel_id: Option<i64>,
    pub credential_id: Option<i64>,
    #[sea_orm(column_type = "Char(Some(16))")]
    pub credential_revision: Option<SensitiveString>,
    #[sea_orm(column_type = "String(StringLen::N(512))")]
    pub upstream_task_id: Option<SensitiveString>,
    #[sea_orm(column_type = "Char(Some(64))")]
    pub binding_fingerprint: Option<SensitiveString>,
    pub attempt_timeout_millis: Option<i64>,
    pub video_duration_seconds: Option<i16>,
    pub video_resolution: Option<i16>,
    pub status: Option<i16>,
    pub progress_basis_points: Option<i16>,
    pub failure_kind: Option<i16>,
    pub version: i64,
    pub accepted_at: Option<TimeDateTimeWithTimeZone>,
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
        belongs_to = "super::groups::Entity",
        from = "Column::TargetGroupId",
        to = "super::groups::Column::Id",
        on_update = "Cascade",
        on_delete = "Restrict"
    )]
    TargetGroup,
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

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C>(self, _database: &C, insert: bool) -> Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        // claim 的状态推进只能经过带尝试所有者和版本条件的仓储 CAS。
        if !insert {
            return Err(DbErr::Custom(
                "异步任务提交 claim 必须通过仓储更新".to_owned(),
            ));
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
        let request_fingerprint = self
            .request_fingerprint
            .try_as_ref()
            .map(SensitiveString::as_str);
        let created_at = self.created_at.try_as_ref().copied();
        let updated_at = self.updated_at.try_as_ref().copied();
        if task_key.is_none_or(|value| AsyncTaskId::from_persistence_key(value).is_err())
            || idempotency_key
                .is_none_or(|value| AsyncTaskRequestId::from_persistence_key(value).is_err())
            || protocol.is_none_or(|value| Protocol::from_str(value).is_err())
            || requested_model.is_none_or(|value| !valid_model_name(value))
            || request_fingerprint.is_none_or(|value| {
                AsyncTaskRequestFingerprint::from_persistence_key(value).is_err()
            })
            || [
                self.user_id.try_as_ref().copied(),
                self.token_id.try_as_ref().copied(),
                self.group_id.try_as_ref().copied(),
            ]
            .into_iter()
            .any(|value| value.is_none_or(|value| value <= 0))
            || self.state.try_as_ref().copied() != Some(1)
            || self.version.try_as_ref().copied() != Some(1)
            || self.attempt_key.try_as_ref().is_some_and(Option::is_some)
            || self
                .target_group_id
                .try_as_ref()
                .is_some_and(Option::is_some)
            || self
                .upstream_model
                .try_as_ref()
                .is_some_and(Option::is_some)
            || self.channel_id.try_as_ref().is_some_and(Option::is_some)
            || self.credential_id.try_as_ref().is_some_and(Option::is_some)
            || self
                .credential_revision
                .try_as_ref()
                .is_some_and(Option::is_some)
            || self
                .upstream_task_id
                .try_as_ref()
                .is_some_and(Option::is_some)
            || self
                .binding_fingerprint
                .try_as_ref()
                .is_some_and(Option::is_some)
            || self
                .attempt_timeout_millis
                .try_as_ref()
                .is_some_and(Option::is_some)
            || self
                .video_duration_seconds
                .try_as_ref()
                .is_none_or(|value| value.is_some_and(|value| !(1..=15).contains(&value)))
            || self
                .video_resolution
                .try_as_ref()
                .is_some_and(Option::is_some)
            || self.status.try_as_ref().is_some_and(Option::is_some)
            || self
                .progress_basis_points
                .try_as_ref()
                .is_some_and(Option::is_some)
            || self.failure_kind.try_as_ref().is_some_and(Option::is_some)
            || self.accepted_at.try_as_ref().is_some_and(Option::is_some)
            || created_at.is_none()
            || updated_at.is_none()
            || updated_at < created_at
        {
            return Err(DbErr::Custom(
                "异步任务提交 claim 持久化字段无效".to_owned(),
            ));
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
