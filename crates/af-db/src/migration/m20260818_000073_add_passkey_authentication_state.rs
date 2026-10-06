use sea_orm::ConnectionTrait;
use sea_orm_migration::prelude::*;

use super::{
    iden::{passkey_authentication_challenges, passkeys, users},
    schema::{self, auto_id, nullable_timestamp, table, timestamp},
};

/// 扩展 Passkey 计数并创建一次性登录挑战存储。
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        schema::rebuild_auth_challenge_rate_limits_for_passkey(manager, true).await?;
        manager
            .alter_table(
                Table::alter()
                    .table(passkeys::Entity)
                    .add_column(
                        ColumnDef::new(passkeys::Column::SignCount)
                            .big_integer()
                            .not_null()
                            .default(0_i64)
                            .check(Expr::col(passkeys::Column::SignCount).gte(0_i64)),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(passkeys::Entity)
                    .add_column(nullable_timestamp(manager, passkeys::Column::AnomalyAt))
                    .to_owned(),
            )
            .await?;
        backfill_passkey_sign_counts(manager).await?;
        manager
            .create_table(
                table(manager, passkey_authentication_challenges::Entity)
                    .col(auto_id(passkey_authentication_challenges::Column::Id))
                    .col(
                        ColumnDef::new(passkey_authentication_challenges::Column::UserId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(passkey_authentication_challenges::Column::ChallengeDigest)
                            .char_len(64)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(
                            passkey_authentication_challenges::Column::AuthenticationState,
                        )
                        .json_binary()
                        .not_null(),
                    )
                    .col(
                        ColumnDef::new(passkey_authentication_challenges::Column::SessionVersion)
                            .big_integer()
                            .not_null()
                            .check(
                                Expr::col(
                                    passkey_authentication_challenges::Column::SessionVersion,
                                )
                                .gte(1_i64),
                            ),
                    )
                    .col(timestamp(
                        manager,
                        passkey_authentication_challenges::Column::ExpiresAt,
                    ))
                    .col(nullable_timestamp(
                        manager,
                        passkey_authentication_challenges::Column::ConsumedAt,
                    ))
                    .col(timestamp(
                        manager,
                        passkey_authentication_challenges::Column::CreatedAt,
                    ))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_passkey_authentication_challenges_user")
                            .from(
                                passkey_authentication_challenges::Entity,
                                passkey_authentication_challenges::Column::UserId,
                            )
                            .to(users::Entity, users::Column::Id)
                            .on_update(ForeignKeyAction::Cascade)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("uq_passkey_authentication_challenges_digest")
                    .table(passkey_authentication_challenges::Entity)
                    .col(passkey_authentication_challenges::Column::ChallengeDigest)
                    .unique()
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_passkey_authentication_challenges_user_state")
                    .table(passkey_authentication_challenges::Entity)
                    .col(passkey_authentication_challenges::Column::UserId)
                    .col(passkey_authentication_challenges::Column::ConsumedAt)
                    .col(passkey_authentication_challenges::Column::ExpiresAt)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_passkey_authentication_challenges_expires_at")
                    .table(passkey_authentication_challenges::Entity)
                    .col(passkey_authentication_challenges::Column::ExpiresAt)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(passkey_authentication_challenges::Entity)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(passkeys::Entity)
                    .drop_column(passkeys::Column::AnomalyAt)
                    .to_owned(),
            )
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(passkeys::Entity)
                    .drop_column(passkeys::Column::SignCount)
                    .to_owned(),
            )
            .await?;
        schema::rebuild_auth_challenge_rate_limits_for_passkey(manager, false).await
    }
}

/// 从第 72 号迁移保存的 Passkey JSON 回填计数，避免升级时把已有基线清零。
async fn backfill_passkey_sign_counts(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let select = Query::select()
        .columns([passkeys::Column::Id, passkeys::Column::Passkey])
        .from(passkeys::Entity)
        .to_owned();
    let rows = manager
        .get_connection()
        .query_all(manager.get_database_backend().build(&select))
        .await?;

    for row in rows {
        let id = row.try_get::<i64>("", "id")?;
        let passkey = row.try_get::<serde_json::Value>("", "passkey")?;
        let sign_count = legacy_passkey_sign_count(&passkey)?;
        manager
            .exec_stmt(
                Query::update()
                    .table(passkeys::Entity)
                    .value(passkeys::Column::SignCount, sign_count)
                    .and_where(Expr::col(passkeys::Column::Id).eq(id))
                    .to_owned(),
            )
            .await?;
    }
    Ok(())
}

fn legacy_passkey_sign_count(passkey: &serde_json::Value) -> Result<i64, DbErr> {
    let sign_count = passkey
        .get("cred")
        .and_then(serde_json::Value::as_object)
        .and_then(|credential| credential.get("counter"))
        .and_then(serde_json::Value::as_u64)
        .and_then(|counter| <i64 as std::convert::TryFrom<u64>>::try_from(counter).ok())
        .ok_or_else(|| DbErr::Custom("Passkey 凭证计数缺失或无效，拒绝迁移".to_owned()))?;
    Ok(sign_count)
}
