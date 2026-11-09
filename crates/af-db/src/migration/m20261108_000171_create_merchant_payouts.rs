use super::schema::table;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                table(manager, Alias::new("merchant_payout_settings"))
                    .col(
                        ColumnDef::new(Alias::new("id"))
                            .small_integer()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("version"))
                            .big_integer()
                            .not_null()
                            .default(0_i64),
                    )
                    .col(ColumnDef::new(Alias::new("config_json")).text().not_null())
                    .col(ColumnDef::new(Alias::new("secret_json")).text())
                    .to_owned(),
            )
            .await?;
        let insert = Query::insert().into_table(Alias::new("merchant_payout_settings"))
            .columns(["id", "config_json"].map(Alias::new))
            .values_panic([1_i16.into(), r#"{"enabled":false,"auto_approve_enabled":false,"gateway_url":"","currency":"USD","quota_per_unit":500000,"min_amount_minor":100,"max_amount_minor":100000,"daily_limit_minor":1000000}"#.into()]).to_owned();
        manager
            .get_connection()
            .execute(manager.get_database_backend().build(&insert))
            .await?;
        manager
            .create_table(
                table(manager, Alias::new("merchant_payout_accounts"))
                    .col(
                        ColumnDef::new(Alias::new("merchant_id"))
                            .big_integer()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("version"))
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("label"))
                            .string_len(80)
                            .not_null(),
                    )
                    .col(ColumnDef::new(Alias::new("secret_json")).text())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_payout_account_merchant")
                            .from(
                                Alias::new("merchant_payout_accounts"),
                                Alias::new("merchant_id"),
                            )
                            .to(Alias::new("merchant_profiles"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .to_owned(),
            )
            .await?;
        let mut attempts = table(manager, Alias::new("merchant_payouts"));
        attempts
            .col(
                ColumnDef::new(Alias::new("withdrawal_id"))
                    .big_integer()
                    .not_null()
                    .primary_key(),
            )
            .col(
                ColumnDef::new(Alias::new("merchant_id"))
                    .big_integer()
                    .not_null(),
            )
            .col(
                ColumnDef::new(Alias::new("event_key"))
                    .char_len(32)
                    .not_null()
                    .unique_key(),
            )
            .col(
                ColumnDef::new(Alias::new("status"))
                    .string_len(16)
                    .not_null(),
            )
            .col(
                ColumnDef::new(Alias::new("amount_minor"))
                    .big_integer()
                    .not_null(),
            )
            .col(
                ColumnDef::new(Alias::new("currency"))
                    .char_len(3)
                    .not_null(),
            );
        for field in ["gateway_url", "gateway_secret", "account_secret"] {
            attempts.col(ColumnDef::new(Alias::new(field)).text().not_null());
        }
        for field in ["created_at", "updated_at", "next_at", "lease_until"] {
            attempts.col(
                ColumnDef::new(Alias::new(field))
                    .big_integer()
                    .not_null()
                    .default(0_i64),
            );
        }
        attempts
            .col(
                ColumnDef::new(Alias::new("lease_owner"))
                    .string_len(64)
                    .not_null()
                    .default(""),
            )
            .col(
                ColumnDef::new(Alias::new("reference"))
                    .string_len(256)
                    .not_null()
                    .default(""),
            )
            .col(ColumnDef::new(Alias::new("reason")).string_len(64))
            .check(Expr::col(Alias::new("status")).is_in([
                "pending",
                "processing",
                "unknown",
                "paid",
                "failed",
            ]))
            .check(Expr::col(Alias::new("amount_minor")).gt(0_i64))
            .foreign_key(
                ForeignKey::create()
                    .name("fk_payout_withdrawal")
                    .from(Alias::new("merchant_payouts"), Alias::new("withdrawal_id"))
                    .to(Alias::new("merchant_withdrawals"), Alias::new("id"))
                    .on_delete(ForeignKeyAction::Restrict),
            );
        manager.create_table(attempts).await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_merchant_payout_due")
                    .table(Alias::new("merchant_payouts"))
                    .col(Alias::new("status"))
                    .col(Alias::new("next_at"))
                    .col(Alias::new("lease_until"))
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for name in [
            "merchant_payouts",
            "merchant_payout_accounts",
            "merchant_payout_settings",
        ] {
            manager
                .drop_table(Table::drop().table(Alias::new(name)).to_owned())
                .await?;
        }
        Ok(())
    }
}
