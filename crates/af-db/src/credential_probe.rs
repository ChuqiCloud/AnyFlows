use std::{fmt, time::Duration};

use af_domain::{
    ChannelId, ChannelTimeout, ChannelType, CredentialKind, Protocol, ResponsesCompactMode, Status,
};
use sea_orm::{
    ColumnTrait, EntityTrait, QueryFilter, QueryOrder, entity::prelude::TimeDateTimeWithTimeZone,
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    ChannelModelMappings, DatabasePool,
    channel_settings::{
        responses_compact_mode, responses_compact_model_mapping,
        validate_responses_compact_capability, validate_responses_compact_model_mapping,
    },
    entity::{channel_models, channels, credentials},
    model_price::is_valid_model_name,
};

const DEFAULT_LOAD_TIMEOUT: Duration = Duration::from_secs(5);
/// XChaCha20-Poly1305 nonce 的固定字节数。
pub const CREDENTIAL_ENVELOPE_NONCE_BYTES: usize = 24;
/// Poly1305 认证标签的固定字节数。
pub const CREDENTIAL_ENVELOPE_TAG_BYTES: usize = 16;
/// 单个凭据密文允许占用的最大字节数。
pub const MAX_CREDENTIAL_ENVELOPE_CIPHERTEXT_BYTES: usize = 1_048_576;
/// 密钥标识允许占用的最大 UTF-8 字节数。
pub const MAX_CREDENTIAL_ENVELOPE_KEY_ID_BYTES: usize = 512;

/// 密文封套构造错误；不保留密钥标识、nonce 或密文内容。
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
#[error("凭据密文封套无效")]
pub struct EncryptedCredentialEnvelopeError;

/// 已验证且默认脱敏的 XChaCha20-Poly1305 密文封套。
#[derive(Clone, Eq, PartialEq)]
pub struct EncryptedCredentialEnvelope {
    key_id: String,
    nonce: [u8; CREDENTIAL_ENVELOPE_NONCE_BYTES],
    ciphertext: Vec<u8>,
}

impl EncryptedCredentialEnvelope {
    /// 使用已编码的封套字段构造密文视图。
    pub fn new(
        key_id: impl Into<String>,
        nonce: [u8; CREDENTIAL_ENVELOPE_NONCE_BYTES],
        ciphertext: Vec<u8>,
    ) -> Result<Self, EncryptedCredentialEnvelopeError> {
        let key_id = key_id.into();
        if key_id.is_empty()
            || key_id.len() > MAX_CREDENTIAL_ENVELOPE_KEY_ID_BYTES
            || key_id.trim() != key_id
            || key_id.bytes().any(|byte| byte.is_ascii_control())
            || ciphertext.len() < CREDENTIAL_ENVELOPE_TAG_BYTES
            || ciphertext.len() > MAX_CREDENTIAL_ENVELOPE_CIPHERTEXT_BYTES
        {
            return Err(EncryptedCredentialEnvelopeError);
        }
        Ok(Self {
            key_id,
            nonce,
            ciphertext,
        })
    }

    /// 返回选择解密密钥使用的稳定标识。
    #[must_use]
    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    /// 返回固定长度 nonce；调用方不得写入日志。
    #[must_use]
    pub const fn nonce(&self) -> &[u8; CREDENTIAL_ENVELOPE_NONCE_BYTES] {
        &self.nonce
    }

    /// 返回带 Poly1305 标签的密文；调用方不得写入日志。
    #[must_use]
    pub fn ciphertext(&self) -> &[u8] {
        &self.ciphertext
    }
}

impl fmt::Debug for EncryptedCredentialEnvelope {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EncryptedCredentialEnvelope")
            .field("key_id", &"<已脱敏>")
            .field("nonce", &"<已脱敏>")
            .field("ciphertext_bytes", &self.ciphertext.len())
            .finish()
    }
}

/// 渠道探活需要附加的非认证请求头。
#[derive(Clone, Eq, PartialEq)]
pub struct ChannelProbeHeader {
    name: String,
    value: String,
}

impl ChannelProbeHeader {
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

impl fmt::Debug for ChannelProbeHeader {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ChannelProbeHeader")
            .field("name", &self.name)
            .field("value", &"<已脱敏>")
            .finish()
    }
}

