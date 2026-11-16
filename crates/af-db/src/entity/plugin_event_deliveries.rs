//! Durable, host-owned delivery state for explicitly registered plugin events.

use sea_orm::entity::prelude::*;

/// Rows contain only a bounded, host-projected JSON payload. The plugin never owns this queue.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "plugin_event_deliveries")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub plugin_id: String,
    pub event_key: String,
    pub event_kind: String,
    pub artifact_id: i64,
    pub artifact_digest: String,
    pub deployment_generation: i64,
    pub capability_id: String,
    pub capability_version: i32,
    pub payload_json: String,
    pub result_json: Option<String>,
    pub status: i16,
    pub attempt_count: i16,
    pub max_attempts: i16,
    pub next_attempt_at: i64,
    pub lease_expires_at: Option<i64>,
    pub completed_at: Option<i64>,
    pub expires_at: i64,
    pub version: i64,
    pub created_at: i64,
    pub updated_at: i64,
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
            || !matches!(
                self.event_kind.try_as_ref().map(String::as_str),
                Some("channel.health.changed" | "merchant.service.changed" | "report.requested")
            )
            || self
                .plugin_id
                .try_as_ref()
                .is_none_or(|value| value.is_empty() || value.len() > 128)
            || self
                .event_key
                .try_as_ref()
                .is_none_or(|value| value.is_empty() || value.len() > 128)
            || self
                .artifact_id
                .try_as_ref()
                .is_none_or(|value| *value <= 0)
            || self
                .artifact_digest
                .try_as_ref()
                .is_none_or(|value| value.len() != 64)
            || self
                .deployment_generation
                .try_as_ref()
                .is_none_or(|value| *value <= 0)
            || self
                .capability_version
                .try_as_ref()
                .is_none_or(|value| *value <= 0 || *value > i32::from(u16::MAX))
            || self
                .payload_json
                .try_as_ref()
                .is_none_or(|value| value.len() > 32 * 1024)
            || self.result_json.try_as_ref().is_some_and(Option::is_some)
            || self.status.try_as_ref() != Some(&1)
            || self.attempt_count.try_as_ref() != Some(&0)
            || self
                .lease_expires_at
                .try_as_ref()
                .is_some_and(Option::is_some)
            || self.completed_at.try_as_ref().is_some_and(Option::is_some)
            || self
                .max_attempts
                .try_as_ref()
                .is_none_or(|value| *value <= 0 || *value > 12)
            || self.expires_at.try_as_ref().is_none_or(|expires| {
                self.created_at.try_as_ref().is_none_or(|created| {
                    *created <= 0
                        || *expires <= *created
                        || expires.saturating_sub(*created) > 30 * 24 * 60 * 60
                })
            })
            || self.next_attempt_at.try_as_ref().is_none()
            || self.version.try_as_ref() != Some(&1)
            || self.created_at.try_as_ref().is_none()
            || self.updated_at.try_as_ref().is_none()
        {
            return Err(DbErr::Custom("插件事件投递初始状态无效".to_owned()));
        }
        Ok(self)
    }
}
