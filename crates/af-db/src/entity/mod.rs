//! SeaORM 持久化实体，仅供 af-db 内部仓储与迁移使用。
//!
//! `deleted_at` 只表示持久化墓碑。仓储层执行软删除时必须在同一事务内撤销依赖的
//! 令牌、凭据与能力索引；所有有效性查询也必须联合检查所属用户、分组和渠道。

// 仓储层尚未落地前，部分实体仅用于声明稳定表结构；保持模块私有并局部关闭死代码告警。
#![allow(dead_code)]
#![allow(unused_imports)]

use rust_decimal::Decimal;
use sea_orm::{ActiveValue, EntityTrait, Set, entity::prelude::*};

use af_domain::{
    BillingReservationId, ChannelTimeout, MAX_ROUTE_MODEL_PATTERN_BYTES, Quota, QuotaDelta,
    RedemptionBatchId, RedemptionBatchStatus, RedemptionCodeId, RedemptionCodeStatus, RouteMode,
    RouteStrategy, SubscriptionCycle, SubscriptionPlanId, SubscriptionPlanStatus,
    SubscriptionWindow, UserSubscriptionId, UserSubscriptionStatus, WalletEventId,
    validate_route_model_pattern,
};

pub(crate) mod abilities;
pub(crate) mod analytics_export_outbox_events;
pub(crate) mod announcements;
pub(crate) mod async_task_billings;
pub(crate) mod async_task_submission_claims;
pub(crate) mod async_tasks;
pub(crate) mod auth_challenge_rate_limits;
pub(crate) mod auth_challenges;
pub(crate) mod authentication_settings;
pub(crate) mod balance_alert_events;
pub(crate) mod balance_alert_settings;
pub(crate) mod billing_batch_checkpoints;
pub(crate) mod billing_group_window_reservations;
pub(crate) mod billing_reservations;
pub(crate) mod billing_subscription_reservations;
pub(crate) mod billing_token_window_reservations;
pub(crate) mod channel_groups;
pub(crate) mod channel_models;
pub(crate) mod channels;
pub(crate) mod credentials;
pub(crate) mod custom_oauth2_identities;
pub(crate) mod custom_oauth2_login_transactions;
pub(crate) mod custom_oauth2_providers;
pub(crate) mod debug_trace_attempts;
pub(crate) mod debug_trace_settings;
pub(crate) mod debug_trace_snapshot_access_audits;
pub(crate) mod debug_trace_snapshots;
pub(crate) mod debug_traces;
pub(crate) mod email_settings;
pub(crate) mod group_model_ratios;
pub(crate) mod groups;
pub(crate) mod invite_rebate_events;
pub(crate) mod model_prices;
pub(crate) mod model_provider_catalog;
pub(crate) mod model_sync_items;
pub(crate) mod model_sync_runs;
pub(crate) mod models;
pub(crate) mod network_settings;
pub(crate) mod oauth_login_providers;
pub(crate) mod oauth_login_transactions;
pub(crate) mod options;
pub(crate) mod passkey_authentication_challenges;
pub(crate) mod passkey_registration_challenges;
pub(crate) mod passkeys;
pub(crate) mod payment_settings;
pub(crate) mod platform_audit_logs;
pub(crate) mod playground_conversations;
pub(crate) mod playground_shares;
pub(crate) mod proxies;
pub(crate) mod redemption_batches;
pub(crate) mod redemption_codes;
pub(crate) mod refund_manual_completions;
pub(crate) mod refund_provider_events;
pub(crate) mod refund_reconciliation_entries;
pub(crate) mod refund_requests;
pub(crate) mod registration_rate_limits;
pub(crate) mod request_outcome_logs;
pub(crate) mod route_channels;
pub(crate) mod routes;
pub(crate) mod scheduler_outbox_events;
pub(crate) mod site_settings;
pub(crate) mod subscription_balance_alert_events;
pub(crate) mod subscription_orders;
pub(crate) mod subscription_payment_events;
pub(crate) mod subscription_plan_prices;
pub(crate) mod subscription_plans;
pub(crate) mod tokens;
pub(crate) mod topup_orders;
pub(crate) mod topup_payment_events;
pub(crate) mod usage_logs;
pub(crate) mod user_notification_events;
pub(crate) mod user_notification_receipts;
pub(crate) mod user_oauth_identities;
pub(crate) mod user_subscriptions;
pub(crate) mod users;
pub(crate) mod wallet_ledger_entries;

mod sensitive;

pub(crate) use sensitive::{
    AuthChallengeHash, BillingBatchFingerprint, BillingBatchWriterKey, BillingReservationKey,
    ChannelBaseUrl, EncryptedJson, HeaderOverrides, MAX_TOKEN_IP_ALLOWLIST_SERIALIZED_BYTES,
    MAX_TOKEN_MODEL_ALLOWLIST_SERIALIZED_BYTES, PasswordHash, PlaygroundConversationKey,
    SensitiveDecimal, SensitiveJson, SensitiveString, TokenHash, TokenIpAllowlist,
    TokenModelAllowlist, WalletLedgerKey,
};

macro_rules! impl_updated_at_behavior {
    ($($active_model:path => $validator:path),+ $(,)?) => {
        $(
            #[async_trait::async_trait]
            impl ActiveModelBehavior for $active_model {
                async fn before_save<C>(
                    mut self,
                    database: &C,
                    insert: bool,
                ) -> Result<Self, DbErr>
                where
                    C: ConnectionTrait,
                {
                    $validator(&self, database, insert).await?;
                    // 新增记录由数据库默认值初始化，更新时统一刷新审计时间。
                    if !insert {
                        self.updated_at = Set(TimeDateTimeWithTimeZone::now_utc());
                    }
                    Ok(self)
                }
            }
        )+
    };
}

async fn validate_default<T, C>(_model: &T, _database: &C, _insert: bool) -> Result<(), DbErr>
where
    C: ConnectionTrait,
{
    Ok(())
}

async fn validate_route<C>(
    model: &routes::ActiveModel,
    _database: &C,
    insert: bool,
) -> Result<(), DbErr>
where
    C: ConnectionTrait,
{
    let name = model.name.try_as_ref().map(String::as_str);
    let pattern = model.model_pattern.try_as_ref().map(String::as_str);
    if insert && (name.is_none() || pattern.is_none()) {
        return Err(DbErr::Custom("智能路由字段缺失".to_owned()));
    }
    if name.is_some_and(|value| {
        value.is_empty() || value.len() > 128 || value.chars().any(char::is_control)
    }) || pattern.is_some_and(|value| {
        value.len() > MAX_ROUTE_MODEL_PATTERN_BYTES || validate_route_model_pattern(value).is_err()
    }) {
        return Err(DbErr::Custom("智能路由字段无效".to_owned()));
    }
    if model
        .route_mode
        .try_as_ref()
        .is_some_and(|value| RouteMode::try_from(*value).is_err())
        || model
            .strategy
            .try_as_ref()
            .is_some_and(|value| RouteStrategy::try_from(*value).is_err())
    {
        return Err(DbErr::Custom("智能路由模式或策略无效".to_owned()));
    }
    if model.model_mapping.try_as_ref().is_some_and(|value| {
        !value.is_object()
            || serde_json::to_vec(value)
                .map(|encoded| encoded.len() > 16 * 1024)
                .unwrap_or(true)
    }) {
        return Err(DbErr::Custom("智能路由模型映射无效".to_owned()));
    }
    if insert && model.model_mapping.try_as_ref().is_none() {
        return Err(DbErr::Custom("智能路由模型映射缺失".to_owned()));
    }
    Ok(())
}

async fn validate_route_channel<C>(
    model: &route_channels::ActiveModel,
    _database: &C,
    insert: bool,
) -> Result<(), DbErr>
where
    C: ConnectionTrait,
{
    let required_positive = [
        model.route_id.try_as_ref().copied(),
        model.channel_id.try_as_ref().copied(),
        model.credential_id.try_as_ref().copied(),
    ];
    if insert && required_positive.iter().any(Option::is_none) {
        return Err(DbErr::Custom("智能路由候选引用缺失".to_owned()));
    }
    if required_positive.iter().flatten().any(|value| *value <= 0)
        || model.priority.try_as_ref().is_some_and(|value| *value < 0)
        || model.weight.try_as_ref().is_some_and(|value| *value < 0)
        || model
            .success_count
            .try_as_ref()
            .is_some_and(|value| *value < 0)
        || model
            .fail_count
            .try_as_ref()
            .is_some_and(|value| *value < 0)
        || model
            .total_latency
            .try_as_ref()
            .is_some_and(|value| *value < 0)
        || model
            .cooldown_level
            .try_as_ref()
            .is_some_and(|value| !(0..=3).contains(value))
    {
        return Err(DbErr::Custom("智能路由候选字段无效".to_owned()));
    }
    Ok(())
}

async fn validate_group<C>(
    model: &groups::ActiveModel,
    database: &C,
    insert: bool,
) -> Result<(), DbErr>
where
    C: ConnectionTrait,
{
    validate_group_pricing(model, database, insert).await?;
    if [
        model.daily_limit.try_as_ref().copied().flatten(),
        model.weekly_limit.try_as_ref().copied().flatten(),
        model.monthly_limit.try_as_ref().copied().flatten(),
    ]
    .into_iter()
    .flatten()
    .any(|limit| limit < 0)
        || [
            model.daily_usage.try_as_ref().copied(),
            model.weekly_usage.try_as_ref().copied(),
            model.monthly_usage.try_as_ref().copied(),
        ]
        .into_iter()
        .flatten()
        .any(|usage| usage < 0)
        || [
            model.daily_window_start.try_as_ref().copied(),
            model.weekly_window_start.try_as_ref().copied(),
            model.monthly_window_start.try_as_ref().copied(),
        ]
        .into_iter()
        .flatten()
        .any(|started_at| started_at.unix_timestamp() < 0)
    {
        return Err(DbErr::Custom("分组额度窗口字段无效".to_owned()));
    }
    if let Some(reference_id) = changed_reference(&model.fallback_group_id, insert) {
        validate_not_self_reference(&model.id, reference_id, "fallback_group_id")?;
        if groups::Entity::find_by_id(reference_id)
            .one(database)
            .await?
            .is_none()
        {
            return Err(DbErr::Custom(
                "fallback_group_id 必须引用已存在的分组".to_owned(),
            ));
        }
    }
    Ok(())
}

