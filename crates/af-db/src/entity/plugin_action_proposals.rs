//! Durable, host-owned plugin action proposals and confirmation audit facts.

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "plugin_action_proposals")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub proposal_key: String,
    pub idempotency_key: String,
    pub requested_by_user_id: i64,
    pub confirmed_by_user_id: Option<i64>,
    pub plugin_id: String,
    pub artifact_id: i64,
    pub artifact_digest: String,
    pub deployment_generation: i64,
    pub capability_id: String,
    pub capability_version: i32,
    pub action_id: String,
    pub action_version: i32,
    pub input_json: String,
    pub target_id: i64,
    pub expected_target_version: i64,
    pub expected_enabled: bool,
    pub requested_enabled: bool,
    /// 1=pending, 2=confirmed, 3=expired, 4=stale, 5=revoked.
    pub status: i16,
    pub expires_at: i64,
    pub created_at: i64,
    pub confirmed_at: Option<i64>,
    pub confirmation_key: Option<String>,
    pub outcome_json: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C>(self, _connection: &C, insert: bool) -> Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        if !insert
            || self
                .proposal_key
                .try_as_ref()
                .is_none_or(|value| !valid_token(value, 36))
            || self
                .idempotency_key
                .try_as_ref()
                .is_none_or(|value| !valid_token(value, 128))
            || self
                .requested_by_user_id
                .try_as_ref()
                .is_none_or(|value| *value <= 0)
            || self
                .plugin_id
                .try_as_ref()
                .is_none_or(|value| value.is_empty() || value.len() > 128)
            || self
                .artifact_id
                .try_as_ref()
                .is_none_or(|value| *value <= 0)
            || self.artifact_digest.try_as_ref().is_none_or(|value| {
                value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
            || self
                .deployment_generation
                .try_as_ref()
                .is_none_or(|value| *value <= 0)
            || self
                .capability_id
                .try_as_ref()
                .is_none_or(|value| value.is_empty() || value.len() > 128)
            || self
                .capability_version
                .try_as_ref()
                .is_none_or(|value| *value <= 0 || *value > i32::from(u16::MAX))
            || self
                .action_id
                .try_as_ref()
                .is_none_or(|value| value != "active-channel-monitor.target.set-enabled")
            || self.action_version.try_as_ref() != Some(&1)
            || self
                .input_json
                .try_as_ref()
                .is_none_or(|value| value.len() > 4 * 1024)
            || self.target_id.try_as_ref().is_none_or(|value| *value <= 0)
            || self
                .expected_target_version
                .try_as_ref()
                .is_none_or(|value| *value <= 0)
            || self.status.try_as_ref() != Some(&1)
            || self.expires_at.try_as_ref().is_none_or(|expires| {
                self.created_at.try_as_ref().is_none_or(|created| {
                    *created <= 0 || *expires <= *created || *expires - *created > 600
                })
            })
            || self
                .confirmed_by_user_id
                .try_as_ref()
                .is_some_and(Option::is_some)
            || self.confirmed_at.try_as_ref().is_some_and(Option::is_some)
            || self
                .confirmation_key
                .try_as_ref()
                .is_some_and(Option::is_some)
            || self.outcome_json.try_as_ref().is_some_and(Option::is_some)
        {
            return Err(DbErr::Custom("插件动作提案初始状态无效".to_owned()));
        }
        Ok(self)
    }
}

fn valid_token(value: &str, maximum: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}
