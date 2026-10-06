use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    sync::Arc,
    time::Duration,
};

use af_domain::{ChannelId, ProxyId, Status};
use sea_orm::{
    ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect,
    entity::prelude::TimeDateTimeWithTimeZone,
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool, SchedulerAbilityRecord, SchedulerAbilityRepository,
    SchedulerAbilityRepositoryError, SchedulerCatalogSubject,
    entity::{channels, credentials, proxies},
};

use super::{
    SchedulerRuntimeRecord,
    selection::{SchedulerRuntimeSelectionError, select_supported_targets},
    source::{
        SchedulerRuntimeChannelSource, SchedulerRuntimeCredentialSource,
        SchedulerRuntimeSourceError, proxy_from_entity,
    },
};

const DEFAULT_LOAD_TIMEOUT: Duration = Duration::from_secs(5);
/// 一次运行时目录重建允许读取的凭据总数。
pub const MAX_SCHEDULER_RUNTIME_CREDENTIAL_ENTRIES: usize = 200_000;
const RUNTIME_PARENT_QUERY_BATCH_SIZE: usize = 500;

/// 运行时渠道目录读取失败；错误不携带 URL、Header、模型或凭据内容。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum SchedulerRuntimeRepositoryError {
    /// 查询截止时间配置为零。
    #[error("运行时渠道目录查询超时必须大于零")]
    InvalidConfiguration,
    /// 基础能力目录读取失败。
    #[error("读取运行时渠道能力失败")]
    Ability(#[source] SchedulerAbilityRepositoryError),
    /// 获取连接或读取渠道、凭据失败。
    #[error("读取运行时渠道目录失败")]
    Query,
    /// 读取目录超过硬截止时间。
    #[error("读取运行时渠道目录超时")]
    Timeout,
    /// 标识、类型、密文或 Header 违反持久化不变量。
    #[error("运行时渠道目录持久化状态损坏")]
    Invariant,
}

impl From<SchedulerRuntimeSelectionError> for SchedulerRuntimeRepositoryError {
    fn from(error: SchedulerRuntimeSelectionError) -> Self {
        match error {
            SchedulerRuntimeSelectionError::Invariant => Self::Invariant,
        }
    }
}

impl From<SchedulerRuntimeSourceError> for SchedulerRuntimeRepositoryError {
    fn from(error: SchedulerRuntimeSourceError) -> Self {
        match error {
            SchedulerRuntimeSourceError::Invariant => Self::Invariant,
        }
    }
}

/// 全量读取生产可用能力、渠道配置与加密凭据的数据库仓储。
#[derive(Clone)]
pub struct SchedulerRuntimeRepository {
    pool: DatabasePool,
    abilities: SchedulerAbilityRepository,
    load_timeout: Duration,
}

impl SchedulerRuntimeRepository {
    /// 使用默认五秒截止时间创建仓储。
    #[must_use]
    pub fn new(pool: DatabasePool) -> Self {
        Self {
            abilities: SchedulerAbilityRepository::new(pool.clone()),
            pool,
            load_timeout: DEFAULT_LOAD_TIMEOUT,
        }
    }

    /// 使用显式非零截止时间创建仓储。
    pub fn with_load_timeout(
        pool: DatabasePool,
        load_timeout: Duration,
    ) -> Result<Self, SchedulerRuntimeRepositoryError> {
        if load_timeout.is_zero() {
            return Err(SchedulerRuntimeRepositoryError::InvalidConfiguration);
        }
        let abilities = SchedulerAbilityRepository::with_load_timeout(pool.clone(), load_timeout)
            .map_err(|_| SchedulerRuntimeRepositoryError::InvalidConfiguration)?;
        Ok(Self {
            pool,
            abilities,
            load_timeout,
        })
    }

    /// 全量读取生产转发可用的能力与运行时目标。
    ///
    /// 没有当前支持的渠道类型、协议或凭据时，该渠道能力会被 fail-closed 地排除，
    /// 避免请求侧拿到无法安全装配的候选后再回退静态上游。
    pub async fn load_all(
        &self,
    ) -> Result<Vec<SchedulerRuntimeRecord>, SchedulerRuntimeRepositoryError> {
        let abilities = self
            .abilities
            .load_all()
            .await
            .map_err(SchedulerRuntimeRepositoryError::Ability)?;
        self.load_records(abilities).await
    }