async fn validate_channel<C>(
    model: &channels::ActiveModel,
    _database: &C,
    _insert: bool,
) -> Result<(), DbErr>
where
    C: ConnectionTrait,
{
    // MySQL 5.7 会忽略 CHECK；ORM 写入口仍必须拒绝超出领域边界的渠道超时。
    if let Some(Some(timeout_secs)) = model.timeout_secs.try_as_ref()
        && <u64 as std::convert::TryFrom<i32>>::try_from(*timeout_secs)
            .ok()
            .and_then(|value| ChannelTimeout::new(value).ok())
            .is_none()
    {
        return Err(DbErr::Custom("渠道上游超时无效".to_owned()));
    }
    Ok(())
}

async fn validate_group_pricing<C>(
    model: &groups::ActiveModel,
    database: &C,
    insert: bool,
) -> Result<(), DbErr>
where
    C: ConnectionTrait,
{
    if model
        .ratio_micros
        .try_as_ref()
        .is_some_and(|ratio| *ratio < 0)
    {
        return Err(DbErr::Custom("分组基础倍率不能为负数".to_owned()));
    }
    let peak_changed = insert
        || model.peak_ratio_micros.is_set()
        || model.peak_start.is_set()
        || model.peak_end.is_set();
    if !peak_changed {
        return Ok(());
    }

    // 局部更新必须与当前行合并后再校验，避免只改一个字段绕过跨列不变量。
    let persisted = if insert {
        None
    } else {
        let id = model
            .id
            .try_as_ref()
            .copied()
            .ok_or_else(|| DbErr::Custom("分组计费更新缺少标识".to_owned()))?;
        Some(
            groups::Entity::find_by_id(id)
                .one(database)
                .await?
                .ok_or_else(|| DbErr::Custom("分组计费记录不存在".to_owned()))?,
        )
    };
    let peak_ratio = model
        .peak_ratio_micros
        .try_as_ref()
        .copied()
        .unwrap_or_else(|| persisted.as_ref().and_then(|row| row.peak_ratio_micros));
    let peak_start = model
        .peak_start
        .try_as_ref()
        .copied()
        .unwrap_or_else(|| persisted.as_ref().and_then(|row| row.peak_start));
    let peak_end = model
        .peak_end
        .try_as_ref()
        .copied()
        .unwrap_or_else(|| persisted.as_ref().and_then(|row| row.peak_end));
    match (peak_ratio, peak_start, peak_end) {
        (None, None, None) => Ok(()),
        (Some(ratio), Some(start), Some(end)) if ratio >= 0 && start != end => Ok(()),
        _ => Err(DbErr::Custom("分组高峰倍率窗口无效".to_owned())),
    }
}

async fn validate_user<C>(
    model: &users::ActiveModel,
    database: &C,
    insert: bool,
) -> Result<(), DbErr>
where
    C: ConnectionTrait,
{
    if model
        .session_version
        .try_as_ref()
        .is_some_and(|version| *version < 1)
        || model
            .balance_alert_threshold
            .try_as_ref()
            .is_some_and(|threshold| threshold.is_some_and(|threshold| threshold <= 0))
    {
        return Err(DbErr::Custom("用户会话版本或余额预警阈值无效".to_owned()));
    }
    let Some(reference_id) = changed_reference(&model.inviter_id, insert) else {
        return Ok(());
    };
    validate_not_self_reference(&model.id, reference_id, "inviter_id")?;
    if users::Entity::find_by_id(reference_id)
        .one(database)
        .await?
        .is_none()
    {
        return Err(DbErr::Custom("inviter_id 必须引用已存在的用户".to_owned()));
    }
    Ok(())
}

async fn validate_credential<C>(
    model: &credentials::ActiveModel,
    database: &C,
    insert: bool,
) -> Result<(), DbErr>
where
    C: ConnectionTrait,
{
    if model
        .oauth_revision
        .try_as_ref()
        .is_some_and(|revision| *revision < 0)
        || model
            .oauth_expires_at_epoch_seconds
            .try_as_ref()
            .is_some_and(|expires_at| expires_at.is_some_and(|value| value < 0))
    {
        return Err(DbErr::Custom("OAuth 凭据版本或到期时间无效".to_owned()));
    }
    let Some(reference_id) = changed_reference(&model.parent_id, insert) else {
        return Ok(());
    };
    validate_not_self_reference(&model.id, reference_id, "parent_id")?;
    let Some(parent) = credentials::Entity::find_by_id(reference_id)
        .one(database)
        .await?
    else {
        return Err(DbErr::Custom("parent_id 必须引用已存在的凭据".to_owned()));
    };
    if let Some(channel_id) = model.channel_id.try_as_ref()
        && parent.channel_id != *channel_id
    {
        return Err(DbErr::Custom("parent_id 必须引用同一渠道的凭据".to_owned()));
    }
    Ok(())
}

async fn validate_group_model_ratio<C>(
    model: &group_model_ratios::ActiveModel,
    _database: &C,
    _insert: bool,
) -> Result<(), DbErr>
where
    C: ConnectionTrait,
{
    if model
        .ratio_micros
        .try_as_ref()
        .is_some_and(|ratio| *ratio < 0)
    {
        return Err(DbErr::Custom("分组附加倍率不能为负数".to_owned()));
    }
    Ok(())
}

async fn validate_model_price<C>(
    model: &model_prices::ActiveModel,
    database: &C,
    insert: bool,
) -> Result<(), DbErr>
where
    C: ConnectionTrait,
{
    let Some(model_name) = model.model.try_as_ref() else {
        return Err(DbErr::Custom("模型价格名称无效".to_owned()));
    };
    if !crate::model_price::is_valid_model_name(model_name.as_str()) {
        return Err(DbErr::Custom("模型价格名称无效".to_owned()));
    }

    // MySQL 5.7 会忽略 CHECK；更新时合并当前行，防止局部 ActiveModel 绕过跨列约束。
    let persisted = if insert {
        None
    } else {
        Some(
            model_prices::Entity::find_by_id(model_name.clone())
                .one(database)
                .await?
                .ok_or_else(|| DbErr::Custom("模型价格记录不存在".to_owned()))?,
        )
    };
    let billing_mode = model
        .billing_mode
        .try_as_ref()
        .copied()
        .or_else(|| persisted.as_ref().map(|record| record.billing_mode))
        .ok_or_else(|| DbErr::Custom("模型价格计费模式无效".to_owned()))?;
    let prices = [
        effective_price(
            &model.input_price,
            persisted.as_ref().map(|row| row.input_price),
        ),
        effective_price(
            &model.output_price,
            persisted.as_ref().map(|row| row.output_price),
        ),
        effective_price(
            &model.cache_read_price,
            persisted.as_ref().map(|row| row.cache_read_price),
        ),
        effective_price(
            &model.cache_creation_5m_price,
            persisted.as_ref().map(|row| row.cache_creation_5m_price),
        ),
        effective_price(
            &model.cache_creation_1h_price,
            persisted.as_ref().map(|row| row.cache_creation_1h_price),
        ),
    ];
    let billing_expression = match &model.billing_expression {
        ActiveValue::Set(value) | ActiveValue::Unchanged(value) => {
            value.as_ref().map(SensitiveString::as_str)
        }
        ActiveValue::NotSet => persisted
            .as_ref()
            .and_then(|record| record.billing_expression.as_ref())
            .map(SensitiveString::as_str),
    };
    let version = model
        .version
        .try_as_ref()
        .copied()
        .or_else(|| persisted.as_ref().map(|record| record.version))
        .unwrap_or(1);
    let billing_mode = crate::model_price::ModelPriceBillingMode::from_code(billing_mode)
        .map_err(|_| DbErr::Custom("模型价格计费模式无效".to_owned()))?;
    if version < 1
        || prices
            .iter()
            .any(|price| !crate::model_price::is_valid_model_price_decimal(*price))
        || !crate::model_price::is_valid_model_price_shape(billing_mode, prices, billing_expression)
    {
        return Err(DbErr::Custom("模型价格状态无效".to_owned()));
    }
    Ok(())
}

async fn validate_auth_challenge<C>(
    model: &auth_challenges::ActiveModel,
    database: &C,
    insert: bool,
) -> Result<(), DbErr>
where
    C: ConnectionTrait,
{
    // MySQL 5.7 会忽略 CHECK；局部更新必须与持久化行合并后再校验全部跨列不变量。
    let persisted = if insert {
        None
    } else {
        let id = model
            .id
            .try_as_ref()
            .copied()
            .ok_or_else(|| DbErr::Custom("认证挑战更新缺少标识".to_owned()))?;
        Some(
            auth_challenges::Entity::find_by_id(id)
                .one(database)
                .await?
                .ok_or_else(|| DbErr::Custom("认证挑战记录不存在".to_owned()))?,
        )
    };
    let purpose = effective_value(&model.purpose, persisted.as_ref().map(|row| row.purpose))
        .ok_or_else(|| DbErr::Custom("认证挑战用途无效".to_owned()))?;
    let target_user_id = effective_optional_value(
        &model.target_user_id,
        persisted.as_ref().and_then(|row| row.target_user_id),
    );
    let attempts = effective_value(&model.attempts, persisted.as_ref().map(|row| row.attempts))
        .ok_or_else(|| DbErr::Custom("认证挑战错误次数无效".to_owned()))?;
    let max_attempts = effective_value(
        &model.max_attempts,
        persisted.as_ref().map(|row| row.max_attempts),
    )
    .ok_or_else(|| DbErr::Custom("认证挑战错误次数上限无效".to_owned()))?;
    let version = effective_value(&model.version, persisted.as_ref().map(|row| row.version))
        .ok_or_else(|| DbErr::Custom("认证挑战版本无效".to_owned()))?;
    let issued_at = effective_value(
        &model.issued_at,
        persisted.as_ref().map(|row| row.issued_at),
    )
    .ok_or_else(|| DbErr::Custom("认证挑战签发时间无效".to_owned()))?;
    let expires_at = effective_value(
        &model.expires_at,
        persisted.as_ref().map(|row| row.expires_at),
    )
    .ok_or_else(|| DbErr::Custom("认证挑战过期时间无效".to_owned()))?;
    let next_send_at = effective_value(
        &model.next_send_at,
        persisted.as_ref().map(|row| row.next_send_at),
    )
    .ok_or_else(|| DbErr::Custom("认证挑战重发时间无效".to_owned()))?;
    let consumed_at = effective_optional_value(
        &model.consumed_at,
        persisted.as_ref().and_then(|row| row.consumed_at),
    );

    if !matches!((purpose, target_user_id), (1, None) | (2, Some(_)))
        || !(0..=max_attempts).contains(&attempts)
        || !(1..=10).contains(&max_attempts)
        || version < 1
        || expires_at <= issued_at
        || next_send_at < issued_at
        || next_send_at > expires_at
        || consumed_at.is_some_and(|consumed| consumed < issued_at || consumed >= expires_at)
    {
        return Err(DbErr::Custom("认证挑战状态无效".to_owned()));
    }
    Ok(())
}

