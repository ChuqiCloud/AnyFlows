use sea_orm::{ConnectionTrait, TryGetable};
use sea_orm_migration::prelude::*;
use serde::{Deserialize, Serialize};

use super::{
    iden::{authentication_settings, options, site_settings},
    schema,
};

const AUTHENTICATION_SETTINGS_ID: i16 = 1;
const LEGACY_REGISTRATION_POLICY_KEY: &str = "system.registration.policy.v1";
const LEGACY_REGISTRATION_POLICY_VERSION: u8 = 1;

#[derive(DeriveMigrationName)]
pub(super) struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        schema::create_site_settings(manager).await?;
        schema::create_authentication_settings(manager).await?;
        migrate_legacy_registration_policy(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        restore_legacy_registration_policy(manager).await?;
        manager
            .drop_table(
                Table::drop()
                    .table(authentication_settings::Entity)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(Table::drop().table(site_settings::Entity).to_owned())
            .await
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct LegacyRegistrationPolicy {
    version: u8,
    enabled: bool,
    default_group_id: i64,
    initial_quota: i64,
    email_required: bool,
    rate_limit_attempts: u32,
    rate_limit_window_seconds: u64,
}

/// 把旧 Option JSON 一次性搬入强类型认证记录，成功后删除旧键避免双真相。
async fn migrate_legacy_registration_policy(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let Some(value) = read_legacy_registration_policy(manager).await? else {
        return Ok(());
    };
    let policy = parse_legacy_registration_policy(&value)?;
    let rate_limit_attempts =
        <i32 as std::convert::TryFrom<u32>>::try_from(policy.rate_limit_attempts)
            .map_err(|_| invalid_legacy_policy())?;
    let rate_limit_window_seconds =
        <i64 as std::convert::TryFrom<u64>>::try_from(policy.rate_limit_window_seconds)
            .map_err(|_| invalid_legacy_policy())?;
    manager
        .exec_stmt(
            Query::update()
                .table(authentication_settings::Entity)
                .value(authentication_settings::Column::PasswordLoginEnabled, true)
                .value(
                    authentication_settings::Column::RegistrationEnabled,
                    policy.enabled,
                )
                .value(
                    authentication_settings::Column::RegistrationDefaultGroupId,
                    policy.default_group_id,
                )
                .value(
                    authentication_settings::Column::RegistrationInitialQuota,
                    policy.initial_quota,
                )
                .value(
                    authentication_settings::Column::RegistrationEmailRequired,
                    policy.email_required,
                )
                .value(
                    authentication_settings::Column::RegistrationRateLimitAttempts,
                    rate_limit_attempts,
                )
                .value(
                    authentication_settings::Column::RegistrationRateLimitWindowSeconds,
                    rate_limit_window_seconds,
                )
                .and_where(
                    Expr::col(authentication_settings::Column::Id).eq(AUTHENTICATION_SETTINGS_ID),
                )
                .to_owned(),
        )
        .await?;
    manager
        .exec_stmt(
            Query::delete()
                .from_table(options::Entity)
                .and_where(Expr::col(options::Column::Key).eq(LEGACY_REGISTRATION_POLICY_KEY))
                .to_owned(),
        )
        .await
}

/// 回滚到旧版本前重建 Option JSON，使旧二进制仍能读取注册策略。
async fn restore_legacy_registration_policy(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let select = Query::select()
        .columns([
            authentication_settings::Column::RegistrationEnabled,
            authentication_settings::Column::RegistrationDefaultGroupId,
            authentication_settings::Column::RegistrationInitialQuota,
            authentication_settings::Column::RegistrationEmailRequired,
            authentication_settings::Column::RegistrationRateLimitAttempts,
            authentication_settings::Column::RegistrationRateLimitWindowSeconds,
        ])
        .from(authentication_settings::Entity)
        .and_where(Expr::col(authentication_settings::Column::Id).eq(AUTHENTICATION_SETTINGS_ID))
        .to_owned();
    let row = manager
        .get_connection()
        .query_one(manager.get_database_backend().build(&select))
        .await?
        .ok_or_else(|| DbErr::Custom("认证设置固定记录缺失".to_owned()))?;
    let default_group_id = try_get::<Option<i64>>(&row, "registration_default_group_id")?;
    let Some(default_group_id) = default_group_id else {
        manager
            .exec_stmt(
                Query::delete()
                    .from_table(options::Entity)
                    .and_where(Expr::col(options::Column::Key).eq(LEGACY_REGISTRATION_POLICY_KEY))
                    .to_owned(),
            )
            .await?;
        return Ok(());
    };
    let attempts = try_get::<i32>(&row, "registration_rate_limit_attempts")?;
    let window = try_get::<i64>(&row, "registration_rate_limit_window_seconds")?;
    let policy = LegacyRegistrationPolicy {
        version: LEGACY_REGISTRATION_POLICY_VERSION,
        enabled: try_get(&row, "registration_enabled")?,
        default_group_id,
        initial_quota: try_get(&row, "registration_initial_quota")?,
        email_required: try_get(&row, "registration_email_required")?,
        rate_limit_attempts: <u32 as std::convert::TryFrom<i32>>::try_from(attempts)
            .map_err(|_| DbErr::Custom("认证设置限流次数无效".to_owned()))?,
        rate_limit_window_seconds: <u64 as std::convert::TryFrom<i64>>::try_from(window)
            .map_err(|_| DbErr::Custom("认证设置限流窗口无效".to_owned()))?,
    };
    let value = serde_json::to_string(&policy)
        .map_err(|_| DbErr::Custom("无法序列化旧注册策略".to_owned()))?;
    manager
        .exec_stmt(
            Query::insert()
                .into_table(options::Entity)
                .columns([options::Column::Key, options::Column::Value])
                .values_panic([LEGACY_REGISTRATION_POLICY_KEY.into(), value.into()])
                .on_conflict(
                    OnConflict::column(options::Column::Key)
                        .update_column(options::Column::Value)
                        .to_owned(),
                )
                .to_owned(),
        )
        .await
}

async fn read_legacy_registration_policy(
    manager: &SchemaManager<'_>,
) -> Result<Option<String>, DbErr> {
    let select = Query::select()
        .column(options::Column::Value)
        .from(options::Entity)
        .and_where(Expr::col(options::Column::Key).eq(LEGACY_REGISTRATION_POLICY_KEY))
        .to_owned();
    manager
        .get_connection()
        .query_one(manager.get_database_backend().build(&select))
        .await?
        .map(|row| try_get(&row, "value"))
        .transpose()
}

fn parse_legacy_registration_policy(value: &str) -> Result<LegacyRegistrationPolicy, DbErr> {
    let policy = serde_json::from_str::<LegacyRegistrationPolicy>(value)
        .map_err(|_| invalid_legacy_policy())?;
    if policy.version != LEGACY_REGISTRATION_POLICY_VERSION
        || policy.default_group_id <= 0
        || policy.initial_quota < 0
        || !(1..=100).contains(&policy.rate_limit_attempts)
        || !(60..=86_400).contains(&policy.rate_limit_window_seconds)
    {
        return Err(invalid_legacy_policy());
    }
    Ok(policy)
}

fn try_get<T>(row: &sea_orm::QueryResult, column: &str) -> Result<T, DbErr>
where
    T: TryGetable,
{
    row.try_get("", column)
        .map_err(|_| DbErr::Custom("站点与认证设置迁移读取失败".to_owned()))
}

fn invalid_legacy_policy() -> DbErr {
    DbErr::Custom("旧注册策略状态无效，拒绝迁移".to_owned())
}
