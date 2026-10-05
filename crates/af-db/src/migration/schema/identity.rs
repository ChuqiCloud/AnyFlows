use sea_orm::DbBackend;
use sea_orm_migration::prelude::*;

use crate::migration::iden::{groups, options, tokens, users};

use super::{auto_id, nullable_timestamp, table, timestamp};

pub(in crate::migration) async fn create_groups(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let mut statement = table(manager, groups::Entity);
    statement
        .col(auto_id(groups::Column::Id))
        .col(
            ColumnDef::new(groups::Column::Name)
                .string_len(64)
                .not_null(),
        )
        .col(
            ColumnDef::new(groups::Column::DisplayName)
                .string_len(128)
                .not_null(),
        )
        .col(
            ColumnDef::new(groups::Column::RatioMicros)
                .big_integer()
                .not_null()
                .default(1_000_000_i64)
                .check(Expr::col(groups::Column::RatioMicros).gte(0_i64)),
        )
        .col(
            ColumnDef::new(groups::Column::PeakRatioMicros)
                .big_integer()
                .check(Expr::col(groups::Column::PeakRatioMicros).gte(0_i64)),
        )
        .col(ColumnDef::new(groups::Column::PeakStart).time())
        .col(ColumnDef::new(groups::Column::PeakEnd).time())
        .col(
            ColumnDef::new(groups::Column::IsExclusive)
                .boolean()
                .not_null()
                .default(false),
        )
        .col(
            ColumnDef::new(groups::Column::DailyLimit)
                .big_integer()
                .check(Expr::col(groups::Column::DailyLimit).gte(0_i64)),
        )
        .col(
            ColumnDef::new(groups::Column::WeeklyLimit)
                .big_integer()
                .check(Expr::col(groups::Column::WeeklyLimit).gte(0_i64)),
        )
        .col(
            ColumnDef::new(groups::Column::MonthlyLimit)
                .big_integer()
                .check(Expr::col(groups::Column::MonthlyLimit).gte(0_i64)),
        )
        .col(
            ColumnDef::new(groups::Column::RpmLimit)
                .integer()
                .check(Expr::col(groups::Column::RpmLimit).gte(0_i32)),
        )
        .col(ColumnDef::new(groups::Column::FallbackGroupId).big_integer())
        .col(
            ColumnDef::new(groups::Column::Flags)
                .json_binary()
                .not_null(),
        )
        .col(timestamp(manager, groups::Column::CreatedAt))
        .col(timestamp(manager, groups::Column::UpdatedAt))
        .col(nullable_timestamp(manager, groups::Column::DeletedAt))
        .check(
            Expr::col(groups::Column::PeakRatioMicros)
                .is_null()
                .and(Expr::col(groups::Column::PeakStart).is_null())
                .and(Expr::col(groups::Column::PeakEnd).is_null())
                .or(Expr::col(groups::Column::PeakRatioMicros)
                    .is_not_null()
                    .and(Expr::col(groups::Column::PeakStart).is_not_null())
                    .and(Expr::col(groups::Column::PeakEnd).is_not_null())),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_groups_fallback_group")
                .from(groups::Entity, groups::Column::FallbackGroupId)
                .to(groups::Entity, groups::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::SetNull),
        );
    if manager.get_database_backend() != DbBackend::MySql {
        statement
            .check(Expr::col(groups::Column::FallbackGroupId).ne(Expr::col(groups::Column::Id)));
    }
    manager.create_table(statement).await
}

pub(in crate::migration) async fn create_users(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let mut statement = table(manager, users::Entity);
    statement
        .col(auto_id(users::Column::Id))
        .col(
            ColumnDef::new(users::Column::Username)
                .string_len(64)
                .not_null(),
        )
        .col(ColumnDef::new(users::Column::Email).string_len(320))
        .col(ColumnDef::new(users::Column::PasswordHash).string_len(255))
        .col(
            ColumnDef::new(users::Column::Role)
                .small_integer()
                .not_null()
                .default(0_i16)
                .check(Expr::col(users::Column::Role).is_in([0_i16, 1_i16])),
        )
        .col(
            ColumnDef::new(users::Column::Status)
                .small_integer()
                .not_null()
                .default(2_i16)
                .check(Expr::col(users::Column::Status).is_in([1_i16, 2_i16])),
        )
        .col(
            ColumnDef::new(users::Column::DefaultGroupId)
                .big_integer()
                .not_null(),
        )
        .col(big_integer_zero(users::Column::Quota))
        .col(big_integer_zero(users::Column::UsedQuota))
        .col(big_integer_zero(users::Column::FrozenQuota))
        .col(big_integer_zero(users::Column::RequestCount))
        .col(
            ColumnDef::new(users::Column::AffCode)
                .string_len(64)
                .not_null(),
        )
        .col(ColumnDef::new(users::Column::InviterId).big_integer())
        .col(big_integer_zero(users::Column::AffQuota))
        .col(big_integer_zero(users::Column::AffHistoryQuota))
        .col(ColumnDef::new(users::Column::TotpSecret).json_binary())
        .col(
            ColumnDef::new(users::Column::RpmLimit)
                .integer()
                .check(Expr::col(users::Column::RpmLimit).gte(0_i32)),
        )
        .col(
            ColumnDef::new(users::Column::Concurrency)
                .integer()
                .check(Expr::col(users::Column::Concurrency).gte(0_i32)),
        )
        .col(
            ColumnDef::new(users::Column::Settings)
                .json_binary()
                .not_null(),
        )
        .col(timestamp(manager, users::Column::CreatedAt))
        .col(timestamp(manager, users::Column::UpdatedAt))
        .col(nullable_timestamp(manager, users::Column::DeletedAt))
        .foreign_key(
            ForeignKey::create()
                .name("fk_users_default_group")
                .from(users::Entity, users::Column::DefaultGroupId)
                .to(groups::Entity, groups::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::Restrict),
        )
        .foreign_key(
            ForeignKey::create()
                .name("fk_users_inviter")
                .from(users::Entity, users::Column::InviterId)
                .to(users::Entity, users::Column::Id)
                .on_update(ForeignKeyAction::Cascade)
                .on_delete(ForeignKeyAction::SetNull),
        );
    if manager.get_database_backend() != DbBackend::MySql {
        statement.check(Expr::col(users::Column::InviterId).ne(Expr::col(users::Column::Id)));
    }
    manager.create_table(statement).await
}