async fn validate_usage_log<C>(
    model: &usage_logs::ActiveModel,
    _database: &C,
    insert: bool,
) -> Result<(), DbErr>
where
    C: ConnectionTrait,
{
    // 用量日志只追加；MySQL 5.7 忽略 CHECK 时仍由 ORM 写入口拒绝非法值和更新。
    if !insert {
        return Err(DbErr::Custom("用量日志不允许更新".to_owned()));
    }
    let valid_modes = matches!(model.billing_mode.try_as_ref(), Some(1..=3));
    let valid_event_type = matches!(model.event_type.try_as_ref(), Some(1));
    let valid_source = matches!(model.usage_source.try_as_ref(), Some(1 | 2));
    let valid_semantics = matches!(model.usage_semantics.try_as_ref(), Some(1 | 2));
    let organization_id = model.organization_id.try_as_ref().copied().flatten();
    let organization_team_id = model.organization_team_id.try_as_ref().copied().flatten();
    let valid_organization = match (organization_id, organization_team_id) {
        (None, None) => true,
        (Some(organization_id), team_id) => {
            organization_id > 0 && team_id.is_none_or(|team_id| team_id > 0)
        }
        (None, Some(_)) => false,
    };
    let non_negative = [
        model.input_tokens.try_as_ref(),
        model.output_tokens.try_as_ref(),
        model.cache_read.try_as_ref(),
        model.cache_creation_5m.try_as_ref(),
        model.cache_creation_1h.try_as_ref(),
        model.reasoning_tokens.try_as_ref(),
        model.audio_input_tokens.try_as_ref(),
        model.audio_output_tokens.try_as_ref(),
        model.quota.try_as_ref(),
    ]
    .iter()
    .all(|value| matches!(value, Some(value) if **value >= 0));
    if model.event_id.try_as_ref().is_none()
        || model.user_id.try_as_ref().is_none()
        || model.token_id.try_as_ref().is_none()
        || model.group_id.try_as_ref().is_none()
        || !valid_event_type
        || !valid_modes
        || !valid_source
        || !valid_semantics
        || !valid_organization
        || !non_negative
    {
        return Err(DbErr::Custom("用量日志状态无效".to_owned()));
    }
    Ok(())
}

async fn validate_platform_audit_log<C>(
    model: &platform_audit_logs::ActiveModel,
    _database: &C,
    insert: bool,
) -> Result<(), DbErr>
where
    C: ConnectionTrait,
{
    // 平台审计只追加，JSON 字段只接受受限对象，避免把任意正文写入审计库。
    if !insert {
        return Err(DbErr::Custom("平台管理审计不允许更新".to_owned()));
    }
    let valid_text = |value: Option<&String>, max_length: usize| {
        value.is_some_and(|value| {
            !value.is_empty()
                && value.len() <= max_length
                && value.trim() == value
                && !value.chars().any(char::is_control)
        })
    };
    let valid_json = |value: Option<&String>| {
        value.is_none_or(|value| {
            value.len() <= 2_048
                && serde_json::from_str::<serde_json::Value>(value)
                    .ok()
                    .is_some_and(|value| value.is_object())
        })
    };
    if !matches!(model.operator_user_id.try_as_ref(), Some(value) if *value > 0)
        || !valid_text(model.permission_code.try_as_ref(), 96)
        || !valid_text(model.route.try_as_ref(), 128)
        || !valid_text(model.operation.try_as_ref(), 96)
        || !valid_text(model.resource.try_as_ref(), 64)
        || model
            .resource_id
            .try_as_ref()
            .and_then(Option::as_ref)
            .is_some_and(|value| {
                value.is_empty()
                    || value.len() > 128
                    || value.trim() != value
                    || value.chars().any(char::is_control)
            })
        || !matches!(model.outcome.try_as_ref(), Some(1..=3))
        || !valid_json(model.before_value.try_as_ref().and_then(Option::as_ref))
        || !valid_json(model.after_value.try_as_ref().and_then(Option::as_ref))
        || !valid_json(model.audit_info.try_as_ref().and_then(Option::as_ref))
        || !valid_text(model.request_id.try_as_ref(), 128)
    {
        return Err(DbErr::Custom("平台管理审计状态无效".to_owned()));
    }
    Ok(())
}

async fn validate_wallet_ledger_entry<C>(
    model: &wallet_ledger_entries::ActiveModel,
    _database: &C,
    insert: bool,
) -> Result<(), DbErr>
where
    C: ConnectionTrait,
{
    // 钱包账本只追加；MySQL 5.7 忽略 CHECK 时仍由 ORM 写入口闭合全部跨列不变量。
    if !insert {
        return Err(DbErr::Custom("钱包账本不允许更新".to_owned()));
    }
    let event_key = model
        .event_key
        .try_as_ref()
        .ok_or_else(|| DbErr::Custom("钱包账本事件键无效".to_owned()))?;
    let event_id = WalletEventId::from_persistence_key(event_key.as_str())
        .map_err(|_| DbErr::Custom("钱包账本事件键无效".to_owned()))?;
    let user_id = model
        .user_id
        .try_as_ref()
        .copied()
        .ok_or_else(|| DbErr::Custom("钱包账本用户无效".to_owned()))?;
    let actor_user_id = model.actor_user_id.try_as_ref().copied().flatten();
    let entry_type = model
        .entry_type
        .try_as_ref()
        .copied()
        .ok_or_else(|| DbErr::Custom("钱包账本类型无效".to_owned()))?;
    let delta = model
        .quota_delta
        .try_as_ref()
        .copied()
        .and_then(|value| QuotaDelta::new(value).ok())
        .filter(|value| !value.is_zero())
        .ok_or_else(|| DbErr::Custom("钱包账本增量无效".to_owned()))?;
    let before = model
        .balance_before
        .try_as_ref()
        .copied()
        .and_then(|value| Quota::new(value).ok())
        .ok_or_else(|| DbErr::Custom("钱包账本变更前余额无效".to_owned()))?;
    let after = model
        .balance_after
        .try_as_ref()
        .copied()
        .and_then(|value| Quota::new(value).ok())
        .ok_or_else(|| DbErr::Custom("钱包账本变更后余额无效".to_owned()))?;
    let reason = model.reason.try_as_ref().and_then(Option::as_ref);
    if user_id <= 0 || before.checked_apply(delta) != Ok(after) {
        return Err(DbErr::Custom("钱包账本余额快照无效".to_owned()));
    }

    let valid_reason = reason.is_some_and(|reason| {
        let reason = reason.as_str();
        !reason.is_empty()
            && reason.trim() == reason
            && reason.len() <= 500
            && !reason.chars().any(char::is_control)
    });
    let valid_shape = match entry_type {
        1 => {
            event_id.is_system_opening()
                && actor_user_id.is_none()
                && delta.is_positive()
                && before.is_zero()
                && reason.is_none()
        }
        2 => {
            !event_id.is_system_opening()
                && actor_user_id.is_some_and(|actor| actor > 0)
                && valid_reason
        }
        3..=5 => {
            !event_id.is_system_opening()
                && actor_user_id.is_none()
                && delta.is_positive()
                && reason.is_none()
        }
        _ => false,
    };
    valid_shape
        .then_some(())
        .ok_or_else(|| DbErr::Custom("钱包账本事件形态无效".to_owned()))
}

async fn validate_redemption_batch<C>(
    model: &redemption_batches::ActiveModel,
    _database: &C,
    insert: bool,
) -> Result<(), DbErr>
where
    C: ConnectionTrait,
{
    // 批次创建后只有仓储 CAS 可以改变状态，避免通用实体更新绕过版本检查。
    if !insert {
        return Err(DbErr::Custom("兑换码批次必须通过仓储 CAS 更新".to_owned()));
    }
    let batch_key = model
        .batch_key
        .try_as_ref()
        .ok_or_else(|| DbErr::Custom("兑换码批次标识缺失".to_owned()))?;
    let name = model
        .name
        .try_as_ref()
        .ok_or_else(|| DbErr::Custom("兑换码批次名称缺失".to_owned()))?;
    let created_by_user_id = model
        .created_by_user_id
        .try_as_ref()
        .copied()
        .unwrap_or_default();
    let status = model.status.try_as_ref().copied().unwrap_or_default();
    let quota_amount = model.quota_amount.try_as_ref().copied().unwrap_or_default();
    let code_count = model.code_count.try_as_ref().copied().unwrap_or_default();
    let version = model.version.try_as_ref().copied().unwrap_or_default();
    let expires_at = model.expires_at.try_as_ref().copied().flatten();
    let disabled_at = model.disabled_at.try_as_ref().copied().flatten();
    let created_at = model
        .created_at
        .try_as_ref()
        .copied()
        .ok_or_else(|| DbErr::Custom("兑换码批次创建时间缺失".to_owned()))?;
    let updated_at = model
        .updated_at
        .try_as_ref()
        .copied()
        .ok_or_else(|| DbErr::Custom("兑换码批次更新时间缺失".to_owned()))?;

    if RedemptionBatchId::from_persistence_key(batch_key.as_str()).is_err()
        || name.is_empty()
        || name.len() > 80
        || name.trim() != name.as_str()
        || name.chars().any(char::is_control)
        || created_by_user_id <= 0
        || status != RedemptionBatchStatus::Active.code()
        || quota_amount <= 0
        || !(1..=1_000).contains(&code_count)
        || version != 1
        || disabled_at.is_some()
        || created_at.unix_timestamp() < 0
        || updated_at < created_at
        || expires_at.is_some_and(|value| value <= created_at)
    {
        return Err(DbErr::Custom("兑换码批次状态无效".to_owned()));
    }
    Ok(())
}