/// 已从数据库读取并完成类型校验的真实探活目标。
#[derive(Clone, Eq, PartialEq)]
pub struct ChannelProbeTargetRecord {
    channel_id: ChannelId,
    credential_id: i64,
    channel_type: ChannelType,
    protocol: Protocol,
    base_url: Option<String>,
    timeout: Option<ChannelTimeout>,
    model: String,
    credential_kind: CredentialKind,
    oauth_provider: Option<String>,
    oauth_account_key: Option<String>,
    envelope: EncryptedCredentialEnvelope,
    headers: Vec<ChannelProbeHeader>,
    proxy_required: bool,
    revision: ChannelProbeTargetRevision,
    responses_compact_mode: ResponsesCompactMode,
    responses_compact_model: Option<String>,
}

/// 渠道探活开始时观察到的配置版本，用于拒绝旧探测覆盖新配置。
#[derive(Clone, Eq, PartialEq)]
pub struct ChannelProbeTargetRevision(TimeDateTimeWithTimeZone);

impl ChannelProbeTargetRevision {
    /// 返回数据库 CAS 使用的原始时间戳；仅限仓储边界读取。
    pub(crate) fn timestamp(&self) -> TimeDateTimeWithTimeZone {
        self.0
    }
}

impl fmt::Debug for ChannelProbeTargetRevision {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ChannelProbeTargetRevision(<已脱敏>)")
    }
}

impl ChannelProbeTargetRecord {
    /// 返回目标渠道标识。
    #[must_use]
    pub const fn channel_id(&self) -> ChannelId {
        self.channel_id
    }

    /// 返回凭据数据库标识，用于绑定密文 AAD。
    #[must_use]
    pub const fn credential_id(&self) -> i64 {
        self.credential_id
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

    /// 返回渠道级读取与完整请求超时；空值表示使用探活默认值。
    #[must_use]
    pub const fn timeout(&self) -> Option<ChannelTimeout> {
        self.timeout
    }

    /// 返回用于最小探活请求的规范模型名；调用方不得写入日志。
    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    /// 返回数据库声明的凭据类型。
    #[must_use]
    pub const fn credential_kind(&self) -> CredentialKind {
        self.credential_kind
    }

    /// 返回 OAuth 厂商标识；非 OAuth 凭据通常为空。
    #[must_use]
    pub fn oauth_provider(&self) -> Option<&str> {
        self.oauth_provider.as_deref()
    }

    /// 返回 OAuth 账号标识；调用方不得记录或回传给客户端。
    #[must_use]
    pub fn oauth_account_key(&self) -> Option<&str> {
        self.oauth_account_key.as_deref()
    }

    /// 返回版本化密文封套。
    #[must_use]
    pub const fn envelope(&self) -> &EncryptedCredentialEnvelope {
        &self.envelope
    }

    /// 返回已验证的非认证请求头覆盖。
    #[must_use]
    pub fn headers(&self) -> &[ChannelProbeHeader] {
        &self.headers
    }

    /// 返回该凭据是否绑定专属代理；为 true 时当前探活不得回退直连。
    #[must_use]
    pub const fn proxy_required(&self) -> bool {
        self.proxy_required
    }

    /// 返回探活开始时的渠道配置版本，供确定性能力事实执行 CAS 写回。
    #[must_use]
    pub fn revision(&self) -> ChannelProbeTargetRevision {
        self.revision.clone()
    }

    /// 返回渠道配置的 Compact 三态策略。
    #[must_use]
    pub const fn responses_compact_mode(&self) -> ResponsesCompactMode {
        self.responses_compact_mode
    }

    /// 返回依次应用普通映射与 Compact 专属映射后的探测模型。
    #[must_use]
    pub fn responses_compact_model(&self) -> Option<&str> {
        self.responses_compact_model.as_deref()
    }
}

impl fmt::Debug for ChannelProbeTargetRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ChannelProbeTargetRecord")
            .field("channel_id", &self.channel_id)
            .field("credential_id", &self.credential_id)
            .field("channel_type", &self.channel_type)
            .field("protocol", &self.protocol)
            .field("base_url", &self.base_url.as_ref().map(|_| "<已脱敏>"))
            .field("timeout", &self.timeout)
            .field("model", &"<已脱敏>")
            .field("credential_kind", &self.credential_kind)
            .field("oauth_provider", &self.oauth_provider)
            .field(
                "oauth_account_key",
                &self.oauth_account_key.as_ref().map(|_| "<已脱敏>"),
            )
            .field("envelope", &self.envelope)
            .field("header_count", &self.headers.len())
            .field("proxy_required", &self.proxy_required)
            .field("responses_compact_mode", &self.responses_compact_mode)
            .field(
                "has_responses_compact_model",
                &self.responses_compact_model.is_some(),
            )
            .finish()
    }
}

