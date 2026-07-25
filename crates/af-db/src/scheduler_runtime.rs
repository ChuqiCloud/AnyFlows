use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    sync::Arc,
    time::Duration,
};

use af_domain::{ChannelId, ChannelType, CredentialKind, Protocol, Status};
use sea_orm::{
    ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect,
    entity::prelude::{Json, TimeDateTimeWithTimeZone},
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    DatabasePool, SchedulerAbilityRecord, SchedulerAbilityRepository,
    SchedulerAbilityRepositoryError,
    credential_probe::EncryptedCredentialEnvelope,
    entity::{ChannelBaseUrl, HeaderOverrides, channels, credentials},
};

const DEFAULT_LOAD_TIMEOUT: Duration = Duration::from_secs(5);
/// 一次运行时目录重建允许读取的凭据总数。
pub const MAX_SCHEDULER_RUNTIME_CREDENTIAL_ENTRIES: usize = 200_000;

/// 渠道转发时需要附加的非认证请求头。
#[derive(Clone, Eq, PartialEq)]
pub struct SchedulerRuntimeHeader {
    name: String,
    value: String,
}

impl SchedulerRuntimeHeader {
    /// 返回已验证的规范头名。
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 返回已验证的头值；调用方不得写入日志。
    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }
}

impl fmt::Debug for SchedulerRuntimeHeader {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SchedulerRuntimeHeader")
            .field("name", &self.name)
            .field("value", &"<已脱敏>")
            .finish()
    }
}

/// 渠道运行时目标构造失败；错误不携带 URL、Header 或凭据内容。
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
#[error("渠道运行时目标无效")]
pub struct SchedulerRuntimeTargetRecordError;

/// 已校验且仍保持密文形态的运行时凭据。
#[derive(Clone, Eq, PartialEq)]
pub struct SchedulerRuntimeCredentialRecord {
    credential_id: i64,
    credential_kind: CredentialKind,
    envelope: EncryptedCredentialEnvelope,
    proxy_required: bool,
}

impl SchedulerRuntimeCredentialRecord {
    /// 校验当前 M1 支持的凭据类型和持久化标识后构造记录。
    pub fn new(
        credential_id: i64,
        credential_kind: CredentialKind,
        envelope: EncryptedCredentialEnvelope,
        proxy_required: bool,
    ) -> Result<Self, SchedulerRuntimeTargetRecordError> {
        if credential_id <= 0
            || !matches!(
                credential_kind,
                CredentialKind::ApiKey | CredentialKind::Oauth
            )
        {
            return Err(SchedulerRuntimeTargetRecordError);
        }
        Ok(Self {
            credential_id,
            credential_kind,
            envelope,
            proxy_required,
        })
    }

    /// 返回凭据数据库标识，用于绑定密文 AAD。
    #[must_use]
    pub const fn credential_id(&self) -> i64 {
        self.credential_id
    }

    /// 返回数据库声明的凭据类型。
    #[must_use]
    pub const fn credential_kind(&self) -> CredentialKind {
        self.credential_kind
    }

    /// 返回版本化密文封套。
    #[must_use]
    pub const fn envelope(&self) -> &EncryptedCredentialEnvelope {
        &self.envelope
    }

    /// 返回该凭据是否绑定专属代理。
    #[must_use]
    pub const fn proxy_required(&self) -> bool {
        self.proxy_required
    }
}

impl fmt::Debug for SchedulerRuntimeCredentialRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SchedulerRuntimeCredentialRecord")
            .field("credential_id", &self.credential_id)
            .field("credential_kind", &self.credential_kind)
            .field("envelope", &self.envelope)
            .field("proxy_required", &self.proxy_required)
            .finish()
    }
}

/// 已从数据库读取并校验的渠道运行时目标。
#[derive(Clone, Eq, PartialEq)]
pub struct SchedulerRuntimeTargetRecord {
    channel_id: ChannelId,
    channel_type: ChannelType,
    protocol: Protocol,
    base_url: Option<String>,
    credential: SchedulerRuntimeCredentialRecord,
    headers: Vec<SchedulerRuntimeHeader>,
}

