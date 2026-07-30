//! SeaORM 持久化实体，仅供 af-db 内部仓储与迁移使用。
//!
//! `deleted_at` 只表示持久化墓碑。仓储层执行软删除时必须在同一事务内撤销依赖的
//! 令牌、凭据与能力索引；所有有效性查询也必须联合检查所属用户、分组和渠道。

// 仓储层尚未落地前，部分实体仅用于声明稳定表结构；保持模块私有并局部关闭死代码告警。
#![allow(dead_code)]

use rust_decimal::Decimal;
use sea_orm::{ActiveValue, EntityTrait, Set, entity::prelude::*};

pub(crate) mod abilities;
pub(crate) mod billing_batch_checkpoints;
pub(crate) mod billing_reservations;
pub(crate) mod channel_groups;
pub(crate) mod channel_models;
pub(crate) mod channels;
pub(crate) mod credentials;
pub(crate) mod group_model_ratios;
pub(crate) mod groups;
pub(crate) mod model_prices;
pub(crate) mod options;
pub(crate) mod playground_conversations;
pub(crate) mod playground_shares;
pub(crate) mod registration_rate_limits;
pub(crate) mod tokens;
pub(crate) mod usage_logs;
pub(crate) mod users;

mod sensitive;

pub(crate) use sensitive::{
    BillingBatchFingerprint, BillingBatchWriterKey, BillingReservationKey, ChannelBaseUrl,
    EncryptedJson, HeaderOverrides, MAX_TOKEN_IP_ALLOWLIST_SERIALIZED_BYTES,
    MAX_TOKEN_MODEL_ALLOWLIST_SERIALIZED_BYTES, PasswordHash, PlaygroundConversationKey,
    SensitiveDecimal, SensitiveJson, SensitiveString, TokenHash, TokenIpAllowlist,
    TokenModelAllowlist,
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

async fn validate_group<C>(
    model: &groups::ActiveModel,
    database: &C,
    insert: bool,
) -> Result<(), DbErr>
where
    C: ConnectionTrait,
{
    validate_group_pricing(model, database, insert).await?;
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
    let version = model
        .version
        .try_as_ref()
        .copied()
        .or_else(|| persisted.as_ref().map(|record| record.version))
        .unwrap_or(1);
    if !matches!(billing_mode, 1 | 2)
        || version < 1
        || prices.iter().any(|price| *price < Decimal::ZERO)
        || (billing_mode == 2 && prices.iter().any(|price| !price.is_zero()))
    {
        return Err(DbErr::Custom("模型价格状态无效".to_owned()));
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
    let valid_modes = matches!(model.billing_mode.try_as_ref(), Some(1 | 2));
    let valid_event_type = matches!(model.event_type.try_as_ref(), Some(1));
    let valid_source = matches!(model.usage_source.try_as_ref(), Some(1 | 2));
    let valid_semantics = matches!(model.usage_semantics.try_as_ref(), Some(1 | 2));
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
        || !non_negative
    {
        return Err(DbErr::Custom("用量日志状态无效".to_owned()));
    }
    Ok(())
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
    billing_batch_checkpoints::ActiveModel => validate_default,
    billing_reservations::ActiveModel => validate_default,
    channel_groups::ActiveModel => validate_default,
    channel_models::ActiveModel => validate_default,
    channels::ActiveModel => validate_default,
    credentials::ActiveModel => validate_credential,
    group_model_ratios::ActiveModel => validate_group_model_ratio,
    groups::ActiveModel => validate_group,
    model_prices::ActiveModel => validate_model_price,
    options::ActiveModel => validate_default,
    registration_rate_limits::ActiveModel => validate_default,
    tokens::ActiveModel => validate_default,
    users::ActiveModel => validate_user,
);

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
impl ActiveModelBehavior for playground_shares::ActiveModel {}

#[async_trait::async_trait]
impl ActiveModelBehavior for playground_conversations::ActiveModel {}