/// 探活目标读取失败；错误不携带 URL、模型、头值或密文。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum ChannelProbeTargetRepositoryError {
    /// 查询截止时间配置为零。
    #[error("渠道探活目标查询超时必须大于零")]
    InvalidConfiguration,
    /// 获取连接或读取目标失败。
    #[error("读取渠道探活目标失败")]
    Query,
    /// 读取目标超过硬截止时间。
    #[error("读取渠道探活目标超时")]
    Timeout,
    /// 数据库字段违反持久化不变量。
    #[error("渠道探活目标持久化状态损坏")]
    Invariant,
}

/// 按渠道读取真实探活所需配置和加密凭据的数据库仓储。
#[derive(Clone)]
pub struct ChannelProbeTargetRepository {
    pool: DatabasePool,
    load_timeout: Duration,
}

impl ChannelProbeTargetRepository {
    /// 使用默认五秒截止时间创建仓储。
    #[must_use]
    pub fn new(pool: DatabasePool) -> Self {
        Self {
            pool,
            load_timeout: DEFAULT_LOAD_TIMEOUT,
        }
    }

    /// 使用显式非零截止时间创建仓储。
    pub fn with_load_timeout(
        pool: DatabasePool,
        load_timeout: Duration,
    ) -> Result<Self, ChannelProbeTargetRepositoryError> {
        if load_timeout.is_zero() {
            return Err(ChannelProbeTargetRepositoryError::InvalidConfiguration);
        }
        Ok(Self { pool, load_timeout })
    }