async fn validate_redemption_code<C>(
    model: &redemption_codes::ActiveModel,
    _database: &C,
    insert: bool,
) -> Result<(), DbErr>
where
    C: ConnectionTrait,
{
    // 单码消费只能由仓储在余额和账本事务中推进，通用实体入口保持只追加。
    if !insert {
        return Err(DbErr::Custom("兑换码状态必须通过仓储原子消费".to_owned()));
    }
    let code_key = model
        .code_key
        .try_as_ref()
        .ok_or_else(|| DbErr::Custom("兑换码标识缺失".to_owned()))?;
    let code_id = RedemptionCodeId::from_persistence_key(code_key.as_str())
        .map_err(|_| DbErr::Custom("兑换码标识无效".to_owned()))?;
    let wallet_event = WalletEventId::new(code_id.bytes())
        .map_err(|_| DbErr::Custom("兑换码标识无效".to_owned()))?;
    let batch_id = model.batch_id.try_as_ref().copied().unwrap_or_default();
    let digest = model
        .code_sha256
        .try_as_ref()
        .ok_or_else(|| DbErr::Custom("兑换码摘要缺失".to_owned()))?;
    let status = model.status.try_as_ref().copied().unwrap_or_default();
    let used_by_user_id = model.used_by_user_id.try_as_ref().copied().flatten();
    let redeemed_at = model.redeemed_at.try_as_ref().copied().flatten();
    let created_at = model
        .created_at
        .try_as_ref()
        .copied()
        .ok_or_else(|| DbErr::Custom("兑换码创建时间缺失".to_owned()))?;

    if wallet_event.is_system_opening()
        || batch_id <= 0
        || !valid_hex(digest.as_str(), 64, false)
        || status != RedemptionCodeStatus::Available.code()
        || used_by_user_id.is_some()
        || redeemed_at.is_some()
        || created_at.unix_timestamp() < 0
    {
        return Err(DbErr::Custom("兑换码状态无效".to_owned()));
    }
    Ok(())
}

async fn validate_subscription_plan<C>(
    model: &subscription_plans::ActiveModel,
    _database: &C,
    insert: bool,
) -> Result<(), DbErr>
where
    C: ConnectionTrait,
{
    // 计划事实创建后只允许仓储通过 CAS 停用，避免改额度或周期影响既有语义。
    if !insert {
        return Err(DbErr::Custom("订阅计划必须通过仓储 CAS 更新".to_owned()));
    }
    let plan_key = model
        .plan_key
        .try_as_ref()
        .ok_or_else(|| DbErr::Custom("订阅计划标识缺失".to_owned()))?;
    let name = model
        .name
        .try_as_ref()
        .ok_or_else(|| DbErr::Custom("订阅计划名称缺失".to_owned()))?;
    let creator_id = model
        .created_by_user_id
        .try_as_ref()
        .copied()
        .unwrap_or_default();
    let status = model.status.try_as_ref().copied().unwrap_or_default();
    let quota_amount = model.quota_amount.try_as_ref().copied().unwrap_or_default();
    let cycle = model.cycle.try_as_ref().copied().unwrap_or_default();
    let version = model.version.try_as_ref().copied().unwrap_or_default();
    let disabled_at = model.disabled_at.try_as_ref().copied().flatten();
    let created_at = model
        .created_at
        .try_as_ref()
        .copied()
        .ok_or_else(|| DbErr::Custom("订阅计划创建时间缺失".to_owned()))?;
    let updated_at = model
        .updated_at
        .try_as_ref()
        .copied()
        .ok_or_else(|| DbErr::Custom("订阅计划更新时间缺失".to_owned()))?;

    if SubscriptionPlanId::from_persistence_key(plan_key.as_str()).is_err()
        || name.is_empty()
        || name.len() > 80
        || name.trim() != name.as_str()
        || name.chars().any(char::is_control)
        || creator_id <= 0
        || status != SubscriptionPlanStatus::Active.code()
        || quota_amount <= 0
        || SubscriptionCycle::try_from(cycle).is_err()
        || version != 1
        || disabled_at.is_some()
        || created_at.unix_timestamp() < 0
        || updated_at < created_at
    {
        return Err(DbErr::Custom("订阅计划状态无效".to_owned()));
    }
    Ok(())
}

async fn validate_invite_rebate_event<C>(
    model: &invite_rebate_events::ActiveModel,
    _database: &C,
    insert: bool,
) -> Result<(), DbErr>
where
    C: ConnectionTrait,
{
    // 返利事件和对应钱包账本只能在同一仓储事务中首次追加，禁止通用实体更新。
    if !insert {
        return Err(DbErr::Custom("邀请返利事件不允许更新".to_owned()));
    }
    let event_key = model
        .event_key
        .try_as_ref()
        .ok_or_else(|| DbErr::Custom("邀请返利事件标识缺失".to_owned()))?;
    let event_id = WalletEventId::from_persistence_key(event_key.as_str())
        .map_err(|_| DbErr::Custom("邀请返利事件标识无效".to_owned()))?;
    let inviter = model
        .inviter_user_id
        .try_as_ref()
        .copied()
        .unwrap_or_default();
    let invitee = model
        .invitee_user_id
        .try_as_ref()
        .copied()
        .unwrap_or_default();
    let quota = model.quota_amount.try_as_ref().copied().unwrap_or_default();
    let balance_after = model
        .balance_after
        .try_as_ref()
        .copied()
        .unwrap_or_default();
    let wallet_entry = model
        .wallet_ledger_entry_id
        .try_as_ref()
        .copied()
        .unwrap_or_default();
    let (Some(credited_at), Some(created_at)) = (
        model.credited_at.try_as_ref().copied(),
        model.created_at.try_as_ref().copied(),
    ) else {
        return Err(DbErr::Custom("邀请返利事件时间缺失".to_owned()));
    };
    if event_id.is_system_opening()
        || inviter <= 0
        || invitee <= 0
        || inviter == invitee
        || quota <= 0
        || balance_after < quota
        || wallet_entry <= 0
        || created_at < credited_at
    {
        return Err(DbErr::Custom("邀请返利事件状态无效".to_owned()));
    }
    Ok(())
}

async fn validate_user_subscription<C>(
    model: &user_subscriptions::ActiveModel,
    _database: &C,
    insert: bool,
) -> Result<(), DbErr>
where
    C: ConnectionTrait,
{
    // 用量、周期推进和生命周期迁移必须由后续仓储原子维护，通用实体入口只允许首次绑定。
    if !insert {
        return Err(DbErr::Custom("用户订阅必须通过仓储更新".to_owned()));
    }
    let subscription_key = model
        .subscription_key
        .try_as_ref()
        .ok_or_else(|| DbErr::Custom("用户订阅标识缺失".to_owned()))?;
    let user_id = model.user_id.try_as_ref().copied().unwrap_or_default();
    let plan_id = model.plan_id.try_as_ref().copied().unwrap_or_default();
    let plan_version = model.plan_version.try_as_ref().copied().unwrap_or_default();
    let status = model.status.try_as_ref().copied().unwrap_or_default();
    let quota_amount = model.quota_amount.try_as_ref().copied().unwrap_or_default();
    let quota_used = model.quota_used.try_as_ref().copied().unwrap_or_default();
    let cycle = model.cycle.try_as_ref().copied().unwrap_or_default();
    let version = model.version.try_as_ref().copied().unwrap_or_default();
    let window_started_at = model
        .window_started_at
        .try_as_ref()
        .copied()
        .ok_or_else(|| DbErr::Custom("用户订阅窗口起点缺失".to_owned()))?;
    let window_ends_at = model
        .window_ends_at
        .try_as_ref()
        .copied()
        .ok_or_else(|| DbErr::Custom("用户订阅窗口终点缺失".to_owned()))?;
    let bound_at = model
        .bound_at
        .try_as_ref()
        .copied()
        .ok_or_else(|| DbErr::Custom("用户订阅绑定时间缺失".to_owned()))?;
    let status_changed_at = model
        .status_changed_at
        .try_as_ref()
        .copied()
        .ok_or_else(|| DbErr::Custom("用户订阅状态时间缺失".to_owned()))?;
    let created_at = model
        .created_at
        .try_as_ref()
        .copied()
        .ok_or_else(|| DbErr::Custom("用户订阅创建时间缺失".to_owned()))?;
    let updated_at = model
        .updated_at
        .try_as_ref()
        .copied()
        .ok_or_else(|| DbErr::Custom("用户订阅更新时间缺失".to_owned()))?;

    if UserSubscriptionId::from_persistence_key(subscription_key.as_str()).is_err()
        || user_id <= 0
        || plan_id <= 0
        || plan_version <= 0
        || status != UserSubscriptionStatus::Active.code()
        || quota_amount <= 0
        || quota_used != 0
        || SubscriptionCycle::try_from(cycle).is_err()
        || version != 1
        || window_started_at.unix_timestamp() < 0
        || window_ends_at <= window_started_at
        || bound_at.unix_timestamp() < 0
        || bound_at >= window_ends_at
        || status_changed_at != bound_at
        || created_at != bound_at
        || updated_at < created_at
    {
        return Err(DbErr::Custom("用户订阅状态无效".to_owned()));
    }
    Ok(())
}

async fn validate_topup_order<C>(
    model: &topup_orders::ActiveModel,
    database: &C,
    insert: bool,
) -> Result<(), DbErr>
where
    C: ConnectionTrait,
{
    let persisted = if insert {
        None
    } else {
        let id = model
            .id
            .try_as_ref()
            .copied()
            .ok_or_else(|| DbErr::Custom("充值订单更新缺少标识".to_owned()))?;
        Some(
            topup_orders::Entity::find_by_id(id)
                .one(database)
                .await?
                .ok_or_else(|| DbErr::Custom("充值订单不存在".to_owned()))?,
        )
    };
    let order_key = merge_required(
        &model.order_key,
        persisted.as_ref().map(|row| row.order_key.clone()),
        "充值订单标识缺失",
    )?;
    let provider = merge_required(
        &model.provider,
        persisted.as_ref().map(|row| row.provider.clone()),
        "充值渠道缺失",
    )?;
    let status = merge_required(
        &model.status,
        persisted.as_ref().map(|row| row.status),
        "充值订单状态缺失",
    )?;
    let amount_minor = merge_required(
        &model.amount_minor,
        persisted.as_ref().map(|row| row.amount_minor),
        "充值金额缺失",
    )?;
    let currency = merge_required(
        &model.currency,
        persisted.as_ref().map(|row| row.currency.clone()),
        "充值币种缺失",
    )?;
    let quota_amount = merge_required(
        &model.quota_amount,
        persisted.as_ref().map(|row| row.quota_amount),
        "充值到账额度缺失",
    )?;
    let idempotency_key = merge_required(
        &model.idempotency_key,
        persisted.as_ref().map(|row| row.idempotency_key.clone()),
        "充值幂等键缺失",
    )?;
    let created_at = merge_required(
        &model.created_at,
        persisted.as_ref().map(|row| row.created_at),
        "充值创建时间缺失",
    )?;
    let expires_at = merge_optional(
        &model.expires_at,
        persisted.as_ref().and_then(|row| row.expires_at),
    );
    let paid_at = merge_optional(
        &model.paid_at,
        persisted.as_ref().and_then(|row| row.paid_at),
    );
    let closed_at = merge_optional(
        &model.closed_at,
        persisted.as_ref().and_then(|row| row.closed_at),
    );

    if !valid_hex(&order_key, 32, true)
        || !valid_hex(&idempotency_key, 32, true)
        || provider.is_empty()
        || !valid_currency(&currency)
        || amount_minor <= 0
        || quota_amount <= 0
        || model
            .user_id
            .try_as_ref()
            .copied()
            .or_else(|| persisted.as_ref().map(|row| row.user_id))
            .is_none_or(|id| id <= 0)
        || expires_at.is_some_and(|expires_at| expires_at <= created_at)
        || !valid_topup_order_state(status, paid_at, closed_at)
    {
        return Err(DbErr::Custom("充值订单状态无效".to_owned()));
    }
    Ok(())
}

