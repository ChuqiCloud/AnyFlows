use sea_orm::{ConnectionTrait, DbBackend};
use sea_orm_migration::prelude::*;

use super::{
    iden::{
        organization_memberships, organization_sso_domains, organization_sso_identities,
        organization_sso_login_transactions, organization_sso_providers, organizations,
    },
    schema::{auto_id, nullable_timestamp, table, timestamp},
};

const PROVIDER_SCOPE_KEY_INDEX: &str = "uq_organization_sso_providers_scope_key";
const PROVIDER_SCOPE_ID_INDEX: &str = "uq_organization_sso_providers_scope_id";
const PROVIDER_ACTIVE_INDEX: &str = "uq_organization_sso_providers_active";
const DOMAIN_SCOPE_ID_INDEX: &str = "uq_organization_sso_domains_scope_id";
const DOMAIN_ACTIVE_INDEX: &str = "uq_organization_sso_domains_active";
const IDENTITY_SUBJECT_INDEX: &str = "uq_organization_sso_identities_subject";
const IDENTITY_MEMBERSHIP_INDEX: &str = "uq_organization_sso_identities_membership";
const IDENTITY_SCOPE_ID_INDEX: &str = "uq_organization_sso_identities_scope_id";
const TRANSACTION_STATE_INDEX: &str = "uq_organization_sso_login_transactions_state";
const TRANSACTION_TICKET_INDEX: &str = "uq_organization_sso_login_transactions_ticket";
const TRANSACTION_ASSERTION_INDEX: &str = "uq_organization_sso_login_transactions_assertion";
const TRANSACTION_EXPIRES_INDEX: &str = "idx_organization_sso_login_transactions_expires";
const TRANSACTION_TICKET_EXPIRES_INDEX: &str =
    "idx_organization_sso_login_transactions_ticket_expires";

/// 创建企业 SSO 的独立存储边界；本迁移不开放登录 API 或策略执行。
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        create_providers(manager).await?;
        create_domains(manager).await?;
        create_identities(manager).await?;
        create_login_transactions(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_login_transactions(manager).await?;
        drop_identities(manager).await?;
        drop_domains(manager).await?;
        drop_providers(manager).await
    }
}

async fn create_providers(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let mut statement = table(manager, organization_sso_providers::Entity);
    statement
        .col(auto_id(organization_sso_providers::Column::Id))
        .col(
            ColumnDef::new(organization_sso_providers::Column::OrganizationId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_sso_providers::Column::ProviderKey)
                .char_len(32)
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_sso_providers::Column::Protocol)
                .small_integer()
                .not_null()
                .check(
                    Expr::col(organization_sso_providers::Column::Protocol).between(1_i16, 2_i16),
                ),
        )
        .col(
            ColumnDef::new(organization_sso_providers::Column::Status)
                .small_integer()
                .not_null()
                .default(1_i16)
                .check(Expr::col(organization_sso_providers::Column::Status).between(1_i16, 3_i16)),
        )
        .col(
            ColumnDef::new(organization_sso_providers::Column::DisplayName)
                .string_len(128)
                .not_null()
                .check(Expr::col(organization_sso_providers::Column::DisplayName).ne("")),
        )
        .col(ColumnDef::new(organization_sso_providers::Column::MetadataProjection).json_binary())
        .col(ColumnDef::new(organization_sso_providers::Column::SecretEnvelope).json_binary())
        .col(
            ColumnDef::new(organization_sso_providers::Column::Version)
                .big_integer()
                .not_null()
                .default(1_i64)
                .check(Expr::col(organization_sso_providers::Column::Version).gt(0_i64)),
        )
        .col(timestamp(
            manager,
            organization_sso_providers::Column::CreatedAt,
        ))
        .col(timestamp(
            manager,
            organization_sso_providers::Column::UpdatedAt,
        ))
        .foreign_key(
            ForeignKey::create()
                .name("fk_organization_sso_providers_organization")
                .from(
                    organization_sso_providers::Entity,
                    organization_sso_providers::Column::OrganizationId,
                )
                .to(organizations::Entity, organizations::Column::Id)
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .check(provider_key_format_check(manager.get_database_backend()));
    manager.create_table(statement).await?;
    manager
        .create_index(
            Index::create()
                .name(PROVIDER_SCOPE_KEY_INDEX)
                .table(organization_sso_providers::Entity)
                .col(organization_sso_providers::Column::OrganizationId)
                .col(organization_sso_providers::Column::ProviderKey)
                .unique()
                .to_owned(),
        )
        .await?;
    manager
        .create_index(
            Index::create()
                .name(PROVIDER_SCOPE_ID_INDEX)
                .table(organization_sso_providers::Entity)
                .col(organization_sso_providers::Column::OrganizationId)
                .col(organization_sso_providers::Column::Id)
                .unique()
                .to_owned(),
        )
        .await?;
    create_provider_active_constraint(manager).await
}

