use std::{collections::BTreeSet, fmt};

use af_domain::{
    ChannelAutoBanRules, ChannelId, ChannelTimeout, ChannelType, ConcurrencyLimit, CredentialKind,
    Protocol, ProxyId, ResponsesCompactMode, ResponsesCompactProbeResult,
};
use sea_orm::entity::prelude::Json;
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{
    CredentialProxyScheme,
    credential_probe::EncryptedCredentialEnvelope,
    entity::{ChannelBaseUrl, HeaderOverrides},
};

use super::{ChannelModelMappings, ChannelParameterOverrides};

/// 单个渠道进入运行时快照的凭据数量上限。
pub const MAX_SCHEDULER_RUNTIME_CREDENTIALS_PER_CHANNEL: usize = 64;

/// 判断渠道类型与原生协议是否已经完成生产运行时装配。
pub(super) const fn is_supported_runtime_target(
    channel_type: ChannelType,
    protocol: Protocol,
) -> bool {
    matches!(
        (channel_type, protocol),
        (
            ChannelType::OpenAi,
            Protocol::OpenAiChat
                | Protocol::OpenAiResponses
                | Protocol::OpenAiEmbeddings
                | Protocol::OpenAiImages
                | Protocol::OpenAiAudio
        ) | (ChannelType::Anthropic, Protocol::Anthropic)
            | (ChannelType::Gemini, Protocol::Gemini)
    )
}

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

/// 已校验且仍保持密码密文形态的凭据专属代理运行时快照。
#[derive(Clone, Eq, PartialEq)]
pub struct SchedulerRuntimeProxyRecord {
    proxy_id: ProxyId,
    scheme: CredentialProxyScheme,
    host: String,
    port: u16,
    username: Option<String>,
    password_secret: Option<EncryptedCredentialEnvelope>,
    trust_proxy_dns: bool,
    version: i64,
}

impl SchedulerRuntimeProxyRecord {
    /// 构造专属代理快照；认证用户名与密码必须同时存在或同时为空。
    #[allow(clippy::too_many_arguments, reason = "字段与代理运行时投影一一对应")]
    pub fn new(
        proxy_id: ProxyId,
        scheme: CredentialProxyScheme,
        host: String,
        port: u16,
        username: Option<String>,
        password_secret: Option<EncryptedCredentialEnvelope>,
        trust_proxy_dns: bool,
        version: i64,
    ) -> Result<Self, SchedulerRuntimeTargetRecordError> {
        if port == 0
            || version < 1
            || !valid_proxy_host(&host)
            || username.is_some() != password_secret.is_some()
            || username.as_deref().is_some_and(|value| {
                value.is_empty()
                    || value.len() > crate::MAX_CREDENTIAL_PROXY_USERNAME_BYTES
                    || value.trim() != value
                    || value.chars().any(char::is_control)
            })
        {
            return Err(SchedulerRuntimeTargetRecordError);
        }
        Ok(Self {
            proxy_id,
            scheme,
            host,
            port,
            username,
            password_secret,
            trust_proxy_dns,
            version,
        })
    }

    #[must_use]
    pub const fn proxy_id(&self) -> ProxyId {
        self.proxy_id
    }
    #[must_use]
    pub const fn scheme(&self) -> CredentialProxyScheme {
        self.scheme
    }
    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }
    #[must_use]
    pub const fn port(&self) -> u16 {
        self.port
    }
    #[must_use]
    pub fn username(&self) -> Option<&str> {
        self.username.as_deref()
    }
    #[must_use]
    pub fn password_secret(&self) -> Option<&EncryptedCredentialEnvelope> {
        self.password_secret.as_ref()
    }
    #[must_use]
    pub const fn trust_proxy_dns(&self) -> bool {
        self.trust_proxy_dns
    }
    #[must_use]
    pub const fn version(&self) -> i64 {
        self.version
    }
}

impl fmt::Debug for SchedulerRuntimeProxyRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SchedulerRuntimeProxyRecord")
            .field("proxy_id", &self.proxy_id)
            .field("scheme", &self.scheme)
            .field("host", &"<已脱敏>")
            .field("port", &self.port)
            .field("username", &self.username.as_ref().map(|_| "<已脱敏>"))
            .field("password_configured", &self.password_secret.is_some())
            .field("trust_proxy_dns", &self.trust_proxy_dns)
            .field("version", &self.version)
            .finish()
    }
}

