use af_domain::QuotaDelta;
use sea_orm::{
    ColumnTrait,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{
        Expr, InsertStatement, IntoColumnRef, LockType, Query, SelectStatement, SimpleExpr,
        UpdateStatement,
    },
};

use crate::entity::{billing_batch_checkpoints, channels, tokens, users};

use super::types::{BillingBatchWrite, ChannelBillingWrite, TokenBillingWrite, UserBillingWrite};

pub(super) fn checkpoint_state(writer_key: String, lock: bool) -> SelectStatement {
    let mut statement = Query::select();
    statement
        .columns([
            billing_batch_checkpoints::Column::LastStartSequence,
            billing_batch_checkpoints::Column::LastEndSequence,
            billing_batch_checkpoints::Column::LastEventCount,
            billing_batch_checkpoints::Column::LastFingerprint,
        ])
        .from(billing_batch_checkpoints::Entity)
        .and_where(billing_batch_checkpoints::Column::WriterKey.eq(writer_key))
        .limit(1);
    if lock {
        statement.lock(LockType::Update);
    }
    statement.to_owned()
}

/// SQLite 不支持 `FOR UPDATE`，用不改变 checkpoint 的写语句取得数据库写锁。
pub(super) fn sqlite_lock_checkpoint(writer_key: String) -> UpdateStatement {
    Query::update()
        .table(billing_batch_checkpoints::Entity)
        .value(
            billing_batch_checkpoints::Column::LastEndSequence,
            Expr::col(billing_batch_checkpoints::Column::LastEndSequence),
        )
        .and_where(billing_batch_checkpoints::Column::WriterKey.eq(writer_key))
        .to_owned()
}

pub(super) fn insert_checkpoint(
    batch: &BillingBatchWrite,
    now: TimeDateTimeWithTimeZone,
) -> InsertStatement {
    Query::insert()
        .into_table(billing_batch_checkpoints::Entity)
        .columns([
            billing_batch_checkpoints::Column::WriterKey,
            billing_batch_checkpoints::Column::LastStartSequence,
            billing_batch_checkpoints::Column::LastEndSequence,
            billing_batch_checkpoints::Column::LastEventCount,
            billing_batch_checkpoints::Column::LastFingerprint,
            billing_batch_checkpoints::Column::CreatedAt,
            billing_batch_checkpoints::Column::UpdatedAt,
        ])
        .values_panic([
            batch.writer_key().into(),
            batch.start_sequence().into(),
            batch.end_sequence().into(),
            batch.event_count().into(),
            batch.fingerprint_key().into(),
            now.into(),
            now.into(),
        ])
        .to_owned()
}

pub(super) fn update_checkpoint(
    batch: &BillingBatchWrite,
    previous_start: i64,
    previous_end: i64,
    previous_event_count: i64,
    previous_fingerprint: String,
    now: TimeDateTimeWithTimeZone,
) -> UpdateStatement {
    Query::update()
        .table(billing_batch_checkpoints::Entity)
        .value(
            billing_batch_checkpoints::Column::LastStartSequence,
            batch.start_sequence(),
        )
        .value(
            billing_batch_checkpoints::Column::LastEndSequence,
            batch.end_sequence(),
        )
        .value(
            billing_batch_checkpoints::Column::LastEventCount,
            batch.event_count(),
        )
        .value(
            billing_batch_checkpoints::Column::LastFingerprint,
            batch.fingerprint_key(),
        )
        .value(billing_batch_checkpoints::Column::UpdatedAt, now)
        .and_where(billing_batch_checkpoints::Column::WriterKey.eq(batch.writer_key()))
        .and_where(billing_batch_checkpoints::Column::LastStartSequence.eq(previous_start))
        .and_where(billing_batch_checkpoints::Column::LastEndSequence.eq(previous_end))
        .and_where(billing_batch_checkpoints::Column::LastEventCount.eq(previous_event_count))
        .and_where(billing_batch_checkpoints::Column::LastFingerprint.eq(previous_fingerprint))
        .to_owned()
}

pub(super) fn apply_user(
    delta: UserBillingWrite,
    now: TimeDateTimeWithTimeZone,
) -> UpdateStatement {
    let (quota_value, quota_guard) = checked_adjustment(users::Column::Quota, delta.quota_delta());
    let (used_value, used_guard) =
        checked_adjustment(users::Column::UsedQuota, delta.used_quota_delta());
    let (requests_value, requests_guard) =
        checked_nonnegative_addition(users::Column::RequestCount, delta.request_count_delta());
    Query::update()
        .table(users::Entity)
        .value(users::Column::Quota, quota_value)
        .value(users::Column::UsedQuota, used_value)
        .value(users::Column::RequestCount, requests_value)
        .value(users::Column::UpdatedAt, now)
        .and_where(users::Column::Id.eq(delta.user_id().get()))
        .and_where(quota_guard)
        .and_where(used_guard)
        .and_where(requests_guard)
        .and_where(users::Column::FrozenQuota.gte(0_i64))
        .to_owned()
}

