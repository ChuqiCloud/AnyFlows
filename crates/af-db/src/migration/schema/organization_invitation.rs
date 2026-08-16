use sea_orm::DbBackend;
use sea_orm_migration::prelude::*;

use crate::migration::iden::{organization_invitations, organization_teams, organizations, users};

use super::{auto_id, nullable_timestamp, table, timestamp};

const INVITATION_KEY_INDEX: &str = "uq_organization_invitations_key";
const TOKEN_DIGEST_INDEX: &str = "uq_organization_invitations_token_digest";
const SCOPE_EMAIL_STATUS_INDEX: &str = "idx_organization_invitations_email_status";
const SCOPE_STATUS_EXPIRY_INDEX: &str = "idx_organization_invitations_status_expiry";
const SCOPE_BATCH_EMAIL_INDEX: &str = "uq_organization_invitations_batch_email";

/// 创建企业邀请、批次幂等和单次消费所需的持久化边界。
pub(in crate::migration) async fn create_organization_invitations(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    let mut statement = table(manager, organization_invitations::Entity);
    statement
        .col(auto_id(organization_invitations::Column::Id))
        .col(
            ColumnDef::new(organization_invitations::Column::InvitationKey)
                .char_len(32)
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_invitations::Column::OrganizationId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_invitations::Column::EmailFingerprint)
                .char_len(64)
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_invitations::Column::EmailSecret)
                .json_binary()
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_invitations::Column::TokenDigest)
                .char_len(64)
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_invitations::Column::Role)
                .small_integer()
                .not_null()
                .check(Expr::col(organization_invitations::Column::Role).between(1_i16, 5_i16)),
        )
        .col(ColumnDef::new(organization_invitations::Column::TeamId).big_integer())
        .col(
            ColumnDef::new(organization_invitations::Column::Status)
                .small_integer()
                .not_null()
                .default(1_i16)
                .check(Expr::col(organization_invitations::Column::Status).between(1_i16, 4_i16)),
        )
        .col(
            ColumnDef::new(organization_invitations::Column::InvitedByUserId)
                .big_integer()
                .not_null(),
        )
        .col(ColumnDef::new(organization_invitations::Column::AcceptedByUserId).big_integer())
        .col(ColumnDef::new(organization_invitations::Column::BatchKey).char_len(32))
        .col(ColumnDef::new(organization_invitations::Column::BatchDigest).char_len(64))
        .col(ColumnDef::new(organization_invitations::Column::BatchSize).integer())
        .col(
            ColumnDef::new(organization_invitations::Column::Version)
                .big_integer()
                .not_null()
                .default(1_i64)
                .check(Expr::col(organization_invitations::Column::Version).gt(0_i64)),
        )
        .col(timestamp(
            manager,
            organization_invitations::Column::CreatedAt,
        ))
        .col(timestamp(
            manager,
            organization_invitations::Column::ExpiresAt,
        ))
        .col(nullable_timestamp(
            manager,
            organization_invitations::Column::FinalizedAt,
        ))
        .foreign_key(
            ForeignKey::create()
                .name("fk_organization_invitations_organization")
                .from(
                    organization_invitations::Entity,
                    organization_invitations::Column::OrganizationId,
                )
                .to(organizations::Entity, organizations::Column::Id)
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_organization_invitations_inviter")
                .from(
                    organization_invitations::Entity,
                    organization_invitations::Column::InvitedByUserId,
                )
                .to(users::Entity, users::Column::Id)
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_organization_invitations_acceptor")
                .from(
                    organization_invitations::Entity,
                    organization_invitations::Column::AcceptedByUserId,
                )
                .to(users::Entity, users::Column::Id)
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_organization_invitations_team_scope")
                .from(
                    organization_invitations::Entity,
                    (
                        organization_invitations::Column::OrganizationId,
                        organization_invitations::Column::TeamId,
                    ),
                )
                .to(
                    organization_teams::Entity,
                    (
                        organization_teams::Column::OrganizationId,
                        organization_teams::Column::Id,
                    ),
                )
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .check(fixed_lower_hex_check(
            manager.get_database_backend(),
            organization_invitations::Column::InvitationKey,
            32,
            true,
        ))
        .check(fixed_lower_hex_check(
            manager.get_database_backend(),
            organization_invitations::Column::EmailFingerprint,
            64,
            false,
        ))
        .check(fixed_lower_hex_check(
            manager.get_database_backend(),
            organization_invitations::Column::TokenDigest,
            64,
            false,
        ))
        .check(
            Expr::col(organization_invitations::Column::ExpiresAt)
                .gt(Expr::col(organization_invitations::Column::CreatedAt)),
        )
        .check(batch_shape_check(manager.get_database_backend()))
        .check(invitation_lifecycle_check());
    manager.create_table(statement).await?;

    for index in [
        Index::create()
            .name(INVITATION_KEY_INDEX)
            .table(organization_invitations::Entity)
            .col(organization_invitations::Column::InvitationKey)
            .unique()
            .to_owned(),
        Index::create()
            .name(TOKEN_DIGEST_INDEX)
            .table(organization_invitations::Entity)
            .col(organization_invitations::Column::TokenDigest)
            .unique()
            .to_owned(),
        Index::create()
            .name(SCOPE_EMAIL_STATUS_INDEX)
            .table(organization_invitations::Entity)
            .col(organization_invitations::Column::OrganizationId)
            .col(organization_invitations::Column::EmailFingerprint)
            .col(organization_invitations::Column::Status)
            .col(organization_invitations::Column::Id)
            .to_owned(),
        Index::create()
            .name(SCOPE_STATUS_EXPIRY_INDEX)
            .table(organization_invitations::Entity)
            .col(organization_invitations::Column::OrganizationId)
            .col(organization_invitations::Column::Status)
            .col(organization_invitations::Column::ExpiresAt)
            .col(organization_invitations::Column::Id)
            .to_owned(),
        Index::create()
            .name(SCOPE_BATCH_EMAIL_INDEX)
            .table(organization_invitations::Entity)
            .col(organization_invitations::Column::OrganizationId)
            .col(organization_invitations::Column::BatchKey)
            .col(organization_invitations::Column::EmailFingerprint)
            .unique()
            .to_owned(),
    ] {
        manager.create_index(index).await?;
    }
    Ok(())
}

