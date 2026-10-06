//! 智能路由候选实体及其运行时统计。

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "route_channels")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub route_id: i64,
    pub channel_id: i64,
    pub credential_id: i64,
    pub priority: i32,
    pub weight: i32,
    pub enabled: bool,
    pub success_count: i64,
    pub fail_count: i64,
    pub total_latency: i64,
    pub cooldown_level: i16,
    pub cooldown_until: Option<TimeDateTimeWithTimeZone>,
    pub last_selected_at: Option<TimeDateTimeWithTimeZone>,
    pub last_failure_at: Option<TimeDateTimeWithTimeZone>,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::routes::Entity",
        from = "Column::RouteId",
        to = "super::routes::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    Route,
    #[sea_orm(
        belongs_to = "super::channels::Entity",
        from = "Column::ChannelId",
        to = "super::channels::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    Channel,
    #[sea_orm(
        belongs_to = "super::credentials::Entity",
        from = "Column::CredentialId",
        to = "super::credentials::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    Credential,
}

impl Related<super::routes::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Route.def()
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
