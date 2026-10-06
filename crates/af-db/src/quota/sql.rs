use af_domain::{BillingContractPriceSnapshot, GatewayPrincipal, Quota};
use sea_orm::{
    ColumnTrait,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{
        Expr, InsertStatement, IntoColumnRef, LockType, Query, SelectStatement, SimpleExpr,
        UpdateStatement,
    },
};

use crate::entity::{billing_reservations, groups, tokens, users};

use super::state::{QuotaFundingSource, QuotaReservationKind, QuotaReservationStatus};

pub(crate) fn insert_reservation(
    key: String,
    principal: GatewayPrincipal,
    amount: Quota,
    reservation_kind: QuotaReservationKind,
    now: TimeDateTimeWithTimeZone,
    expires_at: TimeDateTimeWithTimeZone,
    contract_price: Option<BillingContractPriceSnapshot>,
) -> InsertStatement {
    let contract_prices = contract_price.map(|snapshot| snapshot.prices());
    Query::insert()
        .into_table(billing_reservations::Entity)
        .columns([
            billing_reservations::Column::IdempotencyKey,
            billing_reservations::Column::UserId,
            billing_reservations::Column::TokenId,
            billing_reservations::Column::GroupId,
            billing_reservations::Column::OrganizationId,
            billing_reservations::Column::ContractPriceId,
            billing_reservations::Column::ContractPriceVersion,
            billing_reservations::Column::ContractInputPrice,
            billing_reservations::Column::ContractOutputPrice,
            billing_reservations::Column::ContractCacheReadPrice,
            billing_reservations::Column::ContractCacheCreation5mPrice,
            billing_reservations::Column::ContractCacheCreation1hPrice,
            billing_reservations::Column::Status,
            billing_reservations::Column::ReservationKind,
            billing_reservations::Column::FundingSource,
            billing_reservations::Column::ReservedQuota,
            billing_reservations::Column::TokenReservedQuota,
            billing_reservations::Column::ActualQuota,
            billing_reservations::Column::ExpiresAt,
            billing_reservations::Column::FinalizedAt,
            billing_reservations::Column::CreatedAt,
            billing_reservations::Column::UpdatedAt,
        ])
        .values_panic([
            key.into(),
            principal.user_id().get().into(),
            principal.token_id().get().into(),
            principal.group_id().get().into(),
            principal
                .organization_principal()
                .map(|organization| organization.organization_id().get())
                .into(),
            contract_price.map(|snapshot| snapshot.id().get()).into(),
            contract_price.map(|snapshot| snapshot.version()).into(),
            contract_prices.map(|prices| prices[0]).into(),
            contract_prices.map(|prices| prices[1]).into(),
            contract_prices.map(|prices| prices[2]).into(),
            contract_prices.map(|prices| prices[3]).into(),
            contract_prices.map(|prices| prices[4]).into(),
            QuotaReservationStatus::Reserved.code().into(),
            reservation_kind.code().into(),
            QuotaFundingSource::Wallet.code().into(),
            amount.units().into(),
            0_i64.into(),
            Option::<i64>::None.into(),
            expires_at.into(),
            Option::<TimeDateTimeWithTimeZone>::None.into(),
            now.into(),
            now.into(),
        ])
        .to_owned()
}

pub(crate) fn reservation_state(key: String, lock: bool) -> SelectStatement {
    let mut statement = Query::select();
    statement
        .columns([
            billing_reservations::Column::UserId,
            billing_reservations::Column::TokenId,
            billing_reservations::Column::GroupId,
            billing_reservations::Column::OrganizationId,
            billing_reservations::Column::ContractPriceId,
            billing_reservations::Column::ContractPriceVersion,
            billing_reservations::Column::ContractInputPrice,
            billing_reservations::Column::ContractOutputPrice,
            billing_reservations::Column::ContractCacheReadPrice,
            billing_reservations::Column::ContractCacheCreation5mPrice,
            billing_reservations::Column::ContractCacheCreation1hPrice,
            billing_reservations::Column::Status,
            billing_reservations::Column::ReservationKind,
            billing_reservations::Column::FundingSource,
            billing_reservations::Column::ReservedQuota,
            billing_reservations::Column::TokenReservedQuota,
            billing_reservations::Column::ActualQuota,
            billing_reservations::Column::FinalizedAt,
        ])
        .from(billing_reservations::Entity)
        .and_where(billing_reservations::Column::IdempotencyKey.eq(key))
        .limit(1);
    if lock {
        // SeaQuery 会为 SQLite 自动省略不支持的 FOR UPDATE；此前的写语句已持有写事务。
        statement.lock(LockType::Update);
    }
    statement.to_owned()
}