pub(super) fn apply_token(
    delta: TokenBillingWrite,
    now: TimeDateTimeWithTimeZone,
) -> UpdateStatement {
    let (remain_value, remain_guard) =
        checked_adjustment(tokens::Column::RemainQuota, delta.remain_quota_delta());
    let (used_value, used_guard) =
        checked_adjustment(tokens::Column::UsedQuota, delta.used_quota_delta());
    let mut statement = Query::update();
    statement
        .table(tokens::Entity)
        .value(tokens::Column::RemainQuota, remain_value)
        .value(tokens::Column::UsedQuota, used_value)
        .value(tokens::Column::UpdatedAt, now)
        .and_where(tokens::Column::Id.eq(delta.token_id().get()))
        .and_where(remain_guard)
        .and_where(used_guard);
    if !delta.remain_quota_delta().is_zero() {
        // 无限令牌的 remain_quota 只是占位值，批量路径不得改变它。
        statement.and_where(tokens::Column::UnlimitedQuota.eq(false));
    }
    statement.to_owned()
}

pub(super) fn apply_channel(
    delta: ChannelBillingWrite,
    now: TimeDateTimeWithTimeZone,
) -> UpdateStatement {
    let (used_value, used_guard) =
        checked_adjustment(channels::Column::UsedQuota, delta.used_quota_delta());
    Query::update()
        .table(channels::Entity)
        .value(channels::Column::UsedQuota, used_value)
        .value(channels::Column::UpdatedAt, now)
        .and_where(channels::Column::Id.eq(delta.channel_id().get()))
        .and_where(used_guard)
        .to_owned()
}

fn checked_adjustment<C>(column: C, delta: QuotaDelta) -> (SimpleExpr, SimpleExpr)
where
    C: IntoColumnRef + Copy,
{
    let units = delta.units();
    if units > 0 {
        (
            Expr::col(column).add(units),
            Expr::col(column)
                .gte(0_i64)
                .and(Expr::col(column).lte(i64::MAX - units)),
        )
    } else if units < 0 {
        let magnitude = -units;
        (
            Expr::col(column).sub(magnitude),
            Expr::col(column)
                .gte(magnitude)
                .and(Expr::col(column).lte(i64::MAX)),
        )
    } else {
        (
            Expr::col(column).into(),
            Expr::col(column)
                .gte(0_i64)
                .and(Expr::col(column).lte(i64::MAX)),
        )
    }
}

fn checked_nonnegative_addition<C>(column: C, delta: i64) -> (SimpleExpr, SimpleExpr)
where
    C: IntoColumnRef + Copy,
{
    (
        Expr::col(column).add(delta),
        Expr::col(column)
            .gte(0_i64)
            .and(Expr::col(column).lte(i64::MAX - delta)),
    )
}

#[cfg(test)]
mod tests {
    use af_domain::{ChannelId, TokenId, UserId};
    use sea_orm::DbBackend;

    use super::*;

    fn now() -> TimeDateTimeWithTimeZone {
        TimeDateTimeWithTimeZone::from_unix_timestamp_nanos(1_700_000_000_123_456_000)
            .expect("测试审计时间必须有效")
    }

    #[test]
    fn subject_updates_keep_checked_guards_for_all_dialects() {
        let statements = [
            apply_user(
                UserBillingWrite::new(
                    UserId::new(1).unwrap(),
                    QuotaDelta::new(-7).unwrap(),
                    QuotaDelta::new(9).unwrap(),
                    1,
                )
                .unwrap(),
                now(),
            ),
            apply_token(
                TokenBillingWrite::new(
                    TokenId::new(2).unwrap(),
                    QuotaDelta::new(-7).unwrap(),
                    QuotaDelta::new(9).unwrap(),
                )
                .unwrap(),
                now(),
            ),
            apply_channel(
                ChannelBillingWrite::new(ChannelId::new(3).unwrap(), QuotaDelta::new(9).unwrap())
                    .unwrap(),
                now(),
            ),
        ];

        for backend in [DbBackend::Postgres, DbBackend::MySql, DbBackend::Sqlite] {
            for statement in &statements {
                let built = backend.build(statement);
                assert_placeholders(backend, &built.sql);
                assert!(built.sql.matches(" AND ").count() >= 2);
                assert!(!built.sql.contains("CURRENT_TIMESTAMP"));
            }
        }
    }

    #[test]
    fn checkpoint_locks_follow_database_dialect() {
        for backend in [DbBackend::Postgres, DbBackend::MySql, DbBackend::Sqlite] {
            let built = backend.build(&checkpoint_state("a".repeat(32), true));
            if backend == DbBackend::Sqlite {
                assert!(!built.sql.contains("FOR UPDATE"));
            } else {
                assert!(built.sql.contains("FOR UPDATE"));
            }
            assert_placeholders(backend, &built.sql);
        }

        let sqlite_lock = DbBackend::Sqlite.build(&sqlite_lock_checkpoint("a".repeat(32)));
        assert!(
            sqlite_lock
                .sql
                .starts_with("UPDATE \"billing_batch_checkpoints\"")
        );
        assert_placeholders(DbBackend::Sqlite, &sqlite_lock.sql);
    }

    fn assert_placeholders(backend: DbBackend, sql: &str) {
        if backend == DbBackend::Postgres {
            assert!(sql.contains("$1"));
            assert!(!sql.contains('?'));
        } else {
            assert!(sql.contains('?'));
            assert!(!sql.contains("$1"));
        }
    }
}
