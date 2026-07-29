use std::{collections::BTreeSet, fmt};

use af_domain::{ChannelId, ChannelType, CredentialKind, Protocol};
use sea_orm::entity::prelude::Json;
use thiserror::Error;

use crate::{
    credential_probe::EncryptedCredentialEnvelope,
    entity::{ChannelBaseUrl, HeaderOverrides},
};

use super::{ChannelModelMappings, ChannelParameterOverrides};

/// 单个渠道进入运行时快照的凭据数量上限。
pub const MAX_SCHEDULER_RUNTIME_CREDENTIALS_PER_CHANNEL: usize = 64;

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
    priority: i32,
    weight: u32,
}

impl SchedulerRuntimeCredentialRecord {
    /// 校验当前 M1 支持的凭据类型和持久化标识后构造记录。
    pub fn new(
        credential_id: i64,
        credential_kind: CredentialKind,
        envelope: EncryptedCredentialEnvelope,
        proxy_required: bool,
    ) -> Result<Self, SchedulerRuntimeTargetRecordError> {
        Self::with_scheduling(
            credential_id,
            credential_kind,
            envelope,
            proxy_required,
            0,
            0,
        )
    }

    /// 连同凭据池调度优先级和权重构造运行时记录。
    pub fn with_scheduling(
        credential_id: i64,
        credential_kind: CredentialKind,
        envelope: EncryptedCredentialEnvelope,
        proxy_required: bool,
        priority: i32,
        weight: u32,
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
            priority,
            weight,
        })
    }

    /// 返回凭据数据库标识，用于绑定密文 AAD。
    #[must_use]
    pub fn credential_id(&self) -> i64 {
        self.credential_id
    }

    /// 返回数据库声明的凭据类型。
    #[must_use]
    pub fn credential_kind(&self) -> CredentialKind {
        self.credential_kind
    }

    /// 返回版本化密文封套。
    #[must_use]
    pub fn envelope(&self) -> &EncryptedCredentialEnvelope {
        &self.envelope
    }

    /// 返回该凭据是否绑定专属代理。
    #[must_use]
    pub fn proxy_required(&self) -> bool {
        self.proxy_required
    }

    /// 返回凭据池内部调度优先级。
    #[must_use]
    pub const fn priority(&self) -> i32 {
        self.priority
    }

    /// 返回凭据池同优先级成员的非负权重。
    #[must_use]
    pub const fn weight(&self) -> u32 {
        self.weight
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
            .field("priority", &self.priority)
            .field("weight", &self.weight)
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
    credentials: Vec<SchedulerRuntimeCredentialRecord>,
    model_mappings: ChannelModelMappings,
    parameter_overrides: ChannelParameterOverrides,
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
        Self::new_pool(
            channel_id,
            channel_type,
            protocol,
            base_url,
            vec![credential],
            headers,
        )
    }

    /// 校验渠道配置和有界凭据池后构造共享运行时目标。
    pub fn new_pool(
        channel_id: ChannelId,
        channel_type: ChannelType,
        protocol: Protocol,
        base_url: Option<String>,
        credentials: Vec<SchedulerRuntimeCredentialRecord>,
        headers: Vec<(String, String)>,
    ) -> Result<Self, SchedulerRuntimeTargetRecordError> {
        Self::new_pool_with_request_policy(
            channel_id,
            channel_type,
            protocol,
            base_url,
            credentials,
            ChannelModelMappings::default(),
            ChannelParameterOverrides::default(),
            headers,
        )
    }

    /// 连同已验证的模型映射和 Canonical 参数覆盖构造共享运行时目标。
    #[allow(
        clippy::too_many_arguments,
        reason = "字段与不可变运行时目标契约一一对应"
    )]
    pub fn new_pool_with_request_policy(
        channel_id: ChannelId,
        channel_type: ChannelType,
        protocol: Protocol,
        base_url: Option<String>,
        credentials: Vec<SchedulerRuntimeCredentialRecord>,
        model_mappings: ChannelModelMappings,
        parameter_overrides: ChannelParameterOverrides,
        headers: Vec<(String, String)>,
    ) -> Result<Self, SchedulerRuntimeTargetRecordError> {
        if channel_type != ChannelType::OpenAi
            || protocol != Protocol::OpenAiChat
            || credentials.is_empty()
            || credentials.len() > MAX_SCHEDULER_RUNTIME_CREDENTIALS_PER_CHANNEL
        {
            return Err(SchedulerRuntimeTargetRecordError);
        }
        let unique_ids = credentials
            .iter()
            .map(SchedulerRuntimeCredentialRecord::credential_id)
            .collect::<BTreeSet<_>>();
        if unique_ids.len() != credentials.len() {
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
            credentials,
            model_mappings,
            parameter_overrides,
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
    pub fn credential_id(&self) -> i64 {
        self.credentials[0].credential_id()
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
    pub fn credential_kind(&self) -> CredentialKind {
        self.credentials[0].credential_kind()
    }

    /// 返回版本化密文封套。
    #[must_use]
    pub fn envelope(&self) -> &EncryptedCredentialEnvelope {
        self.credentials[0].envelope()
    }

    /// 返回该渠道当前快照内的全部可调度凭据。
    #[must_use]
    pub fn credentials(&self) -> &[SchedulerRuntimeCredentialRecord] {
        &self.credentials
    }

    /// 返回请求模型命中的上游模型；未命中时为空。
    #[must_use]
    pub fn mapped_model(&self, requested_model: &str) -> Option<&str> {
        self.model_mappings.resolve(requested_model)
    }

    /// 返回已验证的 Canonical 参数覆盖。
    #[must_use]
    pub const fn parameter_overrides(&self) -> &ChannelParameterOverrides {
        &self.parameter_overrides
    }

    /// 返回已验证的非认证请求头覆盖。
    #[must_use]
    pub fn headers(&self) -> &[SchedulerRuntimeHeader] {
        &self.headers
    }

    /// 返回该凭据是否绑定专属代理；为 true 时生产转发不得回退直连。
    #[must_use]
    pub fn proxy_required(&self) -> bool {
        self.credentials[0].proxy_required()
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
            .field("credential_count", &self.credentials.len())
            .field("model_mapping_count", &self.model_mappings.len())
            .field("parameter_overrides", &self.parameter_overrides)
            .field("header_count", &self.headers.len())
            .finish()
    }
}