fn fixed_lower_hex_check(
    backend: DbBackend,
    column: organization_invitations::Column,
    length: usize,
    non_zero: bool,
) -> SimpleExpr {
    let name = column.to_string();
    let zeros = "0".repeat(length);
    match backend {
        DbBackend::Postgres => Expr::cust(format!(
            "\"{name}\" ~ '^[0-9a-f]{{{length}}}$'{}",
            if non_zero {
                format!(" AND \"{name}\" <> '{zeros}'")
            } else {
                String::new()
            }
        )),
        DbBackend::MySql => Expr::cust(format!(
            "CHAR_LENGTH(`{name}`) = {length} AND `{name}` REGEXP '^[0-9a-f]{{{length}}}$'{}",
            if non_zero {
                format!(" AND `{name}` <> '{zeros}'")
            } else {
                String::new()
            }
        )),
        DbBackend::Sqlite => Expr::cust(format!(
            "length(\"{name}\") = {length} AND \"{name}\" NOT GLOB '*[^0-9a-f]*'{}",
            if non_zero {
                format!(" AND \"{name}\" <> '{zeros}'")
            } else {
                String::new()
            }
        )),
    }
}

fn batch_shape_check(backend: DbBackend) -> SimpleExpr {
    let empty = Expr::col(organization_invitations::Column::BatchKey)
        .is_null()
        .and(Expr::col(organization_invitations::Column::BatchDigest).is_null())
        .and(Expr::col(organization_invitations::Column::BatchSize).is_null());
    let present = Expr::col(organization_invitations::Column::BatchKey)
        .is_not_null()
        .and(Expr::col(organization_invitations::Column::BatchDigest).is_not_null())
        .and(Expr::col(organization_invitations::Column::BatchSize).between(1_i32, 200_i32))
        .and(fixed_lower_hex_check(
            backend,
            organization_invitations::Column::BatchKey,
            32,
            true,
        ))
        .and(fixed_lower_hex_check(
            backend,
            organization_invitations::Column::BatchDigest,
            64,
            false,
        ));
    empty.or(present)
}

fn invitation_lifecycle_check() -> SimpleExpr {
    let status = organization_invitations::Column::Status;
    let accepted_by = organization_invitations::Column::AcceptedByUserId;
    let finalized_at = organization_invitations::Column::FinalizedAt;
    Expr::col(status)
        .eq(1_i16)
        .and(Expr::col(accepted_by).is_null())
        .and(Expr::col(finalized_at).is_null())
        .or(Expr::col(status)
            .eq(2_i16)
            .and(Expr::col(accepted_by).is_not_null())
            .and(Expr::col(finalized_at).is_not_null()))
        .or(Expr::col(status)
            .is_in([3_i16, 4_i16])
            .and(Expr::col(accepted_by).is_null())
            .and(Expr::col(finalized_at).is_not_null()))
}

#[cfg(test)]
mod tests {
    use sea_orm::sea_query::{MysqlQueryBuilder, Query};

    use super::*;

    #[test]
    fn mysql_hex_checks_do_not_use_binary_regex_operands() {
        let rendered = Query::select()
            .expr(fixed_lower_hex_check(
                DbBackend::MySql,
                organization_invitations::Column::TokenDigest,
                64,
                false,
            ))
            .to_owned()
            .to_string(MysqlQueryBuilder);
        assert!(rendered.contains("REGEXP '^[0-9a-f]{64}$'"));
        assert!(!rendered.contains("BINARY"));
    }
}
