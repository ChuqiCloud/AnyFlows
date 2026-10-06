use sea_orm::DbBackend;
use sea_orm_migration::prelude::*;

use crate::migration::iden::{billing_reservations, groups, tokens, users};

use super::{nullable_timestamp, table, timestamp};

const PRICE_PRECISION: u32 = 38;
const PRICE_SCALE: u32 = 28;

pub(in crate::migration) async fn create_billing_reservations(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    let mut statement = table(manager, billing_reservations::Entity);
    statement
        .col(
            ColumnDef::new(billing_reservations::Column::IdempotencyKey)
                .char_len(32)
                .not_null()
                .primary_key(),
        )
        .col(
            ColumnDef::new(billing_reservations::Column::UserId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(billing_reservations::Column::TokenId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(billing_reservations::Column::GroupId)
                .big_integer()
                .not_null(),
        )
        .col(ColumnDef::new(billing_reservations::Column::OrganizationId).big_integer())
        // 将合同价版本及其快照写入预扣记录，保证后续结算不受合同调价影响。
        .col(ColumnDef::new(billing_reservations::Column::ContractPriceId).big_integer())
        .col(
            ColumnDef::new(billing_reservations::Column::ContractPriceVersion).big_integer(),
        )
        .col(price(manager, billing_reservations::Column::ContractInputPrice))
        .col(price(manager, billing_reservations::Column::ContractOutputPrice))
        .col(price(manager, billing_reservations::Column::ContractCacheReadPrice))
        .col(price(
            manager,
            billing_reservations::Column::ContractCacheCreation5mPrice,
        ))
        .col(price(
            manager,
            billing_reservations::Column::ContractCacheCreation1hPrice,
        ))
        .col(
            ColumnDef::new(billing_reservations::Column::Status)
                .small_integer()
                .not_null()
                .default(1_i16)
                .check(
                    Expr::col(billing_reservations::Column::Status)
                        .is_in([1_i16, 2_i16, 3_i16, 4_i16]),
                ),
        )
        .col(
            ColumnDef::new(billing_reservations::Column::ReservedQuota)
                .big_integer()
                .not_null()
                .check(Expr::col(billing_reservations::Column::ReservedQuota).gt(0_i64)),
        )
        .col(
            ColumnDef::new(billing_reservations::Column::TokenReservedQuota)
                .big_integer()
                .not_null()
                .default(0_i64)
                .check(
                    Expr::col(billing_reservations::Column::TokenReservedQuota).gte(0_i64),
                ),
        )
        .col(
            ColumnDef::new(billing_reservations::Column::ActualQuota)
                .big_integer()
                .check(Expr::col(billing_reservations::Column::ActualQuota).gte(0_i64)),
        )
        .col(nullable_timestamp(
            manager,
            billing_reservations::Column::ExpiresAt,
        ).not_null())
        .col(nullable_timestamp(
            manager,
            billing_reservations::Column::FinalizedAt,
        ))
        .col(timestamp(
            manager,
            billing_reservations::Column::CreatedAt,
        ))
        .col(timestamp(
            manager,
            billing_reservations::Column::UpdatedAt,
        ))
        // 只有未受令牌额度约束或与用户钱包等额预扣两种合法形态。
        .check(
            Expr::col(billing_reservations::Column::TokenReservedQuota)
                .eq(0_i64)
                .or(Expr::col(billing_reservations::Column::TokenReservedQuota).eq(Expr::col(
                    billing_reservations::Column::ReservedQuota,
                ))),
        )
        // 状态与结算额度、终态时间必须同步演进，禁止构造半完成记录。
        .check(reservation_state_check())
        .foreign_key(
            ForeignKey::create()
                .name("fk_billing_reservations_user")
                .from(
                    billing_reservations::Entity,
                    billing_reservations::Column::UserId,
                )
                .to(users::Entity, users::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_billing_reservations_token")
                .from(
                    billing_reservations::Entity,
                    billing_reservations::Column::TokenId,
                )
                .to(tokens::Entity, tokens::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_billing_reservations_group")
                .from(
                    billing_reservations::Entity,
                    billing_reservations::Column::GroupId,
                )
                .to(groups::Entity, groups::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        );

    // 方言专用表达式同时约束长度和字符集，覆盖绕过实体的原始写入。
    statement
        .check(idempotency_key_format_check(manager.get_database_backend()))
        .check(
            Expr::col(billing_reservations::Column::IdempotencyKey)
                .ne("00000000000000000000000000000000"),
        );
    manager.create_table(statement).await
}

fn price<T>(manager: &SchemaManager<'_>, column: T) -> ColumnDef
where
    T: IntoIden + Clone + 'static,
{
    let mut definition = ColumnDef::new(column);
    if manager.get_database_backend() == DbBackend::Sqlite {
        definition.decimal();
    } else {
        definition.decimal_len(PRICE_PRECISION, PRICE_SCALE);
    }
    definition
}

fn reservation_state_check() -> SimpleExpr {
    Expr::col(billing_reservations::Column::Status)
        .eq(1_i16)
        .and(Expr::col(billing_reservations::Column::ActualQuota).is_null())
        .and(Expr::col(billing_reservations::Column::FinalizedAt).is_null())
        .or(Expr::col(billing_reservations::Column::Status)
            .eq(2_i16)
            .and(Expr::col(billing_reservations::Column::ActualQuota).is_not_null())
            .and(Expr::col(billing_reservations::Column::FinalizedAt).is_null()))
        .or(Expr::col(billing_reservations::Column::Status)
            .eq(3_i16)
            .and(Expr::col(billing_reservations::Column::ActualQuota).is_not_null())
            .and(Expr::col(billing_reservations::Column::FinalizedAt).is_not_null()))
        .or(Expr::col(billing_reservations::Column::Status)
            .eq(4_i16)
            .and(Expr::col(billing_reservations::Column::ActualQuota).is_null())
            .and(Expr::col(billing_reservations::Column::FinalizedAt).is_not_null()))
}

fn idempotency_key_format_check(database_backend: DbBackend) -> SimpleExpr {
    match database_backend {
        DbBackend::Postgres => Expr::cust(r#""idempotency_key" ~ '^[0-9a-f]{32}$'"#),
        DbBackend::MySql => {
            // MySQL 8.0.22 起拒绝 binary 正则参数，大小写语义由表级 utf8mb4_bin 保证。
            Expr::cust(
                "CHAR_LENGTH(`idempotency_key`) = 32 AND `idempotency_key` REGEXP '^[0-9a-f]{32}$'",
            )
        }
        DbBackend::Sqlite => Expr::cust(
            r#"length("idempotency_key") = 32 AND "idempotency_key" NOT GLOB '*[^0-9a-f]*'"#,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mysql_idempotency_key_check_uses_non_binary_case_sensitive_regex() {
        let rendered = Query::select()
            .expr(idempotency_key_format_check(DbBackend::MySql))
            .to_owned()
            .to_string(MysqlQueryBuilder);

        assert_eq!(
            rendered,
            "SELECT CHAR_LENGTH(`idempotency_key`) = 32 AND `idempotency_key` REGEXP '^[0-9a-f]{32}$'"
        );
    }
}
