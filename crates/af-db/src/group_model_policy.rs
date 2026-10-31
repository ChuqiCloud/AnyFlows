//! 分组可选模型白名单持久化与鉴权快照读取。

use std::{fmt, time::Duration};

use af_domain::{GroupId, TokenModelPolicy};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DbBackend, EntityTrait, QueryFilter, QueryOrder, QuerySelect,
    Set, TransactionTrait,
    sea_query::{Expr, LockType, OnConflict},
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool,
    entity::{group_model_policies, group_model_ratio_overrides, groups},
};

/// 单个来源分组到当前目标分组的模型倍率覆盖。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GroupModelRatioOverridePolicyRecord {
    pub source_group_id: GroupId,
    pub target_group_id: GroupId,
    pub model: String,
    pub ratio_micros: i64,
}

/// 一次策略写入允许指定的最大模型倍率覆盖数。
pub const MAX_GROUP_MODEL_RATIO_OVERRIDES: usize = 1_024;

/// 未配置时不限制模型；配置时仅允许列出的 Canonical 模型名。
#[derive(Clone, Eq, PartialEq)]
pub struct GroupModelPolicyRecord {
    pub group_id: GroupId,
    pub allowed_models: Option<Vec<String>>,
    pub ratio_overrides: Vec<GroupModelRatioOverridePolicyRecord>,
}

impl fmt::Debug for GroupModelPolicyRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GroupModelPolicyRecord")
            .field("group_id", &self.group_id)
            .field("restricted", &self.allowed_models.is_some())
            .field("model_count", &self.allowed_models.as_ref().map(Vec::len))
            .field("ratio_override_count", &self.ratio_overrides.len())
            .finish()
    }
}

/// 分组模型策略仓储错误。
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum GroupModelPolicyRepositoryError {
    #[error("分组模型策略输入无效")]
    InvalidInput,
    #[error("分组不存在")]
    NotFound,
    #[error("分组模型策略数据库访问失败")]
    Internal,
}

/// 管理分组模型白名单与网关读取策略共用的仓储。
#[derive(Clone)]
pub struct GroupModelPolicyRepository {
    pool: DatabasePool,
    lookup_timeout: Duration,
}

impl GroupModelPolicyRepository {
    pub fn new(
        pool: DatabasePool,
        lookup_timeout: Duration,
    ) -> Result<Self, GroupModelPolicyRepositoryError> {
        if lookup_timeout.is_zero() {
            return Err(GroupModelPolicyRepositoryError::InvalidInput);
        }
        Ok(Self {
            pool,
            lookup_timeout,
        })
    }

    /// 返回分组当前白名单；缺少记录表示不限制。
    pub async fn get(
        &self,
        group_id: GroupId,
    ) -> Result<GroupModelPolicyRecord, GroupModelPolicyRepositoryError> {
        self.bounded(async move {
            if !active_group_exists(self.pool.connection(), group_id).await? {
                return Err(GroupModelPolicyRepositoryError::NotFound);
            }
            let policy = group_model_policies::Entity::find_by_id(group_id.get())
                .one(self.pool.connection())
                .await
                .map_err(internal)?;
            let ratio_overrides = load_ratio_overrides(self.pool.connection(), group_id).await?;
            Ok(GroupModelPolicyRecord {
                group_id,
                allowed_models: policy
                    .map(|record| {
                        record
                            .allowed_models
                            .into_raw()
                            .as_array()
                            .ok_or(GroupModelPolicyRepositoryError::Internal)?
                            .iter()
                            .map(|value| {
                                value
                                    .as_str()
                                    .map(str::to_owned)
                                    .ok_or(GroupModelPolicyRepositoryError::Internal)
                            })
                            .collect::<Result<Vec<_>, _>>()
                    })
                    .transpose()?,
                ratio_overrides,
            })
        })
        .await
    }

    /// 原子替换白名单；`None` 删除策略并恢复不限制。
    pub async fn replace(
        &self,
        group_id: GroupId,
        allowed_models: Option<Vec<String>>,
    ) -> Result<GroupModelPolicyRecord, GroupModelPolicyRepositoryError> {
        self.replace_with_ratio_overrides(group_id, allowed_models, None)
            .await
    }

