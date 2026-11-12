use sea_orm::entity::prelude::*;

#[derive(Clone, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "user_access_token_proofs")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub proof_hash: String,
    pub user_id: i64,
    pub session_version: i64,
    pub session_binding: String,
    pub purpose: String,
    pub target_id: Option<i64>,
    pub payload_digest: String,
    pub issued_at: i64,
    pub expires_at: i64,
    pub consumed_at: Option<i64>,
}

impl std::fmt::Debug for Model {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("UserAccessTokenProof(<redacted>)")
    }
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