pub(crate) fn user_state(user_id: i64, lock: bool) -> SelectStatement {
    let mut statement = Query::select();
    statement
        .columns([
            users::Column::Quota,
            users::Column::UsedQuota,
            users::Column::FrozenQuota,
            users::Column::RequestCount,
        ])
        .from(users::Entity)
        .and_where(users::Column::Id.eq(user_id))
        .limit(1);
    if lock {
        statement.lock(LockType::Update);
    }
    statement.to_owned()
}

pub(crate) fn group_state(group_id: i64, lock: bool) -> SelectStatement {
    let mut statement = Query::select();
    statement
        .columns([
            groups::Column::Id,
            groups::Column::DailyLimit,
            groups::Column::WeeklyLimit,
            groups::Column::MonthlyLimit,
            groups::Column::DailyUsage,
            groups::Column::WeeklyUsage,
            groups::Column::MonthlyUsage,
            groups::Column::DailyWindowStart,
            groups::Column::WeeklyWindowStart,
            groups::Column::MonthlyWindowStart,
        ])
        .from(groups::Entity)
        .and_where(groups::Column::Id.eq(group_id))
        .limit(1);
    if lock {
        statement.lock(LockType::Update);
    }
    statement.to_owned()
}

pub(crate) fn token_state(token_id: i64, lock: bool) -> SelectStatement {
    let mut statement = Query::select();
    statement
        .columns([
            tokens::Column::UserId,
            tokens::Column::OrganizationId,
            tokens::Column::OrganizationMembershipId,
            tokens::Column::OrganizationTeamId,
            tokens::Column::RemainQuota,
            tokens::Column::UnlimitedQuota,
            tokens::Column::UsedQuota,
            tokens::Column::RateLimit5h,
            tokens::Column::RateLimit1d,
            tokens::Column::RateLimit7d,
            tokens::Column::Usage5h,
            tokens::Column::Usage1d,
            tokens::Column::Usage7d,
            tokens::Column::Window5hStart,
            tokens::Column::Window1dStart,
            tokens::Column::Window7dStart,
        ])
        .from(tokens::Entity)
        .and_where(tokens::Column::Id.eq(token_id))
        .limit(1);
    if lock {
        statement.lock(LockType::Update);
    }
    statement.to_owned()
}

/// SQLite 不支持 `FOR UPDATE`，用不改变业务值的写语句提前取得分组行写锁。
pub(crate) fn sqlite_lock_group(group_id: i64) -> UpdateStatement {
    Query::update()
        .table(groups::Entity)
        .value(
            groups::Column::RatioMicros,
            Expr::col(groups::Column::RatioMicros),
        )
        .and_where(groups::Column::Id.eq(group_id))
        .to_owned()
}

/// 在分组写锁之后取得用户行写锁。
pub(crate) fn sqlite_lock_user(user_id: i64) -> UpdateStatement {
    Query::update()
        .table(users::Entity)
        .value(users::Column::Quota, Expr::col(users::Column::Quota))
        .and_where(users::Column::Id.eq(user_id))
        .to_owned()
}

/// SQLite 在用户写锁之后取得令牌行写锁，使三种数据库遵循相同的主体锁序。
pub(crate) fn sqlite_lock_token(token_id: i64) -> UpdateStatement {
    Query::update()
        .table(tokens::Entity)
        .value(
            tokens::Column::RemainQuota,
            Expr::col(tokens::Column::RemainQuota),
        )
        .and_where(tokens::Column::Id.eq(token_id))
        .to_owned()
}

