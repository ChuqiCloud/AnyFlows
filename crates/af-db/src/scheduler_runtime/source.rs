use af_domain::{
    ChannelAutoBanRules, ChannelId, ChannelTimeout, ChannelType, ClientSimulationBodyProfile,
    ClientSimulationProfile, ConcurrencyLimit, CredentialId, CredentialKind,
    CredentialQuotaDimension, Protocol, ProxyId, ResponsesCompactMode, ResponsesCompactProbeResult,
    Status,
};
use sea_orm::entity::prelude::TimeDateTimeWithTimeZone;
use thiserror::Error;

use crate::{
    CredentialProxyScheme,
    channel_settings::{
        auto_ban_rules, client_simulation_body_profile, client_simulation_profile, pool_mode,
        responses_compact_mode, responses_compact_model_mapping, responses_compact_probe_result,
        responses_websocket_enabled, validate_client_simulation_body_capability,
        validate_client_simulation_capability, validate_responses_compact_capability,
        validate_responses_compact_model_mapping, validate_responses_compact_probe_result,
        validate_responses_websocket_capability,
    },
    credential_probe::EncryptedCredentialEnvelope,
    entity::{channels, credentials, proxies},
};

use super::{ChannelModelMappings, ChannelParameterOverrides, SchedulerRuntimeProxyRecord};

const SHADOW_REFERENCE_KEY_ID: &str = "shadow-reference";

/// 将持久化实体转换为运行时选择策略可理解的内部来源对象时发生的不变量错误。
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
#[error("调度运行时来源不变量无效")]
pub(super) enum SchedulerRuntimeSourceError {
    Invariant,
}

/// 已从数据库实体中提取并校验的渠道运行时来源。
pub(super) struct SchedulerRuntimeChannelSource {
    channel_id: ChannelId,
    channel_type: ChannelType,
    protocol: Protocol,
    base_url: Option<String>,
    timeout: Option<ChannelTimeout>,
    model_mappings: ChannelModelMappings,
    parameter_overrides: ChannelParameterOverrides,
    headers: Vec<(String, String)>,
    auto_ban_rules: ChannelAutoBanRules,
    pool_mode: bool,
    client_simulation_profile: Option<ClientSimulationProfile>,
    client_simulation_body_profile: Option<ClientSimulationBodyProfile>,
    responses_websocket_enabled: bool,
    responses_compact_mode: ResponsesCompactMode,
    responses_compact_probe_result: ResponsesCompactProbeResult,
    responses_compact_model_mapping: ChannelModelMappings,
}