impl SchedulerRuntimeTargetRecord {
    /// 校验当前 M1 支持的 OpenAI Chat 目标及安全 Header 后构造记录。
    pub fn new(
        channel_id: ChannelId,
        channel_type: ChannelType,
        protocol: Protocol,
        base_url: Option<String>,
        credential: SchedulerRuntimeCredentialRecord,
        headers: Vec<(String, String)>,
    ) -> Result<Self, SchedulerRuntimeTargetRecordError> {
        if channel_type != ChannelType::OpenAi || protocol != Protocol::OpenAiChat {
            return Err(SchedulerRuntimeTargetRecordError);
        }
        let base_url = base_url
            .map(|value| {
                ChannelBaseUrl::parse(&value)
                    .map(|validated| validated.as_str().to_owned())
                    .map_err(|_| SchedulerRuntimeTargetRecordError)
            })
            .transpose()?;
        let header_count = headers.len();
        let header_json = Json::Object(
            headers
                .into_iter()
                .map(|(name, value)| (name, Json::String(value)))
                .collect(),
        );
        let headers = HeaderOverrides::validate(header_json)
            .map_err(|_| SchedulerRuntimeTargetRecordError)?
            .pairs()
            .map_err(|_| SchedulerRuntimeTargetRecordError)?
            .into_iter()
            .map(|(name, value)| SchedulerRuntimeHeader { name, value })
            .collect::<Vec<_>>();
        if headers.len() != header_count {
            return Err(SchedulerRuntimeTargetRecordError);
        }
        Ok(Self {
            channel_id,
            channel_type,
            protocol,
            base_url,
            credential,
            headers,
        })
    }

    /// 返回目标渠道标识。
    #[must_use]
    pub const fn channel_id(&self) -> ChannelId {
        self.channel_id
    }

    /// 返回凭据数据库标识，用于绑定密文 AAD。
    #[must_use]
    pub const fn credential_id(&self) -> i64 {
        self.credential.credential_id()
    }

    /// 返回渠道适配器类型。
    #[must_use]
    pub const fn channel_type(&self) -> ChannelType {
        self.channel_type
    }

    /// 返回渠道原生协议。
    #[must_use]
    pub const fn protocol(&self) -> Protocol {
        self.protocol
    }

    /// 返回可选基础地址；调用方不得写入日志。
    #[must_use]
    pub fn base_url(&self) -> Option<&str> {
        self.base_url.as_deref()
    }

    /// 返回数据库声明的凭据类型。
    #[must_use]
    pub const fn credential_kind(&self) -> CredentialKind {
        self.credential.credential_kind()
    }

    /// 返回版本化密文封套。
    #[must_use]
    pub const fn envelope(&self) -> &EncryptedCredentialEnvelope {
        self.credential.envelope()
    }

    /// 返回已验证的非认证请求头覆盖。
    #[must_use]
    pub fn headers(&self) -> &[SchedulerRuntimeHeader] {
        &self.headers
    }

    /// 返回该凭据是否绑定专属代理；为 true 时生产转发不得回退直连。
    #[must_use]
    pub const fn proxy_required(&self) -> bool {
        self.credential.proxy_required()
    }
}

impl fmt::Debug for SchedulerRuntimeTargetRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SchedulerRuntimeTargetRecord")
            .field("channel_id", &self.channel_id)
            .field("channel_type", &self.channel_type)
            .field("protocol", &self.protocol)
            .field("base_url", &self.base_url.as_ref().map(|_| "<已脱敏>"))
            .field("credential", &self.credential)
            .field("header_count", &self.headers.len())
            .finish()
    }
}

/// 已完成能力与运行时目标配对的生产候选记录。
#[derive(Clone, Eq, PartialEq)]
pub struct SchedulerRuntimeRecord {
    ability: SchedulerAbilityRecord,
    target: Arc<SchedulerRuntimeTargetRecord>,
}

impl SchedulerRuntimeRecord {
    /// 返回调度能力元数据。
    #[must_use]
    pub const fn ability(&self) -> &SchedulerAbilityRecord {
        &self.ability
    }

    /// 返回该能力对应渠道的运行时目标。
    #[must_use]
    pub fn target(&self) -> &SchedulerRuntimeTargetRecord {
        self.target.as_ref()
    }

    /// 消费记录并返回能力与共享运行时目标。
    #[must_use]
    pub fn into_parts(self) -> (SchedulerAbilityRecord, Arc<SchedulerRuntimeTargetRecord>) {
        (self.ability, self.target)
    }
}