async fn validate_subscription_order<C>(
    model: &subscription_orders::ActiveModel,
    _database: &C,
    insert: bool,
) -> Result<(), DbErr>
where
    C: ConnectionTrait,
{
    // 订单状态和价格快照只允许通过订阅仓储写入，避免通用实体入口绕过幂等校验。
    if !insert {
        return Err(DbErr::Custom("订阅订单必须通过受信任仓储更新".to_owned()));
    }
    let order_key = model
        .order_key
        .try_as_ref()
        .ok_or_else(|| DbErr::Custom("订阅订单标识缺失".to_owned()))?;
    let idempotency_key = model
        .idempotency_key
        .try_as_ref()
        .ok_or_else(|| DbErr::Custom("订阅订单幂等键缺失".to_owned()))?;
    let user_id = model.user_id.try_as_ref().copied().unwrap_or_default();
    let plan_id = model.plan_id.try_as_ref().copied().unwrap_or_default();
    let plan_key = model
        .plan_key
        .try_as_ref()
        .ok_or_else(|| DbErr::Custom("订阅订单计划标识缺失".to_owned()))?;
    let plan_version = model.plan_version.try_as_ref().copied().unwrap_or_default();
    let status = model.status.try_as_ref().copied().unwrap_or_default();
    let version = model.version.try_as_ref().copied().unwrap_or_default();
    let amount_minor = model.amount_minor.try_as_ref().copied().unwrap_or_default();
    let quota_amount = model.quota_amount.try_as_ref().copied().unwrap_or_default();
    let provider = model
        .provider
        .try_as_ref()
        .ok_or_else(|| DbErr::Custom("订阅订单支付 Provider 缺失".to_owned()))?;
    let currency = model
        .currency
        .try_as_ref()
        .ok_or_else(|| DbErr::Custom("订阅订单币种缺失".to_owned()))?;
    let created_at = model
        .created_at
        .try_as_ref()
        .copied()
        .ok_or_else(|| DbErr::Custom("订阅订单创建时间缺失".to_owned()))?;
    let expires_at = model.expires_at.try_as_ref().copied().flatten();
    let paid_at = model.paid_at.try_as_ref().copied().flatten();
    let closed_at = model.closed_at.try_as_ref().copied().flatten();

    if !valid_hex(order_key, 32, true)
        || !valid_hex(idempotency_key, 32, true)
        || user_id <= 0
        || plan_id <= 0
        || af_domain::SubscriptionPlanId::from_persistence_key(plan_key.as_str()).is_err()
        || plan_version <= 0
        || provider.is_empty()
        || provider.len() > 32
        || provider
            .bytes()
            .any(|byte| !(byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-'))
        || !valid_currency(currency)
        || amount_minor <= 0
        || quota_amount <= 0
        || version != 1
        || created_at.unix_timestamp() < 0
        || expires_at.is_some_and(|value| value <= created_at)
        || !valid_subscription_order_state(status, paid_at, closed_at)
    {
        return Err(DbErr::Custom("订阅订单状态无效".to_owned()));
    }
    Ok(())
}

async fn validate_refund_request<C>(
    model: &refund_requests::ActiveModel,
    database: &C,
    insert: bool,
) -> Result<(), DbErr>
where
    C: ConnectionTrait,
{
    let persisted = if insert {
        None
    } else {
        let id = model
            .id
            .try_as_ref()
            .copied()
            .ok_or_else(|| DbErr::Custom("退款请求更新缺少标识".to_owned()))?;
        Some(
            refund_requests::Entity::find_by_id(id)
                .one(database)
                .await?
                .ok_or_else(|| DbErr::Custom("退款请求不存在".to_owned()))?,
        )
    };
    let request_key = merge_required(
        &model.request_key,
        persisted.as_ref().map(|row| row.request_key.clone()),
        "退款请求标识缺失",
    )?;
    let idempotency_key = merge_required(
        &model.idempotency_key,
        persisted.as_ref().map(|row| row.idempotency_key.clone()),
        "退款请求幂等键缺失",
    )?;
    let order_key = merge_required(
        &model.order_key,
        persisted.as_ref().map(|row| row.order_key.clone()),
        "退款订单标识缺失",
    )?;
    let provider = merge_required(
        &model.provider,
        persisted.as_ref().map(|row| row.provider.clone()),
        "退款支付渠道缺失",
    )?;
    let currency = merge_required(
        &model.currency,
        persisted.as_ref().map(|row| row.currency.clone()),
        "退款币种缺失",
    )?;
    let order_kind = merge_required(
        &model.order_kind,
        persisted.as_ref().map(|row| row.order_kind),
        "退款订单类型缺失",
    )?;
    let original_amount_minor = merge_required(
        &model.original_amount_minor,
        persisted.as_ref().map(|row| row.original_amount_minor),
        "退款原始金额缺失",
    )?;
    let refund_amount_minor = merge_required(
        &model.refund_amount_minor,
        persisted.as_ref().map(|row| row.refund_amount_minor),
        "退款金额缺失",
    )?;
    let provider_refund_id = model
        .provider_refund_id
        .try_as_ref()
        .and_then(Clone::clone)
        .or_else(|| {
            persisted
                .as_ref()
                .and_then(|row| row.provider_refund_id.clone())
        });
    let status = merge_required(
        &model.status,
        persisted.as_ref().map(|row| row.status),
        "退款请求状态缺失",
    )?;
    let approval_status = merge_required(
        &model.approval_status,
        persisted.as_ref().map(|row| row.approval_status),
        "退款审批状态缺失",
    )?;
    let approval_actor_id = model
        .approval_actor_id
        .try_as_ref()
        .and_then(Clone::clone)
        .or_else(|| persisted.as_ref().and_then(|row| row.approval_actor_id));
    let approval_reason = model
        .approval_reason
        .try_as_ref()
        .and_then(Clone::clone)
        .or_else(|| {
            persisted
                .as_ref()
                .and_then(|row| row.approval_reason.clone())
        });
    let version = merge_required(
        &model.version,
        persisted.as_ref().map(|row| row.version),
        "退款请求版本缺失",
    )?;
    let created_at = merge_required(
        &model.created_at,
        persisted.as_ref().map(|row| row.created_at),
        "退款请求创建时间缺失",
    )?;
    let updated_at = merge_required(
        &model.updated_at,
        persisted.as_ref().map(|row| row.updated_at),
        "退款请求更新时间缺失",
    )?;
    let user_id = model
        .user_id
        .try_as_ref()
        .copied()
        .or_else(|| persisted.as_ref().map(|row| row.user_id))
        .unwrap_or_default();

    if !valid_hex(&request_key, 32, true)
        || !valid_hex(&idempotency_key, 32, true)
        || !valid_hex(&order_key, 32, true)
        || user_id <= 0
        || !matches!(order_kind, 1 | 2)
        || provider.is_empty()
        || provider.len() > 64
        || provider
            .bytes()
            .any(|byte| !(byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-'))
        || !valid_currency(&currency)
        || original_amount_minor <= 0
        || refund_amount_minor <= 0
        || refund_amount_minor > original_amount_minor
        || !matches!(status, 1..=7)
        || !matches!(approval_status, 1..=3)
        || approval_actor_id.is_some_and(|value| value <= 0)
        || (approval_status == 1 && (approval_actor_id.is_some() || approval_reason.is_some()))
        || (approval_status != 1 && approval_actor_id.is_none())
        || approval_reason.as_deref().is_some_and(|value| {
            value.is_empty()
                || value.len() > 512
                || value.trim() != value
                || value.chars().any(char::is_control)
        })
        || provider_refund_id.as_deref().is_some_and(|value| {
            value.is_empty()
                || value.len() > 128
                || value.trim() != value
                || value.chars().any(char::is_control)
        })
        || (status == 3 && provider_refund_id.is_none())
        || version <= 0
        || created_at.unix_timestamp() < 0
        || updated_at < created_at
    {
        return Err(DbErr::Custom("退款请求状态无效".to_owned()));
    }
    Ok(())
}

async fn validate_topup_payment_event<C>(
    model: &topup_payment_events::ActiveModel,
    _database: &C,
    insert: bool,
) -> Result<(), DbErr>
where
    C: ConnectionTrait,
{
    // 支付事件是只追加审计事实；重复 webhook 只能通过唯一键幂等识别，不能更新旧事件。
    if !insert {
        return Err(DbErr::Custom("支付事件不允许更新".to_owned()));
    }
    let event_key = model
        .event_key
        .try_as_ref()
        .ok_or_else(|| DbErr::Custom("支付事件标识缺失".to_owned()))?;
    let provider = model
        .provider
        .try_as_ref()
        .ok_or_else(|| DbErr::Custom("支付事件渠道缺失".to_owned()))?;
    let provider_event_id = model
        .provider_event_id
        .try_as_ref()
        .ok_or_else(|| DbErr::Custom("支付事件 Provider 标识缺失".to_owned()))?;
    let signature_key_fingerprint = model
        .signature_key_fingerprint
        .try_as_ref()
        .ok_or_else(|| DbErr::Custom("支付事件签名指纹缺失".to_owned()))?;
    let payload_sha256 = model
        .payload_sha256
        .try_as_ref()
        .ok_or_else(|| DbErr::Custom("支付事件负载哈希缺失".to_owned()))?;
    let order_id = model.order_id.try_as_ref().copied().unwrap_or_default();
    let event_type = model.event_type.try_as_ref().copied().unwrap_or_default();
    if order_id <= 0
        || !valid_hex(event_key, 32, true)
        || provider.is_empty()
        || provider_event_id.as_str().is_empty()
        || !matches!(event_type, 1..=3)
        || !valid_hex(signature_key_fingerprint.as_str(), 64, false)
        || !valid_hex(payload_sha256.as_str(), 64, false)
        || model.received_at.try_as_ref().is_none()
        || model.created_at.try_as_ref().is_none()
    {
        return Err(DbErr::Custom("支付事件状态无效".to_owned()));
    }
    Ok(())
}

async fn validate_subscription_payment_event<C>(
    model: &subscription_payment_events::ActiveModel,
    _database: &C,
    insert: bool,
) -> Result<(), DbErr>
where
    C: ConnectionTrait,
{
    // 订阅支付事件与充值事件隔离，且同样只能通过唯一键追加。
    if !insert {
        return Err(DbErr::Custom("订阅支付事件不允许更新".to_owned()));
    }
    let event_key = model
        .event_key
        .try_as_ref()
        .ok_or_else(|| DbErr::Custom("订阅支付事件标识缺失".to_owned()))?;
    let provider = model
        .provider
        .try_as_ref()
        .ok_or_else(|| DbErr::Custom("订阅支付事件渠道缺失".to_owned()))?;
    let provider_event_id = model
        .provider_event_id
        .try_as_ref()
        .ok_or_else(|| DbErr::Custom("订阅支付事件 Provider 标识缺失".to_owned()))?;
    let signature_key_fingerprint = model
        .signature_key_fingerprint
        .try_as_ref()
        .ok_or_else(|| DbErr::Custom("订阅支付事件签名指纹缺失".to_owned()))?;
    let payload_sha256 = model
        .payload_sha256
        .try_as_ref()
        .ok_or_else(|| DbErr::Custom("订阅支付事件负载哈希缺失".to_owned()))?;
    let order_id = model.order_id.try_as_ref().copied().unwrap_or_default();
    let event_type = model.event_type.try_as_ref().copied().unwrap_or_default();
    let amount_minor = model.amount_minor.try_as_ref().copied().flatten();
    let currency = model.currency.try_as_ref().and_then(Option::as_deref);
    let payment_method = model.payment_method.try_as_ref().and_then(Option::as_deref);
    if order_id <= 0
        || !valid_hex(event_key, 32, true)
        || provider.is_empty()
        || provider_event_id.as_str().is_empty()
        || !matches!(event_type, 1..=3)
        || amount_minor.is_some_and(|value| value <= 0)
        || currency.is_some_and(|value| !valid_currency(value))
        || payment_method.is_some_and(|value| value.is_empty() || value.len() > 32)
        || !valid_hex(signature_key_fingerprint.as_str(), 64, false)
        || !valid_hex(payload_sha256.as_str(), 64, false)
        || model.received_at.try_as_ref().is_none()
        || model.created_at.try_as_ref().is_none()
    {
        return Err(DbErr::Custom("订阅支付事件状态无效".to_owned()));
    }
    Ok(())
}

fn merge_required<T>(
    active: &ActiveValue<T>,
    persisted: Option<T>,
    message: &'static str,
) -> Result<T, DbErr>
where
    T: Clone + Into<sea_orm::Value>,
{
    active
        .try_as_ref()
        .cloned()
        .or(persisted)
        .ok_or_else(|| DbErr::Custom(message.to_owned()))
}

fn merge_optional<T>(active: &ActiveValue<Option<T>>, persisted: Option<T>) -> Option<T>
where
    T: Copy + Into<sea_orm::Value> + sea_orm::sea_query::Nullable,
{
    match active {
        ActiveValue::Set(value) | ActiveValue::Unchanged(value) => *value,
        ActiveValue::NotSet => persisted,
    }
}

fn valid_topup_order_state(
    status: i16,
    paid_at: Option<TimeDateTimeWithTimeZone>,
    closed_at: Option<TimeDateTimeWithTimeZone>,
) -> bool {
    match status {
        1 | 2 => paid_at.is_none() && closed_at.is_none(),
        3 => paid_at.is_some() && closed_at.is_none(),
        4..=6 => paid_at.is_none() && closed_at.is_some(),
        _ => false,
    }
}

fn valid_subscription_order_state(
    status: i16,
    paid_at: Option<TimeDateTimeWithTimeZone>,
    closed_at: Option<TimeDateTimeWithTimeZone>,
) -> bool {
    match status {
        1 | 2 => paid_at.is_none() && closed_at.is_none(),
        3 => paid_at.is_some() && closed_at.is_none(),
        4..=6 => paid_at.is_none() && closed_at.is_some(),
        _ => false,
    }
}

fn valid_currency(value: &str) -> bool {
    value.len() == 3 && value.bytes().all(|byte| byte.is_ascii_uppercase())
}

fn valid_hex(value: &str, length: usize, non_zero: bool) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        && (!non_zero || value.bytes().any(|byte| byte != b'0'))
}

fn effective_price(
    active: &ActiveValue<SensitiveDecimal>,
    persisted: Option<SensitiveDecimal>,
) -> Decimal {
    active
        .try_as_ref()
        .copied()
        .or(persisted)
        .map(SensitiveDecimal::expose)
        .unwrap_or(Decimal::ZERO)
}

fn effective_value<T>(value: &ActiveValue<T>, persisted: Option<T>) -> Option<T>
where
    T: Copy + Into<sea_orm::Value>,
{
    value.try_as_ref().copied().or(persisted)
}

fn effective_optional_value<T>(value: &ActiveValue<Option<T>>, persisted: Option<T>) -> Option<T>
where
    T: Copy + Into<sea_orm::Value> + sea_orm::sea_query::Nullable,
{
    match value {
        ActiveValue::Set(value) | ActiveValue::Unchanged(value) => *value,
        ActiveValue::NotSet => persisted,
    }
}

/// MySQL 无法用 CHECK 约束自增主键，自引用在持久化入口补充拒绝。
fn validate_not_self_reference(
    id: &ActiveValue<i64>,
    reference_id: i64,
    field: &'static str,
) -> Result<(), DbErr> {
    if id.try_as_ref() == Some(&reference_id) {
        return Err(DbErr::Custom(format!("{field} 不得引用自身")));
    }
    Ok(())
}

fn changed_reference(reference: &ActiveValue<Option<i64>>, insert: bool) -> Option<i64> {
    if !insert && !reference.is_set() {
        return None;
    }
    reference.try_as_ref().copied().flatten()
}

impl_updated_at_behavior!(
    abilities::ActiveModel => validate_default,
    auth_challenge_rate_limits::ActiveModel => validate_default,
    auth_challenges::ActiveModel => validate_auth_challenge,
    authentication_settings::ActiveModel => validate_default,
    balance_alert_settings::ActiveModel => validate_default,
    billing_batch_checkpoints::ActiveModel => validate_default,
    billing_reservations::ActiveModel => validate_billing_reservation,
    channel_groups::ActiveModel => validate_default,
    channel_models::ActiveModel => validate_default,
    channels::ActiveModel => validate_channel,
    credentials::ActiveModel => validate_credential,
    custom_oauth2_identities::ActiveModel => validate_default,
    custom_oauth2_login_transactions::ActiveModel => validate_default,
    debug_trace_settings::ActiveModel => validate_default,
    email_settings::ActiveModel => validate_default,
    group_model_ratios::ActiveModel => validate_group_model_ratio,
    model_prices::ActiveModel => validate_model_price,
    models::ActiveModel => validate_default,
    network_settings::ActiveModel => validate_default,
    oauth_login_providers::ActiveModel => validate_default,
    oauth_login_transactions::ActiveModel => validate_default,
    options::ActiveModel => validate_default,
    registration_rate_limits::ActiveModel => validate_default,
    route_channels::ActiveModel => validate_route_channel,
    routes::ActiveModel => validate_route,
    site_settings::ActiveModel => validate_default,
    tokens::ActiveModel => validate_default,
    topup_orders::ActiveModel => validate_topup_order,
    subscription_orders::ActiveModel => validate_subscription_order,
    refund_requests::ActiveModel => validate_refund_request,
    user_oauth_identities::ActiveModel => validate_default,
    users::ActiveModel => validate_user,
);

#[async_trait::async_trait]
impl ActiveModelBehavior for user_notification_events::ActiveModel {}

#[async_trait::async_trait]
impl ActiveModelBehavior for announcements::ActiveModel {}

#[async_trait::async_trait]
impl ActiveModelBehavior for user_notification_receipts::ActiveModel {}

#[async_trait::async_trait]
impl ActiveModelBehavior for groups::ActiveModel {
    async fn before_save<C>(mut self, database: &C, insert: bool) -> Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        if insert {
            let starts = current_group_window_starts()?;
            if self.daily_usage.is_not_set() {
                self.daily_usage = Set(0);
            }
            if self.weekly_usage.is_not_set() {
                self.weekly_usage = Set(0);
            }
            if self.monthly_usage.is_not_set() {
                self.monthly_usage = Set(0);
            }
            if self.daily_window_start.is_not_set() {
                self.daily_window_start = Set(starts[0]);
            }
            if self.weekly_window_start.is_not_set() {
                self.weekly_window_start = Set(starts[1]);
            }
            if self.monthly_window_start.is_not_set() {
                self.monthly_window_start = Set(starts[2]);
            }
        }
        validate_group(&self, database, insert).await?;
        if !insert {
            self.updated_at = Set(TimeDateTimeWithTimeZone::now_utc());
        }
        Ok(self)
    }
}

fn current_group_window_starts() -> Result<[TimeDateTimeWithTimeZone; 3], DbErr> {
    let now = TimeDateTimeWithTimeZone::now_utc();
    let now = u64::try_from(now.unix_timestamp())
        .map_err(|_| DbErr::Custom("分组额度窗口当前时间无效".to_owned()))?;
    let mut starts = [TimeDateTimeWithTimeZone::UNIX_EPOCH; 3];
    for (index, cycle) in [
        SubscriptionCycle::Daily,
        SubscriptionCycle::Weekly,
        SubscriptionCycle::Monthly,
    ]
    .into_iter()
    .enumerate()
    {
        let window = SubscriptionWindow::initial(cycle, now)
            .map_err(|_| DbErr::Custom("分组额度窗口边界无效".to_owned()))?;
        starts[index] = TimeDateTimeWithTimeZone::from_unix_timestamp(
            i64::try_from(window.started_at())
                .map_err(|_| DbErr::Custom("分组额度窗口边界无效".to_owned()))?,
        )
        .map_err(|_| DbErr::Custom("分组额度窗口边界无效".to_owned()))?;
    }
    Ok(starts)
}

async fn validate_billing_reservation<C>(
    model: &billing_reservations::ActiveModel,
    _database: &C,
    _insert: bool,
) -> Result<(), DbErr>
where
    C: ConnectionTrait,
{
    if model
        .reservation_kind
        .try_as_ref()
        .is_some_and(|kind| !matches!(*kind, 1 | 2))
    {
        return Err(DbErr::Custom("计费预留用途无效".to_owned()));
    }
    if model
        .funding_source
        .try_as_ref()
        .is_some_and(|source| !matches!(*source, 1 | 2))
    {
        return Err(DbErr::Custom("计费预留资金来源无效".to_owned()));
    }
    Ok(())
}

#[async_trait::async_trait]
impl ActiveModelBehavior for billing_subscription_reservations::ActiveModel {
    async fn before_save<C>(self, _database: &C, insert: bool) -> Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        // 订阅实际分摊只能由额度仓储与父预留状态在同一事务内固化。
        if !insert {
            return Err(DbErr::Custom("订阅计费预留必须通过额度仓储更新".to_owned()));
        }
        let key = self
            .idempotency_key
            .try_as_ref()
            .ok_or_else(|| DbErr::Custom("订阅计费预留标识缺失".to_owned()))?;
        let subscription_id = self
            .user_subscription_id
            .try_as_ref()
            .copied()
            .unwrap_or_default();
        let reserved = self
            .reserved_quota
            .try_as_ref()
            .copied()
            .unwrap_or_default();
        let actual = self
            .subscription_actual_quota
            .try_as_ref()
            .copied()
            .flatten();
        let (Some(window_started_at), Some(window_ends_at), Some(created_at), Some(updated_at)) = (
            self.window_started_at.try_as_ref().copied(),
            self.window_ends_at.try_as_ref().copied(),
            self.created_at.try_as_ref().copied(),
            self.updated_at.try_as_ref().copied(),
        ) else {
            return Err(DbErr::Custom("订阅计费预留时间缺失".to_owned()));
        };
        if BillingReservationId::from_persistence_key(key.as_str()).is_err()
            || subscription_id <= 0
            || reserved <= 0
            || actual.is_some()
            || window_started_at.unix_timestamp() < 0
            || window_ends_at <= window_started_at
            || created_at < window_started_at
            || created_at >= window_ends_at
            || updated_at != created_at
        {
            return Err(DbErr::Custom("订阅计费预留状态无效".to_owned()));
        }
        Ok(self)
    }
}

#[async_trait::async_trait]
impl ActiveModelBehavior for billing_token_window_reservations::ActiveModel {
    async fn before_save<C>(self, _database: &C, insert: bool) -> Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        // 窗口起点只能在父预留创建时固化，后续结算仅读取该快照。
        if !insert {
            return Err(DbErr::Custom("令牌窗口预留必须通过额度仓储更新".to_owned()));
        }
        let key = self
            .idempotency_key
            .try_as_ref()
            .ok_or_else(|| DbErr::Custom("令牌窗口预留标识缺失".to_owned()))?;
        let reserved = self
            .reserved_quota
            .try_as_ref()
            .copied()
            .unwrap_or_default();
        let (Some(window_5h_start), Some(window_1d_start), Some(window_7d_start)) = (
            self.window_5h_start.try_as_ref().copied(),
            self.window_1d_start.try_as_ref().copied(),
            self.window_7d_start.try_as_ref().copied(),
        ) else {
            return Err(DbErr::Custom("令牌窗口预留起点缺失".to_owned()));
        };
        let (Some(created_at), Some(updated_at)) = (
            self.created_at.try_as_ref().copied(),
            self.updated_at.try_as_ref().copied(),
        ) else {
            return Err(DbErr::Custom("令牌窗口预留审计时间缺失".to_owned()));
        };
        if BillingReservationId::from_persistence_key(key.as_str()).is_err()
            || reserved <= 0
            || [window_5h_start, window_1d_start, window_7d_start]
                .into_iter()
                .any(|started_at| started_at.unix_timestamp() < 0 || started_at > created_at)
            || updated_at != created_at
        {
            return Err(DbErr::Custom("令牌窗口预留状态无效".to_owned()));
        }
        Ok(self)
    }
}