    /// 读取活动渠道、首个稳定模型和最高优先级可调度凭据。
    ///
    /// 自动恢复资格仍由 `ChannelProbeLease` 单独约束；本仓储同时服务管理端手动测活，
    /// 因此不能把启用或手动停用渠道错误隐藏。
    pub async fn load(
        &self,
        channel_id: ChannelId,
    ) -> Result<Option<ChannelProbeTargetRecord>, ChannelProbeTargetRepositoryError> {
        let operation = async {
            let Some(channel) = channels::Entity::find_by_id(channel_id.get())
                .one(self.pool.connection())
                .await
                .map_err(|_| ChannelProbeTargetRepositoryError::Query)?
            else {
                return Ok(None);
            };
            Status::try_from(channel.status)
                .map_err(|_| ChannelProbeTargetRepositoryError::Invariant)?;
            if channel.deleted_at.is_some() {
                return Ok(None);
            }

            let Some(channel_model) = channel_models::Entity::find()
                .filter(channel_models::Column::ChannelId.eq(channel_id.get()))
                .order_by_asc(channel_models::Column::Model)
                .one(self.pool.connection())
                .await
                .map_err(|_| ChannelProbeTargetRepositoryError::Query)?
            else {
                return Ok(None);
            };
            if !is_valid_model_name(&channel_model.model) {
                return Err(ChannelProbeTargetRepositoryError::Invariant);
            }

            let Some(credential) = credentials::Entity::find()
                .filter(credentials::Column::ChannelId.eq(channel_id.get()))
                .filter(credentials::Column::Status.eq(Status::Enabled.code()))
                .filter(credentials::Column::Schedulable.eq(true))
                .filter(credentials::Column::OauthTokenPending.eq(false))
                .filter(credentials::Column::DeletedAt.is_null())
                .order_by_desc(credentials::Column::Priority)
                .order_by_asc(credentials::Column::Id)
                .one(self.pool.connection())
                .await
                .map_err(|_| ChannelProbeTargetRepositoryError::Query)?
            else {
                return Ok(None);
            };
            if credential.id <= 0 {
                return Err(ChannelProbeTargetRepositoryError::Invariant);
            }

            let channel_type = channel
                .r#type
                .parse()
                .map_err(|_| ChannelProbeTargetRepositoryError::Invariant)?;
            let protocol = channel
                .protocol
                .parse()
                .map_err(|_| ChannelProbeTargetRepositoryError::Invariant)?;
            let credential_kind = credential
                .kind
                .parse()
                .map_err(|_| ChannelProbeTargetRepositoryError::Invariant)?;
            let settings = channel.settings.clone().into_inner();
            let responses_compact_mode = responses_compact_mode(&settings)
                .map_err(|_| ChannelProbeTargetRepositoryError::Invariant)?;
            validate_responses_compact_capability(channel_type, protocol, responses_compact_mode)
                .map_err(|_| ChannelProbeTargetRepositoryError::Invariant)?;
            let compact_model_mappings = responses_compact_model_mapping(&settings)
                .map_err(|_| ChannelProbeTargetRepositoryError::Invariant)?;
            validate_responses_compact_model_mapping(
                channel_type,
                protocol,
                &compact_model_mappings,
            )
            .map_err(|_| ChannelProbeTargetRepositoryError::Invariant)?;
            let model_mappings = ChannelModelMappings::parse(&channel.model_mapping)
                .map_err(|_| ChannelProbeTargetRepositoryError::Invariant)?;
            // 探活必须使用与真实转发相同的上游模型名，否则配置模型映射的渠道会被误判为不健康。
            let probe_model = model_mappings
                .resolve(&channel_model.model)
                .unwrap_or(&channel_model.model)
                .to_owned();
            let responses_compact_model = (channel_type == ChannelType::OpenAi
                && protocol == Protocol::OpenAiResponses)
                .then(|| {
                    let base_model = model_mappings
                        .resolve(&channel_model.model)
                        .unwrap_or(&channel_model.model);
                    compact_model_mappings
                        .resolve(base_model)
                        .unwrap_or(base_model)
                        .to_owned()
                });
            let channel_timeout = channel
                .timeout_secs
                .map(|value| {
                    <u64 as std::convert::TryFrom<i32>>::try_from(value)
                        .ok()
                        .and_then(|value| ChannelTimeout::new(value).ok())
                        .ok_or(ChannelProbeTargetRepositoryError::Invariant)
                })
                .transpose()?;
            let (key_id, nonce, ciphertext) = credential
                .secret
                .envelope_parts()
                .map_err(|_| ChannelProbeTargetRepositoryError::Invariant)?;
            let envelope = EncryptedCredentialEnvelope::new(key_id, nonce, ciphertext)
                .map_err(|_| ChannelProbeTargetRepositoryError::Invariant)?;
            let headers = channel
                .header_override
                .pairs()
                .map_err(|_| ChannelProbeTargetRepositoryError::Invariant)?
                .into_iter()
                .map(|(name, value)| ChannelProbeHeader { name, value })
                .collect();

            Ok(Some(ChannelProbeTargetRecord {
                channel_id,
                credential_id: credential.id,
                channel_type,
                protocol,
                base_url: channel.base_url.map(|value| value.as_str().to_owned()),
                timeout: channel_timeout,
                model: probe_model,
                credential_kind,
                oauth_provider: credential.oauth_provider,
                oauth_account_key: credential.oauth_account_key,
                envelope,
                headers,
                proxy_required: credential.proxy_id.is_some(),
                revision: ChannelProbeTargetRevision(channel.updated_at),
                responses_compact_mode,
                responses_compact_model,
            }))
        }
        .with_subscriber(NoSubscriber::default());

        match timeout(self.load_timeout, operation).await {
            Ok(result) => result.map_err(record_internal_error),
            Err(_) => Err(record_internal_error(
                ChannelProbeTargetRepositoryError::Timeout,
            )),
        }
    }
}

impl fmt::Debug for ChannelProbeTargetRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ChannelProbeTargetRepository")
            .field("load_timeout", &self.load_timeout)
            .finish_non_exhaustive()
    }
}

fn record_internal_error(
    error: ChannelProbeTargetRepositoryError,
) -> ChannelProbeTargetRepositoryError {
    let error_kind = match error {
        ChannelProbeTargetRepositoryError::InvalidConfiguration => return error,
        ChannelProbeTargetRepositoryError::Query => "channel_probe_target_query",
        ChannelProbeTargetRepositoryError::Timeout => "channel_probe_target_timeout",
        ChannelProbeTargetRepositoryError::Invariant => "channel_probe_target_invariant",
    };
    tracing::error!(
        target: "af_db::credential_probe",
        error_kind,
        "渠道探活目标仓储发生内部错误"
    );
    error
}