async fn create_domains(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let mut statement = table(manager, organization_sso_domains::Entity);
    statement
        .col(auto_id(organization_sso_domains::Column::Id))
        .col(
            ColumnDef::new(organization_sso_domains::Column::OrganizationId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_sso_domains::Column::ProviderId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_sso_domains::Column::Domain)
                .string_len(253)
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_sso_domains::Column::Status)
                .small_integer()
                .not_null()
                .default(1_i16)
                .check(Expr::col(organization_sso_domains::Column::Status).between(1_i16, 3_i16)),
        )
        .col(
            ColumnDef::new(organization_sso_domains::Column::ProofDigest)
                .string_len(64)
                .not_null(),
        )
        .col(nullable_timestamp(
            manager,
            organization_sso_domains::Column::ProofExpiresAt,
        ))
        .col(nullable_timestamp(
            manager,
            organization_sso_domains::Column::VerifiedAt,
        ))
        .col(nullable_timestamp(
            manager,
            organization_sso_domains::Column::RecheckAt,
        ))
        .col(nullable_timestamp(
            manager,
            organization_sso_domains::Column::RevokedAt,
        ))
        .col(
            ColumnDef::new(organization_sso_domains::Column::Version)
                .big_integer()
                .not_null()
                .default(1_i64)
                .check(Expr::col(organization_sso_domains::Column::Version).gt(0_i64)),
        )
        .col(timestamp(
            manager,
            organization_sso_domains::Column::CreatedAt,
        ))
        .col(timestamp(
            manager,
            organization_sso_domains::Column::UpdatedAt,
        ))
        .foreign_key(
            ForeignKey::create()
                .name("fk_organization_sso_domains_organization")
                .from(
                    organization_sso_domains::Entity,
                    organization_sso_domains::Column::OrganizationId,
                )
                .to(organizations::Entity, organizations::Column::Id)
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_organization_sso_domains_provider_scope")
                .from(
                    organization_sso_domains::Entity,
                    (
                        organization_sso_domains::Column::OrganizationId,
                        organization_sso_domains::Column::ProviderId,
                    ),
                )
                .to(
                    organization_sso_providers::Entity,
                    (
                        organization_sso_providers::Column::OrganizationId,
                        organization_sso_providers::Column::Id,
                    ),
                )
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .check(domain_lifecycle_check());
    manager.create_table(statement).await?;
    manager
        .create_index(
            Index::create()
                .name(DOMAIN_SCOPE_ID_INDEX)
                .table(organization_sso_domains::Entity)
                .col(organization_sso_domains::Column::OrganizationId)
                .col(organization_sso_domains::Column::Id)
                .unique()
                .to_owned(),
        )
        .await?;
    create_domain_active_constraint(manager).await
}