#[async_trait::async_trait]
impl ActiveModelBehavior for billing_group_window_reservations::ActiveModel {
    async fn before_save<C>(self, _database: &C, insert: bool) -> Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        // 窗口起点只能在父预留创建时固化，后续结算仅读取该快照。
        if !insert {
            return Err(DbErr::Custom("分组窗口预留必须通过额度仓储更新".to_owned()));
        }
        let key = self
            .idempotency_key
            .try_as_ref()
            .ok_or_else(|| DbErr::Custom("分组窗口预留标识缺失".to_owned()))?;
        let reserved = self
            .reserved_quota
            .try_as_ref()
            .copied()
            .unwrap_or_default();
        let (Some(daily_start), Some(weekly_start), Some(monthly_start)) = (
            self.daily_window_start.try_as_ref().copied(),
            self.weekly_window_start.try_as_ref().copied(),
            self.monthly_window_start.try_as_ref().copied(),
        ) else {
            return Err(DbErr::Custom("分组窗口预留起点缺失".to_owned()));
        };
        let (Some(created_at), Some(updated_at)) = (
            self.created_at.try_as_ref().copied(),
            self.updated_at.try_as_ref().copied(),
        ) else {
            return Err(DbErr::Custom("分组窗口预留审计时间缺失".to_owned()));
        };
        if BillingReservationId::from_persistence_key(key.as_str()).is_err()
            || reserved <= 0
            || [daily_start, weekly_start, monthly_start]
                .into_iter()
                .any(|started_at| started_at.unix_timestamp() < 0 || started_at > created_at)
            || updated_at != created_at
        {
            return Err(DbErr::Custom("分组窗口预留状态无效".to_owned()));
        }
        Ok(self)
    }
}

