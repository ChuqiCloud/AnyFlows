use sea_orm::DbBackend;
use sea_orm_migration::prelude::*;

use crate::migration::iden::{
    organization_approval_contract_price_executions, organization_approval_decisions,
    organization_approval_requests, organization_contract_prices, organization_memberships,
    organizations,
};

use super::{auto_id, nullable_timestamp, table};

const CONTRACT_PRICE_SCOPE_ID_INDEX: &str = "uq_org_contract_prices_scope_id";
const REQUEST_EXECUTION_INDEX: &str = "uq_org_approval_contract_price_exec_request";

/// 创建合同价审批执行事实及合同价企业范围复合引用。
pub(in crate::migration) async fn create_organization_approval_contract_price_executions(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    manager
        .create_index(
            Index::create()
                .name(CONTRACT_PRICE_SCOPE_ID_INDEX)
                .table(organization_contract_prices::Entity)
                .col(organization_contract_prices::Column::OrganizationId)
                .col(organization_contract_prices::Column::Id)
                .unique()
                .to_owned(),
        )
        .await?;

    use organization_approval_contract_price_executions::Column;
    let statement = table(
        manager,
        organization_approval_contract_price_executions::Entity,
    )
    .col(auto_id(Column::Id))
    .col(
        ColumnDef::new(Column::OrganizationId)
            .big_integer()
            .not_null(),
    )
    .col(ColumnDef::new(Column::RequestId).big_integer().not_null())
    .col(ColumnDef::new(Column::DecisionId).big_integer().not_null())
    .col(
        ColumnDef::new(Column::ContractPriceId)
            .big_integer()
            .not_null(),
    )
    .col(
        ColumnDef::new(Column::Model)
            .string_len(256)
            .not_null()
            .check(Expr::col(Column::Model).ne("")),
    )
    .col(
        ColumnDef::new(Column::Protocol)
            .string_len(32)
            .not_null()
            .check(Expr::col(Column::Protocol).is_in([
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
            ])),
    )
    .col(price(manager, Column::InputPrice))
    .col(price(manager, Column::OutputPrice))
    .col(price(manager, Column::CacheReadPrice))
    .col(price(manager, Column::CacheCreation5mPrice))
    .col(price(manager, Column::CacheCreation1hPrice))
    .col(nullable_timestamp(manager, Column::EffectiveFrom).not_null())
    .col(nullable_timestamp(manager, Column::EffectiveUntil))
    .col(ColumnDef::new(Column::ExpectedVersion).big_integer())
    .col(
        ColumnDef::new(Column::AppliedVersion)
            .big_integer()
            .not_null()
            .check(Expr::col(Column::AppliedVersion).gt(0_i64)),
    )
    .col(
        ColumnDef::new(Column::ExecutedByMembershipId)
            .big_integer()
            .not_null(),
    )
    .col(nullable_timestamp(manager, Column::ExecutedAt).not_null())
    .check(
        Expr::col(Column::EffectiveUntil)
            .is_null()
            .or(Expr::col(Column::EffectiveUntil).gt(Expr::col(Column::EffectiveFrom))),
    )
    .check(
        Expr::col(Column::ExpectedVersion)
            .is_null()
            .or(Expr::col(Column::ExpectedVersion).between(0_i64, i64::MAX - 1)),
    )
    .check(
        Expr::col(Column::ExpectedVersion)
            .is_null()
            .and(Expr::col(Column::AppliedVersion).eq(1_i64))
            .or(Expr::col(Column::ExpectedVersion).is_not_null().and(
                Expr::col(Column::ExpectedVersion)
                    .add(1_i64)
                    .eq(Expr::col(Column::AppliedVersion)),
            )),
    )
    .foreign_key(
        ForeignKey::create()
            .name("fk_org_approval_contract_price_exec_org")
            .from(
                organization_approval_contract_price_executions::Entity,
                Column::OrganizationId,
            )
            .to(organizations::Entity, organizations::Column::Id)
            .on_update(ForeignKeyAction::Restrict)
            .on_delete(ForeignKeyAction::Restrict),
    )
    .foreign_key(
        ForeignKey::create()
            .name("fk_org_approval_contract_price_exec_request")
            .from(
                organization_approval_contract_price_executions::Entity,
                (Column::OrganizationId, Column::RequestId),
            )
            .to(
                organization_approval_requests::Entity,
                (
                    organization_approval_requests::Column::OrganizationId,
                    organization_approval_requests::Column::Id,
                ),
            )
            .on_update(ForeignKeyAction::Restrict)
            .on_delete(ForeignKeyAction::Restrict),
    )
    .foreign_key(
        ForeignKey::create()
            .name("fk_org_approval_contract_price_exec_decision")
            .from(
                organization_approval_contract_price_executions::Entity,
                (Column::OrganizationId, Column::DecisionId),
            )
            .to(
                organization_approval_decisions::Entity,
                (
                    organization_approval_decisions::Column::OrganizationId,
                    organization_approval_decisions::Column::Id,
                ),
            )
            .on_update(ForeignKeyAction::Restrict)
            .on_delete(ForeignKeyAction::Restrict),
    )
    .foreign_key(
        ForeignKey::create()
            .name("fk_org_approval_contract_price_exec_price")
            .from(
                organization_approval_contract_price_executions::Entity,
                (Column::OrganizationId, Column::ContractPriceId),
            )
            .to(
                organization_contract_prices::Entity,
                (
                    organization_contract_prices::Column::OrganizationId,
                    organization_contract_prices::Column::Id,
                ),
            )
            .on_update(ForeignKeyAction::Restrict)
            .on_delete(ForeignKeyAction::Restrict),
    )
    .foreign_key(
        ForeignKey::create()
            .name("fk_org_approval_contract_price_exec_actor")
            .from(
                organization_approval_contract_price_executions::Entity,
                (Column::OrganizationId, Column::ExecutedByMembershipId),
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
    .to_owned();
    manager.create_table(statement).await?;
    manager
        .create_index(
            Index::create()
                .name(REQUEST_EXECUTION_INDEX)
                .table(organization_approval_contract_price_executions::Entity)
                .col(Column::OrganizationId)
                .col(Column::RequestId)
                .unique()
                .to_owned(),
        )
        .await
}

/// 删除合同价审批执行事实及合同价复合引用索引。
pub(in crate::migration) async fn drop_organization_approval_contract_price_executions(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    manager
        .drop_table(
            Table::drop()
                .table(organization_approval_contract_price_executions::Entity)
                .if_exists()
                .to_owned(),
        )
        .await?;
    manager
        .drop_index(
            Index::drop()
                .name(CONTRACT_PRICE_SCOPE_ID_INDEX)
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
        definition.decimal_len(38, 28);
    }
    definition
        .not_null()
        .default(0_i64)
        .check(Expr::col(column).gte(0_i64));
    definition
}