async fn create_identities(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let mut statement = table(manager, organization_sso_identities::Entity);
    statement
        .col(auto_id(organization_sso_identities::Column::Id))
        .col(
            ColumnDef::new(organization_sso_identities::Column::OrganizationId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_sso_identities::Column::ProviderId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_sso_identities::Column::SubjectDigest)
                .string_len(64)
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_sso_identities::Column::OrganizationMembershipId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_sso_identities::Column::Status)
                .small_integer()
                .not_null()
                .default(1_i16)
                .check(
                    Expr::col(organization_sso_identities::Column::Status).between(1_i16, 2_i16),
                ),
        )
        .col(
            ColumnDef::new(organization_sso_identities::Column::BindingMethod)
                .small_integer()
                .not_null()
                .check(
                    Expr::col(organization_sso_identities::Column::BindingMethod)
                        .between(1_i16, 2_i16),
                ),
        )
        .col(
            ColumnDef::new(organization_sso_identities::Column::Version)
                .big_integer()
                .not_null()
                .default(1_i64)
                .check(Expr::col(organization_sso_identities::Column::Version).gt(0_i64)),
        )
        .col(timestamp(
            manager,
            organization_sso_identities::Column::CreatedAt,
        ))
        .col(timestamp(
            manager,
            organization_sso_identities::Column::UpdatedAt,
        ))
        .col(nullable_timestamp(
            manager,
            organization_sso_identities::Column::DisabledAt,
        ))
        .foreign_key(
            ForeignKey::create()
                .name("fk_organization_sso_identities_organization")
                .from(
                    organization_sso_identities::Entity,
                    organization_sso_identities::Column::OrganizationId,
                )
                .to(organizations::Entity, organizations::Column::Id)
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_organization_sso_identities_provider_scope")
                .from(
                    organization_sso_identities::Entity,
                    (
                        organization_sso_identities::Column::OrganizationId,
                        organization_sso_identities::Column::ProviderId,
                    ),
                )
                .to(
                    organization_sso_providers::Entity,
                    (
                        organization_sso_providers::Column::OrganizationId,
                        organization_sso_providers::Column::Id,
                    ),
                )
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_organization_sso_identities_membership_scope")
                .from(
                    organization_sso_identities::Entity,
                    (
                        organization_sso_identities::Column::OrganizationId,
                        organization_sso_identities::Column::OrganizationMembershipId,
                    ),
                )
                .to(
                    organization_memberships::Entity,
                    (
                        organization_memberships::Column::OrganizationId,
                        organization_memberships::Column::Id,
                    ),
                )
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .check(identity_lifecycle_check());
    manager.create_table(statement).await?;
    for (name, columns) in [
        (
            IDENTITY_SUBJECT_INDEX,
            vec![
                organization_sso_identities::Column::OrganizationId,
                organization_sso_identities::Column::ProviderId,
                organization_sso_identities::Column::SubjectDigest,
            ],
        ),
        (
            IDENTITY_MEMBERSHIP_INDEX,
            vec![
                organization_sso_identities::Column::OrganizationId,
                organization_sso_identities::Column::ProviderId,
                organization_sso_identities::Column::OrganizationMembershipId,
            ],
        ),
        (
            IDENTITY_SCOPE_ID_INDEX,
            vec![
                organization_sso_identities::Column::OrganizationId,
                organization_sso_identities::Column::Id,
            ],
        ),
    ] {
        let mut index = Index::create();
        index
            .name(name)
            .table(organization_sso_identities::Entity)
            .unique();
        for column in columns {
            index.col(column);
        }
        manager.create_index(index.to_owned()).await?;
    }
    Ok(())
}

