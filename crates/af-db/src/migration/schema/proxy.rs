use sea_orm::DbBackend;
use sea_orm_migration::prelude::*;

use crate::migration::iden::{credentials, proxies};

use super::{auto_id, nullable_timestamp, table, timestamp};

const PROXY_NAME_INDEX: &str = "uq_proxies_active_name";
const PROXY_ENABLED_INDEX: &str = "idx_proxies_enabled";
pub(in crate::migration) const CREDENTIAL_PROXY_FOREIGN_KEY: &str = "fk_credentials_proxy";

/// 创建凭据专属出口代理目录。
pub(in crate::migration) async fn create_proxies(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            table(manager, proxies::Entity)
                .col(auto_id(proxies::Column::Id))
                .col(
                    ColumnDef::new(proxies::Column::Name)
                        .string_len(128)
                        .not_null(),
                )
                .col(ColumnDef::new(proxies::Column::ActiveName).string_len(128))
                .col(
                    ColumnDef::new(proxies::Column::Scheme)
                        .string_len(16)
                        .not_null()
                        .check(
                            Expr::col(proxies::Column::Scheme)
                                .is_in(["http", "https", "socks5", "socks5h"]),
                        ),
                )
                .col(
                    ColumnDef::new(proxies::Column::Host)
                        .string_len(255)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(proxies::Column::Port)
                        .integer()
                        .not_null()
                        .check(
                            Expr::col(proxies::Column::Port)
                                .gte(1_i32)
                                .and(Expr::col(proxies::Column::Port).lte(65_535_i32)),
                        ),
                )
                .col(ColumnDef::new(proxies::Column::Username).string_len(320))
                .col(ColumnDef::new(proxies::Column::PasswordSecret).json_binary())
                .col(
                    ColumnDef::new(proxies::Column::TrustProxyDns)
                        .boolean()
                        .not_null()
                        .default(false),
                )
                .col(
                    ColumnDef::new(proxies::Column::Enabled)
                        .boolean()
                        .not_null()
                        .default(true),
                )
                .col(
                    ColumnDef::new(proxies::Column::Version)
                        .big_integer()
                        .not_null()
                        .default(1_i64)
                        .check(Expr::col(proxies::Column::Version).gte(1_i64)),
                )
                .col(timestamp(manager, proxies::Column::CreatedAt))
                .col(timestamp(manager, proxies::Column::UpdatedAt))
                .col(nullable_timestamp(manager, proxies::Column::DeletedAt))
                .check(
                    Expr::col(proxies::Column::Username)
                        .is_null()
                        .and(Expr::col(proxies::Column::PasswordSecret).is_null())
                        .or(Expr::col(proxies::Column::Username)
                            .is_not_null()
                            .and(Expr::col(proxies::Column::PasswordSecret).is_not_null())),
                )
                .check(
                    Expr::col(proxies::Column::DeletedAt)
                        .is_null()
                        .and(Expr::col(proxies::Column::ActiveName).is_not_null())
                        .and(Expr::col(proxies::Column::ActiveName).equals(proxies::Column::Name))
                        .or(Expr::col(proxies::Column::DeletedAt)
                            .is_not_null()
                            .and(Expr::col(proxies::Column::ActiveName).is_null())),
                )
                .to_owned(),
        )
        .await?;
    manager
        .create_index(
            Index::create()
                .name(PROXY_NAME_INDEX)
                .table(proxies::Entity)
                .col(proxies::Column::ActiveName)
                .unique()
                .to_owned(),
        )
        .await?;
    manager
        .create_index(
            Index::create()
                .name(PROXY_ENABLED_INDEX)
                .table(proxies::Entity)
                .col(proxies::Column::Enabled)
                .col(proxies::Column::Id)
                .to_owned(),
        )
        .await
}

/// 为历史预留的 `credentials.proxy_id` 补充真实数据库引用完整性。
pub(in crate::migration) async fn create_credential_proxy_reference(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    if manager.get_database_backend() == DbBackend::Sqlite {
        // SQLite 不能通过 ALTER TABLE 补外键，使用对等触发器覆盖新增、改绑和物理删除。
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE TRIGGER trg_credentials_proxy_insert BEFORE INSERT ON credentials \
                 WHEN NEW.proxy_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM proxies WHERE id = NEW.proxy_id) \
                 BEGIN SELECT RAISE(ABORT, 'credentials.proxy_id reference missing'); END",
            )
            .await?;
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE TRIGGER trg_credentials_proxy_update BEFORE UPDATE OF proxy_id ON credentials \
                 WHEN NEW.proxy_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM proxies WHERE id = NEW.proxy_id) \
                 BEGIN SELECT RAISE(ABORT, 'credentials.proxy_id reference missing'); END",
            )
            .await?;
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE TRIGGER trg_proxies_restrict_delete BEFORE DELETE ON proxies \
                 WHEN EXISTS (SELECT 1 FROM credentials WHERE proxy_id = OLD.id) \
                 BEGIN SELECT RAISE(ABORT, 'proxy is referenced by credentials'); END",
            )
            .await?;
        return Ok(());
    }
    manager
        .create_foreign_key(
            ForeignKey::create()
                .name(CREDENTIAL_PROXY_FOREIGN_KEY)
                .from(credentials::Entity, credentials::Column::ProxyId)
                .to(proxies::Entity, proxies::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict)
                .to_owned(),
        )
        .await
}