/// 已校验且仍保持密文形态的运行时凭据。
#[derive(Clone, Eq, PartialEq)]
pub struct SchedulerRuntimeCredentialRecord {
    credential_id: i64,
    credential_kind: CredentialKind,
    envelope: EncryptedCredentialEnvelope,
    credential_revision: u64,
    proxy_required: bool,
    proxy: Option<SchedulerRuntimeProxyRecord>,
    priority: i32,
    weight: u32,
    concurrency: Option<ConcurrencyLimit>,
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
        Self::with_scheduling_and_concurrency(
            credential_id,
            credential_kind,
            envelope,
            proxy_required,
            priority,
            weight,
            None,
        )
    }

    /// 连同账号级并发限制构造运行时记录；空值表示仅追踪负载、不限制槽位。
    #[allow(clippy::too_many_arguments, reason = "字段与凭据运行时投影一一对应")]
    pub fn with_scheduling_and_concurrency(
        credential_id: i64,
        credential_kind: CredentialKind,
        envelope: EncryptedCredentialEnvelope,
        proxy_required: bool,
        priority: i32,
        weight: u32,
        concurrency: Option<ConcurrencyLimit>,
    ) -> Result<Self, SchedulerRuntimeTargetRecordError> {
        if credential_id <= 0
            || !matches!(
                credential_kind,
                CredentialKind::ApiKey | CredentialKind::Oauth
            )
        {
            return Err(SchedulerRuntimeTargetRecordError);
        }
        let credential_revision = derive_credential_revision(&envelope);
        Ok(Self {
            credential_id,
            credential_kind,
            envelope,
            credential_revision,
            proxy_required,
            proxy: None,
            priority,
            weight,
            concurrency,
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

    /// 返回由完整密文封套派生的连接复用版本。
    #[must_use]
    pub const fn credential_revision(&self) -> u64 {
        self.credential_revision
    }

    /// 返回该凭据是否绑定专属代理。
    #[must_use]
    pub fn proxy_required(&self) -> bool {
        self.proxy_required
    }

    /// 绑定已解析的专属代理快照；只有完整快照才能进入生产转发。
    pub fn with_proxy(
        mut self,
        proxy: SchedulerRuntimeProxyRecord,
    ) -> Result<Self, SchedulerRuntimeTargetRecordError> {
        if !self.proxy_required {
            return Err(SchedulerRuntimeTargetRecordError);
        }
        self.proxy = Some(proxy);
        Ok(self)
    }

    /// 返回专属代理快照；`proxy_required=true` 但为空表示旧投影或损坏状态，必须失败关闭。
    #[must_use]
    pub const fn proxy(&self) -> Option<&SchedulerRuntimeProxyRecord> {
        self.proxy.as_ref()
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

    /// 返回账号级并发限制；空值表示不限并发但仍可追踪实时负载。
    #[must_use]
    pub const fn concurrency(&self) -> Option<ConcurrencyLimit> {
        self.concurrency
    }
}

impl fmt::Debug for SchedulerRuntimeCredentialRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SchedulerRuntimeCredentialRecord")
            .field("credential_id", &self.credential_id)
            .field("credential_kind", &self.credential_kind)
            .field("envelope", &self.envelope)
            .field("credential_revision", &"<已脱敏>")
            .field("proxy_required", &self.proxy_required)
            .field("proxy", &self.proxy)
            .field("priority", &self.priority)
            .field("weight", &self.weight)
            .field("concurrency", &self.concurrency)
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
    timeout: Option<ChannelTimeout>,
    credentials: Vec<SchedulerRuntimeCredentialRecord>,
    model_mappings: ChannelModelMappings,
    parameter_overrides: ChannelParameterOverrides,
    headers: Vec<SchedulerRuntimeHeader>,
    auto_ban_rules: ChannelAutoBanRules,
    pool_mode: bool,
    responses_websocket_enabled: bool,
    responses_compact_mode: ResponsesCompactMode,
    responses_compact_probe_result: ResponsesCompactProbeResult,
    responses_compact_model_mapping: ChannelModelMappings,
}

impl SchedulerRuntimeTargetRecord {
    /// 校验当前支持的原生协议目标及安全 Header 后构造记录。
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
        Self::new_pool_with_request_policy_and_compact_mapping(
            channel_id,
            channel_type,
            protocol,
            base_url,
            credentials,
            model_mappings,
            ChannelModelMappings::default(),
            parameter_overrides,
            headers,
        )
    }

    /// 连同 Compact 专属模型映射构造共享运行时目标。
    #[allow(
        clippy::too_many_arguments,
        reason = "字段与不可变运行时目标契约一一对应"
    )]
    pub fn new_pool_with_request_policy_and_compact_mapping(
        channel_id: ChannelId,
        channel_type: ChannelType,
        protocol: Protocol,
        base_url: Option<String>,
        credentials: Vec<SchedulerRuntimeCredentialRecord>,
        model_mappings: ChannelModelMappings,
        responses_compact_model_mapping: ChannelModelMappings,
        parameter_overrides: ChannelParameterOverrides,
        headers: Vec<(String, String)>,
    ) -> Result<Self, SchedulerRuntimeTargetRecordError> {
        if !is_supported_runtime_target(channel_type, protocol)
            || credentials.is_empty()
            || credentials.len() > MAX_SCHEDULER_RUNTIME_CREDENTIALS_PER_CHANNEL
            || parameter_overrides.validate_for_protocol(protocol).is_err()
            || (!responses_compact_model_mapping.is_empty()
                && (channel_type != ChannelType::OpenAi || protocol != Protocol::OpenAiResponses))
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
            timeout: None,
            credentials,
            model_mappings,
            parameter_overrides,
            headers,
            auto_ban_rules: ChannelAutoBanRules::default(),
            pool_mode: false,
            responses_websocket_enabled: false,
            responses_compact_mode: ResponsesCompactMode::Auto,
            responses_compact_probe_result: ResponsesCompactProbeResult::Unknown,
            responses_compact_model_mapping,
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

    /// 覆盖该不可变运行时目标使用的渠道级超时。
    #[must_use]
    pub fn with_timeout(mut self, timeout: Option<ChannelTimeout>) -> Self {
        self.timeout = timeout;
        self
    }

    /// 显式设置已验证的渠道自动禁用规则。
    #[must_use]
    pub fn with_auto_ban_rules(mut self, rules: ChannelAutoBanRules) -> Self {
        self.auto_ban_rules = rules;
        self
    }

    /// 显式设置渠道是否跳过渠道级本地故障状态。
    #[must_use]
    pub fn with_pool_mode(mut self, enabled: bool) -> Self {
        self.pool_mode = enabled;
        self
    }

    /// 显式设置原生 Responses WebSocket 能力。
    pub fn with_responses_websocket_enabled(
        mut self,
        enabled: bool,
    ) -> Result<Self, SchedulerRuntimeTargetRecordError> {
        if enabled
            && (self.channel_type != ChannelType::OpenAi
                || self.protocol != Protocol::OpenAiResponses)
        {
            return Err(SchedulerRuntimeTargetRecordError);
        }
        self.responses_websocket_enabled = enabled;
        Ok(self)
    }

    /// 显式设置 Compact 策略，并拒绝非原生 Responses 渠道的强制开启。
    pub fn with_responses_compact_mode(
        mut self,
        mode: ResponsesCompactMode,
    ) -> Result<Self, SchedulerRuntimeTargetRecordError> {
        if mode == ResponsesCompactMode::ForceOn
            && (self.channel_type != ChannelType::OpenAi
                || self.protocol != Protocol::OpenAiResponses)
        {
            return Err(SchedulerRuntimeTargetRecordError);
        }
        self.responses_compact_mode = mode;
        Ok(self)
    }

    /// 设置最近一次确定性 Compact 探测结论，并拒绝非原生渠道携带支持事实。
    pub fn with_responses_compact_probe_result(
        mut self,
        result: ResponsesCompactProbeResult,
    ) -> Result<Self, SchedulerRuntimeTargetRecordError> {
        if result != ResponsesCompactProbeResult::Unknown
            && (self.channel_type != ChannelType::OpenAi
                || self.protocol != Protocol::OpenAiResponses)
        {
            return Err(SchedulerRuntimeTargetRecordError);
        }
        self.responses_compact_probe_result = result;
        Ok(self)
    }

    /// 返回渠道级读取与完整请求超时；空值表示继承全局配置。
    #[must_use]
    pub const fn timeout(&self) -> Option<ChannelTimeout> {
        self.timeout
    }

    /// 返回当前不可变快照携带的渠道自动禁用规则。
    #[must_use]
    pub const fn auto_ban_rules(&self) -> &ChannelAutoBanRules {
        &self.auto_ban_rules
    }

    /// 返回渠道是否把账号健康交由外部池管理。
    #[must_use]
    pub const fn pool_mode(&self) -> bool {
        self.pool_mode
    }

    /// 返回渠道是否显式允许使用原生 Responses WebSocket。
    #[must_use]
    pub const fn responses_websocket_enabled(&self) -> bool {
        self.responses_websocket_enabled
    }

    /// 返回渠道的 Responses Compact 三态策略。
    #[must_use]
    pub const fn responses_compact_mode(&self) -> ResponsesCompactMode {
        self.responses_compact_mode
    }

    /// 返回最近一次确定性 Compact 探测结论。
    #[must_use]
    pub const fn responses_compact_probe_result(&self) -> ResponsesCompactProbeResult {
        self.responses_compact_probe_result
    }

    /// 按三态策略和探测事实判断该目标是否具备 Compact 调度资格。
    #[must_use]
    pub const fn responses_compact_schedulable(&self) -> bool {
        if self.channel_type != ChannelType::OpenAi || self.protocol != Protocol::OpenAiResponses {
            return false;
        }
        match self.responses_compact_mode {
            ResponsesCompactMode::ForceOn => true,
            ResponsesCompactMode::ForceOff => false,
            ResponsesCompactMode::Auto => {
                self.responses_compact_probe_result == ResponsesCompactProbeResult::Supported
            }
        }
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

    /// 先应用普通映射，再叠加 Compact 专属映射；普通 Responses 永不读取后者。
    #[must_use]
    pub fn mapped_responses_compact_model<'a>(&'a self, requested_model: &'a str) -> &'a str {
        let base_model = self
            .mapped_model(requested_model)
            .unwrap_or(requested_model);
        self.responses_compact_model_mapping
            .resolve(base_model)
            .unwrap_or(base_model)
    }

    /// 仅在目标具备 Compact 调度资格时返回已验证的上游模型。
    #[must_use]
    pub fn schedulable_responses_compact_model<'a>(
        &'a self,
        requested_model: &'a str,
    ) -> Option<&'a str> {
        self.responses_compact_schedulable()
            .then(|| self.mapped_responses_compact_model(requested_model))
    }

    /// 返回受控投影编解码使用的已验证模型映射。
    pub(super) const fn model_mappings(&self) -> &ChannelModelMappings {
        &self.model_mappings
    }

    /// 返回 Compact 专属模型映射，供 Redis 投影复用已验证值。
    pub(super) const fn responses_compact_model_mapping(&self) -> &ChannelModelMappings {
        &self.responses_compact_model_mapping
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
            .field("timeout", &self.timeout)
            .field("credential_count", &self.credentials.len())
            .field("model_mapping_count", &self.model_mappings.len())
            .field("parameter_overrides", &self.parameter_overrides)
            .field("header_count", &self.headers.len())
            .field("auto_ban_rules", &self.auto_ban_rules)
            .field("pool_mode", &self.pool_mode)
            .field(
                "responses_websocket_enabled",
                &self.responses_websocket_enabled,
            )
            .field("responses_compact_mode", &self.responses_compact_mode)
            .field(
                "responses_compact_probe_result",
                &self.responses_compact_probe_result,
            )
            .field(
                "responses_compact_model_mapping_count",
                &self.responses_compact_model_mapping.len(),
            )
            .finish()
    }
}

fn derive_credential_revision(envelope: &EncryptedCredentialEnvelope) -> u64 {
    let mut digest = Sha256::new();
    digest.update(b"AnyFlows:credential-revision:v1\0");
    digest.update(
        u32::try_from(envelope.key_id().len())
            .unwrap_or(u32::MAX)
            .to_be_bytes(),
    );
    digest.update(envelope.key_id().as_bytes());
    digest.update(envelope.nonce());
    digest.update(
        u64::try_from(envelope.ciphertext().len())
            .unwrap_or(u64::MAX)
            .to_be_bytes(),
    );
    digest.update(envelope.ciphertext());
    let output = digest.finalize();
    u64::from_be_bytes(
        output[..8]
            .try_into()
            .expect("SHA-256 截断长度固定为 8 字节"),
    )
}

fn valid_proxy_host(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= crate::MAX_CREDENTIAL_PROXY_HOST_BYTES
        && value.trim() == value
        && value.is_ascii()
        && !value
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
        && !value.contains("://")
        && !value.contains(['/', '\\', '@'])
}