async fn create_login_transactions(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let mut statement = table(manager, organization_sso_login_transactions::Entity);
    statement
        .col(auto_id(organization_sso_login_transactions::Column::Id))
        .col(
            ColumnDef::new(organization_sso_login_transactions::Column::OrganizationId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_sso_login_transactions::Column::ProviderId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_sso_login_transactions::Column::Protocol)
                .small_integer()
                .not_null()
                .check(
                    Expr::col(organization_sso_login_transactions::Column::Protocol)
                        .between(1_i16, 2_i16),
                ),
        )
        .col(
            ColumnDef::new(organization_sso_login_transactions::Column::ProviderVersion)
                .big_integer()
                .not_null()
                .check(
                    Expr::col(organization_sso_login_transactions::Column::ProviderVersion)
                        .gt(0_i64),
                ),
        )
        .col(
            ColumnDef::new(organization_sso_login_transactions::Column::StateDigest)
                .string_len(64)
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_sso_login_transactions::Column::RequestDigest)
                .string_len(64)
                .not_null(),
        )
        .col(timestamp(
            manager,
            organization_sso_login_transactions::Column::ExpiresAt,
        ))
        .col(nullable_timestamp(
            manager,
            organization_sso_login_transactions::Column::ClaimedAt,
        ))
        .col(
            ColumnDef::new(organization_sso_login_transactions::Column::ResultCode).small_integer(),
        )
        .col(
            ColumnDef::new(organization_sso_login_transactions::Column::TicketDigest)
                .string_len(64),
        )
        .col(nullable_timestamp(
            manager,
            organization_sso_login_transactions::Column::TicketExpiresAt,
        ))
        .col(nullable_timestamp(
            manager,
            organization_sso_login_transactions::Column::ExchangedAt,
        ))
        .col(
            ColumnDef::new(organization_sso_login_transactions::Column::AssertionDigest)
                .string_len(64),
        )
        .col(nullable_timestamp(
            manager,
            organization_sso_login_transactions::Column::AssertionExpiresAt,
        ))
        .col(timestamp(
            manager,
            organization_sso_login_transactions::Column::CreatedAt,
        ))
        .col(timestamp(
            manager,
            organization_sso_login_transactions::Column::UpdatedAt,
        ))
        .foreign_key(
            ForeignKey::create()
                .name("fk_organization_sso_login_transactions_provider_scope")
                .from(
                    organization_sso_login_transactions::Entity,
                    (
                        organization_sso_login_transactions::Column::OrganizationId,
                        organization_sso_login_transactions::Column::ProviderId,
                    ),
                )
                .to(
                    organization_sso_providers::Entity,
                    (
                        organization_sso_providers::Column::OrganizationId,
                        organization_sso_providers::Column::Id,
                    ),
                )
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .check(transaction_shape_check());
    manager.create_table(statement).await?;
    for (name, column, unique) in [
        (
            TRANSACTION_STATE_INDEX,
            organization_sso_login_transactions::Column::StateDigest,
            true,
        ),
        (
            TRANSACTION_TICKET_INDEX,
            organization_sso_login_transactions::Column::TicketDigest,
            true,
        ),
        (
            TRANSACTION_ASSERTION_INDEX,
            organization_sso_login_transactions::Column::AssertionDigest,
            true,
        ),
        (
            TRANSACTION_EXPIRES_INDEX,
            organization_sso_login_transactions::Column::ExpiresAt,
            false,
        ),
        (
            TRANSACTION_TICKET_EXPIRES_INDEX,
            organization_sso_login_transactions::Column::TicketExpiresAt,
            false,
        ),
    ] {
        let mut index = Index::create();
        index
            .name(name)
            .table(organization_sso_login_transactions::Entity)
            .col(column);
        if unique {
            index.unique();
        }
        manager.create_index(index.to_owned()).await?;
    }
    Ok(())
}

async fn create_provider_active_constraint(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    if manager.get_database_backend() == DbBackend::MySql {
        for statement in [
            "ALTER TABLE `organization_sso_providers` ADD COLUMN `_active_organization_id` BIGINT NULL",
            "UPDATE `organization_sso_providers` SET `_active_organization_id` = CASE WHEN `status` = 2 THEN `organization_id` ELSE NULL END",
            "CREATE UNIQUE INDEX `uq_organization_sso_providers_active` ON `organization_sso_providers` (`_active_organization_id`)",
            "CREATE TRIGGER `trg_org_sso_provider_active_insert` BEFORE INSERT ON `organization_sso_providers` FOR EACH ROW BEGIN SET NEW.`_active_organization_id` = CASE WHEN NEW.`status` = 2 THEN NEW.`organization_id` ELSE NULL END; END",
            "CREATE TRIGGER `trg_org_sso_provider_active_update` BEFORE UPDATE ON `organization_sso_providers` FOR EACH ROW BEGIN SET NEW.`_active_organization_id` = CASE WHEN NEW.`status` = 2 THEN NEW.`organization_id` ELSE NULL END; END",
        ] {
            manager
                .get_connection()
                .execute_unprepared(statement)
                .await?;
        }
        return Ok(());
    }
    manager
        .create_index(
            Index::create()
                .name(PROVIDER_ACTIVE_INDEX)
                .table(organization_sso_providers::Entity)
                .col(organization_sso_providers::Column::OrganizationId)
                .unique()
                .and_where(Expr::col(organization_sso_providers::Column::Status).eq(2_i16))
                .to_owned(),
        )
        .await
}