impl SchedulerRuntimeChannelSource {
    /// 从 SeaORM 渠道实体提取运行时所需字段；禁用或删除的渠道直接 fail-closed 跳过。
    pub(super) fn from_entity(
        channel: channels::Model,
    ) -> Result<Option<Self>, SchedulerRuntimeSourceError> {
        let status =
            Status::try_from(channel.status).map_err(|_| SchedulerRuntimeSourceError::Invariant)?;
        if !status.is_enabled() || channel.deleted_at.is_some() {
            return Ok(None);
        }

        let channel_id =
            ChannelId::new(channel.id).map_err(|_| SchedulerRuntimeSourceError::Invariant)?;
        let channel_type = channel
            .r#type
            .parse()
            .map_err(|_| SchedulerRuntimeSourceError::Invariant)?;
        let protocol = channel
            .protocol
            .parse()
            .map_err(|_| SchedulerRuntimeSourceError::Invariant)?;
        let timeout = channel
            .timeout_secs
            .map(|value| {
                <u64 as std::convert::TryFrom<i32>>::try_from(value)
                    .ok()
                    .and_then(|value| ChannelTimeout::new(value).ok())
                    .ok_or(SchedulerRuntimeSourceError::Invariant)
            })
            .transpose()?;
        let model_mappings = ChannelModelMappings::parse(&channel.model_mapping)
            .map_err(|_| SchedulerRuntimeSourceError::Invariant)?;
        let parameter_overrides = ChannelParameterOverrides::parse(&channel.param_override)
            .map_err(|_| SchedulerRuntimeSourceError::Invariant)?;
        parameter_overrides
            .validate_for_protocol(protocol)
            .map_err(|_| SchedulerRuntimeSourceError::Invariant)?;
        let headers = channel
            .header_override
            .pairs()
            .map_err(|_| SchedulerRuntimeSourceError::Invariant)?;
        let settings = channel.settings.into_inner();
        let auto_ban_rules =
            auto_ban_rules(&settings).map_err(|_| SchedulerRuntimeSourceError::Invariant)?;
        let pool_mode = pool_mode(&settings).map_err(|_| SchedulerRuntimeSourceError::Invariant)?;
        let client_simulation_profile = client_simulation_profile(&settings)
            .map_err(|_| SchedulerRuntimeSourceError::Invariant)?;
        validate_client_simulation_capability(channel_type, protocol, client_simulation_profile)
            .map_err(|_| SchedulerRuntimeSourceError::Invariant)?;
        let client_simulation_body_profile = client_simulation_body_profile(&settings)
            .map_err(|_| SchedulerRuntimeSourceError::Invariant)?;
        validate_client_simulation_body_capability(
            channel_type,
            protocol,
            client_simulation_profile,
            client_simulation_body_profile,
        )
        .map_err(|_| SchedulerRuntimeSourceError::Invariant)?;
        let responses_websocket_enabled = responses_websocket_enabled(&settings)
            .map_err(|_| SchedulerRuntimeSourceError::Invariant)?;
        validate_responses_websocket_capability(
            channel_type,
            protocol,
            responses_websocket_enabled,
        )
        .map_err(|_| SchedulerRuntimeSourceError::Invariant)?;
        let responses_compact_mode = responses_compact_mode(&settings)
            .map_err(|_| SchedulerRuntimeSourceError::Invariant)?;
        validate_responses_compact_capability(channel_type, protocol, responses_compact_mode)
            .map_err(|_| SchedulerRuntimeSourceError::Invariant)?;
        let responses_compact_probe_result = responses_compact_probe_result(&settings)
            .map_err(|_| SchedulerRuntimeSourceError::Invariant)?;
        validate_responses_compact_probe_result(
            channel_type,
            protocol,
            responses_compact_probe_result,
        )
        .map_err(|_| SchedulerRuntimeSourceError::Invariant)?;
        let responses_compact_model_mapping = responses_compact_model_mapping(&settings)
            .map_err(|_| SchedulerRuntimeSourceError::Invariant)?;
        validate_responses_compact_model_mapping(
            channel_type,
            protocol,
            &responses_compact_model_mapping,
        )
        .map_err(|_| SchedulerRuntimeSourceError::Invariant)?;
        Ok(Some(Self {
            channel_id,
            channel_type,
            protocol,
            base_url: channel.base_url.map(|value| value.as_str().to_owned()),
            timeout,
            model_mappings,
            parameter_overrides,
            headers,
            auto_ban_rules,
            pool_mode,
            client_simulation_profile,
            client_simulation_body_profile,
            responses_websocket_enabled,
            responses_compact_mode,
            responses_compact_probe_result,
            responses_compact_model_mapping,
        }))
    }

    /// 返回渠道数据库标识。
    pub(super) const fn channel_id(&self) -> ChannelId {
        self.channel_id
    }

    /// 返回渠道适配器类型。
    pub(super) const fn channel_type(&self) -> ChannelType {
        self.channel_type
    }

    /// 返回渠道原生协议。
    pub(super) const fn protocol(&self) -> Protocol {
        self.protocol
    }

    /// 返回可选基础地址；调用方不得写入日志。
    pub(super) fn base_url(&self) -> Option<&str> {
        self.base_url.as_deref()
    }

    /// 返回渠道级读取与完整请求超时；空值表示继承全局配置。
    pub(super) const fn timeout(&self) -> Option<ChannelTimeout> {
        self.timeout
    }

    /// 返回已验证的精确模型映射。
    pub(super) const fn model_mappings(&self) -> &ChannelModelMappings {
        &self.model_mappings
    }

    /// 返回已验证的 Canonical 参数覆盖。
    pub(super) const fn parameter_overrides(&self) -> &ChannelParameterOverrides {
        &self.parameter_overrides
    }