pub(crate) fn precharge_user(
    user_id: i64,
    amount: Quota,
    now: TimeDateTimeWithTimeZone,
) -> UpdateStatement {
    let units = amount.units();
    Query::update()
        .table(users::Entity)
        .value(
            users::Column::Quota,
            Expr::col(users::Column::Quota).sub(units),
        )
        .value(
            users::Column::FrozenQuota,
            Expr::col(users::Column::FrozenQuota).add(units),
        )
        .value(users::Column::UpdatedAt, now)
        .and_where(users::Column::Id.eq(user_id))
        .and_where(users::Column::Quota.gte(units))
        .and_where(users::Column::Quota.gte(0_i64))
        .and_where(users::Column::FrozenQuota.gte(0_i64))
        .and_where(users::Column::FrozenQuota.lte(i64::MAX - units))
        .and_where(users::Column::UsedQuota.gte(0_i64))
        .and_where(users::Column::RequestCount.gte(0_i64))
        .to_owned()
}

pub(crate) fn precharge_token(
    principal: GatewayPrincipal,
    amount: Quota,
    now: TimeDateTimeWithTimeZone,
) -> UpdateStatement {
    let units = amount.units();
    Query::update()
        .table(tokens::Entity)
        .value(
            tokens::Column::RemainQuota,
            Expr::case(
                Expr::col(tokens::Column::UnlimitedQuota).eq(true),
                Expr::col(tokens::Column::RemainQuota),
            )
            .finally(Expr::col(tokens::Column::RemainQuota).sub(units)),
        )
        .value(tokens::Column::UpdatedAt, now)
        .and_where(tokens::Column::Id.eq(principal.token_id().get()))
        .and_where(tokens::Column::UserId.eq(principal.user_id().get()))
        .and_where(tokens::Column::RemainQuota.gte(0_i64))
        .and_where(tokens::Column::UsedQuota.gte(0_i64))
        .and_where(
            tokens::Column::UnlimitedQuota
                .eq(true)
                .or(tokens::Column::RemainQuota.gte(units)),
        )
        .to_owned()
}

pub(crate) fn snapshot_token_reservation(
    key: String,
    amount: Quota,
    now: TimeDateTimeWithTimeZone,
) -> UpdateStatement {
    Query::update()
        .table(billing_reservations::Entity)
        .value(
            billing_reservations::Column::TokenReservedQuota,
            amount.units(),
        )
        .value(billing_reservations::Column::UpdatedAt, now)
        .and_where(billing_reservations::Column::IdempotencyKey.eq(key))
        .and_where(billing_reservations::Column::Status.eq(QuotaReservationStatus::Reserved.code()))
        .and_where(billing_reservations::Column::ReservedQuota.eq(amount.units()))
        .and_where(billing_reservations::Column::ReservedQuota.gt(0_i64))
        .and_where(billing_reservations::Column::TokenReservedQuota.eq(0_i64))
        .to_owned()
}

pub(crate) fn begin_settlement(
    key: String,
    actual: Quota,
    now: TimeDateTimeWithTimeZone,
) -> UpdateStatement {
    Query::update()
        .table(billing_reservations::Entity)
        .value(
            billing_reservations::Column::Status,
            QuotaReservationStatus::SettlementPending.code(),
        )
        .value(billing_reservations::Column::ActualQuota, actual.units())
        .value(billing_reservations::Column::UpdatedAt, now)
        .and_where(billing_reservations::Column::IdempotencyKey.eq(key))
        .and_where(billing_reservations::Column::Status.eq(QuotaReservationStatus::Reserved.code()))
        .and_where(billing_reservations::Column::ReservedQuota.gt(0_i64))
        .and_where(billing_reservations::Column::ActualQuota.is_null())
        .and_where(billing_reservations::Column::FinalizedAt.is_null())
        .to_owned()
}

pub(crate) fn settle_user(
    user_id: i64,
    reserved: Quota,
    actual: Quota,
    now: TimeDateTimeWithTimeZone,
) -> UpdateStatement {
    let reserved_units = reserved.units();
    let actual_units = actual.units();
    let (quota_value, quota_guard) =
        settlement_balance(users::Column::Quota, reserved_units, actual_units);
    Query::update()
        .table(users::Entity)
        .value(users::Column::Quota, quota_value)
        .value(
            users::Column::FrozenQuota,
            Expr::col(users::Column::FrozenQuota).sub(reserved_units),
        )
        .value(
            users::Column::UsedQuota,
            Expr::col(users::Column::UsedQuota).add(actual_units),
        )
        .value(
            users::Column::RequestCount,
            Expr::col(users::Column::RequestCount).add(1_i64),
        )
        .value(users::Column::UpdatedAt, now)
        .and_where(users::Column::Id.eq(user_id))
        .and_where(users::Column::Quota.gte(0_i64))
        .and_where(quota_guard)
        .and_where(users::Column::FrozenQuota.gte(reserved_units))
        .and_where(users::Column::UsedQuota.gte(0_i64))
        .and_where(users::Column::UsedQuota.lte(i64::MAX - actual_units))
        .and_where(users::Column::RequestCount.gte(0_i64))
        .and_where(users::Column::RequestCount.lt(i64::MAX))
        .to_owned()
}

