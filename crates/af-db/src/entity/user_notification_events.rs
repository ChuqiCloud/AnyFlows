//! 用户通知事实账本实体；只保存有限业务快照，不保存正文或收件人。
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "user_notification_events")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub user_id: i64,
    pub kind: i16,
    pub channel: i16,
    pub template_version: String,
    pub occurred_at: TimeDateTimeWithTimeZone,
    pub delivery_state: i16,
    pub delivery_attempts: i16,
    pub source_kind: i16,
    pub source_key: String,
    pub observed_quota: Option<i64>,
    pub threshold_quota: Option<i64>,
    pub subscription_id: Option<String>,
    pub window_ends_at: Option<TimeDateTimeWithTimeZone>,
    pub quota_amount: Option<i64>,
    pub quota_used: Option<i64>,
    pub threshold_percent: Option<i16>,
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
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::User.def()
    }
}