    /// 原子替换白名单与当前目标分组的全部模型倍率覆盖。
    pub async fn replace_with_ratio_overrides(
        &self,
        group_id: GroupId,
        allowed_models: Option<Vec<String>>,
        ratio_overrides: Option<Vec<GroupModelRatioOverridePolicyRecord>>,
    ) -> Result<GroupModelPolicyRecord, GroupModelPolicyRepositoryError> {
        allowed_models
            .as_ref()
            .map(|models| {
                TokenModelPolicy::try_from_allowlist(models.clone())
                    .map_err(|_| GroupModelPolicyRepositoryError::InvalidInput)
            })
            .transpose()?;
        if let Some(ratio_overrides) = &ratio_overrides {
            validate_ratio_overrides(ratio_overrides)?;
            if ratio_overrides
                .iter()
                .any(|record| record.target_group_id != group_id)
            {
                return Err(GroupModelPolicyRepositoryError::InvalidInput);
            }
        }
        let mut groups_to_lock = ratio_overrides
            .as_ref()
            .into_iter()
            .flatten()
            .map(|record| record.source_group_id)
            .collect::<Vec<_>>();
        groups_to_lock.push(group_id);
        groups_to_lock.sort_unstable();
        groups_to_lock.dedup();
        self.bounded(async move {
            let transaction = self.pool.connection().begin().await.map_err(internal)?;
            for locked_group_id in groups_to_lock {
                if !active_group_exists_tx(&transaction, locked_group_id).await? {
                    return Err(if locked_group_id == group_id {
                        GroupModelPolicyRepositoryError::NotFound
                    } else {
                        GroupModelPolicyRepositoryError::InvalidInput
                    });
                }
            }
            match &allowed_models {
                Some(models) => {
                    let models_json = serde_json::json!(models);
                    let allowlist = crate::entity::TokenModelAllowlist::validate(models_json)
                        .map_err(|_| GroupModelPolicyRepositoryError::InvalidInput)?;
                    group_model_policies::Entity::insert(group_model_policies::ActiveModel {
                        group_id: Set(group_id.get()),
                        allowed_models: Set(allowlist),
                    })
                    .on_conflict(
                        OnConflict::column(group_model_policies::Column::GroupId)
                            .update_column(group_model_policies::Column::AllowedModels)
                            .to_owned(),
                    )
                    .exec_without_returning(&transaction)
                    .await
                    .map_err(internal)?;
                }
                None => {
                    group_model_policies::Entity::delete_by_id(group_id.get())
                        .exec(&transaction)
                        .await
                        .map_err(internal)?;
                }
            }
            if let Some(ratio_overrides) = &ratio_overrides {
                group_model_ratio_overrides::Entity::delete_many()
                    .filter(group_model_ratio_overrides::Column::TargetGroupId.eq(group_id.get()))
                    .exec(&transaction)
                    .await
                    .map_err(internal)?;
                for override_record in ratio_overrides {
                    group_model_ratio_overrides::ActiveModel {
                        source_group_id: Set(override_record.source_group_id.get()),
                        target_group_id: Set(group_id.get()),
                        model: Set(override_record.model.clone()),
                        ratio_micros: Set(override_record.ratio_micros),
                        ..Default::default()
                    }
                    .insert(&transaction)
                    .await
                    .map_err(internal)?;
                }
            }
            let ratio_overrides = load_ratio_overrides(&transaction, group_id).await?;
            transaction.commit().await.map_err(internal)?;
            Ok(GroupModelPolicyRecord {
                group_id,
                allowed_models,
                ratio_overrides,
            })
        })
        .await
    }

    /// 令牌鉴权和目录读取共用的 fail-closed 模型策略快照。
    pub async fn policy_for_group(
        &self,
        group_id: GroupId,
    ) -> Result<TokenModelPolicy, GroupModelPolicyRepositoryError> {
        self.get(group_id).await?.allowed_models.map_or_else(
            || Ok(TokenModelPolicy::unrestricted()),
            |models| {
                TokenModelPolicy::try_from_allowlist(models)
                    .map_err(|_| GroupModelPolicyRepositoryError::Internal)
            },
        )
    }

