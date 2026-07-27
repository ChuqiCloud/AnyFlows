use sea_orm::DbBackend;
use sea_orm_migration::prelude::*;

use crate::migration::iden::{
    abilities, channel_groups, channel_models, channels, credentials, groups,
};

use super::{auto_id, nullable_timestamp, table, timestamp};

pub(in crate::migration) async fn create_channels(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    manager
        .create_table(
            table(manager, channels::Entity)
                .col(auto_id(channels::Column::Id))
                .col(
                    ColumnDef::new(channels::Column::Name)
                        .string_len(128)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(channels::Column::Type)
                        .string_len(64)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(channels::Column::Protocol)
                        .string_len(64)
                        .not_null(),
                )
                .col(ColumnDef::new(channels::Column::BaseUrl).text())
                .col(
                    ColumnDef::new(channels::Column::Status)
                        .small_integer()
                        .not_null()
                        .default(2_i16)
                        .check(Expr::col(channels::Column::Status).is_in([1_i16, 2_i16, 3_i16])),
                )
                .col(non_negative_integer_zero(channels::Column::Weight))
                .col(integer_zero(channels::Column::Priority))
                .col(
                    ColumnDef::new(channels::Column::AutoBan)
                        .boolean()
                        .not_null()
                        .default(true),
                )
                .col(
                    ColumnDef::new(channels::Column::ModelMapping)
                        .json_binary()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(channels::Column::ParamOverride)
                        .json_binary()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(channels::Column::HeaderOverride)
                        .json_binary()
                        .not_null(),
                )
                .col(ColumnDef::new(channels::Column::Balance).big_integer())
                .col(big_integer_zero(channels::Column::UsedQuota))
                .col(
                    ColumnDef::new(channels::Column::Settings)
                        .json_binary()
                        .not_null(),
                )
                .col(ColumnDef::new(channels::Column::Tag).string_len(64))
                .col(timestamp(manager, channels::Column::CreatedAt))
                .col(timestamp(manager, channels::Column::UpdatedAt))
                .col(nullable_timestamp(manager, channels::Column::DeletedAt))
                .to_owned(),
        )
        .await
}

