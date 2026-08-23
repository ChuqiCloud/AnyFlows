use sea_orm_migration::prelude::*;

use super::iden::{debug_trace_attempts, debug_trace_settings, debug_traces};

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        add_settings_boolean(manager, debug_trace_settings::Column::CaptureHeaders).await?;
        add_settings_boolean(manager, debug_trace_settings::Column::CaptureBodies).await?;
        add_settings_body_limit(manager).await?;
        for column in [
            debug_traces::Column::DownstreamMethod,
            debug_traces::Column::DownstreamPath,
            debug_traces::Column::DownstreamHeadersJson,
            debug_traces::Column::DownstreamBodyJson,
        ] {
            add_text(manager, debug_traces::Entity, column).await?;
        }
        for column in [
            debug_trace_attempts::Column::RequestMethod,
            debug_trace_attempts::Column::RequestUrl,
            debug_trace_attempts::Column::RequestHeadersJson,
            debug_trace_attempts::Column::RequestBodyJson,
            debug_trace_attempts::Column::ResponseHeadersJson,
            debug_trace_attempts::Column::ResponseBodyJson,
        ] {
            add_text(manager, debug_trace_attempts::Entity, column).await?;
        }
        add_response_status(manager).await?;
        add_response_streamed(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for column in [
            debug_trace_attempts::Column::ResponseStreamed,
            debug_trace_attempts::Column::ResponseStatus,
            debug_trace_attempts::Column::ResponseBodyJson,
            debug_trace_attempts::Column::ResponseHeadersJson,
            debug_trace_attempts::Column::RequestBodyJson,
            debug_trace_attempts::Column::RequestHeadersJson,
            debug_trace_attempts::Column::RequestUrl,
            debug_trace_attempts::Column::RequestMethod,
        ] {
            drop_column(manager, debug_trace_attempts::Entity, column).await?;
        }
        for column in [
            debug_traces::Column::DownstreamBodyJson,
            debug_traces::Column::DownstreamHeadersJson,
            debug_traces::Column::DownstreamPath,
            debug_traces::Column::DownstreamMethod,
        ] {
            drop_column(manager, debug_traces::Entity, column).await?;
        }
        for column in [
            debug_trace_settings::Column::MaxBodyBytes,
            debug_trace_settings::Column::CaptureBodies,
            debug_trace_settings::Column::CaptureHeaders,
        ] {
            drop_column(manager, debug_trace_settings::Entity, column).await?;
        }
        Ok(())
    }
}

async fn add_settings_boolean(
    manager: &SchemaManager<'_>,
    column: debug_trace_settings::Column,
) -> Result<(), DbErr> {
    manager
        .alter_table(
            Table::alter()
                .table(debug_trace_settings::Entity)
                .add_column(ColumnDef::new(column).boolean().not_null().default(false))
                .to_owned(),
        )
        .await
}

async fn add_settings_body_limit(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .alter_table(
            Table::alter()
                .table(debug_trace_settings::Entity)
                .add_column(
                    ColumnDef::new(debug_trace_settings::Column::MaxBodyBytes)
                        .integer()
                        .not_null()
                        .default(16_384_i32)
                        .check(
                            Expr::col(debug_trace_settings::Column::MaxBodyBytes)
                                .between(1_024_i32, 65_536_i32),
                        ),
                )
                .to_owned(),
        )
        .await
}

async fn add_response_status(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .alter_table(
            Table::alter()
                .table(debug_trace_attempts::Entity)
                .add_column(
                    ColumnDef::new(debug_trace_attempts::Column::ResponseStatus)
                        .small_integer()
                        .check(
                            Expr::col(debug_trace_attempts::Column::ResponseStatus)
                                .is_null()
                                .or(Expr::col(debug_trace_attempts::Column::ResponseStatus)
                                    .between(100_i16, 599_i16)),
                        ),
                )
                .to_owned(),
        )
        .await
}

async fn add_response_streamed(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .alter_table(
            Table::alter()
                .table(debug_trace_attempts::Entity)
                .add_column(
                    ColumnDef::new(debug_trace_attempts::Column::ResponseStreamed)
                        .boolean()
                        .not_null()
                        .default(false),
                )
                .to_owned(),
        )
        .await
}

async fn add_text<T, C>(manager: &SchemaManager<'_>, table: T, column: C) -> Result<(), DbErr>
where
    T: IntoIden + Clone + 'static,
    C: IntoIden + Clone + 'static,
{
    manager
        .alter_table(
            Table::alter()
                .table(table)
                .add_column(ColumnDef::new(column).text())
                .to_owned(),
        )
        .await
}

async fn drop_column<T, C>(manager: &SchemaManager<'_>, table: T, column: C) -> Result<(), DbErr>
where
    T: IntoIden + Clone + 'static,
    C: IntoIden + Clone + 'static,
{
    manager
        .alter_table(Table::alter().table(table).drop_column(column).to_owned())
        .await
}