    /// 返回已校验的非认证 Header 覆盖。
    pub(super) fn headers(&self) -> &[(String, String)] {
        &self.headers
    }

    /// 返回已校验的渠道自动禁用规则。
    pub(super) const fn auto_ban_rules(&self) -> &ChannelAutoBanRules {
        &self.auto_ban_rules
    }

    /// 返回渠道是否跳过渠道级本地故障状态。
    pub(super) const fn pool_mode(&self) -> bool {
        self.pool_mode
    }

    /// 返回渠道显式选择的版本化客户端仿真档案。
    pub(super) const fn client_simulation_profile(&self) -> Option<ClientSimulationProfile> {
        self.client_simulation_profile
    }

    /// 返回渠道显式选择的版本化客户端仿真正文档案。
    pub(super) const fn client_simulation_body_profile(
        &self,
    ) -> Option<ClientSimulationBodyProfile> {
        self.client_simulation_body_profile
    }

    /// 返回渠道显式开启的 Responses WebSocket 能力。
    pub(super) const fn responses_websocket_enabled(&self) -> bool {
        self.responses_websocket_enabled
    }

    /// 返回渠道配置的 Responses Compact 三态策略。
    pub(super) const fn responses_compact_mode(&self) -> ResponsesCompactMode {
        self.responses_compact_mode
    }

    /// 返回最近一次确定性 Compact 探测结论。
    pub(super) const fn responses_compact_probe_result(&self) -> ResponsesCompactProbeResult {
        self.responses_compact_probe_result
    }

    /// 返回已校验的 Compact 专属上游模型映射。
    pub(super) const fn responses_compact_model_mapping(&self) -> &ChannelModelMappings {
        &self.responses_compact_model_mapping
    }
}

/// 已从数据库实体中提取并校验的凭据运行时来源。
pub(super) struct SchedulerRuntimeCredentialSource {
    routing_credential_id: CredentialId,
    secret_owner_id: CredentialId,
    concurrency_owner_id: CredentialId,
    shared_health_id: CredentialId,
    quota_dimension: CredentialQuotaDimension,
    channel_id: ChannelId,
    credential_kind: CredentialKind,
    envelope: EncryptedCredentialEnvelope,
    proxy: Option<SchedulerRuntimeProxyRecord>,
    priority: i32,
    weight: u32,
    concurrency: Option<ConcurrencyLimit>,
    oauth_provider: Option<String>,
    oauth_account_key: Option<String>,
    rate_limit_reset_at: Option<TimeDateTimeWithTimeZone>,
    overload_until: Option<TimeDateTimeWithTimeZone>,
    temp_unschedulable_until: Option<TimeDateTimeWithTimeZone>,
    shared_auth_unschedulable_until: Option<TimeDateTimeWithTimeZone>,
}