pub(in crate::migration) async fn create_credentials(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    let mut statement = table(manager, credentials::Entity);
    statement
                .col(auto_id(credentials::Column::Id))
                .col(
                    ColumnDef::new(credentials::Column::ChannelId)
                        .big_integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(credentials::Column::Kind)
                        .string_len(64)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(credentials::Column::Secret)
                        .json_binary()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(credentials::Column::Status)
                        .small_integer()
                        .not_null()
                        .default(2_i16)
                        .check(
                            Expr::col(credentials::Column::Status).is_in([1_i16, 2_i16, 3_i16]),
                        ),
                )
                .col(ColumnDef::new(credentials::Column::MultiKeyMode).small_integer())
                .col(integer_zero(credentials::Column::Priority))
                .col(non_negative_integer_zero(credentials::Column::Weight))
                .col(
                    ColumnDef::new(credentials::Column::Concurrency)
                        .integer()
                        .check(Expr::col(credentials::Column::Concurrency).gte(0_i32)),
                )
                .col(
                    ColumnDef::new(credentials::Column::LoadFactorMicros)
                        .big_integer()
                        .check(Expr::col(credentials::Column::LoadFactorMicros).gte(0_i64)),
                )
                .col(
                    ColumnDef::new(credentials::Column::RateMultiplierMicros)
                        .big_integer()
                        .check(Expr::col(credentials::Column::RateMultiplierMicros).gte(0_i64)),
                )
                .col(
                    ColumnDef::new(credentials::Column::Schedulable)
                        .boolean()
                        .not_null()
                        .default(false),
                )
                .col(nullable_timestamp(
                    manager,
                    credentials::Column::RateLimitedAt,
                ))
                .col(nullable_timestamp(
                    manager,
                    credentials::Column::RateLimitResetAt,
                ))
                .col(nullable_timestamp(
                    manager,
                    credentials::Column::OverloadUntil,
                ))
                .col(nullable_timestamp(
                    manager,
                    credentials::Column::TempUnschedulableUntil,
                ))
                .col(
                    ColumnDef::new(credentials::Column::TempUnschedulableReason)
                        .string_len(255),
                )
                .col(nullable_timestamp(
                    manager,
                    credentials::Column::SessionWindowStart,
                ))
                .col(nullable_timestamp(
                    manager,
                    credentials::Column::SessionWindowEnd,
                ))
                .col(ColumnDef::new(credentials::Column::ParentId).big_integer())
                .col(
                    ColumnDef::new(credentials::Column::QuotaDimension)
                        .string_len(32)
                        .not_null()
                        .default("global")
                        .check(
                            Expr::col(credentials::Column::QuotaDimension)
                                .is_in(["global", "spark"]),
                        ),
                )
                // 代理表属于后续切片，当前先保留字段，避免创建悬空外键。
                .col(ColumnDef::new(credentials::Column::ProxyId).big_integer())
                .col(
                    ColumnDef::new(credentials::Column::OauthProvider).string_len(64),
                )
                .col(
                    ColumnDef::new(credentials::Column::OauthAccountKey).string_len(255),
                )
                .col(
                    ColumnDef::new(credentials::Column::OauthProjectId).string_len(255),
                )
                .col(nullable_timestamp(
                    manager,
                    credentials::Column::LastUsedAt,
                ))
                .col(timestamp(manager, credentials::Column::CreatedAt))
                .col(timestamp(manager, credentials::Column::UpdatedAt))
                .col(nullable_timestamp(
                    manager,
                    credentials::Column::DeletedAt,
                ))
                .index(
                    Index::create()
                        .name("uq_credentials_id_channel")
                        .col(credentials::Column::Id)
                        .col(credentials::Column::ChannelId)
                        .unique(),
                )
                .check(
                    Expr::col(credentials::Column::SessionWindowStart)
                        .is_null()
                        .and(Expr::col(credentials::Column::SessionWindowEnd).is_null())
                        .or(
                            Expr::col(credentials::Column::SessionWindowStart)
                                .is_not_null()
                                .and(
                                    Expr::col(credentials::Column::SessionWindowEnd).is_not_null(),
                                ),
                        ),
                )
                .foreign_key(
                    ForeignKey::create()
                        .name("fk_credentials_channel")
                        .from(credentials::Entity, credentials::Column::ChannelId)
                        .to(channels::Entity, channels::Column::Id)
                        .on_update(ForeignKeyAction::Cascade)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .foreign_key(
                    ForeignKey::create()
                        .name("fk_credentials_parent")
                        .from(
                            credentials::Entity,
                            (
                                credentials::Column::ParentId,
                                credentials::Column::ChannelId,
                            ),
                        )
                        .to(
                            credentials::Entity,
                            (credentials::Column::Id, credentials::Column::ChannelId),
                        )
                        .on_update(ForeignKeyAction::Cascade)
                        .on_delete(ForeignKeyAction::Cascade),
                );
    if manager.get_database_backend() != DbBackend::MySql {
        statement
            .check(Expr::col(credentials::Column::ParentId).ne(Expr::col(credentials::Column::Id)));
    }
    manager.create_table(statement).await
}

pub(in crate::migration) async fn create_channel_models(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    manager
        .create_table(
            table(manager, channel_models::Entity)
                .col(
                    ColumnDef::new(channel_models::Column::ChannelId)
                        .big_integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(channel_models::Column::Model)
                        .string_len(255)
                        .not_null(),
                )
                .col(timestamp(manager, channel_models::Column::CreatedAt))
                .col(timestamp(manager, channel_models::Column::UpdatedAt))
                .primary_key(
                    Index::create()
                        .name("pk_channel_models")
                        .col(channel_models::Column::ChannelId)
                        .col(channel_models::Column::Model),
                )
                .foreign_key(
                    ForeignKey::create()
                        .name("fk_channel_models_channel")
                        .from(channel_models::Entity, channel_models::Column::ChannelId)
                        .to(channels::Entity, channels::Column::Id)
                        .on_update(ForeignKeyAction::Cascade)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .to_owned(),
        )
        .await
}

pub(in crate::migration) async fn create_channel_groups(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    manager
        .create_table(
            table(manager, channel_groups::Entity)
                .col(
                    ColumnDef::new(channel_groups::Column::ChannelId)
                        .big_integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(channel_groups::Column::GroupId)
                        .big_integer()
                        .not_null(),
                )
                .col(timestamp(manager, channel_groups::Column::CreatedAt))
                .col(timestamp(manager, channel_groups::Column::UpdatedAt))
                .primary_key(
                    Index::create()
                        .name("pk_channel_groups")
                        .col(channel_groups::Column::ChannelId)
                        .col(channel_groups::Column::GroupId),
                )
                .foreign_key(
                    ForeignKey::create()
                        .name("fk_channel_groups_channel")
                        .from(channel_groups::Entity, channel_groups::Column::ChannelId)
                        .to(channels::Entity, channels::Column::Id)
                        .on_update(ForeignKeyAction::Cascade)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .foreign_key(
                    ForeignKey::create()
                        .name("fk_channel_groups_group")
                        .from(channel_groups::Entity, channel_groups::Column::GroupId)
                        .to(groups::Entity, groups::Column::Id)
                        .on_update(ForeignKeyAction::Cascade)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .to_owned(),
        )
        .await
}

pub(in crate::migration) async fn create_abilities(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    manager
        .create_table(
            table(manager, abilities::Entity)
                .col(
                    ColumnDef::new(abilities::Column::GroupId)
                        .big_integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(abilities::Column::Model)
                        .string_len(255)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(abilities::Column::ChannelId)
                        .big_integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(abilities::Column::Enabled)
                        .boolean()
                        .not_null()
                        .default(false),
                )
                .col(integer_zero(abilities::Column::Priority))
                .col(non_negative_integer_zero(abilities::Column::Weight))
                .col(ColumnDef::new(abilities::Column::Tag).string_len(64))
                .col(timestamp(manager, abilities::Column::CreatedAt))
                .col(timestamp(manager, abilities::Column::UpdatedAt))
                .primary_key(
                    Index::create()
                        .name("pk_abilities")
                        .col(abilities::Column::GroupId)
                        .col(abilities::Column::Model)
                        .col(abilities::Column::ChannelId),
                )
                .foreign_key(
                    ForeignKey::create()
                        .name("fk_abilities_channel_group")
                        .from(
                            abilities::Entity,
                            (abilities::Column::ChannelId, abilities::Column::GroupId),
                        )
                        .to(
                            channel_groups::Entity,
                            (
                                channel_groups::Column::ChannelId,
                                channel_groups::Column::GroupId,
                            ),
                        )
                        .on_update(ForeignKeyAction::Cascade)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .foreign_key(
                    ForeignKey::create()
                        .name("fk_abilities_channel_model")
                        .from(
                            abilities::Entity,
                            (abilities::Column::ChannelId, abilities::Column::Model),
                        )
                        .to(
                            channel_models::Entity,
                            (
                                channel_models::Column::ChannelId,
                                channel_models::Column::Model,
                            ),
                        )
                        .on_update(ForeignKeyAction::Cascade)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .to_owned(),
        )
        .await
}

fn integer_zero<T>(column: T) -> ColumnDef
where
    T: IntoIden,
{
    let mut definition = ColumnDef::new(column);
    definition.integer().not_null().default(0_i32);
    definition
}

fn non_negative_integer_zero<T>(column: T) -> ColumnDef
where
    T: IntoIden + IntoColumnRef + Clone,
{
    let mut definition = integer_zero(column.clone());
    definition.check(Expr::col(column).gte(0_i32));
    definition
}

fn big_integer_zero<T>(column: T) -> ColumnDef
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