async fn create_domain_active_constraint(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    if manager.get_database_backend() == DbBackend::MySql {
        for statement in [
            "ALTER TABLE `organization_sso_domains` ADD COLUMN `_active_domain` VARCHAR(253) NULL",
            "UPDATE `organization_sso_domains` SET `_active_domain` = CASE WHEN `status` <> 3 THEN `domain` ELSE NULL END",
            "CREATE UNIQUE INDEX `uq_organization_sso_domains_active` ON `organization_sso_domains` (`_active_domain`)",
            "CREATE TRIGGER `trg_org_sso_domain_active_insert` BEFORE INSERT ON `organization_sso_domains` FOR EACH ROW BEGIN SET NEW.`_active_domain` = CASE WHEN NEW.`status` <> 3 THEN NEW.`domain` ELSE NULL END; END",
            "CREATE TRIGGER `trg_org_sso_domain_active_update` BEFORE UPDATE ON `organization_sso_domains` FOR EACH ROW BEGIN SET NEW.`_active_domain` = CASE WHEN NEW.`status` <> 3 THEN NEW.`domain` ELSE NULL END; END",
        ] {
            manager
                .get_connection()
                .execute_unprepared(statement)
                .await?;
        }
        return Ok(());
    }
    manager
        .create_index(
            Index::create()
                .name(DOMAIN_ACTIVE_INDEX)
                .table(organization_sso_domains::Entity)
                .col(organization_sso_domains::Column::Domain)
                .unique()
                .and_where(Expr::col(organization_sso_domains::Column::Status).ne(3_i16))
                .to_owned(),
        )
        .await
}

async fn drop_login_transactions(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .drop_table(
            Table::drop()
                .table(organization_sso_login_transactions::Entity)
                .to_owned(),
        )
        .await
}

async fn drop_identities(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .drop_table(
            Table::drop()
                .table(organization_sso_identities::Entity)
                .to_owned(),
        )
        .await
}

async fn drop_domains(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    if manager.get_database_backend() == DbBackend::MySql {
        for statement in [
            "DROP TRIGGER IF EXISTS `trg_org_sso_domain_active_insert`",
            "DROP TRIGGER IF EXISTS `trg_org_sso_domain_active_update`",
            "DROP INDEX `uq_organization_sso_domains_active` ON `organization_sso_domains`",
            "ALTER TABLE `organization_sso_domains` DROP COLUMN `_active_domain`",
        ] {
            manager
                .get_connection()
                .execute_unprepared(statement)
                .await?;
        }
    } else {
        manager
            .drop_index(
                Index::drop()
                    .name(DOMAIN_ACTIVE_INDEX)
                    .table(organization_sso_domains::Entity)
                    .to_owned(),
            )
            .await?;
    }
    manager
        .drop_table(
            Table::drop()
                .table(organization_sso_domains::Entity)
                .to_owned(),
        )
        .await
}

async fn drop_providers(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    if manager.get_database_backend() == DbBackend::MySql {
        for statement in [
            "DROP TRIGGER IF EXISTS `trg_org_sso_provider_active_insert`",
            "DROP TRIGGER IF EXISTS `trg_org_sso_provider_active_update`",
            "DROP INDEX `uq_organization_sso_providers_active` ON `organization_sso_providers`",
            "ALTER TABLE `organization_sso_providers` DROP COLUMN `_active_organization_id`",
        ] {
            manager
                .get_connection()
                .execute_unprepared(statement)
                .await?;
        }
    } else {
        manager
            .drop_index(
                Index::drop()
                    .name(PROVIDER_ACTIVE_INDEX)
                    .table(organization_sso_providers::Entity)
                    .to_owned(),
            )
            .await?;
    }
    manager
        .drop_table(
            Table::drop()
                .table(organization_sso_providers::Entity)
                .to_owned(),
        )
        .await
}

