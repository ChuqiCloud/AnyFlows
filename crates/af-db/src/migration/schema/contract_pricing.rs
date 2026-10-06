use sea_orm::DbBackend;
use sea_orm_migration::prelude::*;

use crate::migration::iden::{organization_contract_prices, organizations, users};

use super::{auto_id, nullable_timestamp, table, timestamp};

const PRICE_PRECISION: u32 = 38;
const PRICE_SCALE: u32 = 28;
const CONTRACT_PRICE_VERSION_INDEX: &str = "uq_organization_contract_prices_scope_version";
const CONTRACT_PRICE_LOOKUP_INDEX: &str = "idx_organization_contract_prices_lookup";

/// 创建企业合同价版本事实表及查询索引。
pub(in crate::migration) async fn create_organization_contract_prices(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    let statement = table(manager, organization_contract_prices::Entity)
        .if_not_exists()
        .col(auto_id(organization_contract_prices::Column::Id))
        .col(
            ColumnDef::new(organization_contract_prices::Column::OrganizationId)
                .big_integer()
                .not_null(),
        )
        .col(
            ColumnDef::new(organization_contract_prices::Column::Model)
                .string_len(256)
                .not_null()
                .check(Expr::col(organization_contract_prices::Column::Model).ne("")),
        )
        .col(
            ColumnDef::new(organization_contract_prices::Column::Protocol)
                .string_len(32)
                .not_null()
                .check(
                    Expr::col(organization_contract_prices::Column::Protocol).is_in([
                        "openai_chat",
                        "openai_responses",
                        "openai_embeddings",
                        "openai_images",
                        "openai_audio",
                        "openai_speech",
                        "jina_rerank",
                        "cohere_rerank",
                        "xai_video",
                        "anthropic",
                        "gemini",
                    ]),
                ),
        )
        .col(price(
            manager,
            organization_contract_prices::Column::InputPrice,
        ))
        .col(price(
            manager,
            organization_contract_prices::Column::OutputPrice,
        ))
        .col(price(
            manager,
            organization_contract_prices::Column::CacheReadPrice,
        ))
        .col(price(
            manager,
            organization_contract_prices::Column::CacheCreation5mPrice,
        ))
        .col(price(
            manager,
            organization_contract_prices::Column::CacheCreation1hPrice,
        ))
        .col(
            nullable_timestamp(manager, organization_contract_prices::Column::EffectiveFrom)
                .not_null(),
        )
        .col(nullable_timestamp(
            manager,
            organization_contract_prices::Column::EffectiveUntil,
        ))
        .col(
            ColumnDef::new(organization_contract_prices::Column::Version)
                .big_integer()
                .not_null()
                .default(1_i64)
                .check(Expr::col(organization_contract_prices::Column::Version).gt(0_i64)),
        )
        .col(
            ColumnDef::new(organization_contract_prices::Column::CreatedByUserId)
                .big_integer()
                .not_null(),
        )
        .col(timestamp(
            manager,
            organization_contract_prices::Column::CreatedAt,
        ))
        .col(timestamp(
            manager,
            organization_contract_prices::Column::UpdatedAt,
        ))
        .check(
            Expr::col(organization_contract_prices::Column::EffectiveUntil)
                .is_null()
                .or(
                    Expr::col(organization_contract_prices::Column::EffectiveUntil).gt(Expr::col(
                        organization_contract_prices::Column::EffectiveFrom,
                    )),
                ),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_organization_contract_prices_organization")
                .from(
                    organization_contract_prices::Entity,
                    organization_contract_prices::Column::OrganizationId,
                )
                .to(organizations::Entity, organizations::Column::Id)
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_organization_contract_prices_creator")
                .from(
                    organization_contract_prices::Entity,
                    organization_contract_prices::Column::CreatedByUserId,
                )
                .to(users::Entity, users::Column::Id)
                .on_update(ForeignKeyAction::Restrict)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .to_owned();
    manager.create_table(statement).await?;

    for index in [
        Index::create()
            .name(CONTRACT_PRICE_VERSION_INDEX)
            .table(organization_contract_prices::Entity)
            .col(organization_contract_prices::Column::OrganizationId)
            .col(organization_contract_prices::Column::Model)
            .col(organization_contract_prices::Column::Protocol)
            .col(organization_contract_prices::Column::Version)
            .unique()
            .to_owned(),
        Index::create()
            .name(CONTRACT_PRICE_LOOKUP_INDEX)
            .table(organization_contract_prices::Entity)
            .col(organization_contract_prices::Column::OrganizationId)
            .col(organization_contract_prices::Column::Model)
            .col(organization_contract_prices::Column::Protocol)
            .col(organization_contract_prices::Column::EffectiveFrom)
            .col(organization_contract_prices::Column::Id)
            .to_owned(),
    ] {
        manager.create_index(index).await?;
    }
    Ok(())
}

/// 按索引和外键依赖逆序删除企业合同价事实表。
pub(in crate::migration) async fn drop_organization_contract_prices(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    manager
        .drop_table(
            Table::drop()
                .table(organization_contract_prices::Entity)
                .to_owned(),
        )
        .await
}

fn price<T>(manager: &SchemaManager<'_>, column: T) -> ColumnDef
where
    T: IntoIden + Clone + 'static,
{
    let mut definition = ColumnDef::new(column.clone());
    // SQLite 不接受高精度声明，生产方言固定 Decimal(38,28) 防止金额截断。
    if manager.get_database_backend() == DbBackend::Sqlite {
        definition.decimal();
    } else {
        definition.decimal_len(PRICE_PRECISION, PRICE_SCALE);
    }
    definition
        .not_null()
        .default(0_i64)
        .check(Expr::col(column).gte(0_i64));
    definition
}
