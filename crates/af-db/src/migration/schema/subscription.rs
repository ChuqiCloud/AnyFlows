use sea_orm::DbBackend;
use sea_orm_migration::prelude::*;

use crate::migration::iden::{subscription_plans, user_subscriptions, users};

use super::{auto_id, nullable_timestamp, table, timestamp};

/// 创建订阅计划与用户订阅时间窗表。
pub(in crate::migration) async fn create_subscription_storage(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    create_plans(manager).await?;
    create_user_subscriptions(manager).await?;
    create_indexes(manager).await
}

async fn create_plans(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let mut statement = table(manager, subscription_plans::Entity);
    statement
        .col(auto_id(subscription_plans::Column::Id))
        .col(
            ColumnDef::new(subscription_plans::Column::PlanKey)
                .char_len(32)
                .not_null(),
        )
        .col(
            ColumnDef::new(subscription_plans::Column::Name)
                .string_len(80)
                .not_null(),
        )
        .col(
            ColumnDef::new(subscription_plans::Column::CreatedByUserId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(subscription_plans::Column::Status)
                .small_integer()
                .not_null()
                .default(1_i16),
        )
        .col(
            ColumnDef::new(subscription_plans::Column::QuotaAmount)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(subscription_plans::Column::Cycle)
                .small_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(subscription_plans::Column::Version)
                .big_integer()
                .not_null()
                .default(1_i64),
        )
        .col(nullable_timestamp(
            manager,
            subscription_plans::Column::DisabledAt,
        ))
        .col(timestamp(manager, subscription_plans::Column::CreatedAt))
        .col(timestamp(manager, subscription_plans::Column::UpdatedAt))
        .foreign_key(
            ForeignKey::create()
                .name("fk_subscription_plans_creator")
                .from(
                    subscription_plans::Entity,
                    subscription_plans::Column::CreatedByUserId,
                )
                .to(users::Entity, users::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .check(hex_check(manager.get_database_backend(), "plan_key", true))
        .check(Expr::col(subscription_plans::Column::Name).ne(""))
        .check(Expr::col(subscription_plans::Column::Status).is_in([1_i16, 2_i16]))
        .check(Expr::col(subscription_plans::Column::QuotaAmount).gt(0_i64))
        .check(Expr::col(subscription_plans::Column::Cycle).is_in([1_i16, 2_i16, 3_i16, 4_i16]))
        .check(Expr::col(subscription_plans::Column::Version).gt(0_i64))
        .check(
            Expr::col(subscription_plans::Column::Status)
                .eq(1_i16)
                .and(Expr::col(subscription_plans::Column::DisabledAt).is_null())
                .or(Expr::col(subscription_plans::Column::Status)
                    .eq(2_i16)
                    .and(Expr::col(subscription_plans::Column::DisabledAt).is_not_null())),
        )
        .check(
            Expr::col(subscription_plans::Column::DisabledAt)
                .is_null()
                .or(Expr::col(subscription_plans::Column::DisabledAt)
                    .gte(Expr::col(subscription_plans::Column::CreatedAt))),
        )
        .check(
            Expr::col(subscription_plans::Column::UpdatedAt)
                .gte(Expr::col(subscription_plans::Column::CreatedAt)),
        )
        .check(
            Expr::col(subscription_plans::Column::DisabledAt)
                .is_null()
                .or(Expr::col(subscription_plans::Column::UpdatedAt)
                    .gte(Expr::col(subscription_plans::Column::DisabledAt))),
        );
    manager.create_table(statement).await
}

async fn create_user_subscriptions(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let mut statement = table(manager, user_subscriptions::Entity);
    statement
        .col(auto_id(user_subscriptions::Column::Id))
        .col(
            ColumnDef::new(user_subscriptions::Column::SubscriptionKey)
                .char_len(32)
                .not_null(),
        )
        .col(
            ColumnDef::new(user_subscriptions::Column::UserId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(user_subscriptions::Column::PlanId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(user_subscriptions::Column::PlanVersion)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(user_subscriptions::Column::Status)
                .small_integer()
                .not_null()
                .default(1_i16),
        )
        .col(
            ColumnDef::new(user_subscriptions::Column::QuotaAmount)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(user_subscriptions::Column::QuotaUsed)
                .big_integer()
                .not_null()
                .default(0_i64),
        )
        .col(
            ColumnDef::new(user_subscriptions::Column::Cycle)
                .small_integer()
                .not_null(),
        )
        .col(timestamp(
            manager,
            user_subscriptions::Column::WindowStartedAt,
        ))
        .col(timestamp(manager, user_subscriptions::Column::WindowEndsAt))
        .col(
            ColumnDef::new(user_subscriptions::Column::Version)
                .big_integer()
                .not_null()
                .default(1_i64),
        )
        .col(timestamp(manager, user_subscriptions::Column::BoundAt))
        .col(timestamp(
            manager,
            user_subscriptions::Column::StatusChangedAt,
        ))
        .col(timestamp(manager, user_subscriptions::Column::CreatedAt))
        .col(timestamp(manager, user_subscriptions::Column::UpdatedAt))
        .foreign_key(
            ForeignKey::create()
                .name("fk_user_subscriptions_user")
                .from(
                    user_subscriptions::Entity,
                    user_subscriptions::Column::UserId,
                )
                .to(users::Entity, users::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_user_subscriptions_plan")
                .from(
                    user_subscriptions::Entity,
                    user_subscriptions::Column::PlanId,
                )
                .to(subscription_plans::Entity, subscription_plans::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .check(hex_check(
            manager.get_database_backend(),
            "subscription_key",
            true,
        ))
        .check(Expr::col(user_subscriptions::Column::PlanVersion).gt(0_i64))
        .check(Expr::col(user_subscriptions::Column::Status).is_in([1_i16, 2_i16, 3_i16, 4_i16]))
        .check(Expr::col(user_subscriptions::Column::QuotaAmount).gt(0_i64))
        .check(Expr::col(user_subscriptions::Column::QuotaUsed).gte(0_i64))
        .check(
            Expr::col(user_subscriptions::Column::QuotaUsed)
                .lte(Expr::col(user_subscriptions::Column::QuotaAmount)),
        )
        .check(Expr::col(user_subscriptions::Column::Cycle).is_in([1_i16, 2_i16, 3_i16, 4_i16]))
        .check(
            Expr::col(user_subscriptions::Column::WindowEndsAt)
                .gt(Expr::col(user_subscriptions::Column::WindowStartedAt)),
        )
        .check(
            Expr::col(user_subscriptions::Column::BoundAt)
                .lt(Expr::col(user_subscriptions::Column::WindowEndsAt)),
        )
        .check(Expr::col(user_subscriptions::Column::Version).gt(0_i64))
        .check(
            Expr::col(user_subscriptions::Column::StatusChangedAt)
                .gte(Expr::col(user_subscriptions::Column::BoundAt)),
        )
        .check(
            Expr::col(user_subscriptions::Column::CreatedAt)
                .eq(Expr::col(user_subscriptions::Column::BoundAt)),
        )
        .check(
            Expr::col(user_subscriptions::Column::UpdatedAt)
                .gte(Expr::col(user_subscriptions::Column::CreatedAt)),
        )
        .check(
            Expr::col(user_subscriptions::Column::UpdatedAt)
                .gte(Expr::col(user_subscriptions::Column::StatusChangedAt)),
        );
    manager.create_table(statement).await
}

async fn create_indexes(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    for index in [
        Index::create()
            .name("uq_subscription_plans_plan_key")
            .table(subscription_plans::Entity)
            .col(subscription_plans::Column::PlanKey)
            .unique()
            .to_owned(),
        Index::create()
            .name("idx_subscription_plans_status_id")
            .table(subscription_plans::Entity)
            .col(subscription_plans::Column::Status)
            .col(subscription_plans::Column::Id)
            .to_owned(),
        Index::create()
            .name("idx_subscription_plans_creator")
            .table(subscription_plans::Entity)
            .col(subscription_plans::Column::CreatedByUserId)
            .col(subscription_plans::Column::Id)
            .to_owned(),
        Index::create()
            .name("uq_user_subscriptions_subscription_key")
            .table(user_subscriptions::Entity)
            .col(user_subscriptions::Column::SubscriptionKey)
            .unique()
            .to_owned(),
        Index::create()
            .name("idx_user_subscriptions_user_window")
            .table(user_subscriptions::Entity)
            .col(user_subscriptions::Column::UserId)
            .col(user_subscriptions::Column::Status)
            .col(user_subscriptions::Column::WindowEndsAt)
            .col(user_subscriptions::Column::Id)
            .to_owned(),
        Index::create()
            .name("idx_user_subscriptions_plan_status")
            .table(user_subscriptions::Entity)
            .col(user_subscriptions::Column::PlanId)
            .col(user_subscriptions::Column::Status)
            .col(user_subscriptions::Column::Id)
            .to_owned(),
        Index::create()
            .name("idx_user_subscriptions_reset_due")
            .table(user_subscriptions::Entity)
            .col(user_subscriptions::Column::Status)
            .col(user_subscriptions::Column::WindowEndsAt)
            .col(user_subscriptions::Column::Id)
            .to_owned(),
    ] {
        manager.create_index(index).await?;
    }
    Ok(())
}

fn hex_check(database_backend: DbBackend, column: &str, non_zero: bool) -> SimpleExpr {
    let zero_clause = if non_zero {
        match database_backend {
            DbBackend::Postgres => format!(r#" AND "{column}" <> '{}'"#, "0".repeat(32)),
            DbBackend::MySql => format!(" AND `{column}` <> '{}'", "0".repeat(32)),
            DbBackend::Sqlite => format!(r#" AND "{column}" <> '{}'"#, "0".repeat(32)),
        }
    } else {
        String::new()
    };
    let sql = match database_backend {
        DbBackend::Postgres => format!(r#""{column}" ~ '^[0-9a-f]{{32}}$'{zero_clause}"#),
        DbBackend::MySql => format!(
            "CHAR_LENGTH(`{column}`) = 32 AND `{column}` REGEXP '^[0-9a-f]{{32}}$'{zero_clause}"
        ),
        DbBackend::Sqlite => {
            format!(r#"length("{column}") = 32 AND "{column}" NOT GLOB '*[^0-9a-f]*'{zero_clause}"#)
        }
    };
    Expr::cust(sql)
}

#[cfg(test)]
mod tests {
    use sea_orm::sea_query::{MysqlQueryBuilder, PostgresQueryBuilder, Query, SqliteQueryBuilder};

    use super::*;

    #[test]
    fn mysql_identifier_checks_do_not_use_binary_regex_operands() {
        let rendered = Query::select()
            .expr(hex_check(DbBackend::MySql, "subscription_key", true))
            .to_owned()
            .to_string(MysqlQueryBuilder);
        assert!(rendered.contains("REGEXP '^[0-9a-f]"));
        assert!(!rendered.contains("BINARY"));
    }

    #[test]
    fn quoted_identifier_checks_do_not_emit_escape_characters() {
        let postgres = Query::select()
            .expr(hex_check(DbBackend::Postgres, "plan_key", true))
            .to_owned()
            .to_string(PostgresQueryBuilder);
        let sqlite = Query::select()
            .expr(hex_check(DbBackend::Sqlite, "subscription_key", true))
            .to_owned()
            .to_string(SqliteQueryBuilder);

        assert!(postgres.contains(r#""plan_key" ~ '^[0-9a-f]{32}$'"#));
        assert!(sqlite.contains(r#"length("subscription_key") = 32"#));
        assert!(!postgres.contains('\\'));
        assert!(!sqlite.contains('\\'));
    }
}