#[async_trait::async_trait]
impl ActiveModelBehavior for debug_traces::ActiveModel {}

#[async_trait::async_trait]
impl ActiveModelBehavior for debug_trace_attempts::ActiveModel {}

#[async_trait::async_trait]
impl ActiveModelBehavior for debug_trace_snapshots::ActiveModel {}

#[async_trait::async_trait]
impl ActiveModelBehavior for debug_trace_snapshot_access_audits::ActiveModel {}

#[async_trait::async_trait]
impl ActiveModelBehavior for balance_alert_events::ActiveModel {
    async fn before_save<C>(self, _database: &C, insert: bool) -> Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        // 投递状态只能由余额预警仓储使用带版本条件的批量更新推进。
        if !insert {
            return Err(DbErr::Custom("余额预警事件必须通过仓储更新".to_owned()));
        }
        let status = self.status.try_as_ref().copied().unwrap_or_default();
        let attempts = self.attempt_count.try_as_ref().copied().unwrap_or_default();
        let lease = self.lease_expires_at.try_as_ref().copied().flatten();
        let sent_at = self.sent_at.try_as_ref().copied().flatten();
        let last_error = self.last_error_kind.try_as_ref().copied().flatten();
        let shape_valid = match status {
            1 => {
                (0..=4).contains(&attempts)
                    && lease.is_none()
                    && sent_at.is_none()
                    && ((attempts == 0 && last_error.is_none())
                        || (attempts > 0 && last_error.is_some()))
            }
            2 => (1..=5).contains(&attempts) && lease.is_some() && sent_at.is_none(),
            3 => (1..=5).contains(&attempts) && lease.is_none() && sent_at.is_some(),
            4 => (1..=5).contains(&attempts) && lease.is_none() && sent_at.is_none(),
            5 => lease.is_none() && sent_at.is_none(),
            _ => false,
        };
        if self.user_id.try_as_ref().copied().unwrap_or_default() <= 0
            || self
                .window_started_at_epoch
                .try_as_ref()
                .copied()
                .unwrap_or(-1)
                < 0
            || self
                .threshold_quota
                .try_as_ref()
                .copied()
                .unwrap_or_default()
                <= 0
            || self.observed_quota.try_as_ref().copied().unwrap_or(-1) < 0
            || self.version.try_as_ref().copied().unwrap_or_default() != 1
            || self.next_attempt_at.try_as_ref().is_none()
            || self.created_at.try_as_ref().is_none()
            || self.updated_at.try_as_ref().is_none()
            || last_error.is_some_and(|kind| !(1..=3).contains(&kind))
            || !shape_valid
        {
            return Err(DbErr::Custom("余额预警事件状态无效".to_owned()));
        }
        Ok(self)
    }
}

#[async_trait::async_trait]
impl ActiveModelBehavior for subscription_balance_alert_events::ActiveModel {
    async fn before_save<C>(self, _database: &C, insert: bool) -> Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        // 投递状态只能由订阅预警仓储使用带版本条件的批量更新推进。
        if !insert {
            return Err(DbErr::Custom("订阅预警事件必须通过仓储更新".to_owned()));
        }
        let status = self.status.try_as_ref().copied().unwrap_or_default();
        let attempts = self.attempt_count.try_as_ref().copied().unwrap_or_default();
        let lease = self.lease_expires_at.try_as_ref().copied().flatten();
        let sent_at = self.sent_at.try_as_ref().copied().flatten();
        let last_error = self.last_error_kind.try_as_ref().copied().flatten();
        let window_started_at = self.window_started_at.try_as_ref().copied();
        let window_ends_at = self.window_ends_at.try_as_ref().copied();
        let quota_amount = self.quota_amount.try_as_ref().copied().unwrap_or_default();
        let observed_quota_used = self.observed_quota_used.try_as_ref().copied().unwrap_or(-1);
        let shape_valid = match status {
            1 => {
                (0..=4).contains(&attempts)
                    && lease.is_none()
                    && sent_at.is_none()
                    && ((attempts == 0 && last_error.is_none())
                        || (attempts > 0 && last_error.is_some()))
            }
            2 => (1..=5).contains(&attempts) && lease.is_some() && sent_at.is_none(),
            3 => (1..=5).contains(&attempts) && lease.is_none() && sent_at.is_some(),
            4 => (1..=5).contains(&attempts) && lease.is_none() && sent_at.is_none(),
            5 => lease.is_none() && sent_at.is_none(),
            _ => false,
        };
        if self
            .user_subscription_id
            .try_as_ref()
            .copied()
            .unwrap_or_default()
            <= 0
            || self.user_id.try_as_ref().copied().unwrap_or_default() <= 0
            || window_started_at.is_none()
            || window_ends_at.is_none()
            || window_ends_at <= window_started_at
            || !(1..=99).contains(
                &self
                    .threshold_percent
                    .try_as_ref()
                    .copied()
                    .unwrap_or_default(),
            )
            || quota_amount <= 0
            || !(0..=quota_amount).contains(&observed_quota_used)
            || self.version.try_as_ref().copied().unwrap_or_default() != 1
            || self.next_attempt_at.try_as_ref().is_none()
            || self.created_at.try_as_ref().is_none()
            || self.updated_at.try_as_ref().is_none()
            || last_error.is_some_and(|kind| !(1..=3).contains(&kind))
            || !shape_valid
        {
            return Err(DbErr::Custom("订阅预警事件状态无效".to_owned()));
        }
        Ok(self)
    }
}

