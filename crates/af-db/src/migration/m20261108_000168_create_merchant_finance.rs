use sea_orm_migration::prelude::*;

use super::schema::{auto_id, table};

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let mut reservations = table(manager, Alias::new("merchant_billing_reservations"));
        reservations.col(
            ColumnDef::new(Alias::new("reservation_key"))
                .char_len(32)
                .not_null()
                .primary_key(),
        );
        for name in [
            "pool_id",
            "merchant_id",
            "service_id",
            "pricing_version",
            "sales_ratio_micros",
            "commission_bps",
            "settlement_delay_secs",
        ] {
            reservations.col(ColumnDef::new(Alias::new(name)).big_integer().not_null());
        }
        for name in [
            "gross_quota",
            "commission_quota",
            "net_quota",
            "available_at",
            "created_at",
        ] {
            reservations.col(
                ColumnDef::new(Alias::new(name))
                    .big_integer()
                    .not_null()
                    .default(0_i64),
            );
            reservations.check(Expr::col(Alias::new(name)).gte(0_i64));
        }
        reservations
            .col(
                ColumnDef::new(Alias::new("status"))
                    .string_len(16)
                    .not_null()
                    .default("reserved"),
            )
            .col(ColumnDef::new(Alias::new("reported_quota")).big_integer())
            .col(
                ColumnDef::new(Alias::new("released"))
                    .boolean()
                    .not_null()
                    .default(false),
            )
            .check(Expr::col(Alias::new("status")).is_in([
                "reserved",
                "settled",
                "canceled",
                "usage_rejected",
                "reversed",
            ]))
            .check(Expr::col(Alias::new("sales_ratio_micros")).gt(0_i64))
            .check(Expr::col(Alias::new("commission_bps")).between(0_i64, 10_000_i64))
            .check(Expr::col(Alias::new("settlement_delay_secs")).between(0_i64, 7_776_000_i64))
            .check(Expr::col(Alias::new("reported_quota")).gte(0_i64))
            .check(Expr::col(Alias::new("gross_quota")).eq(
                Expr::col(Alias::new("commission_quota")).add(Expr::col(Alias::new("net_quota"))),
            ))
            .foreign_key(
                ForeignKey::create()
                    .name("fk_merchant_billing_reservation")
                    .from(
                        Alias::new("merchant_billing_reservations"),
                        Alias::new("reservation_key"),
                    )
                    .to(
                        Alias::new("billing_reservations"),
                        Alias::new("idempotency_key"),
                    )
                    .on_delete(ForeignKeyAction::Restrict),
            )
            .foreign_key(
                ForeignKey::create()
                    .name("fk_merchant_billing_owner")
                    .from(
                        Alias::new("merchant_billing_reservations"),
                        Alias::new("merchant_id"),
                    )
                    .to(Alias::new("merchant_profiles"), Alias::new("id"))
                    .on_delete(ForeignKeyAction::Restrict),
            );
        manager.create_table(reservations.to_owned()).await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_merchant_billing_release")
                    .table(Alias::new("merchant_billing_reservations"))
                    .col(Alias::new("merchant_id"))
                    .col(Alias::new("status"))
                    .col(Alias::new("released"))
                    .col(Alias::new("available_at"))
                    .to_owned(),
            )
            .await?;

        let mut accounts = table(manager, Alias::new("merchant_finance_accounts"));
        accounts.col(
            ColumnDef::new(Alias::new("merchant_id"))
                .big_integer()
                .not_null()
                .primary_key(),
        );
        for name in [
            "pending_quota",
            "available_quota",
            "withdrawing_quota",
            "frozen_quota",
            "debt_quota",
            "total_gross_quota",
            "total_commission_quota",
            "total_paid_quota",
            "updated_at",
        ] {
            accounts.col(
                ColumnDef::new(Alias::new(name))
                    .big_integer()
                    .not_null()
                    .default(0_i64),
            );
            accounts.check(Expr::col(Alias::new(name)).gte(0_i64));
        }
        accounts.foreign_key(
            ForeignKey::create()
                .name("fk_merchant_finance_owner")
                .from(
                    Alias::new("merchant_finance_accounts"),
                    Alias::new("merchant_id"),
                )
                .to(Alias::new("merchant_profiles"), Alias::new("id"))
                .on_delete(ForeignKeyAction::Restrict),
        );
        manager.create_table(accounts.to_owned()).await?;

        let mut entries = table(manager, Alias::new("merchant_finance_entries"));
        entries
            .col(auto_id(Alias::new("id")))
            .col(
                ColumnDef::new(Alias::new("event_key"))
                    .string_len(96)
                    .not_null()
                    .unique_key(),
            )
            .col(
                ColumnDef::new(Alias::new("merchant_id"))
                    .big_integer()
                    .not_null(),
            )
            .col(ColumnDef::new(Alias::new("kind")).string_len(32).not_null())
            .col(
                ColumnDef::new(Alias::new("reference"))
                    .string_len(256)
                    .not_null(),
            );
        for name in ["gross_quota", "commission_quota", "net_quota", "created_at"] {
            entries.col(ColumnDef::new(Alias::new(name)).big_integer().not_null());
        }
        entries.foreign_key(
            ForeignKey::create()
                .name("fk_merchant_finance_entry_owner")
                .from(
                    Alias::new("merchant_finance_entries"),
                    Alias::new("merchant_id"),
                )
                .to(Alias::new("merchant_profiles"), Alias::new("id"))
                .on_delete(ForeignKeyAction::Restrict),
        );
        manager.create_table(entries.to_owned()).await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_merchant_finance_entries_owner")
                    .table(Alias::new("merchant_finance_entries"))
                    .col(Alias::new("merchant_id"))
                    .col(Alias::new("id"))
                    .to_owned(),
            )
            .await?;

        let mut withdrawals = table(manager, Alias::new("merchant_withdrawals"));
        withdrawals
            .col(auto_id(Alias::new("id")))
            .col(
                ColumnDef::new(Alias::new("event_key"))
                    .char_len(32)
                    .not_null()
                    .unique_key(),
            )
            .col(
                ColumnDef::new(Alias::new("merchant_id"))
                    .big_integer()
                    .not_null(),
            )
            .col(
                ColumnDef::new(Alias::new("amount_quota"))
                    .big_integer()
                    .not_null(),
            )
            .col(
                ColumnDef::new(Alias::new("status"))
                    .string_len(16)
                    .not_null(),
            )
            .col(
                ColumnDef::new(Alias::new("reference"))
                    .string_len(256)
                    .not_null()
                    .default(""),
            )
            .col(ColumnDef::new(Alias::new("reason")).string_len(512))
            .col(ColumnDef::new(Alias::new("actor_user_id")).big_integer())
            .col(
                ColumnDef::new(Alias::new("created_at"))
                    .big_integer()
                    .not_null(),
            )
            .col(
                ColumnDef::new(Alias::new("updated_at"))
                    .big_integer()
                    .not_null(),
            )
            .check(Expr::col(Alias::new("amount_quota")).gt(0_i64))
            .check(Expr::col(Alias::new("status")).is_in(["pending", "paid", "rejected"]))
            .foreign_key(
                ForeignKey::create()
                    .name("fk_merchant_withdrawal_owner")
                    .from(
                        Alias::new("merchant_withdrawals"),
                        Alias::new("merchant_id"),
                    )
                    .to(Alias::new("merchant_profiles"), Alias::new("id"))
                    .on_delete(ForeignKeyAction::Restrict),
            );
        manager.create_table(withdrawals.to_owned()).await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_merchant_withdrawal_owner")
                    .table(Alias::new("merchant_withdrawals"))
                    .col(Alias::new("merchant_id"))
                    .col(Alias::new("id"))
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_merchant_withdrawal_status")
                    .table(Alias::new("merchant_withdrawals"))
                    .col(Alias::new("status"))
                    .col(Alias::new("id"))
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for name in [
            "merchant_withdrawals",
            "merchant_finance_entries",
            "merchant_finance_accounts",
            "merchant_billing_reservations",
        ] {
            manager
                .drop_table(Table::drop().table(Alias::new(name)).to_owned())
                .await?;
        }
        Ok(())
    }
}