/// 企业请求只累计用户统计，不冻结、扣减或返还个人钱包余额。
pub(crate) fn settle_organization_user(
    user_id: i64,
    actual: Quota,
    now: TimeDateTimeWithTimeZone,
) -> UpdateStatement {
    let actual_units = actual.units();
    Query::update()
        .table(users::Entity)
        .value(
            users::Column::UsedQuota,
            Expr::col(users::Column::UsedQuota).add(actual_units),
        )
        .value(
            users::Column::RequestCount,
            Expr::col(users::Column::RequestCount).add(1_i64),
        )
        .value(users::Column::UpdatedAt, now)
        .and_where(users::Column::Id.eq(user_id))
        .and_where(users::Column::Quota.gte(0_i64))
        .and_where(users::Column::FrozenQuota.gte(0_i64))
        .and_where(users::Column::UsedQuota.gte(0_i64))
        .and_where(users::Column::UsedQuota.lte(i64::MAX - actual_units))
        .and_where(users::Column::RequestCount.gte(0_i64))
        .and_where(users::Column::RequestCount.lt(i64::MAX))
        .to_owned()
}

pub(crate) fn settle_token(
    token_id: i64,
    user_id: i64,
    token_reserved: Quota,
    reserved: Quota,
    actual: Quota,
    now: TimeDateTimeWithTimeZone,
) -> UpdateStatement {
    let actual_units = actual.units();
    let (remain_value, remain_guard) = if token_reserved.is_zero() {
        (
            Expr::col(tokens::Column::RemainQuota).into(),
            Expr::col(tokens::Column::RemainQuota).lte(i64::MAX),
        )
    } else {
        settlement_balance(tokens::Column::RemainQuota, reserved.units(), actual_units)
    };
    Query::update()
        .table(tokens::Entity)
        .value(tokens::Column::RemainQuota, remain_value)
        .value(
            tokens::Column::UsedQuota,
            Expr::col(tokens::Column::UsedQuota).add(actual_units),
        )
        .value(tokens::Column::UpdatedAt, now)
        .and_where(tokens::Column::Id.eq(token_id))
        .and_where(tokens::Column::UserId.eq(user_id))
        .and_where(tokens::Column::RemainQuota.gte(0_i64))
        .and_where(remain_guard)
        .and_where(tokens::Column::UsedQuota.gte(0_i64))
        .and_where(tokens::Column::UsedQuota.lte(i64::MAX - actual_units))
        .to_owned()
}

pub(crate) fn finalize_settlement(
    key: String,
    actual: Quota,
    now: TimeDateTimeWithTimeZone,
) -> UpdateStatement {
    Query::update()
        .table(billing_reservations::Entity)
        .value(
            billing_reservations::Column::Status,
            QuotaReservationStatus::Settled.code(),
        )
        .value(billing_reservations::Column::FinalizedAt, now)
        .value(billing_reservations::Column::UpdatedAt, now)
        .and_where(billing_reservations::Column::IdempotencyKey.eq(key))
        .and_where(
            billing_reservations::Column::Status
                .eq(QuotaReservationStatus::SettlementPending.code()),
        )
        .and_where(billing_reservations::Column::ReservedQuota.gt(0_i64))
        .and_where(billing_reservations::Column::ActualQuota.eq(actual.units()))
        .and_where(billing_reservations::Column::FinalizedAt.is_null())
        .to_owned()
}

pub(crate) fn begin_refund(key: String, now: TimeDateTimeWithTimeZone) -> UpdateStatement {
    Query::update()
        .table(billing_reservations::Entity)
        .value(
            billing_reservations::Column::Status,
            QuotaReservationStatus::Refunded.code(),
        )
        .value(billing_reservations::Column::FinalizedAt, now)
        .value(billing_reservations::Column::UpdatedAt, now)
        .and_where(billing_reservations::Column::IdempotencyKey.eq(key))
        .and_where(billing_reservations::Column::Status.eq(QuotaReservationStatus::Reserved.code()))
        .and_where(billing_reservations::Column::ReservedQuota.gt(0_i64))
        .and_where(billing_reservations::Column::ActualQuota.is_null())
        .and_where(billing_reservations::Column::FinalizedAt.is_null())
        .to_owned()
}

