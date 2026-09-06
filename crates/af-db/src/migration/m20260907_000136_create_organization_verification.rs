use sea_orm::DbBackend;
use sea_orm_migration::prelude::*;

use super::{
    iden::{
        organization_verification_cases, organization_verification_materials, organizations, users,
    },
    schema::{auto_id, nullable_timestamp, table, timestamp},
};

/// 创建企业认证案件与材料元数据事实。
#[derive(DeriveMigrationName)]
pub(crate) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        use organization_verification_cases::Column as Case;
        let cases = table(manager, organization_verification_cases::Entity)
            .col(auto_id(Case::Id))
            .col(
                ColumnDef::new(Case::OrganizationId)
                    .big_integer()
                    .not_null(),
            )
            .col(
                ColumnDef::new(Case::ApplicantUserId)
                    .big_integer()
                    .not_null(),
            )
            .col(ColumnDef::new(Case::Status).small_integer().not_null())
            .col(
                ColumnDef::new(Case::Version)
                    .big_integer()
                    .not_null()
                    .default(1_i64),
            )
            .col(ColumnDef::new(Case::Summary).string_len(512).not_null())
            .col(ColumnDef::new(Case::ReviewerUserId).big_integer())
            .col(ColumnDef::new(Case::ReviewReason).string_len(512))
            .col(timestamp(manager, Case::CreatedAt))
            .col(timestamp(manager, Case::UpdatedAt))
            .col(nullable_timestamp(manager, Case::DecidedAt))
            .check(Expr::col(Case::OrganizationId).gt(0_i64))
            .check(Expr::col(Case::ApplicantUserId).gt(0_i64))
            .check(Expr::col(Case::Status).between(1_i16, 5_i16))
            .check(Expr::col(Case::Version).gt(0_i64))
            .check(text_shape_check(
                manager.get_database_backend(),
                Case::Summary,
                512,
            ))
            .check(Expr::col(Case::ReviewReason).is_null().or(text_shape_check(
                manager.get_database_backend(),
                Case::ReviewReason,
                512,
            )))
            .check(status_shape_check())
            .foreign_key(
                ForeignKey::create()
                    .name("fk_org_verification_cases_organization")
                    .from(
                        organization_verification_cases::Entity,
                        Case::OrganizationId,
                    )
                    .to(organizations::Entity, organizations::Column::Id)
                    .on_update(ForeignKeyAction::Restrict)
                    .on_delete(ForeignKeyAction::Restrict),
            )
            .foreign_key(
                ForeignKey::create()
                    .name("fk_org_verification_cases_applicant")
                    .from(
                        organization_verification_cases::Entity,
                        Case::ApplicantUserId,
                    )
                    .to(users::Entity, users::Column::Id)
                    .on_update(ForeignKeyAction::Restrict)
                    .on_delete(ForeignKeyAction::Restrict),
            )
            .foreign_key(
                ForeignKey::create()
                    .name("fk_org_verification_cases_reviewer")
                    .from(
                        organization_verification_cases::Entity,
                        Case::ReviewerUserId,
                    )
                    .to(users::Entity, users::Column::Id)
                    .on_update(ForeignKeyAction::Restrict)
                    .on_delete(ForeignKeyAction::Restrict),
            )
            .to_owned();
        manager.create_table(cases).await?;
        for index in [
            Index::create()
                .name("uq_org_verification_cases_organization")
                .table(organization_verification_cases::Entity)
                .col(Case::OrganizationId)
                .unique()
                .to_owned(),
            Index::create()
                .name("idx_org_verification_cases_status")
                .table(organization_verification_cases::Entity)
                .col(Case::Status)
                .col(Case::UpdatedAt)
                .to_owned(),
        ] {
            manager.create_index(index).await?;
        }

        use organization_verification_materials::Column as Material;
        let materials = table(manager, organization_verification_materials::Entity)
            .col(auto_id(Material::Id))
            .col(ColumnDef::new(Material::CaseId).big_integer().not_null())
            .col(ColumnDef::new(Material::Kind).string_len(64).not_null())
            .col(
                ColumnDef::new(Material::ObjectReference)
                    .string_len(256)
                    .not_null(),
            )
            .col(
                ColumnDef::new(Material::FileName)
                    .string_len(255)
                    .not_null(),
            )
            .col(
                ColumnDef::new(Material::ContentType)
                    .string_len(128)
                    .not_null(),
            )
            .col(ColumnDef::new(Material::SizeBytes).big_integer().not_null())
            .col(timestamp(manager, Material::CreatedAt))
            .check(Expr::col(Material::CaseId).gt(0_i64))
            .check(text_shape_check(
                manager.get_database_backend(),
                Material::Kind,
                64,
            ))
            .check(text_shape_check(
                manager.get_database_backend(),
                Material::ObjectReference,
                256,
            ))
            .check(text_shape_check(
                manager.get_database_backend(),
                Material::FileName,
                255,
            ))
            .check(text_shape_check(
                manager.get_database_backend(),
                Material::ContentType,
                128,
            ))
            .check(Expr::col(Material::SizeBytes).between(0_i64, 50_000_000_i64))
            .foreign_key(
                ForeignKey::create()
                    .name("fk_org_verification_materials_case")
                    .from(
                        organization_verification_materials::Entity,
                        Material::CaseId,
                    )
                    .to(organization_verification_cases::Entity, Case::Id)
                    .on_update(ForeignKeyAction::Restrict)
                    .on_delete(ForeignKeyAction::Restrict),
            )
            .to_owned();
        manager.create_table(materials).await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_org_verification_materials_case")
                    .table(organization_verification_materials::Entity)
                    .col(Material::CaseId)
                    .col(Material::Id)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(organization_verification_materials::Entity)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(organization_verification_cases::Entity)
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}

fn text_shape_check<C>(backend: DbBackend, column: C, max: usize) -> SimpleExpr
where
    C: Iden,
{
    let name = Iden::to_string(&column);
    match backend {
        DbBackend::Postgres => Expr::cust(format!(
            "octet_length(\"{name}\") BETWEEN 1 AND {max} AND \"{name}\" = btrim(\"{name}\")"
        )),
        DbBackend::MySql => Expr::cust(format!(
            "OCTET_LENGTH(`{name}`) BETWEEN 1 AND {max} AND `{name}` = TRIM(`{name}`)"
        )),
        DbBackend::Sqlite => Expr::cust(format!(
            "length(CAST(\"{name}\" AS BLOB)) BETWEEN 1 AND {max} AND \"{name}\" = trim(\"{name}\")"
        )),
    }
}

fn status_shape_check() -> SimpleExpr {
    Expr::cust(
        "(status IN (1, 2) AND reviewer_user_id IS NULL AND review_reason IS NULL AND decided_at IS NULL) OR (status = 3 AND reviewer_user_id IS NOT NULL AND review_reason IS NOT NULL AND decided_at IS NOT NULL) OR (status = 4 AND reviewer_user_id IS NOT NULL AND review_reason IS NULL AND decided_at IS NOT NULL) OR (status = 5 AND reviewer_user_id IS NOT NULL AND review_reason IS NOT NULL AND decided_at IS NOT NULL)",
    )
}
