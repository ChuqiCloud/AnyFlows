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
pub(crate) mod groups;
pub(crate) mod model_prices;
pub(crate) mod options;
pub(crate) mod tokens;
pub(crate) mod users;

mod sensitive;

pub(crate) use sensitive::{
    BillingBatchFingerprint, BillingBatchWriterKey, BillingReservationKey, ChannelBaseUrl,
    EncryptedJson, HeaderOverrides, MAX_TOKEN_IP_ALLOWLIST_SERIALIZED_BYTES,
    MAX_TOKEN_MODEL_ALLOWLIST_SERIALIZED_BYTES, PasswordHash, SensitiveDecimal, SensitiveJson,
    SensitiveString, TokenHash, TokenIpAllowlist, TokenModelAllowlist,
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
    let Some(reference_id) = changed_reference(&model.fallback_group_id, insert) else {
        return Ok(());
    };
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
    Ok(())
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
    groups::ActiveModel => validate_group,
    model_prices::ActiveModel => validate_model_price,
    options::ActiveModel => validate_default,
    tokens::ActiveModel => validate_default,
    users::ActiveModel => validate_user,
);