pub(crate) fn refund_user(
    user_id: i64,
    reserved: Quota,
    now: TimeDateTimeWithTimeZone,
) -> UpdateStatement {
    let units = reserved.units();
    Query::update()
        .table(users::Entity)
        .value(
            users::Column::Quota,
            Expr::col(users::Column::Quota).add(units),
        )
        .value(
            users::Column::FrozenQuota,
            Expr::col(users::Column::FrozenQuota).sub(units),
        )
        .value(users::Column::UpdatedAt, now)
        .and_where(users::Column::Id.eq(user_id))
        .and_where(users::Column::Quota.gte(0_i64))
        .and_where(users::Column::Quota.lte(i64::MAX - units))
        .and_where(users::Column::FrozenQuota.gte(units))
        .and_where(users::Column::UsedQuota.gte(0_i64))
        .and_where(users::Column::RequestCount.gte(0_i64))
        .to_owned()
}

pub(crate) fn refund_token(
    token_id: i64,
    user_id: i64,
    token_reserved: Quota,
    now: TimeDateTimeWithTimeZone,
) -> UpdateStatement {
    let units = token_reserved.units();
    Query::update()
        .table(tokens::Entity)
        .value(
            tokens::Column::RemainQuota,
            Expr::col(tokens::Column::RemainQuota).add(units),
        )
        .value(tokens::Column::UpdatedAt, now)
        .and_where(tokens::Column::Id.eq(token_id))
        .and_where(tokens::Column::UserId.eq(user_id))
        .and_where(tokens::Column::RemainQuota.gte(0_i64))
        .and_where(tokens::Column::RemainQuota.lte(i64::MAX - units))
        .and_where(tokens::Column::UsedQuota.gte(0_i64))
        .to_owned()
}

fn settlement_balance<C>(column: C, reserved: i64, actual: i64) -> (SimpleExpr, SimpleExpr)
where
    C: IntoColumnRef + Copy,
{
    if actual > reserved {
        let extra = actual - reserved;
        (Expr::col(column).sub(extra), Expr::col(column).gte(extra))
    } else {
        let release = reserved - actual;
        (
            Expr::col(column).add(release),
            Expr::col(column).lte(i64::MAX - release),
        )
    }
}

#[cfg(test)]
mod tests {
    use af_domain::{GroupId, TokenId, UserId};
    use sea_orm::DbBackend;

    use super::*;

    fn principal() -> GatewayPrincipal {
        GatewayPrincipal::new(
            TokenId::new(7).unwrap(),
            UserId::new(5).unwrap(),
            GroupId::new(3).unwrap(),
        )
    }

    #[test]
    fn reservation_lock_and_placeholders_follow_database_dialect() {
        for backend in [DbBackend::Postgres, DbBackend::MySql, DbBackend::Sqlite] {
            let built = backend.build(&reservation_state("a".repeat(32), true));
            if backend == DbBackend::Sqlite {
                assert!(!built.sql.contains("FOR UPDATE"));
            } else {
                assert!(built.sql.contains("FOR UPDATE"));
            }
            assert_placeholders(backend, &built.sql);
        }
    }

    #[test]
    fn subject_locks_follow_database_dialect() {
        for backend in [DbBackend::Postgres, DbBackend::MySql, DbBackend::Sqlite] {
            for statement in [
                group_state(3, true),
                user_state(5, true),
                token_state(7, true),
            ] {
                let built = backend.build(&statement);
                if backend == DbBackend::Sqlite {
                    assert!(!built.sql.contains("FOR UPDATE"));
                } else {
                    assert!(built.sql.contains("FOR UPDATE"));
                }
                assert_placeholders(backend, &built.sql);
            }
        }

        let group_lock = DbBackend::Sqlite.build(&sqlite_lock_group(3));
        let user_lock = DbBackend::Sqlite.build(&sqlite_lock_user(5));
        let token_lock = DbBackend::Sqlite.build(&sqlite_lock_token(7));
        assert!(group_lock.sql.starts_with("UPDATE \"groups\""));
        assert!(
            group_lock
                .sql
                .contains("\"ratio_micros\" = \"ratio_micros\"")
        );
        assert!(user_lock.sql.starts_with("UPDATE \"users\""));
        assert!(user_lock.sql.contains("\"quota\" = \"quota\""));
        assert!(token_lock.sql.starts_with("UPDATE \"tokens\""));
        assert!(
            token_lock
                .sql
                .contains("\"remain_quota\" = \"remain_quota\"")
        );
        for statement in [&group_lock, &user_lock, &token_lock] {
            assert_placeholders(DbBackend::Sqlite, &statement.sql);
        }
    }

