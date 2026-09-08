use sea_orm::{ConnectionTrait, DbBackend};
use sea_orm_migration::prelude::*;

use crate::migration::iden::models;

use super::{auto_id, nullable_timestamp, table, timestamp};

/// 创建与价格、渠道能力和上游映射完全独立的模型商品元数据表。
pub(in crate::migration) async fn create_models(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let has_input_modality = Expr::col(models::Column::SupportsTextInput)
        .eq(true)
        .or(Expr::col(models::Column::SupportsImageInput).eq(true))
        .or(Expr::col(models::Column::SupportsAudioInput).eq(true))
        .or(Expr::col(models::Column::SupportsVideoInput).eq(true));
    let has_output_modality = Expr::col(models::Column::SupportsTextOutput)
        .eq(true)
        .or(Expr::col(models::Column::SupportsImageOutput).eq(true))
        .or(Expr::col(models::Column::SupportsAudioOutput).eq(true))
        .or(Expr::col(models::Column::SupportsVideoOutput).eq(true));

    manager
        .create_table(
            table(manager, models::Entity)
                .col(auto_id(models::Column::Id))
                .col(
                    ColumnDef::new(models::Column::Model)
                        .string_len(256)
                        .not_null()
                        .check(Expr::col(models::Column::Model).ne("")),
                )
                .col(
                    ColumnDef::new(models::Column::DisplayName)
                        .string_len(128)
                        .not_null()
                        .check(Expr::col(models::Column::DisplayName).ne("")),
                )
                .col(
                    ColumnDef::new(models::Column::Provider)
                        .string_len(64)
                        .not_null()
                        .check(Expr::col(models::Column::Provider).ne("")),
                )
                .col(ColumnDef::new(models::Column::Description).text())
                .col(ColumnDef::new(models::Column::IconUrl).string_len(2048))
                .col(
                    ColumnDef::new(models::Column::Tags)
                        .json_binary()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(models::Column::ContextWindow)
                        .big_integer()
                        .check(Expr::col(models::Column::ContextWindow).gt(0_i64)),
                )
                .col(boolean(models::Column::SupportsTextInput))
                .col(boolean(models::Column::SupportsImageInput))
                .col(boolean(models::Column::SupportsAudioInput))
                .col(boolean(models::Column::SupportsVideoInput))
                .col(boolean(models::Column::SupportsTextOutput))
                .col(boolean(models::Column::SupportsImageOutput))
                .col(boolean(models::Column::SupportsAudioOutput))
                .col(boolean(models::Column::SupportsVideoOutput))
                .col(boolean(models::Column::SupportsReasoning))
                .col(boolean(models::Column::SupportsToolCalls))
                .col(
                    ColumnDef::new(models::Column::Visibility)
                        .small_integer()
                        .not_null()
                        .check(Expr::col(models::Column::Visibility).is_in([1_i16, 2_i16, 3_i16])),
                )
                .col(
                    ColumnDef::new(models::Column::Lifecycle)
                        .small_integer()
                        .not_null()
                        .check(
                            Expr::col(models::Column::Lifecycle)
                                .is_in([1_i16, 2_i16, 3_i16, 4_i16]),
                        ),
                )
                .col(timestamp(manager, models::Column::CreatedAt))
                .col(timestamp(manager, models::Column::UpdatedAt))
                .col(nullable_timestamp(manager, models::Column::DeletedAt))
                .check(has_input_modality)
                .check(has_output_modality)
                .to_owned(),
        )
        .await?;
    create_model_indexes(manager).await
}

fn boolean<T>(column: T) -> ColumnDef
where
    T: IntoIden,
{
    let mut definition = ColumnDef::new(column);
    definition.boolean().not_null().default(false);
    definition
}

async fn create_model_indexes(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    if manager.get_database_backend() == DbBackend::MySql {
        // MySQL 不支持部分索引；摘要生成列同时规避旧版 InnoDB 的长 utf8mb4 索引上限。
        manager
            .get_connection()
            .execute_unprepared(
                r#"ALTER TABLE `models`
ADD COLUMN `_active_model_hash` CHAR(64) CHARACTER SET ascii
    GENERATED ALWAYS AS (CASE WHEN `deleted_at` IS NULL THEN SHA2(`model`, 256) ELSE NULL END) STORED,
ADD UNIQUE INDEX `uq_models_active_model` (`_active_model_hash`)"#,
            )
            .await?;
    } else {
        manager
            .create_index(
                Index::create()
                    .name("uq_models_active_model")
                    .table(models::Entity)
                    .col(models::Column::Model)
                    .unique()
                    .and_where(Expr::col(models::Column::DeletedAt).is_null())
                    .to_owned(),
            )
            .await?;
    }
    manager
        .create_index(
            Index::create()
                .name("idx_models_lifecycle_visibility_id")
                .table(models::Entity)
                .col(models::Column::Lifecycle)
                .col(models::Column::Visibility)
                .col(models::Column::Id)
                .to_owned(),
        )
        .await
}
