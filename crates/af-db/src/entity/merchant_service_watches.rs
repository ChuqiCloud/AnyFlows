//! User-owned subscriptions to public merchant services.
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "merchant_service_watches")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub user_id: i64,
    pub pool_id: i64,
    pub service_id: i64,
    pub active: bool,
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
        on_delete = "Cascade"
    )]
    User,
    #[sea_orm(
        belongs_to = "super::merchant_pools::Entity",
        from = "Column::PoolId",
        to = "super::merchant_pools::Column::Id",
        on_update = "Cascade",
        on_delete = "Restrict"
    )]
    Pool,
    #[sea_orm(
        belongs_to = "super::merchant_services::Entity",
        from = "Column::ServiceId",
        to = "super::merchant_services::Column::Id",
        on_update = "Cascade",
        on_delete = "Restrict"
    )]
    Service,
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::User.def()
    }
}

impl Related<super::merchant_pools::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Pool.def()
    }
}

impl Related<super::merchant_services::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Service.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