    /// 按闭合渠道或分组主体读取当前运行时投影；删除后的主体返回空集合。
    pub async fn load_subject(
        &self,
        subject: SchedulerCatalogSubject,
    ) -> Result<Vec<SchedulerRuntimeRecord>, SchedulerRuntimeRepositoryError> {
        let abilities = self
            .abilities
            .load_subject(subject)
            .await
            .map_err(SchedulerRuntimeRepositoryError::Ability)?;
        let records = self.load_records(abilities).await?;
        if records.iter().any(|record| {
            let ability = record.ability();
            match subject {
                SchedulerCatalogSubject::Channel(channel_id) => ability.channel_id() != channel_id,
                SchedulerCatalogSubject::Group(group_id) => ability.group_id() != group_id,
            }
        }) {
            return Err(record_internal_error(
                SchedulerRuntimeRepositoryError::Invariant,
            ));
        }
        Ok(records)
    }

    async fn load_records(
        &self,
        abilities: Vec<SchedulerAbilityRecord>,
    ) -> Result<Vec<SchedulerRuntimeRecord>, SchedulerRuntimeRepositoryError> {
        if abilities.is_empty() {
            return Ok(Vec::new());
        }

        let raw_channel_ids = abilities
            .iter()
            .map(SchedulerAbilityRecord::channel_id)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(|channel_id| channel_id.get())
            .collect::<Vec<_>>();
        let operation = async {
            let channels = channels::Entity::find()
                .filter(channels::Column::Id.is_in(raw_channel_ids.clone()))
                .filter(channels::Column::Status.eq(Status::Enabled.code()))
                .filter(channels::Column::DeletedAt.is_null())
                .all(self.pool.connection())
                .await
                .map_err(|_| SchedulerRuntimeRepositoryError::Query)?
                .into_iter()
                .try_fold(BTreeMap::new(), collect_channel_source)?;
            let credentials = credentials::Entity::find()
                .filter(credentials::Column::ChannelId.is_in(raw_channel_ids))
                .filter(credentials::Column::Status.eq(Status::Enabled.code()))
                .filter(credentials::Column::Schedulable.eq(true))
                .filter(credentials::Column::OauthTokenPending.eq(false))
                .filter(credentials::Column::DeletedAt.is_null())
                .order_by_asc(credentials::Column::ChannelId)
                .order_by_desc(credentials::Column::Priority)
                .order_by_asc(credentials::Column::Id)
                .limit(runtime_credential_query_limit()?)
                .all(self.pool.connection())
                .await
                .map_err(|_| SchedulerRuntimeRepositoryError::Query)?;
            if credentials.len() > MAX_SCHEDULER_RUNTIME_CREDENTIAL_ENTRIES {
                return Err(SchedulerRuntimeRepositoryError::Invariant);
            }
            let parent_ids = credentials
                .iter()
                .filter_map(|credential| credential.parent_id)
                .collect::<BTreeSet<_>>();
            let parent_records =
                load_parent_credentials(self.pool.connection(), &parent_ids).await?;
            if parent_records.len() != parent_ids.len() {
                return Err(SchedulerRuntimeRepositoryError::Invariant);
            }
            let proxy_ids = credentials
                .iter()
                .filter_map(|credential| match credential.parent_id {
                    Some(parent_id) => parent_records
                        .get(&parent_id)
                        .and_then(|parent| parent.proxy_id),
                    None => credential.proxy_id,
                })
                .collect::<BTreeSet<_>>();
            let proxy_records = if proxy_ids.is_empty() {
                BTreeMap::new()
            } else {
                proxies::Entity::find()
                    .filter(proxies::Column::Id.is_in(proxy_ids.iter().copied()))
                    .filter(proxies::Column::Enabled.eq(true))
                    .filter(proxies::Column::DeletedAt.is_null())
                    .all(self.pool.connection())
                    .await
                    .map_err(|_| SchedulerRuntimeRepositoryError::Query)?
                    .into_iter()
                    .try_fold(BTreeMap::new(), |mut records, proxy| {
                        let record = proxy_from_entity(proxy)?;
                        if records.insert(record.proxy_id(), record).is_some() {
                            return Err(SchedulerRuntimeRepositoryError::Invariant);
                        }
                        Ok(records)
                    })?
            };
            if proxy_records.len() != proxy_ids.len() {
                return Err(SchedulerRuntimeRepositoryError::Invariant);
            }
            let now = TimeDateTimeWithTimeZone::now_utc();
            let credentials =
                credentials
                    .into_iter()
                    .try_fold(Vec::new(), |sources, credential| {
                        let parent = credential
                            .parent_id
                            .map(|parent_id| {
                                parent_records
                                    .get(&parent_id)
                                    .cloned()
                                    .ok_or(SchedulerRuntimeRepositoryError::Invariant)
                            })
                            .transpose()?;
                        let effective_proxy_id = parent
                            .as_ref()
                            .map_or(credential.proxy_id, |parent| parent.proxy_id);
                        let proxy = effective_proxy_id
                            .map(|proxy_id| {
                                ProxyId::new(proxy_id)
                                    .ok()
                                    .and_then(|proxy_id| proxy_records.get(&proxy_id).cloned())
                                    .ok_or(SchedulerRuntimeRepositoryError::Invariant)
                            })
                            .transpose()?;
                        collect_credential_source(sources, credential, parent, proxy, &now)
                    })?;
            let targets = select_supported_targets(&channels, credentials)?;
            Ok(abilities
                .into_iter()
                .filter_map(|ability| {
                    targets
                        .get(&ability.channel_id())
                        .map(|target| SchedulerRuntimeRecord::new(ability, Arc::clone(target)))
                })
                .collect())
        }
        .with_subscriber(NoSubscriber::default());

        match timeout(self.load_timeout, operation).await {
            Ok(result) => result.map_err(record_internal_error),
            Err(_) => Err(record_internal_error(
                SchedulerRuntimeRepositoryError::Timeout,
            )),
        }
    }
}

