use sea_orm_migration::prelude::*;

use crate::migration::iden::scheduler_outbox_events;

use super::{auto_id, nullable_timestamp, table, timestamp};

const DUE_INDEX: &str = "idx_scheduler_outbox_events_due";
const SUBJECT_INDEX: &str = "idx_scheduler_outbox_events_subject";

/// 创建可靠调度目录变更 outbox；表不使用主体外键，确保删除后的事件仍可投递。
pub(in crate::migration) async fn create_scheduler_outbox(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    manager
        .create_table(
            table(manager, scheduler_outbox_events::Entity)
                .col(auto_id(scheduler_outbox_events::Column::Id))
                .col(
                    ColumnDef::new(scheduler_outbox_events::Column::SubjectKind)
                        .small_integer()
                        .not_null()
                        .check(
                            Expr::col(scheduler_outbox_events::Column::SubjectKind)
                                .is_in([1_i16, 2_i16]),
                        ),
                )
                .col(
                    ColumnDef::new(scheduler_outbox_events::Column::SubjectId)
                        .big_integer()
                        .not_null()
                        .check(Expr::col(scheduler_outbox_events::Column::SubjectId).gt(0_i64)),
                )
                .col(
                    ColumnDef::new(scheduler_outbox_events::Column::Status)
                        .small_integer()
                        .not_null()
                        .default(1_i16)
                        .check(
                            Expr::col(scheduler_outbox_events::Column::Status)
                                .is_in([1_i16, 2_i16, 3_i16]),
                        ),
                )
                .col(
                    ColumnDef::new(scheduler_outbox_events::Column::AttemptCount)
                        .small_integer()
                        .not_null()
                        .default(0_i16)
                        .check(Expr::col(scheduler_outbox_events::Column::AttemptCount).gte(0_i16)),
                )
                .col(timestamp(
                    manager,
                    scheduler_outbox_events::Column::NextAttemptAt,
                ))
                .col(nullable_timestamp(
                    manager,
                    scheduler_outbox_events::Column::LeaseExpiresAt,
                ))
                .col(nullable_timestamp(
                    manager,
                    scheduler_outbox_events::Column::PublishedAt,
                ))
                .col(
                    ColumnDef::new(scheduler_outbox_events::Column::Version)
                        .big_integer()
                        .not_null()
                        .default(1_i64)
                        .check(Expr::col(scheduler_outbox_events::Column::Version).gte(1_i64)),
                )
                .col(timestamp(
                    manager,
                    scheduler_outbox_events::Column::CreatedAt,
                ))
                .col(timestamp(
                    manager,
                    scheduler_outbox_events::Column::UpdatedAt,
                ))
                .check(event_state_shape())
                .to_owned(),
        )
        .await?;
    manager
        .create_index(
            Index::create()
                .name(DUE_INDEX)
                .table(scheduler_outbox_events::Entity)
                .col(scheduler_outbox_events::Column::Status)
                .col(scheduler_outbox_events::Column::NextAttemptAt)
                .col(scheduler_outbox_events::Column::Id)
                .to_owned(),
        )
        .await?;
    manager
        .create_index(
            Index::create()
                .name(SUBJECT_INDEX)
                .table(scheduler_outbox_events::Entity)
                .col(scheduler_outbox_events::Column::SubjectKind)
                .col(scheduler_outbox_events::Column::SubjectId)
                .col(scheduler_outbox_events::Column::Id)
                .to_owned(),
        )
        .await
}

fn event_state_shape() -> SimpleExpr {
    let status = Expr::col(scheduler_outbox_events::Column::Status);
    let attempts = Expr::col(scheduler_outbox_events::Column::AttemptCount);
    let lease = Expr::col(scheduler_outbox_events::Column::LeaseExpiresAt);
    let published = Expr::col(scheduler_outbox_events::Column::PublishedAt);
    Condition::any()
        .add(
            Condition::all()
                .add(status.clone().eq(1_i16))
                .add(attempts.clone().gte(0_i16))
                .add(lease.clone().is_null())
                .add(published.clone().is_null()),
        )
        .add(
            Condition::all()
                .add(status.clone().eq(2_i16))
                .add(attempts.clone().gt(0_i16))
                .add(lease.clone().is_not_null())
                .add(published.clone().is_null()),
        )
        .add(
            Condition::all()
                .add(status.eq(3_i16))
                .add(attempts.gt(0_i16))
                .add(lease.is_null())
                .add(published.is_not_null()),
        )
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_shape_renders_for_all_supported_dialects() {
        for rendered in [
            Query::select()
                .expr(event_state_shape())
                .to_string(PostgresQueryBuilder),
            Query::select()
                .expr(event_state_shape())
                .to_string(MysqlQueryBuilder),
            Query::select()
                .expr(event_state_shape())
                .to_string(SqliteQueryBuilder),
        ] {
            assert!(rendered.contains("status"));
            assert!(rendered.contains("attempt_count"));
            assert!(rendered.contains("lease_expires_at"));
            assert!(rendered.contains("published_at"));
        }
    }
}
