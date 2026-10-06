use sea_orm_migration::prelude::*;

use crate::migration::iden::analytics_export_outbox_events;

use super::{auto_id, nullable_timestamp, table, timestamp};

const FACT_INDEX: &str = "uq_analytics_export_outbox_fact";
const DUE_INDEX: &str = "idx_analytics_export_outbox_due";
const FACT_CURSOR_INDEX: &str = "idx_analytics_export_outbox_fact_cursor";

/// 创建 ClickHouse 异步事实投递 outbox；不引用事实表，保留主库事实的独立生命周期。
pub(in crate::migration) async fn create_analytics_export_outbox(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    manager
        .create_table(
            table(manager, analytics_export_outbox_events::Entity)
                .col(auto_id(analytics_export_outbox_events::Column::Id))
                .col(
                    ColumnDef::new(analytics_export_outbox_events::Column::FactKind)
                        .small_integer()
                        .not_null()
                        .check(
                            Expr::col(analytics_export_outbox_events::Column::FactKind)
                                .is_in([1_i16, 2_i16]),
                        ),
                )
                .col(
                    ColumnDef::new(analytics_export_outbox_events::Column::FactId)
                        .big_integer()
                        .not_null()
                        .check(Expr::col(analytics_export_outbox_events::Column::FactId).gt(0_i64)),
                )
                .col(
                    ColumnDef::new(analytics_export_outbox_events::Column::Status)
                        .small_integer()
                        .not_null()
                        .default(1_i16)
                        .check(
                            Expr::col(analytics_export_outbox_events::Column::Status)
                                .is_in([1_i16, 2_i16, 3_i16]),
                        ),
                )
                .col(
                    ColumnDef::new(analytics_export_outbox_events::Column::AttemptCount)
                        .small_integer()
                        .not_null()
                        .default(0_i16)
                        .check(
                            Expr::col(analytics_export_outbox_events::Column::AttemptCount)
                                .gte(0_i16),
                        ),
                )
                .col(timestamp(
                    manager,
                    analytics_export_outbox_events::Column::NextAttemptAt,
                ))
                .col(nullable_timestamp(
                    manager,
                    analytics_export_outbox_events::Column::LeaseExpiresAt,
                ))
                .col(nullable_timestamp(
                    manager,
                    analytics_export_outbox_events::Column::PublishedAt,
                ))
                .col(
                    ColumnDef::new(analytics_export_outbox_events::Column::Version)
                        .big_integer()
                        .not_null()
                        .default(1_i64)
                        .check(
                            Expr::col(analytics_export_outbox_events::Column::Version).gte(1_i64),
                        ),
                )
                .col(timestamp(
                    manager,
                    analytics_export_outbox_events::Column::CreatedAt,
                ))
                .col(timestamp(
                    manager,
                    analytics_export_outbox_events::Column::UpdatedAt,
                ))
                .check(event_state_shape())
                .to_owned(),
        )
        .await?;

    manager
        .create_index(
            Index::create()
                .name(FACT_INDEX)
                .table(analytics_export_outbox_events::Entity)
                .col(analytics_export_outbox_events::Column::FactKind)
                .col(analytics_export_outbox_events::Column::FactId)
                .unique()
                .to_owned(),
        )
        .await?;
    manager
        .create_index(
            Index::create()
                .name(DUE_INDEX)
                .table(analytics_export_outbox_events::Entity)
                .col(analytics_export_outbox_events::Column::Status)
                .col(analytics_export_outbox_events::Column::NextAttemptAt)
                .col(analytics_export_outbox_events::Column::Id)
                .to_owned(),
        )
        .await?;
    manager
        .create_index(
            Index::create()
                .name(FACT_CURSOR_INDEX)
                .table(analytics_export_outbox_events::Entity)
                .col(analytics_export_outbox_events::Column::FactKind)
                .col(analytics_export_outbox_events::Column::FactId)
                .col(analytics_export_outbox_events::Column::Id)
                .to_owned(),
        )
        .await
}

fn event_state_shape() -> SimpleExpr {
    let status = Expr::col(analytics_export_outbox_events::Column::Status);
    let attempts = Expr::col(analytics_export_outbox_events::Column::AttemptCount);
    let lease = Expr::col(analytics_export_outbox_events::Column::LeaseExpiresAt);
    let published = Expr::col(analytics_export_outbox_events::Column::PublishedAt);
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
            assert!(rendered.contains("lease_expires_at"));
            assert!(rendered.contains("published_at"));
        }
    }
}