    #[test]
    fn quota_mutations_keep_checked_guards_for_all_dialects() {
        let reserved = Quota::new(11).unwrap();
        let actual = Quota::new(17).unwrap();
        let now = audit_time();
        let statements = [
            precharge_user(5, reserved, now),
            precharge_token(principal(), reserved, now),
            settle_user(5, reserved, actual, now),
            settle_token(7, 5, reserved, reserved, actual, now),
            refund_user(5, reserved, now),
            refund_token(7, 5, reserved, now),
        ];

        for backend in [DbBackend::Postgres, DbBackend::MySql, DbBackend::Sqlite] {
            let quote = if backend == DbBackend::MySql {
                '`'
            } else {
                '"'
            };
            let built = statements
                .iter()
                .map(|statement| backend.build(statement))
                .collect::<Vec<_>>();
            let rendered = built.iter().map(ToString::to_string).collect::<Vec<_>>();

            for statement in &built {
                assert_placeholders(backend, &statement.sql);
            }
            assert!(rendered[0].contains(&format!("{quote}frozen_quota{quote} + 11")));
            assert!(rendered[0].contains(&format!("{quote}quota{quote} >= 11")));
            assert!(rendered[0].contains(&(i64::MAX - 11).to_string()));
            assert!(rendered[1].contains("CASE WHEN"));
            assert!(rendered[1].contains(&format!("{quote}remain_quota{quote} - 11")));
            assert!(rendered[1].contains(&format!("{quote}remain_quota{quote} >= 11")));
            assert!(rendered[2].contains(&format!("{quote}quota{quote} - 6")));
            assert!(rendered[2].contains(&format!("{quote}frozen_quota{quote} - 11")));
            assert!(rendered[2].contains(&format!("{quote}used_quota{quote} + 17")));
            assert!(rendered[2].contains(&(i64::MAX - 17).to_string()));
            assert!(rendered[3].contains(&format!("{quote}remain_quota{quote} - 6")));
            assert!(rendered[3].contains(&format!("{quote}used_quota{quote} + 17")));
            assert!(rendered[4].contains(&format!("{quote}quota{quote} + 11")));
            assert!(rendered[4].contains(&format!("{quote}frozen_quota{quote} - 11")));
            assert!(rendered[5].contains(&format!("{quote}remain_quota{quote} + 11")));
            assert!(rendered[5].contains(&(i64::MAX - 11).to_string()));
        }
    }

    #[test]
    fn audit_timestamps_are_bound_for_all_dialects() {
        let reserved = Quota::new(11).unwrap();
        let actual = Quota::new(17).unwrap();
        let now = audit_time();
        let key = "a".repeat(32);
        let statements = [
            precharge_user(5, reserved, now),
            precharge_token(principal(), reserved, now),
            snapshot_token_reservation(key.clone(), reserved, now),
            begin_settlement(key.clone(), actual, now),
            settle_user(5, reserved, actual, now),
            settle_token(7, 5, reserved, reserved, actual, now),
            finalize_settlement(key.clone(), actual, now),
            begin_refund(key, now),
            refund_user(5, reserved, now),
            refund_token(7, 5, reserved, now),
        ];

        for backend in [DbBackend::Postgres, DbBackend::MySql, DbBackend::Sqlite] {
            for statement in &statements {
                let built = backend.build(statement);
                assert_placeholders(backend, &built.sql);
                assert!(!built.sql.contains("CURRENT_TIMESTAMP"));
            }
        }
    }

    fn audit_time() -> TimeDateTimeWithTimeZone {
        TimeDateTimeWithTimeZone::from_unix_timestamp_nanos(1_700_000_000_123_456_000)
            .expect("测试审计时间必须有效")
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