async fn load_parent_credentials(
    connection: &sea_orm::DatabaseConnection,
    parent_ids: &BTreeSet<i64>,
) -> Result<BTreeMap<i64, credentials::Model>, SchedulerRuntimeRepositoryError> {
    let mut records = BTreeMap::new();
    let parent_ids = parent_ids.iter().copied().collect::<Vec<_>>();
    for batch in parent_ids.chunks(RUNTIME_PARENT_QUERY_BATCH_SIZE) {
        for parent in credentials::Entity::find()
            .filter(credentials::Column::Id.is_in(batch.iter().copied()))
            .all(connection)
            .await
            .map_err(|_| SchedulerRuntimeRepositoryError::Query)?
        {
            if records.insert(parent.id, parent).is_some() {
                return Err(SchedulerRuntimeRepositoryError::Invariant);
            }
        }
    }
    Ok(records)
}

impl fmt::Debug for SchedulerRuntimeRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SchedulerRuntimeRepository")
            .field("load_timeout", &self.load_timeout)
            .finish_non_exhaustive()
    }
}

fn runtime_credential_query_limit() -> Result<u64, SchedulerRuntimeRepositoryError> {
    u64::try_from(MAX_SCHEDULER_RUNTIME_CREDENTIAL_ENTRIES)
        .ok()
        .and_then(|limit| limit.checked_add(1))
        .ok_or(SchedulerRuntimeRepositoryError::Invariant)
}

fn collect_channel_source(
    mut sources: BTreeMap<ChannelId, SchedulerRuntimeChannelSource>,
    channel: channels::Model,
) -> Result<BTreeMap<ChannelId, SchedulerRuntimeChannelSource>, SchedulerRuntimeRepositoryError> {
    let Some(source) = SchedulerRuntimeChannelSource::from_entity(channel)? else {
        return Ok(sources);
    };
    if sources.insert(source.channel_id(), source).is_some() {
        return Err(SchedulerRuntimeRepositoryError::Invariant);
    }
    Ok(sources)
}

fn collect_credential_source(
    mut sources: Vec<SchedulerRuntimeCredentialSource>,
    credential: credentials::Model,
    parent: Option<credentials::Model>,
    proxy: Option<super::SchedulerRuntimeProxyRecord>,
    now: &TimeDateTimeWithTimeZone,
) -> Result<Vec<SchedulerRuntimeCredentialSource>, SchedulerRuntimeRepositoryError> {
    if let Some(source) = SchedulerRuntimeCredentialSource::from_entity(credential, parent, proxy)?
        .filter(|source| {
            // 冷却窗口属于持久化调度元数据，进入纯选择策略前先按当前快照时间排除。
            !source.is_cooling_down(now)
        })
    {
        sources.push(source);
    }
    Ok(sources)
}

fn record_internal_error(
    error: SchedulerRuntimeRepositoryError,
) -> SchedulerRuntimeRepositoryError {
    let error_kind = match error {
        SchedulerRuntimeRepositoryError::InvalidConfiguration
        | SchedulerRuntimeRepositoryError::Ability(_) => return error,
        SchedulerRuntimeRepositoryError::Query => "scheduler_runtime_query",
        SchedulerRuntimeRepositoryError::Timeout => "scheduler_runtime_timeout",
        SchedulerRuntimeRepositoryError::Invariant => "scheduler_runtime_invariant",
    };
    tracing::error!(
        target: "af_db::scheduler_runtime",
        error_kind,
        "运行时渠道目录仓储发生内部错误"
    );
    error
}