fn provider_key_format_check(database_backend: DbBackend) -> SimpleExpr {
    match database_backend {
        DbBackend::Postgres => Expr::cust(
            r#""provider_key" ~ '^[0-9a-f]{32}$' AND "provider_key" <> '00000000000000000000000000000000'"#,
        ),
        DbBackend::MySql => Expr::cust(
            "CHAR_LENGTH(`provider_key`) = 32 AND `provider_key` REGEXP '^[0-9a-f]{32}$' AND `provider_key` <> '00000000000000000000000000000000'",
        ),
        DbBackend::Sqlite => Expr::cust(
            r#"length("provider_key") = 32 AND "provider_key" NOT GLOB '*[^0-9a-f]*' AND "provider_key" <> '00000000000000000000000000000000'"#,
        ),
    }
}

fn domain_lifecycle_check() -> SimpleExpr {
    let status = organization_sso_domains::Column::Status;
    let verified_at = organization_sso_domains::Column::VerifiedAt;
    let revoked_at = organization_sso_domains::Column::RevokedAt;
    Expr::col(status)
        .eq(1_i16)
        .and(Expr::col(verified_at).is_null())
        .and(Expr::col(revoked_at).is_null())
        .or(Expr::col(status)
            .eq(2_i16)
            .and(Expr::col(verified_at).is_not_null())
            .and(Expr::col(revoked_at).is_null()))
        .or(Expr::col(status)
            .eq(3_i16)
            .and(Expr::col(revoked_at).is_not_null()))
}

fn identity_lifecycle_check() -> SimpleExpr {
    Expr::col(organization_sso_identities::Column::Status)
        .eq(1_i16)
        .and(Expr::col(organization_sso_identities::Column::DisabledAt).is_null())
        .or(Expr::col(organization_sso_identities::Column::Status)
            .eq(2_i16)
            .and(Expr::col(organization_sso_identities::Column::DisabledAt).is_not_null()))
}

fn transaction_shape_check() -> SimpleExpr {
    // 票据、断言及交换时间必须与对应摘要成对出现，避免持久化半成品状态。
    let ticket_shape = Expr::col(organization_sso_login_transactions::Column::TicketDigest)
        .is_null()
        .and(Expr::col(organization_sso_login_transactions::Column::TicketExpiresAt).is_null())
        .or(
            Expr::col(organization_sso_login_transactions::Column::TicketDigest)
                .is_not_null()
                .and(
                    Expr::col(organization_sso_login_transactions::Column::TicketExpiresAt)
                        .is_not_null(),
                ),
        );
    let assertion_shape = Expr::col(organization_sso_login_transactions::Column::AssertionDigest)
        .is_null()
        .and(Expr::col(organization_sso_login_transactions::Column::AssertionExpiresAt).is_null())
        .or(
            Expr::col(organization_sso_login_transactions::Column::AssertionDigest)
                .is_not_null()
                .and(
                    Expr::col(organization_sso_login_transactions::Column::AssertionExpiresAt)
                        .is_not_null(),
                ),
        );
    let exchange_shape = Expr::col(organization_sso_login_transactions::Column::ExchangedAt)
        .is_null()
        .or(Expr::col(organization_sso_login_transactions::Column::TicketDigest).is_not_null());
    ticket_shape.and(assertion_shape).and(exchange_shape)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mysql_provider_key_check_avoids_binary_regex_operands() {
        let rendered = Query::select()
            .expr(provider_key_format_check(DbBackend::MySql))
            .to_owned()
            .to_string(MysqlQueryBuilder);
        assert!(rendered.contains("REGEXP '^[0-9a-f]{32}$'"));
        assert!(!rendered.contains("BINARY"));
    }

    #[test]
    fn provider_key_checks_render_native_quoted_identifiers() {
        let postgres = Query::select()
            .expr(provider_key_format_check(DbBackend::Postgres))
            .to_owned()
            .to_string(PostgresQueryBuilder);
        assert!(postgres.contains(r#""provider_key" ~ '^[0-9a-f]{32}$'"#));
        assert!(!postgres.contains('\\'));

        let sqlite = Query::select()
            .expr(provider_key_format_check(DbBackend::Sqlite))
            .to_owned()
            .to_string(SqliteQueryBuilder);
        assert!(sqlite.contains(r#"length("provider_key") = 32"#));
        assert!(!sqlite.contains('\\'));
    }
}