#[async_trait::async_trait]
impl ActiveModelBehavior for topup_payment_events::ActiveModel {
    async fn before_save<C>(self, database: &C, insert: bool) -> Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        validate_topup_payment_event(&self, database, insert).await?;
        Ok(self)
    }
}

#[async_trait::async_trait]
impl ActiveModelBehavior for subscription_payment_events::ActiveModel {
    async fn before_save<C>(self, database: &C, insert: bool) -> Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        validate_subscription_payment_event(&self, database, insert).await?;
        Ok(self)
    }
}

#[async_trait::async_trait]
impl ActiveModelBehavior for refund_provider_events::ActiveModel {}

#[async_trait::async_trait]
impl ActiveModelBehavior for refund_manual_completions::ActiveModel {}

#[async_trait::async_trait]
impl ActiveModelBehavior for refund_reconciliation_entries::ActiveModel {
    async fn before_save<C>(self, _database: &C, insert: bool) -> Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        // 成功退款对账事实只能追加，且订阅订单永远不能伪造企业资金主体。
        if !insert {
            return Err(DbErr::Custom("退款对账事实不允许更新".to_owned()));
        }
        let id = self.id.try_as_ref().copied().unwrap_or_default();
        let request_key = self.request_key.try_as_ref();
        let provider_event_id = self.provider_event_id.try_as_ref().copied().flatten();
        let manual_completion_id = self.manual_completion_id.try_as_ref().copied().flatten();
        let user_id = self.user_id.try_as_ref().copied().unwrap_or_default();
        let organization_id = self.organization_id.try_as_ref().copied().flatten();
        let approval_actor_id = self
            .approval_actor_id
            .try_as_ref()
            .copied()
            .unwrap_or_default();
        let order_kind = self.order_kind.try_as_ref().copied().unwrap_or_default();
        let order_key = self.order_key.try_as_ref();
        let provider = self.provider.try_as_ref();
        let amount_delta_minor = self
            .amount_delta_minor
            .try_as_ref()
            .copied()
            .unwrap_or_default();
        let currency = self.currency.try_as_ref();
        if id != 0
            || request_key.is_none_or(|value| !valid_hex(value, 32, true))
            || (provider_event_id.is_some() == manual_completion_id.is_some())
            || provider_event_id.is_some_and(|value| value <= 0)
            || manual_completion_id.is_some_and(|value| value <= 0)
            || user_id <= 0
            || organization_id.is_some_and(|value| value <= 0)
            || approval_actor_id <= 0
            || !matches!(order_kind, 1 | 2)
            || (order_kind == 2 && organization_id.is_some())
            || order_key.is_none_or(|value| !valid_hex(value, 32, true))
            || provider.is_none_or(|value| {
                value.is_empty()
                    || value.len() > 64
                    || !value.bytes().all(|byte| {
                        byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
                    })
            })
            || amount_delta_minor >= 0
            || amount_delta_minor == i64::MIN
            || currency.is_none_or(|value| {
                value.len() != 3 || !value.bytes().all(|byte| byte.is_ascii_uppercase())
            })
            || self.created_at.try_as_ref().is_none()
        {
            return Err(DbErr::Custom("退款对账事实字段无效".to_owned()));
        }
        Ok(self)
    }
}

#[async_trait::async_trait]
impl ActiveModelBehavior for redemption_batches::ActiveModel {
    async fn before_save<C>(self, database: &C, insert: bool) -> Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        validate_redemption_batch(&self, database, insert).await?;
        Ok(self)
    }
}

#[async_trait::async_trait]
impl ActiveModelBehavior for redemption_codes::ActiveModel {
    async fn before_save<C>(self, database: &C, insert: bool) -> Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        validate_redemption_code(&self, database, insert).await?;
        Ok(self)
    }
}

#[async_trait::async_trait]
impl ActiveModelBehavior for invite_rebate_events::ActiveModel {
    async fn before_save<C>(self, database: &C, insert: bool) -> Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        validate_invite_rebate_event(&self, database, insert).await?;
        Ok(self)
    }
}

#[async_trait::async_trait]
impl ActiveModelBehavior for subscription_plans::ActiveModel {
    async fn before_save<C>(self, database: &C, insert: bool) -> Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        validate_subscription_plan(&self, database, insert).await?;
        Ok(self)
    }
}

#[async_trait::async_trait]
impl ActiveModelBehavior for user_subscriptions::ActiveModel {
    async fn before_save<C>(self, database: &C, insert: bool) -> Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        validate_user_subscription(&self, database, insert).await?;
        Ok(self)
    }
}

#[async_trait::async_trait]
impl ActiveModelBehavior for model_sync_runs::ActiveModel {}

#[async_trait::async_trait]
impl ActiveModelBehavior for model_sync_items::ActiveModel {}

#[async_trait::async_trait]
impl ActiveModelBehavior for model_provider_catalog::ActiveModel {}

#[async_trait::async_trait]
impl ActiveModelBehavior for custom_oauth2_providers::ActiveModel {}

#[async_trait::async_trait]
impl ActiveModelBehavior for usage_logs::ActiveModel {
    async fn before_save<C>(self, database: &C, insert: bool) -> Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        validate_usage_log(&self, database, insert).await?;
        Ok(self)
    }
}

#[async_trait::async_trait]
impl ActiveModelBehavior for platform_audit_logs::ActiveModel {
    async fn before_save<C>(self, database: &C, insert: bool) -> Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        validate_platform_audit_log(&self, database, insert).await?;
        Ok(self)
    }
}

#[async_trait::async_trait]
impl ActiveModelBehavior for request_outcome_logs::ActiveModel {
    async fn before_save<C>(self, _database: &C, insert: bool) -> Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        if !insert {
            return Err(DbErr::Custom("请求终态事实只允许追加".to_owned()));
        }
        let request_id = self.request_id.try_as_ref().map(String::as_str);
        let protocol = self.protocol.try_as_ref().map(String::as_str);
        let operation = self.operation.try_as_ref().map(String::as_str);
        let model = self.model.try_as_ref().map(String::as_str);
        let outcome = self.outcome.try_as_ref().copied();
        let error_kind = self.error_kind.try_as_ref().and_then(Option::as_deref);
        let user_id = self.user_id.try_as_ref().copied().flatten();
        let token_id = self.token_id.try_as_ref().copied().flatten();
        let group_id = self.group_id.try_as_ref().copied().flatten();
        let organization_id = self.organization_id.try_as_ref().copied().flatten();
        let organization_team_id = self.organization_team_id.try_as_ref().copied().flatten();
        let public_error_code = self
            .public_error_code
            .try_as_ref()
            .and_then(Option::as_deref);
        let public_error_message = self
            .public_error_message
            .try_as_ref()
            .and_then(Option::as_deref);
        let subject_present = [
            user_id,
            token_id,
            group_id,
            organization_id,
            organization_team_id,
        ]
        .into_iter()
        .any(|value| value.is_some());
        let subject_complete = user_id.is_some()
            && token_id.is_some()
            && group_id.is_some()
            && (organization_team_id.is_none() || organization_id.is_some());
        if request_id.is_none_or(|value| !valid_request_outcome_text(value, 128))
            || protocol.is_none_or(|value| value.parse::<af_domain::Protocol>().is_err())
            || operation.is_none_or(|value| value.parse::<af_domain::Operation>().is_err())
            || model.is_none_or(|value| !valid_request_outcome_text(value, 255))
            || self.duration_ms.try_as_ref().is_none_or(|value| *value < 0)
            || self.created_at.try_as_ref().is_none()
            || !matches!((outcome, error_kind), (Some(1), None) | (Some(2), Some(_)))
            || error_kind.is_some_and(|value| value.parse::<crate::RequestFailureKind>().is_err())
            || subject_present && !subject_complete
            || [
                user_id,
                token_id,
                group_id,
                organization_id,
                organization_team_id,
            ]
            .into_iter()
            .flatten()
            .any(|value| value <= 0)
            || (organization_id.is_none() && organization_team_id.is_some())
            || (outcome == Some(1)
                && (public_error_code.is_some() || public_error_message.is_some()))
            || (outcome == Some(2)
                && (public_error_code.is_some() != public_error_message.is_some()
                    || public_error_code
                        .is_some_and(|value| !valid_request_outcome_text(value, 64))
                    || public_error_message
                        .is_some_and(|value| !valid_request_outcome_text(value, 255))))
        {
            return Err(DbErr::Custom("请求终态事实字段无效".to_owned()));
        }
        Ok(self)
    }
}

fn valid_request_outcome_text(value: &str, max_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_bytes
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

#[async_trait::async_trait]
impl ActiveModelBehavior for wallet_ledger_entries::ActiveModel {
    async fn before_save<C>(self, database: &C, insert: bool) -> Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        validate_wallet_ledger_entry(&self, database, insert).await?;
        Ok(self)
    }
}

#[async_trait::async_trait]
impl ActiveModelBehavior for playground_shares::ActiveModel {}

#[async_trait::async_trait]
impl ActiveModelBehavior for playground_conversations::ActiveModel {}

pub(crate) mod account_verification_materials;
pub(crate) mod account_verification_settings;
pub(crate) mod account_verifications;
