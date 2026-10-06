//! 调度目录变更 outbox 实体。

use sea_orm::entity::prelude::*;

/// 调度目录变更只保存主体，不携带模型、URL、Header 或凭据内容。
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "scheduler_outbox_events")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub subject_kind: i16,
    pub subject_id: i64,
    pub status: i16,
    pub attempt_count: i16,
    pub next_attempt_at: TimeDateTimeWithTimeZone,
    pub lease_expires_at: Option<TimeDateTimeWithTimeZone>,
    pub published_at: Option<TimeDateTimeWithTimeZone>,
    pub version: i64,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C>(self, _database: &C, insert: bool) -> Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        // outbox 初始事件只能以待投递形态插入；后续状态必须由带版本条件的仓储推进。
        if !insert
            || !matches!(self.subject_kind.try_as_ref().copied(), Some(1 | 2))
            || self.subject_id.try_as_ref().copied().unwrap_or_default() <= 0
            || self.status.try_as_ref().copied() != Some(1)
            || self.attempt_count.try_as_ref().copied() != Some(0)
            || self
                .lease_expires_at
                .try_as_ref()
                .is_some_and(|value| value.is_some())
            || self
                .published_at
                .try_as_ref()
                .is_some_and(|value| value.is_some())
            || self.version.try_as_ref().copied() != Some(1)
            || self.next_attempt_at.try_as_ref().is_none()
            || self.created_at.try_as_ref().is_none()
            || self.updated_at.try_as_ref().is_none()
        {
            return Err(DbErr::Custom("调度 outbox 初始状态无效".to_owned()));
        }
        Ok(self)
    }
}