impl SchedulerRuntimeCredentialSource {
    /// 从 SeaORM 凭据实体提取运行时所需字段；禁用、删除或不可调度凭据直接 fail-closed 跳过。
    pub(super) fn from_entity(
        credential: credentials::Model,
        parent: Option<credentials::Model>,
        proxy: Option<SchedulerRuntimeProxyRecord>,
    ) -> Result<Option<Self>, SchedulerRuntimeSourceError> {
        let routing_credential_id =
            CredentialId::new(credential.id).map_err(|_| SchedulerRuntimeSourceError::Invariant)?;
        let status = Status::try_from(credential.status)
            .map_err(|_| SchedulerRuntimeSourceError::Invariant)?;
        if !status.is_enabled()
            || !credential.schedulable
            || credential.oauth_token_pending
            || credential.deleted_at.is_some()
        {
            return Ok(None);
        }

        let channel_id = ChannelId::new(credential.channel_id)
            .map_err(|_| SchedulerRuntimeSourceError::Invariant)?;
        let credential_kind = credential
            .kind
            .parse()
            .map_err(|_| SchedulerRuntimeSourceError::Invariant)?;
        let quota_dimension = credential
            .quota_dimension
            .parse::<CredentialQuotaDimension>()
            .map_err(|_| SchedulerRuntimeSourceError::Invariant)?;
        let (secret_owner, secret_owner_id, concurrency, shared_auth_unschedulable_until) =
            match quota_dimension {
                CredentialQuotaDimension::Global => {
                    if credential.parent_id.is_some() || parent.is_some() {
                        return Err(SchedulerRuntimeSourceError::Invariant);
                    }
                    (
                        credential.clone(),
                        routing_credential_id,
                        parse_concurrency(credential.concurrency)?,
                        None,
                    )
                }
                CredentialQuotaDimension::Spark => {
                    let parent = parent.ok_or(SchedulerRuntimeSourceError::Invariant)?;
                    let parent_id = CredentialId::new(parent.id)
                        .map_err(|_| SchedulerRuntimeSourceError::Invariant)?;
                    let parent_status = Status::try_from(parent.status)
                        .map_err(|_| SchedulerRuntimeSourceError::Invariant)?;
                    if !parent_status.is_enabled()
                        || parent.oauth_token_pending
                        || parent.deleted_at.is_some()
                    {
                        return Ok(None);
                    }
                    let parent_kind = parent
                        .kind
                        .parse::<CredentialKind>()
                        .map_err(|_| SchedulerRuntimeSourceError::Invariant)?;
                    let parent_dimension = parent
                        .quota_dimension
                        .parse::<CredentialQuotaDimension>()
                        .map_err(|_| SchedulerRuntimeSourceError::Invariant)?;
                    let shadow_marker = credential
                        .secret
                        .envelope_parts()
                        .map_err(|_| SchedulerRuntimeSourceError::Invariant)?
                        .0;
                    if credential_kind != CredentialKind::Oauth
                        || credential.parent_id != Some(parent.id)
                        || credential.proxy_id.is_some()
                        || credential.concurrency.is_some()
                        || credential.oauth_provider.is_some()
                        || credential.oauth_account_key.is_some()
                        || credential.oauth_project_id.is_some()
                        || shadow_marker != SHADOW_REFERENCE_KEY_ID
                        || parent_kind != CredentialKind::Oauth
                        || parent.parent_id.is_some()
                        || parent_dimension != CredentialQuotaDimension::Global
                    {
                        return Err(SchedulerRuntimeSourceError::Invariant);
                    }
                    let concurrency = parse_concurrency(parent.concurrency)?;
                    let shared_auth_unschedulable_until = shared_auth_unschedulable_until(&parent)?;
                    (
                        parent,
                        parent_id,
                        concurrency,
                        shared_auth_unschedulable_until,
                    )
                }
            };
        let (key_id, nonce, ciphertext) = secret_owner
            .secret
            .envelope_parts()
            .map_err(|_| SchedulerRuntimeSourceError::Invariant)?;
        if key_id == SHADOW_REFERENCE_KEY_ID {
            return Err(SchedulerRuntimeSourceError::Invariant);
        }
        let envelope = EncryptedCredentialEnvelope::new(key_id, nonce, ciphertext)
            .map_err(|_| SchedulerRuntimeSourceError::Invariant)?;
        let oauth_provider = secret_owner.oauth_provider.clone();
        let oauth_account_key = secret_owner.oauth_account_key.clone();
        let weight =
            u32::try_from(credential.weight).map_err(|_| SchedulerRuntimeSourceError::Invariant)?;
        if secret_owner.proxy_id.is_some() != proxy.is_some()
            || secret_owner.proxy_id != proxy.as_ref().map(|proxy| proxy.proxy_id().get())
        {
            return Err(SchedulerRuntimeSourceError::Invariant);
        }
        Ok(Some(Self {
            routing_credential_id,
            secret_owner_id,
            concurrency_owner_id: secret_owner_id,
            shared_health_id: secret_owner_id,
            quota_dimension,
            channel_id,
            credential_kind,
            envelope,
            proxy,
            priority: credential.priority,
            weight,
            concurrency,
            oauth_provider,
            oauth_account_key,
            rate_limit_reset_at: credential.rate_limit_reset_at,
            overload_until: credential.overload_until,
            temp_unschedulable_until: credential.temp_unschedulable_until,
            shared_auth_unschedulable_until,
        }))
    }

    /// 返回所属渠道数据库标识。
    pub(super) const fn channel_id(&self) -> ChannelId {
        self.channel_id
    }

