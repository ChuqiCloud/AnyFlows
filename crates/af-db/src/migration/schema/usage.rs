use sea_orm::{ConnectionTrait, DbBackend, Statement};
use sea_orm_migration::prelude::*;

use crate::migration::iden::{groups, tokens, usage_logs, users};

use super::{auto_id, table, timestamp};

/// 创建当前计费生命周期能够可靠提供的最小用量日志事实。
pub(in crate::migration) async fn create_usage_logs(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    let mut statement = table(manager, usage_logs::Entity);
    statement
        .col(auto_id(usage_logs::Column::Id))
        .col(
            ColumnDef::new(usage_logs::Column::EventId)
                .char_len(32)
                .not_null(),
        )
        .col(
            ColumnDef::new(usage_logs::Column::EventType)
                .small_integer()
                .not_null()
                .default(1_i16)
                .check(Expr::col(usage_logs::Column::EventType).eq(1_i16)),
        )
        .col(
            ColumnDef::new(usage_logs::Column::UserId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(usage_logs::Column::TokenId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(usage_logs::Column::GroupId)
                .big_integer()
                .not_null(),
        )
        .col(ColumnDef::new(usage_logs::Column::OrganizationId).big_integer())
        .col(ColumnDef::new(usage_logs::Column::OrganizationTeamId).big_integer())
        .col(
            ColumnDef::new(usage_logs::Column::BillingMode)
                .small_integer()
                .not_null()
                .check(Expr::col(usage_logs::Column::BillingMode).is_in([1_i16, 2_i16])),
        )
        .col(non_negative(usage_logs::Column::InputTokens))
        .col(non_negative(usage_logs::Column::OutputTokens))
        .col(non_negative(usage_logs::Column::CacheRead))
        .col(non_negative(usage_logs::Column::CacheCreation5m))
        .col(non_negative(usage_logs::Column::CacheCreation1h))
        .col(non_negative(usage_logs::Column::ReasoningTokens))
        .col(non_negative(usage_logs::Column::AudioInputTokens))
        .col(non_negative(usage_logs::Column::AudioOutputTokens))
        .col(
            ColumnDef::new(usage_logs::Column::UsageSource)
                .small_integer()
                .not_null()
                .check(Expr::col(usage_logs::Column::UsageSource).is_in([1_i16, 2_i16])),
        )
        .col(
            ColumnDef::new(usage_logs::Column::UsageSemantics)
                .small_integer()
                .not_null()
                .check(Expr::col(usage_logs::Column::UsageSemantics).is_in([1_i16, 2_i16])),
        )
        .col(non_negative(usage_logs::Column::Quota))
        .col(timestamp(manager, usage_logs::Column::CreatedAt))
        .foreign_key(
            ForeignKey::create()
                .name("fk_usage_logs_user")
                .from(usage_logs::Entity, usage_logs::Column::UserId)
                .to(users::Entity, users::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_usage_logs_token")
                .from(usage_logs::Entity, usage_logs::Column::TokenId)
                .to(tokens::Entity, tokens::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_usage_logs_group")
                .from(usage_logs::Entity, usage_logs::Column::GroupId)
                .to(groups::Entity, groups::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .check(hex_format_check(manager.get_database_backend()))
        .check(Expr::col(usage_logs::Column::EventId).ne("00000000000000000000000000000000"));
    manager.create_table(statement).await
}

/// 创建用量日志的幂等与看板基础索引。
pub(in crate::migration) async fn create_usage_logs_indexes(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    for index in [
        Index::create()
            .name("uq_usage_logs_event_id")
            .table(usage_logs::Entity)
            .col(usage_logs::Column::EventId)
            .unique()
            .to_owned(),
        Index::create()
            .name("idx_usage_logs_user_created")
            .table(usage_logs::Entity)
            .col(usage_logs::Column::UserId)
            .col(usage_logs::Column::CreatedAt)
            .to_owned(),
        Index::create()
            .name("idx_usage_logs_token_created")
            .table(usage_logs::Entity)
            .col(usage_logs::Column::TokenId)
            .col(usage_logs::Column::CreatedAt)
            .to_owned(),
    ] {
        manager.create_index(index).await?;
    }
    Ok(())
}

fn non_negative<T>(column: T) -> ColumnDef
where
    T: IntoIden + Copy + 'static,
{
    let mut definition = ColumnDef::new(column);
    definition
        .big_integer()
        .not_null()
        .check(Expr::col(column).gte(0_i64));
    definition
}

fn hex_format_check(database_backend: DbBackend) -> SimpleExpr {
    match database_backend {
        DbBackend::Postgres => Expr::cust(r#""event_id" ~ '^[0-9a-f]{32}$'"#),
        DbBackend::MySql => {
            Expr::cust("CHAR_LENGTH(`event_id`) = 32 AND `event_id` REGEXP '^[0-9a-f]{32}$'")
        }
        DbBackend::Sqlite => {
            Expr::cust(r#"length("event_id") = 32 AND "event_id" NOT GLOB '*[^0-9a-f]*'"#)
        }
    }
}

const USAGE_LOGS_TABLE: &str = "usage_logs";
const USAGE_LOGS_NEXT_TABLE: &str = "usage_logs_next_v55";

/// 重建用量日志表，把按次计费加入闭合集合并保留现有音视频审计列。
///
/// MySQL DDL 失败可能留下恢复表；重试只会在权威源表仍存在时清理中间副本。
pub(in crate::migration) async fn rebuild_usage_logs_for_per_call(
    manager: &SchemaManager<'_>,
    allow_per_call: bool,
) -> Result<(), DbErr> {
    if !allow_per_call {
        ensure_no_per_call_usage(manager).await?;
    }
    let current_exists = manager.has_table(USAGE_LOGS_TABLE).await?;
    let next_exists = manager.has_table(USAGE_LOGS_NEXT_TABLE).await?;
    match (current_exists, next_exists) {
        (true, _) => {
            if next_exists {
                manager
                    .drop_table(
                        Table::drop()
                            .table(Alias::new(USAGE_LOGS_NEXT_TABLE))
                            .to_owned(),
                    )
                    .await?;
            }
            create_latest_usage_table(manager, allow_per_call).await?;
            copy_usage_logs(manager).await?;
            manager
                .drop_table(Table::drop().table(Alias::new(USAGE_LOGS_TABLE)).to_owned())
                .await?;
            rename_usage_logs(manager).await?;
            create_usage_logs_indexes(manager).await?;
            reset_postgres_usage_sequence(manager).await
        }
        (false, true) => {
            rename_usage_logs(manager).await?;
            create_usage_logs_indexes(manager).await?;
            reset_postgres_usage_sequence(manager).await
        }
        (false, false) => Err(DbErr::Custom("用量日志表重建缺少源表和恢复表".to_owned())),
    }
}

async fn create_latest_usage_table(
    manager: &SchemaManager<'_>,
    allow_per_call: bool,
) -> Result<(), DbErr> {
    let table_name = Alias::new(USAGE_LOGS_NEXT_TABLE);
    let billing_modes = if allow_per_call {
        vec![1_i16, 2_i16, 3_i16]
    } else {
        vec![1_i16, 2_i16]
    };
    let statement = table(manager, table_name.clone())
        .col(auto_id(usage_logs::Column::Id))
        .col(
            ColumnDef::new(usage_logs::Column::EventId)
                .char_len(32)
                .not_null(),
        )
        .col(
            ColumnDef::new(usage_logs::Column::EventType)
                .small_integer()
                .not_null()
                .default(1_i16)
                .check(Expr::col(usage_logs::Column::EventType).eq(1_i16)),
        )
        .col(
            ColumnDef::new(usage_logs::Column::UserId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(usage_logs::Column::TokenId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(usage_logs::Column::GroupId)
                .big_integer()
                .not_null(),
        )
        .col(ColumnDef::new(usage_logs::Column::OrganizationId).big_integer())
        .col(ColumnDef::new(usage_logs::Column::OrganizationTeamId).big_integer())
        .col(
            ColumnDef::new(usage_logs::Column::BillingMode)
                .small_integer()
                .not_null()
                .check(Expr::col(usage_logs::Column::BillingMode).is_in(billing_modes)),
        )
        .col(non_negative(usage_logs::Column::InputTokens))
        .col(non_negative(usage_logs::Column::OutputTokens))
        .col(non_negative(usage_logs::Column::CacheRead))
        .col(non_negative(usage_logs::Column::CacheCreation5m))
        .col(non_negative(usage_logs::Column::CacheCreation1h))
        .col(non_negative(usage_logs::Column::ReasoningTokens))
        .col(non_negative(usage_logs::Column::AudioInputTokens))
        .col(non_negative(usage_logs::Column::AudioOutputTokens))
        .col(
            ColumnDef::new(usage_logs::Column::AudioDurationNanoseconds)
                .big_integer()
                .check(
                    Expr::col(usage_logs::Column::AudioDurationNanoseconds)
                        .is_null()
                        .or(Expr::col(usage_logs::Column::AudioDurationNanoseconds)
                            .between(0_i64, 86_400_000_000_000_i64)),
                ),
        )
        .col(
            ColumnDef::new(usage_logs::Column::VideoDurationSeconds)
                .big_integer()
                .check(
                    Expr::col(usage_logs::Column::VideoDurationSeconds)
                        .is_null()
                        .or(Expr::col(usage_logs::Column::VideoDurationSeconds)
                            .between(1_i64, 86_400_i64)),
                ),
        )
        .col(
            ColumnDef::new(usage_logs::Column::VideoResolution)
                .small_integer()
                .check(
                    Expr::col(usage_logs::Column::VideoResolution)
                        .is_null()
                        .or(Expr::col(usage_logs::Column::VideoResolution).is_in([1_i16, 2, 3])),
                ),
        )
        .col(
            ColumnDef::new(usage_logs::Column::UsageSource)
                .small_integer()
                .not_null()
                .check(Expr::col(usage_logs::Column::UsageSource).is_in([1_i16, 2_i16])),
        )
        .col(
            ColumnDef::new(usage_logs::Column::UsageSemantics)
                .small_integer()
                .not_null()
                .check(Expr::col(usage_logs::Column::UsageSemantics).is_in([1_i16, 2_i16])),
        )
        .col(non_negative(usage_logs::Column::Quota))
        .col(timestamp(manager, usage_logs::Column::CreatedAt))
        .foreign_key(
            ForeignKey::create()
                .name(if allow_per_call {
                    "fk_usage_logs_v55_user"
                } else {
                    "fk_usage_logs_v55d_user"
                })
                .from(table_name.clone(), usage_logs::Column::UserId)
                .to(users::Entity, users::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name(if allow_per_call {
                    "fk_usage_logs_v55_token"
                } else {
                    "fk_usage_logs_v55d_token"
                })
                .from(table_name.clone(), usage_logs::Column::TokenId)
                .to(tokens::Entity, tokens::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name(if allow_per_call {
                    "fk_usage_logs_v55_group"
                } else {
                    "fk_usage_logs_v55d_group"
                })
                .from(table_name, usage_logs::Column::GroupId)
                .to(groups::Entity, groups::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .check(hex_format_check(manager.get_database_backend()))
        .check(Expr::col(usage_logs::Column::EventId).ne("00000000000000000000000000000000"))
        .to_owned();
    manager.create_table(statement).await
}

async fn ensure_no_per_call_usage(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let table_name = if manager.has_table(USAGE_LOGS_TABLE).await? {
        USAGE_LOGS_TABLE
    } else {
        USAGE_LOGS_NEXT_TABLE
    };
    let sql = match manager.get_database_backend() {
        DbBackend::MySql => {
            format!("SELECT 1 FROM `{table_name}` WHERE `billing_mode` = 3 LIMIT 1")
        }
        DbBackend::Postgres | DbBackend::Sqlite => {
            format!("SELECT 1 FROM \"{table_name}\" WHERE \"billing_mode\" = 3 LIMIT 1")
        }
    };
    if manager
        .get_connection()
        .query_one(Statement::from_string(manager.get_database_backend(), sql))
        .await?
        .is_some()
    {
        return Err(DbErr::Custom(
            "存在按次用量记录，无法回退用量计费模式".to_owned(),
        ));
    }
    Ok(())
}

async fn copy_usage_logs(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let columns = "id, event_id, event_type, user_id, token_id, group_id, billing_mode, \
input_tokens, output_tokens, cache_read, cache_creation_5m, cache_creation_1h, \
reasoning_tokens, audio_input_tokens, audio_output_tokens, audio_duration_nanoseconds, \
video_duration_seconds, video_resolution, usage_source, usage_semantics, quota, created_at";
    let sql = match manager.get_database_backend() {
        DbBackend::MySql => format!(
            "INSERT INTO `{USAGE_LOGS_NEXT_TABLE}` (`{}`) SELECT `{}` FROM `{USAGE_LOGS_TABLE}`",
            columns.replace(", ", "`, `"),
            columns.replace(", ", "`, `")
        ),
        DbBackend::Postgres | DbBackend::Sqlite => format!(
            "INSERT INTO \"{USAGE_LOGS_NEXT_TABLE}\" (\"{}\") SELECT \"{}\" FROM \"{USAGE_LOGS_TABLE}\"",
            columns.replace(", ", "\", \""),
            columns.replace(", ", "\", \"")
        ),
    };
    manager.get_connection().execute_unprepared(&sql).await?;
    Ok(())
}

async fn rename_usage_logs(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let sql = match manager.get_database_backend() {
        DbBackend::MySql => {
            format!("RENAME TABLE `{USAGE_LOGS_NEXT_TABLE}` TO `{USAGE_LOGS_TABLE}`")
        }
        DbBackend::Postgres | DbBackend::Sqlite => {
            format!("ALTER TABLE \"{USAGE_LOGS_NEXT_TABLE}\" RENAME TO \"{USAGE_LOGS_TABLE}\"")
        }
    };
    manager.get_connection().execute_unprepared(&sql).await?;
    Ok(())
}

async fn reset_postgres_usage_sequence(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    if manager.get_database_backend() != DbBackend::Postgres {
        return Ok(());
    }
    manager
        .get_connection()
        .execute_unprepared(
            r#"
SELECT setval(
    pg_get_serial_sequence('"usage_logs"', 'id'),
    COALESCE((SELECT MAX("id") FROM "usage_logs"), 1),
    EXISTS (SELECT 1 FROM "usage_logs")
)"#,
        )
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postgres_event_id_check_uses_identifier_quotes_without_backslashes() {
        let rendered = Query::select()
            .expr(hex_format_check(DbBackend::Postgres))
            .to_owned()
            .to_string(PostgresQueryBuilder);
        assert!(rendered.contains(r#""event_id" ~ '^[0-9a-f]{32}$'"#));
        assert!(!rendered.contains('\\'));
    }

    #[test]
    fn mysql_event_id_check_does_not_use_binary_regex_operands() {
        let rendered = Query::select()
            .expr(hex_format_check(DbBackend::MySql))
            .to_owned()
            .to_string(MysqlQueryBuilder);
        assert!(rendered.contains("REGEXP '^[0-9a-f]{32}$'"));
        assert!(!rendered.contains("BINARY"));
    }
}