pub(in crate::migration) async fn create_tokens(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            table(manager, tokens::Entity)
                .col(auto_id(tokens::Column::Id))
                .col(
                    ColumnDef::new(tokens::Column::UserId)
                        .big_integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(tokens::Column::KeyHash)
                        .char_len(64)
                        .not_null()
                        .check(
                            Expr::col(tokens::Column::KeyHash)
                                .eq(Func::lower(Expr::col(tokens::Column::KeyHash))),
                        ),
                )
                .col(
                    ColumnDef::new(tokens::Column::KeyPrefix)
                        .string_len(32)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(tokens::Column::Name)
                        .string_len(128)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(tokens::Column::Status)
                        .small_integer()
                        .not_null()
                        .default(2_i16)
                        .check(Expr::col(tokens::Column::Status).is_in([1_i16, 2_i16])),
                )
                .col(ColumnDef::new(tokens::Column::GroupId).big_integer())
                // 企业扩展通过可选上下文列关联令牌；公共核心不创建企业表或外键。
                .col(ColumnDef::new(tokens::Column::OrganizationId).big_integer())
                .col(
                    ColumnDef::new(tokens::Column::OrganizationMembershipId).big_integer(),
                )
                .col(ColumnDef::new(tokens::Column::OrganizationTeamId).big_integer())
                .col(ColumnDef::new(tokens::Column::OrganizationDepartmentId).big_integer())
                .col(big_integer_zero(tokens::Column::RemainQuota))
                .col(
                    ColumnDef::new(tokens::Column::UnlimitedQuota)
                        .boolean()
                        .not_null()
                        .default(false),
                )
                .col(big_integer_zero(tokens::Column::UsedQuota))
                .col(nullable_timestamp(manager, tokens::Column::ExpiredAt))
                .col(ColumnDef::new(tokens::Column::ModelLimits).json_binary())
                .col(ColumnDef::new(tokens::Column::AllowIps).json_binary())
                .col(
                    ColumnDef::new(tokens::Column::CrossGroupRetry)
                        .boolean()
                        .not_null()
                        .default(false),
                )
                .col(
                    ColumnDef::new(tokens::Column::RateLimit5h)
                        .big_integer()
                        .check(Expr::col(tokens::Column::RateLimit5h).gte(0_i64)),
                )
                .col(
                    ColumnDef::new(tokens::Column::RateLimit1d)
                        .big_integer()
                        .check(Expr::col(tokens::Column::RateLimit1d).gte(0_i64)),
                )
                .col(
                    ColumnDef::new(tokens::Column::RateLimit7d)
                        .big_integer()
                        .check(Expr::col(tokens::Column::RateLimit7d).gte(0_i64)),
                )
                .col(big_integer_zero(tokens::Column::Usage5h))
                .col(big_integer_zero(tokens::Column::Usage1d))
                .col(big_integer_zero(tokens::Column::Usage7d))
                .col(timestamp(manager, tokens::Column::Window5hStart))
                .col(timestamp(manager, tokens::Column::Window1dStart))
                .col(timestamp(manager, tokens::Column::Window7dStart))
                .col(
                    ColumnDef::new(tokens::Column::MaxRequests)
                        .big_integer()
                        .check(Expr::col(tokens::Column::MaxRequests).gte(0_i64)),
                )
                .col(big_integer_zero(tokens::Column::UsedRequests))
                .col(timestamp(manager, tokens::Column::CreatedAt))
                .col(timestamp(manager, tokens::Column::UpdatedAt))
                .col(nullable_timestamp(manager, tokens::Column::DeletedAt))
                .foreign_key(
                    ForeignKey::create()
                        .name("fk_tokens_user")
                        .from(tokens::Entity, tokens::Column::UserId)
                        .to(users::Entity, users::Column::Id)
                        .on_update(ForeignKeyAction::Cascade)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .foreign_key(
                    ForeignKey::create()
                        .name("fk_tokens_group")
                        .from(tokens::Entity, tokens::Column::GroupId)
                        .to(groups::Entity, groups::Column::Id)
                        .on_update(ForeignKeyAction::Cascade)
                        .on_delete(ForeignKeyAction::Restrict),
                )
                .to_owned(),
        )
        .await
}

pub(in crate::migration) async fn create_options(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            table(manager, options::Entity)
                .col(
                    ColumnDef::new(options::Column::Key)
                        .string_len(255)
                        .not_null()
                        .primary_key(),
                )
                .col(ColumnDef::new(options::Column::Value).text().not_null())
                .col(timestamp(manager, options::Column::CreatedAt))
                .col(timestamp(manager, options::Column::UpdatedAt))
                .to_owned(),
        )
        .await
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