    /// 判断凭据当前是否仍处于临时不可调度窗口。
    pub(super) fn is_cooling_down(&self, now: &TimeDateTimeWithTimeZone) -> bool {
        [
            self.rate_limit_reset_at.as_ref(),
            self.overload_until.as_ref(),
            self.temp_unschedulable_until.as_ref(),
            self.shared_auth_unschedulable_until.as_ref(),
        ]
        .into_iter()
        .flatten()
        .any(|until| until > now)
    }

    /// 消费来源对象，返回构造运行时凭据记录所需的安全字段。
    #[allow(clippy::type_complexity)]
    pub(super) fn into_runtime_parts(
        self,
    ) -> (
        CredentialId,
        CredentialId,
        CredentialId,
        CredentialId,
        CredentialQuotaDimension,
        CredentialKind,
        EncryptedCredentialEnvelope,
        Option<SchedulerRuntimeProxyRecord>,
        i32,
        u32,
        Option<ConcurrencyLimit>,
        Option<String>,
        Option<String>,
    ) {
        (
            self.routing_credential_id,
            self.secret_owner_id,
            self.concurrency_owner_id,
            self.shared_health_id,
            self.quota_dimension,
            self.credential_kind,
            self.envelope,
            self.proxy,
            self.priority,
            self.weight,
            self.concurrency,
            self.oauth_provider,
            self.oauth_account_key,
        )
    }
}

fn parse_concurrency(
    value: Option<i32>,
) -> Result<Option<ConcurrencyLimit>, SchedulerRuntimeSourceError> {
    match value {
        None | Some(0) => Ok(None),
        Some(value) if value > 0 => u32::try_from(value)
            .ok()
            .and_then(|value| ConcurrencyLimit::new(value).ok())
            .map(Some)
            .ok_or(SchedulerRuntimeSourceError::Invariant),
        Some(_) => Err(SchedulerRuntimeSourceError::Invariant),
    }
}

fn shared_auth_unschedulable_until(
    parent: &credentials::Model,
) -> Result<Option<TimeDateTimeWithTimeZone>, SchedulerRuntimeSourceError> {
    match (
        parent.temp_unschedulable_until.as_ref(),
        parent.temp_unschedulable_reason.as_deref(),
    ) {
        (None, None) => Ok(None),
        (Some(_), Some("quota_exhausted")) => Ok(None),
        (Some(until), Some("auth_expired" | "oauth_refresh_transient")) => {
            Ok(Some(until.to_owned()))
        }
        // 未识别的共享冷却原因不能放行引用同一密钥的影子。
        (Some(until), Some(_)) => Ok(Some(until.to_owned())),
        _ => Err(SchedulerRuntimeSourceError::Invariant),
    }
}

/// 把启用且未删除的代理实体转换为密文运行时快照。
pub(super) fn proxy_from_entity(
    proxy: proxies::Model,
) -> Result<SchedulerRuntimeProxyRecord, SchedulerRuntimeSourceError> {
    if !proxy.enabled || proxy.deleted_at.is_some() {
        return Err(SchedulerRuntimeSourceError::Invariant);
    }
    let proxy_id = ProxyId::new(proxy.id).map_err(|_| SchedulerRuntimeSourceError::Invariant)?;
    let scheme = CredentialProxyScheme::parse(&proxy.scheme)
        .map_err(|_| SchedulerRuntimeSourceError::Invariant)?;
    let password_secret = proxy
        .password_secret
        .map(|secret| {
            let (key_id, nonce, ciphertext) = secret
                .envelope_parts()
                .map_err(|_| SchedulerRuntimeSourceError::Invariant)?;
            EncryptedCredentialEnvelope::new(key_id, nonce, ciphertext)
                .map_err(|_| SchedulerRuntimeSourceError::Invariant)
        })
        .transpose()?;
    SchedulerRuntimeProxyRecord::new(
        proxy_id,
        scheme,
        proxy.host.as_str().to_owned(),
        u16::try_from(proxy.port).map_err(|_| SchedulerRuntimeSourceError::Invariant)?,
        proxy.username.map(|value| value.as_str().to_owned()),
        password_secret,
        proxy.trust_proxy_dns,
        proxy.version,
    )
    .map_err(|_| SchedulerRuntimeSourceError::Invariant)
}