impl fmt::Debug for SchedulerRuntimeRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SchedulerRuntimeRecord")
            .field("ability", &self.ability)
            .field("target", &self.target)
            .finish()
    }
}

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
                .map(|channel| (channel.id, channel))
                .collect::<BTreeMap<_, _>>();
            let credentials = credentials::Entity::find()
                .filter(credentials::Column::ChannelId.is_in(raw_channel_ids))
                .filter(credentials::Column::Status.eq(Status::Enabled.code()))
                .filter(credentials::Column::Schedulable.eq(true))
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
            let targets = select_supported_targets(
                &channels,
                credentials,
                TimeDateTimeWithTimeZone::now_utc(),
            )?;
            Ok(abilities
                .into_iter()
                .filter_map(|ability| {
                    targets
                        .get(&ability.channel_id())
                        .map(|target| SchedulerRuntimeRecord {
                            ability,
                            target: Arc::clone(target),
                        })
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

fn select_supported_targets(
    channels: &BTreeMap<i64, channels::Model>,
    credentials: Vec<credentials::Model>,
    now: TimeDateTimeWithTimeZone,
) -> Result<BTreeMap<ChannelId, Arc<SchedulerRuntimeTargetRecord>>, SchedulerRuntimeRepositoryError>
{
    let mut targets = BTreeMap::new();
    for credential in credentials {
        let channel_id = ChannelId::new(credential.channel_id)
            .map_err(|_| SchedulerRuntimeRepositoryError::Invariant)?;
        if targets.contains_key(&channel_id) {
            continue;
        }
        if credential_is_cooling_down(&credential, &now) {
            continue;
        }
        let Some(channel) = channels.get(&credential.channel_id) else {
            continue;
        };
        if let Some(target) = try_runtime_target(channel, credential)? {
            targets.insert(channel_id, Arc::new(target));
        }
    }
    Ok(targets)
}

fn credential_is_cooling_down(
    credential: &credentials::Model,
    now: &TimeDateTimeWithTimeZone,
) -> bool {
    [
        credential.rate_limit_reset_at.as_ref(),
        credential.overload_until.as_ref(),
        credential.temp_unschedulable_until.as_ref(),
    ]
    .into_iter()
    .flatten()
    .any(|until| until > now)
}

fn try_runtime_target(
    channel: &channels::Model,
    credential: credentials::Model,
) -> Result<Option<SchedulerRuntimeTargetRecord>, SchedulerRuntimeRepositoryError> {
    if credential.id <= 0 || credential.channel_id != channel.id {
        return Err(SchedulerRuntimeRepositoryError::Invariant);
    }
    let channel_status =
        Status::try_from(channel.status).map_err(|_| SchedulerRuntimeRepositoryError::Invariant)?;
    if !channel_status.is_enabled() || channel.deleted_at.is_some() {
        return Ok(None);
    }
    let channel_id =
        ChannelId::new(channel.id).map_err(|_| SchedulerRuntimeRepositoryError::Invariant)?;
    let channel_type = channel
        .r#type
        .parse()
        .map_err(|_| SchedulerRuntimeRepositoryError::Invariant)?;
    let protocol = channel
        .protocol
        .parse()
        .map_err(|_| SchedulerRuntimeRepositoryError::Invariant)?;
    if channel_type != ChannelType::OpenAi || protocol != Protocol::OpenAiChat {
        return Ok(None);
    }
    let credential_kind = credential
        .kind
        .parse()
        .map_err(|_| SchedulerRuntimeRepositoryError::Invariant)?;
    if !matches!(
        credential_kind,
        CredentialKind::ApiKey | CredentialKind::Oauth
    ) {
        return Ok(None);
    }
    let (key_id, nonce, ciphertext) = credential
        .secret
        .envelope_parts()
        .map_err(|_| SchedulerRuntimeRepositoryError::Invariant)?;
    let envelope = EncryptedCredentialEnvelope::new(key_id, nonce, ciphertext)
        .map_err(|_| SchedulerRuntimeRepositoryError::Invariant)?;
    let headers = channel
        .header_override
        .pairs()
        .map_err(|_| SchedulerRuntimeRepositoryError::Invariant)?;
    let credential = SchedulerRuntimeCredentialRecord::new(
        credential.id,
        credential_kind,
        envelope,
        credential.proxy_id.is_some(),
    )
    .map_err(|_| SchedulerRuntimeRepositoryError::Invariant)?;
    SchedulerRuntimeTargetRecord::new(
        channel_id,
        channel_type,
        protocol,
        channel
            .base_url
            .clone()
            .map(|value| value.as_str().to_owned()),
        credential,
        headers,
    )
    .map(Some)
    .map_err(|_| SchedulerRuntimeRepositoryError::Invariant)
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
