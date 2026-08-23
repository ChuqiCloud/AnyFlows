//! 企业邀请实体；邮箱密文、指纹和令牌摘要均禁止进入公开投影。
use sea_orm::entity::prelude::*;

use super::EncryptedJson;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "organization_invitations")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(column_type = "Char(Some(32))")]
    pub invitation_key: String,
    pub organization_id: i64,
    #[sea_orm(column_type = "Char(Some(64))")]
    pub email_fingerprint: String,
    #[sea_orm(column_type = "JsonBinary")]
    pub email_secret: EncryptedJson,
    #[sea_orm(column_type = "Char(Some(64))")]
    pub token_digest: String,
    pub role: i16,
    pub team_id: Option<i64>,
    pub status: i16,
    pub invited_by_user_id: i64,
    pub accepted_by_user_id: Option<i64>,
    #[sea_orm(column_type = "Char(Some(32))", nullable)]
    pub batch_key: Option<String>,
    #[sea_orm(column_type = "Char(Some(64))", nullable)]
    pub batch_digest: Option<String>,
    pub batch_size: Option<i32>,
    pub version: i64,
    pub created_at: TimeDateTimeWithTimeZone,
    pub expires_at: TimeDateTimeWithTimeZone,
    pub finalized_at: Option<TimeDateTimeWithTimeZone>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::organizations::Entity",
        from = "Column::OrganizationId",
        to = "super::organizations::Column::Id",
        on_update = "Restrict",
        on_delete = "Restrict"
    )]
    Organization,
    #[sea_orm(
        belongs_to = "super::organization_teams::Entity",
        from = "Column::TeamId",
        to = "super::organization_teams::Column::Id",
        on_update = "Restrict",
        on_delete = "Restrict"
    )]
    Team,
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::InvitedByUserId",
        to = "super::users::Column::Id",
        on_update = "Restrict",
        on_delete = "Restrict"
    )]
    InvitedBy,
}

impl Related<super::organizations::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Organization.def()
    }
}

impl Related<super::organization_teams::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Team.def()
    }
}
