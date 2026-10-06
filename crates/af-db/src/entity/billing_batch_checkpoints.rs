//! 计费批量 writer 的 exactly-once 持久化检查点。

use sea_orm::entity::prelude::*;

use super::{BillingBatchFingerprint, BillingBatchWriterKey};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "billing_batch_checkpoints")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false, column_type = "Char(Some(32))")]
    pub writer_key: BillingBatchWriterKey,
    pub last_start_sequence: i64,
    pub last_end_sequence: i64,
    pub last_event_count: i64,
    #[sea_orm(column_type = "Char(Some(64))")]
    pub last_fingerprint: BillingBatchFingerprint,
    pub created_at: TimeDateTimeWithTimeZone,
    pub updated_at: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
