use sea_orm_migration::prelude::*;

use crate::migration::iden::{channels, credentials, route_channels, routes};

use super::{auto_id, nullable_timestamp, table, timestamp};

const ROUTE_PATTERN_INDEX: &str = "idx_routes_model_pattern";
const ROUTE_ENABLED_INDEX: &str = "idx_routes_enabled";
const ROUTE_CHANNEL_INDEX: &str = "idx_route_channels_route_enabled";
const ROUTE_CHANNEL_UNIQUE_INDEX: &str = "uq_route_channels_route_channel_credential";

/// 创建智能路由规则表。
pub(in crate::migration) async fn create_routes(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            table(manager, routes::Entity)
                .col(auto_id(routes::Column::Id))
                .col(
                    ColumnDef::new(routes::Column::Name)
                        .string_len(128)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(routes::Column::ModelPattern)
                        .string_len(255)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(routes::Column::RouteMode)
                        .small_integer()
                        .not_null()
                        .default(1_i16)
                        .check(Expr::col(routes::Column::RouteMode).is_in([1_i16, 2_i16])),
                )
                .col(
                    ColumnDef::new(routes::Column::Strategy)
                        .small_integer()
                        .not_null()
                        .default(1_i16)
                        .check(Expr::col(routes::Column::Strategy).is_in([1_i16, 2_i16, 3_i16])),
                )
                .col(
                    ColumnDef::new(routes::Column::ModelMapping)
                        .json_binary()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(routes::Column::Enabled)
                        .boolean()
                        .not_null()
                        .default(true),
                )
                .col(timestamp(manager, routes::Column::CreatedAt))
                .col(timestamp(manager, routes::Column::UpdatedAt))
                .col(nullable_timestamp(manager, routes::Column::DeletedAt))
                .to_owned(),
        )
        .await?;
    manager
        .create_index(
            Index::create()
                .name(ROUTE_PATTERN_INDEX)
                .table(routes::Entity)
                .col(routes::Column::ModelPattern)
                .to_owned(),
        )
        .await?;
    manager
        .create_index(
            Index::create()
                .name(ROUTE_ENABLED_INDEX)
                .table(routes::Entity)
                .col(routes::Column::Enabled)
                .col(routes::Column::Id)
                .to_owned(),
        )
        .await
}

/// 创建路由候选及其可观测运行时统计表。
pub(in crate::migration) async fn create_route_channels(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    manager
        .create_table(
            table(manager, route_channels::Entity)
                .col(auto_id(route_channels::Column::Id))
                .col(
                    ColumnDef::new(route_channels::Column::RouteId)
                        .big_integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(route_channels::Column::ChannelId)
                        .big_integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(route_channels::Column::CredentialId)
                        .big_integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(route_channels::Column::Priority)
                        .integer()
                        .not_null()
                        .default(0_i32)
                        .check(Expr::col(route_channels::Column::Priority).gte(0_i32)),
                )
                .col(
                    ColumnDef::new(route_channels::Column::Weight)
                        .integer()
                        .not_null()
                        .default(10_i32)
                        .check(Expr::col(route_channels::Column::Weight).gte(0_i32)),
                )
                .col(
                    ColumnDef::new(route_channels::Column::Enabled)
                        .boolean()
                        .not_null()
                        .default(true),
                )
                .col(non_negative_big_integer(route_channels::Column::SuccessCount))
                .col(non_negative_big_integer(route_channels::Column::FailCount))
                .col(non_negative_big_integer(route_channels::Column::TotalLatency))
                .col(
                    ColumnDef::new(route_channels::Column::CooldownLevel)
                        .small_integer()
                        .not_null()
                        .default(0_i16)
                        .check(
                            Condition::all()
                                .add(
                                    Expr::col(route_channels::Column::CooldownLevel)
                                        .gte(0_i16),
                                )
                                .add(
                                    Expr::col(route_channels::Column::CooldownLevel)
                                        .lte(3_i16),
                                ),
                        ),
                )
                .col(nullable_timestamp(manager, route_channels::Column::CooldownUntil))
                .col(nullable_timestamp(manager, route_channels::Column::LastSelectedAt))
                .col(nullable_timestamp(manager, route_channels::Column::LastFailureAt))
                .col(timestamp(manager, route_channels::Column::CreatedAt))
                .col(timestamp(manager, route_channels::Column::UpdatedAt))
                .foreign_key(
                    ForeignKey::create()
                        .name("fk_route_channels_route")
                        .from(route_channels::Entity, route_channels::Column::RouteId)
                        .to(routes::Entity, routes::Column::Id)
                        .on_update(ForeignKeyAction::Cascade)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .foreign_key(
                    ForeignKey::create()
                        .name("fk_route_channels_channel")
                        .from(route_channels::Entity, route_channels::Column::ChannelId)
                        .to(channels::Entity, channels::Column::Id)
                        .on_update(ForeignKeyAction::Cascade)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                // 复合外键同时校验凭据归属渠道，避免管理端拼接跨渠道候选。
                .foreign_key(
                    ForeignKey::create()
                        .name("fk_route_channels_credential_channel")
                        .from(
                            route_channels::Entity,
                            (
                                route_channels::Column::CredentialId,
                                route_channels::Column::ChannelId,
                            ),
                        )
                        .to(
                            credentials::Entity,
                            (credentials::Column::Id, credentials::Column::ChannelId),
                        )
                        .on_update(ForeignKeyAction::Cascade)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .to_owned(),
        )
        .await?;
    manager
        .create_index(
            Index::create()
                .name(ROUTE_CHANNEL_INDEX)
                .table(route_channels::Entity)
                .col(route_channels::Column::RouteId)
                .col(route_channels::Column::Enabled)
                .col(route_channels::Column::Priority)
                .col(route_channels::Column::Id)
                .to_owned(),
        )
        .await?;
    manager
        .create_index(
            Index::create()
                .name(ROUTE_CHANNEL_UNIQUE_INDEX)
                .table(route_channels::Entity)
                .col(route_channels::Column::RouteId)
                .col(route_channels::Column::ChannelId)
                .col(route_channels::Column::CredentialId)
                .unique()
                .to_owned(),
        )
        .await
}

fn non_negative_big_integer<T>(column: T) -> ColumnDef
where
    T: IntoIden + IntoColumnRef + Clone,
{
    let mut definition = ColumnDef::new(column.clone());
    definition
        .big_integer()
        .not_null()
        .default(0_i64)
        .check(Expr::col(column).gte(0_i64));
    definition
}