    async fn bounded<T>(
        &self,
        operation: impl std::future::Future<Output = Result<T, GroupModelPolicyRepositoryError>>,
    ) -> Result<T, GroupModelPolicyRepositoryError> {
        timeout(
            self.lookup_timeout,
            operation.with_subscriber(NoSubscriber::default()),
        )
        .await
        .map_err(|_| GroupModelPolicyRepositoryError::Internal)?
    }
}

async fn load_ratio_overrides<C: sea_orm::ConnectionTrait>(
    connection: &C,
    target_group_id: GroupId,
) -> Result<Vec<GroupModelRatioOverridePolicyRecord>, GroupModelPolicyRepositoryError> {
    let rows = group_model_ratio_overrides::Entity::find()
        .filter(group_model_ratio_overrides::Column::TargetGroupId.eq(target_group_id.get()))
        .order_by_asc(group_model_ratio_overrides::Column::SourceGroupId)
        .order_by_asc(group_model_ratio_overrides::Column::Model)
        .all(connection)
        .await
        .map_err(internal)?;
    if rows.len() > MAX_GROUP_MODEL_RATIO_OVERRIDES {
        return Err(GroupModelPolicyRepositoryError::Internal);
    }
    let mut records = Vec::with_capacity(rows.len());
    for row in rows {
        if row.ratio_micros < 0
            || !crate::model_price::is_valid_model_ratio_override_name(&row.model)
        {
            return Err(GroupModelPolicyRepositoryError::Internal);
        }
        let source_group_id = GroupId::new(row.source_group_id)
            .map_err(|_| GroupModelPolicyRepositoryError::Internal)?;
        if !active_group_exists(connection, source_group_id).await? {
            return Err(GroupModelPolicyRepositoryError::Internal);
        }
        records.push(GroupModelRatioOverridePolicyRecord {
            source_group_id,
            target_group_id: GroupId::new(row.target_group_id)
                .map_err(|_| GroupModelPolicyRepositoryError::Internal)?,
            model: row.model,
            ratio_micros: row.ratio_micros,
        });
    }
    Ok(records)
}

fn validate_ratio_overrides(
    ratio_overrides: &[GroupModelRatioOverridePolicyRecord],
) -> Result<(), GroupModelPolicyRepositoryError> {
    if ratio_overrides.len() > MAX_GROUP_MODEL_RATIO_OVERRIDES {
        return Err(GroupModelPolicyRepositoryError::InvalidInput);
    }
    let mut keys = std::collections::BTreeSet::new();
    for record in ratio_overrides {
        if record.target_group_id.get() <= 0
            || record.source_group_id.get() <= 0
            || record.ratio_micros < 0
            || !crate::model_price::is_valid_model_ratio_override_name(&record.model)
            || !keys.insert((record.source_group_id, record.model.as_str()))
        {
            return Err(GroupModelPolicyRepositoryError::InvalidInput);
        }
    }
    Ok(())
}

async fn active_group_exists<C: sea_orm::ConnectionTrait>(
    connection: &C,
    group_id: GroupId,
) -> Result<bool, GroupModelPolicyRepositoryError> {
    groups::Entity::find_by_id(group_id.get())
        .select_only()
        .column(groups::Column::Id)
        .filter(groups::Column::DeletedAt.is_null())
        .into_tuple::<i64>()
        .one(connection)
        .await
        .map(|row| row.is_some())
        .map_err(internal)
}

async fn active_group_exists_tx<C: sea_orm::ConnectionTrait>(
    connection: &C,
    group_id: GroupId,
) -> Result<bool, GroupModelPolicyRepositoryError> {
    if connection.get_database_backend() == DbBackend::Sqlite {
        groups::Entity::update_many()
            .filter(groups::Column::Id.eq(group_id.get()))
            .filter(groups::Column::DeletedAt.is_null())
            .col_expr(
                groups::Column::RatioMicros,
                Expr::col(groups::Column::RatioMicros).into(),
            )
            .exec(connection)
            .await
            .map_err(internal)?;
    }
    let mut query = groups::Entity::find_by_id(group_id.get())
        .select_only()
        .column(groups::Column::Id)
        .filter(groups::Column::DeletedAt.is_null());
    if connection.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .into_tuple::<i64>()
        .one(connection)
        .await
        .map(|row| row.is_some())
        .map_err(internal)
}

fn internal(_: sea_orm::DbErr) -> GroupModelPolicyRepositoryError {
    GroupModelPolicyRepositoryError::Internal
}
