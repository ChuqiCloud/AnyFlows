use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    sync::Arc,
};

use af_domain::{
    ChannelId, ChannelTimeout, ChannelType, ClientSimulationBodyProfile, ClientSimulationProfile,
    CredentialId, CredentialKind, CredentialQuotaDimension, GroupId, Protocol,
    ResponsesCompactMode, ResponsesCompactProbeResult,
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use sea_orm::entity::prelude::Json;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    SchedulerAbilityRecord, SchedulerCatalogSubject, credential_probe::EncryptedCredentialEnvelope,
};

use super::{
    ChannelModelMappings, ChannelParameterOverrides, SchedulerRuntimeCredentialRecord,
    SchedulerRuntimeProxyRecord, SchedulerRuntimeRecord, SchedulerRuntimeTargetRecord,
};

const PROJECTION_WIRE_VERSION: u8 = 1;
/// 单个主体投影允许写入 Redis 的最大字节数。
pub const MAX_SCHEDULER_RUNTIME_PROJECTION_BYTES: usize = 16 * 1024 * 1024;

/// 版本化调度运行时投影违反编码、容量或强类型不变量。
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum SchedulerRuntimeProjectionError {
    /// 投影无法编码为当前闭合 wire。
    #[error("编码调度运行时投影失败")]
    Encode,
    /// 投影不是当前支持的闭合 wire。
    #[error("解析调度运行时投影失败")]
    Decode,
    /// 投影超过单主体安全容量。
    #[error("调度运行时投影超过容量上限")]
    TooLarge,
    /// 版本、主体或运行时记录违反不变量。
    #[error("调度运行时投影状态损坏")]
    Invariant,
}

/// 可写入 Redis 并按主体增量应用的版本化运行时目录。
pub struct SchedulerRuntimeProjection {
    version: u64,
    subject: SchedulerCatalogSubject,
    records: Vec<SchedulerRuntimeRecord>,
}

impl SchedulerRuntimeProjection {
    /// 使用 outbox 事件版本与数据库当前主体真相构造投影。
    pub fn new(
        version: u64,
        subject: SchedulerCatalogSubject,
        records: Vec<SchedulerRuntimeRecord>,
    ) -> Result<Self, SchedulerRuntimeProjectionError> {
        if version == 0
            || version > i64::MAX as u64
            || records.len() > crate::MAX_SCHEDULER_ABILITY_SNAPSHOT_ENTRIES
        {
            return Err(SchedulerRuntimeProjectionError::Invariant);
        }
        for record in &records {
            if !record_matches_subject(record, subject)
                || record.ability().channel_id() != record.target().channel_id()
            {
                return Err(SchedulerRuntimeProjectionError::Invariant);
            }
        }
        Ok(Self {
            version,
            subject,
            records,
        })
    }

    /// 返回作为主体单调版本的 outbox 事件标识。
    #[must_use]
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// 返回本投影唯一允许替换的闭合主体。
    #[must_use]
    pub const fn subject(&self) -> SchedulerCatalogSubject {
        self.subject
    }

    /// 返回已验证的运行时记录；模型与运行时配置不得写入日志。
    #[must_use]
    pub fn records(&self) -> &[SchedulerRuntimeRecord] {
        &self.records
    }

    /// 消费投影并返回增量应用所需的强类型组成部分。
    #[must_use]
    pub fn into_parts(self) -> (u64, SchedulerCatalogSubject, Vec<SchedulerRuntimeRecord>) {
        (self.version, self.subject, self.records)
    }

    /// 编码闭合 wire；输出可能包含模型、地址、Header 与加密凭据，不得进入日志或广播。
    pub fn encode(&self) -> Result<Vec<u8>, SchedulerRuntimeProjectionError> {
        let wire = ProjectionWire::try_from(self)?;
        let payload =
            serde_json::to_vec(&wire).map_err(|_| SchedulerRuntimeProjectionError::Encode)?;
        if payload.is_empty() || payload.len() > MAX_SCHEDULER_RUNTIME_PROJECTION_BYTES {
            return Err(SchedulerRuntimeProjectionError::TooLarge);
        }
        Ok(payload)
    }

    /// 解码 Redis 中的闭合投影，并重新执行所有运行时构造校验。
    pub fn decode(payload: &[u8]) -> Result<Self, SchedulerRuntimeProjectionError> {
        if payload.is_empty() || payload.len() > MAX_SCHEDULER_RUNTIME_PROJECTION_BYTES {
            return Err(SchedulerRuntimeProjectionError::TooLarge);
        }
        let wire: ProjectionWire =
            serde_json::from_slice(payload).map_err(|_| SchedulerRuntimeProjectionError::Decode)?;
        wire.try_into_projection()
    }
}

impl fmt::Debug for SchedulerRuntimeProjection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SchedulerRuntimeProjection")
            .field("version", &self.version)
            .field("subject", &self.subject)
            .field("record_count", &self.records.len())
            .finish()
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ProjectionWire {
    wire_version: u8,
    projection_version: u64,
    subject: ProjectionSubjectWire,
    abilities: Vec<ProjectionAbilityWire>,
    targets: Vec<ProjectionTargetWire>,
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum ProjectionSubjectWire {
    Channel { id: i64 },
    Group { id: i64 },
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ProjectionAbilityWire {
    group_id: i64,
    model: String,
    channel_id: i64,
    priority: i32,
    weight: u32,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ProjectionTargetWire {
    channel_id: i64,
    channel_type: String,
    protocol: String,
    base_url: Option<String>,
    timeout_seconds: Option<u64>,
    credentials: Vec<ProjectionCredentialWire>,
    model_mappings: Json,
    parameter_overrides: Json,
    headers: Vec<ProjectionHeaderWire>,
    #[serde(default)]
    auto_ban_status_codes: Vec<u16>,
    #[serde(default)]
    auto_ban_keywords: Vec<String>,
    #[serde(default)]
    pool_mode: bool,
    /// 旧投影缺少该字段时保持默认关闭。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    client_simulation_profile: Option<String>,
    /// 旧投影缺少该字段时保持默认关闭。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    client_simulation_body_profile: Option<String>,
    responses_websocket_enabled: bool,
    #[serde(default)]
    responses_compact_mode: Option<String>,
    /// 旧投影缺少该字段时必须失败关闭为未知。
    #[serde(default)]
    responses_compact_probe_result: Option<String>,
    /// 旧投影缺少该字段时按空映射兼容。
    #[serde(default)]
    responses_compact_model_mapping: Option<Json>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ProjectionCredentialWire {
    credential_id: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    secret_owner_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    concurrency_owner_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    shared_health_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    quota_dimension: Option<String>,
    credential_kind: String,
    /// 旧投影缺少 OAuth 身份字段时按普通凭据兼容。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    oauth_provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    oauth_account_key: Option<String>,
    key_id: String,
    nonce_base64: String,
    ciphertext_base64: String,
    proxy_required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    proxy: Option<ProjectionProxyWire>,
    priority: i32,
    weight: u32,
    #[serde(default)]
    concurrency: Option<u32>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ProjectionProxyWire {
    proxy_id: i64,
    scheme: String,
    host: String,
    port: u16,
    username: Option<String>,
    password: Option<ProjectionSecretWire>,
    trust_proxy_dns: bool,
    version: i64,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ProjectionSecretWire {
    key_id: String,
    nonce_base64: String,
    ciphertext_base64: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ProjectionHeaderWire {
    name: String,
    value: String,
}

impl TryFrom<&SchedulerRuntimeProjection> for ProjectionWire {
    type Error = SchedulerRuntimeProjectionError;

    fn try_from(projection: &SchedulerRuntimeProjection) -> Result<Self, Self::Error> {
        let mut targets = BTreeMap::<ChannelId, &SchedulerRuntimeTargetRecord>::new();
        let mut abilities = Vec::with_capacity(projection.records.len());
        for record in &projection.records {
            let ability = record.ability();
            abilities.push(ProjectionAbilityWire {
                group_id: ability.group_id().get(),
                model: ability.model().to_owned(),
                channel_id: ability.channel_id().get(),
                priority: ability.priority(),
                weight: ability.weight(),
            });
            match targets.get(&ability.channel_id()) {
                Some(existing) if *existing != record.target() => {
                    return Err(SchedulerRuntimeProjectionError::Invariant);
                }
                Some(_) => {}
                None => {
                    targets.insert(ability.channel_id(), record.target());
                }
            }
        }
        let targets = targets
            .into_values()
            .map(ProjectionTargetWire::from_target)
            .collect();
        Ok(Self {
            wire_version: PROJECTION_WIRE_VERSION,
            projection_version: projection.version,
            subject: projection.subject.into(),
            abilities,
            targets,
        })
    }
}

impl ProjectionWire {
    fn try_into_projection(
        self,
    ) -> Result<SchedulerRuntimeProjection, SchedulerRuntimeProjectionError> {
        if self.wire_version != PROJECTION_WIRE_VERSION
            || self.projection_version == 0
            || self.abilities.len() > crate::MAX_SCHEDULER_ABILITY_SNAPSHOT_ENTRIES
            || self.targets.len() > self.abilities.len()
        {
            return Err(SchedulerRuntimeProjectionError::Invariant);
        }
        let subject = self.subject.try_into_subject()?;
        let mut targets = BTreeMap::<ChannelId, Arc<SchedulerRuntimeTargetRecord>>::new();
        for target in self.targets {
            let target = target.try_into_target()?;
            let channel_id = target.channel_id();
            if targets.insert(channel_id, Arc::new(target)).is_some() {
                return Err(SchedulerRuntimeProjectionError::Invariant);
            }
        }

        let mut used_targets = BTreeSet::new();
        let mut records = Vec::with_capacity(self.abilities.len());
        for ability in self.abilities {
            let group_id = GroupId::new(ability.group_id)
                .map_err(|_| SchedulerRuntimeProjectionError::Invariant)?;
            let channel_id = ChannelId::new(ability.channel_id)
                .map_err(|_| SchedulerRuntimeProjectionError::Invariant)?;
            let target = targets
                .get(&channel_id)
                .cloned()
                .ok_or(SchedulerRuntimeProjectionError::Invariant)?;
            let ability = SchedulerAbilityRecord::from_projection_parts(
                group_id,
                ability.model,
                channel_id,
                ability.priority,
                ability.weight,
            )
            .map_err(|_| SchedulerRuntimeProjectionError::Invariant)?;
            used_targets.insert(channel_id);
            records.push(SchedulerRuntimeRecord::new(ability, target));
        }
        if used_targets.len() != targets.len() {
            return Err(SchedulerRuntimeProjectionError::Invariant);
        }
        SchedulerRuntimeProjection::new(self.projection_version, subject, records)
    }
}

impl From<SchedulerCatalogSubject> for ProjectionSubjectWire {
    fn from(subject: SchedulerCatalogSubject) -> Self {
        match subject {
            SchedulerCatalogSubject::Channel(channel_id) => Self::Channel {
                id: channel_id.get(),
            },
            SchedulerCatalogSubject::Group(group_id) => Self::Group { id: group_id.get() },
        }
    }
}

impl ProjectionSubjectWire {
    fn try_into_subject(self) -> Result<SchedulerCatalogSubject, SchedulerRuntimeProjectionError> {
        match self {
            Self::Channel { id } => ChannelId::new(id)
                .map(SchedulerCatalogSubject::Channel)
                .map_err(|_| SchedulerRuntimeProjectionError::Invariant),
            Self::Group { id } => GroupId::new(id)
                .map(SchedulerCatalogSubject::Group)
                .map_err(|_| SchedulerRuntimeProjectionError::Invariant),
        }
    }
}

impl ProjectionTargetWire {
    fn from_target(target: &SchedulerRuntimeTargetRecord) -> Self {
        Self {
            channel_id: target.channel_id().get(),
            channel_type: target.channel_type().to_string(),
            protocol: target.protocol().to_string(),
            base_url: target.base_url().map(str::to_owned),
            timeout_seconds: target.timeout().map(ChannelTimeout::seconds),
            credentials: target
                .credentials()
                .iter()
                .map(ProjectionCredentialWire::from_credential)
                .collect(),
            model_mappings: target.model_mappings().to_projection_json(),
            parameter_overrides: target.parameter_overrides().to_projection_json(),
            headers: target
                .headers()
                .iter()
                .map(|header| ProjectionHeaderWire {
                    name: header.name().to_owned(),
                    value: header.value().to_owned(),
                })
                .collect(),
            auto_ban_status_codes: target
                .auto_ban_rules()
                .server_statuses()
                .iter()
                .map(|status| status.get())
                .collect(),
            auto_ban_keywords: target.auto_ban_rules().keywords().to_vec(),
            pool_mode: target.pool_mode(),
            client_simulation_profile: target
                .client_simulation_profile()
                .map(|profile| profile.to_string()),
            client_simulation_body_profile: target
                .client_simulation_body_profile()
                .map(|profile| profile.to_string()),
            responses_websocket_enabled: target.responses_websocket_enabled(),
            responses_compact_mode: Some(target.responses_compact_mode().to_string()),
            responses_compact_probe_result: Some(
                target.responses_compact_probe_result().to_string(),
            ),
            responses_compact_model_mapping: Some(
                target
                    .responses_compact_model_mapping()
                    .to_projection_json(),
            ),
        }
    }

    fn try_into_target(
        self,
    ) -> Result<SchedulerRuntimeTargetRecord, SchedulerRuntimeProjectionError> {
        let channel_id = ChannelId::new(self.channel_id)
            .map_err(|_| SchedulerRuntimeProjectionError::Invariant)?;
        let channel_type: ChannelType = self
            .channel_type
            .parse()
            .map_err(|_| SchedulerRuntimeProjectionError::Invariant)?;
        let protocol: Protocol = self
            .protocol
            .parse()
            .map_err(|_| SchedulerRuntimeProjectionError::Invariant)?;
        let timeout = self
            .timeout_seconds
            .map(|seconds| {
                ChannelTimeout::new(seconds).map_err(|_| SchedulerRuntimeProjectionError::Invariant)
            })
            .transpose()?;
        let credentials = self
            .credentials
            .into_iter()
            .map(ProjectionCredentialWire::try_into_credential)
            .collect::<Result<Vec<_>, _>>()?;
        let model_mappings = ChannelModelMappings::parse(&self.model_mappings)
            .map_err(|_| SchedulerRuntimeProjectionError::Invariant)?;
        let parameter_overrides = ChannelParameterOverrides::parse(&self.parameter_overrides)
            .map_err(|_| SchedulerRuntimeProjectionError::Invariant)?;
        let headers = self
            .headers
            .into_iter()
            .map(|header| (header.name, header.value))
            .collect();
        let auto_ban_rules =
            af_domain::ChannelAutoBanRules::new(self.auto_ban_status_codes, self.auto_ban_keywords)
                .map_err(|_| SchedulerRuntimeProjectionError::Invariant)?;
        let client_simulation_profile = self
            .client_simulation_profile
            .map(|profile| profile.parse::<ClientSimulationProfile>())
            .transpose()
            .map_err(|_| SchedulerRuntimeProjectionError::Invariant)?;
        let client_simulation_body_profile = self
            .client_simulation_body_profile
            .map(|profile| profile.parse::<ClientSimulationBodyProfile>())
            .transpose()
            .map_err(|_| SchedulerRuntimeProjectionError::Invariant)?;
        let responses_compact_mode = self
            .responses_compact_mode
            .unwrap_or_else(|| ResponsesCompactMode::Auto.to_string())
            .parse::<ResponsesCompactMode>()
            .map_err(|_| SchedulerRuntimeProjectionError::Invariant)?;
        let responses_compact_probe_result = self
            .responses_compact_probe_result
            .unwrap_or_else(|| ResponsesCompactProbeResult::Unknown.to_string())
            .parse::<ResponsesCompactProbeResult>()
            .map_err(|_| SchedulerRuntimeProjectionError::Invariant)?;
        let responses_compact_model_mapping = self
            .responses_compact_model_mapping
            .unwrap_or_else(|| Json::Object(Default::default()));
        let responses_compact_model_mapping =
            ChannelModelMappings::parse(&responses_compact_model_mapping)
                .map_err(|_| SchedulerRuntimeProjectionError::Invariant)?;
        SchedulerRuntimeTargetRecord::new_pool_with_request_policy_and_compact_mapping(
            channel_id,
            channel_type,
            protocol,
            self.base_url,
            credentials,
            model_mappings,
            responses_compact_model_mapping,
            parameter_overrides,
            headers,
        )
        .map_err(|_| SchedulerRuntimeProjectionError::Invariant)?
        .with_timeout(timeout)
        .with_auto_ban_rules(auto_ban_rules)
        .with_pool_mode(self.pool_mode)
        .with_client_simulation_profile(client_simulation_profile)
        .and_then(|target| {
            target.with_client_simulation_body_profile(client_simulation_body_profile)
        })
        .and_then(|target| {
            target.with_responses_websocket_enabled(self.responses_websocket_enabled)
        })
        .and_then(|target| target.with_responses_compact_mode(responses_compact_mode))
        .and_then(|target| {
            target.with_responses_compact_probe_result(responses_compact_probe_result)
        })
        .map_err(|_| SchedulerRuntimeProjectionError::Invariant)
    }
}

impl ProjectionCredentialWire {
    fn from_credential(credential: &SchedulerRuntimeCredentialRecord) -> Self {
        Self {
            credential_id: credential.credential_id(),
            secret_owner_id: Some(credential.secret_owner_id().get()),
            concurrency_owner_id: Some(credential.concurrency_owner_id().get()),
            shared_health_id: Some(credential.shared_health_id().get()),
            quota_dimension: Some(credential.quota_dimension().to_string()),
            credential_kind: credential.credential_kind().to_string(),
            oauth_provider: credential.oauth_provider().map(str::to_owned),
            oauth_account_key: credential.oauth_account_key().map(str::to_owned),
            key_id: credential.envelope().key_id().to_owned(),
            nonce_base64: BASE64_STANDARD.encode(credential.envelope().nonce()),
            ciphertext_base64: BASE64_STANDARD.encode(credential.envelope().ciphertext()),
            proxy_required: credential.proxy_required(),
            proxy: credential.proxy().map(ProjectionProxyWire::from_proxy),
            priority: credential.priority(),
            weight: credential.weight(),
            concurrency: credential
                .concurrency()
                .map(af_domain::ConcurrencyLimit::get),
        }
    }

    fn try_into_credential(
        self,
    ) -> Result<SchedulerRuntimeCredentialRecord, SchedulerRuntimeProjectionError> {
        let routing_credential_id = CredentialId::new(self.credential_id)
            .map_err(|_| SchedulerRuntimeProjectionError::Invariant)?;
        let parse_owner = |value: Option<i64>| {
            value
                .map(CredentialId::new)
                .transpose()
                .map(|value| value.unwrap_or(routing_credential_id))
                .map_err(|_| SchedulerRuntimeProjectionError::Invariant)
        };
        let secret_owner_id = parse_owner(self.secret_owner_id)?;
        let concurrency_owner_id = parse_owner(self.concurrency_owner_id)?;
        let shared_health_id = parse_owner(self.shared_health_id)?;
        let quota_dimension = self
            .quota_dimension
            .map(|value| value.parse::<CredentialQuotaDimension>())
            .transpose()
            .map_err(|_| SchedulerRuntimeProjectionError::Invariant)?
            .unwrap_or(CredentialQuotaDimension::Global);
        let credential_kind: CredentialKind = self
            .credential_kind
            .parse()
            .map_err(|_| SchedulerRuntimeProjectionError::Invariant)?;
        let nonce = BASE64_STANDARD
            .decode(self.nonce_base64)
            .map_err(|_| SchedulerRuntimeProjectionError::Invariant)?
            .try_into()
            .map_err(|_: Vec<u8>| SchedulerRuntimeProjectionError::Invariant)?;
        let ciphertext = BASE64_STANDARD
            .decode(self.ciphertext_base64)
            .map_err(|_| SchedulerRuntimeProjectionError::Invariant)?;
        let envelope = EncryptedCredentialEnvelope::new(self.key_id, nonce, ciphertext)
            .map_err(|_| SchedulerRuntimeProjectionError::Invariant)?;
        let concurrency = self
            .concurrency
            .map(af_domain::ConcurrencyLimit::new)
            .transpose()
            .map_err(|_| SchedulerRuntimeProjectionError::Invariant)?;
        let mut credential = SchedulerRuntimeCredentialRecord::with_runtime_identity(
            routing_credential_id,
            secret_owner_id,
            concurrency_owner_id,
            shared_health_id,
            quota_dimension,
            credential_kind,
            envelope,
            self.proxy_required,
            self.priority,
            self.weight,
            concurrency,
        )
        .map_err(|_| SchedulerRuntimeProjectionError::Invariant)?;
        credential = credential.with_oauth_identity(self.oauth_provider, self.oauth_account_key);
        if let Some(proxy) = self.proxy {
            credential = credential
                .with_proxy(proxy.try_into_proxy()?)
                .map_err(|_| SchedulerRuntimeProjectionError::Invariant)?;
        }
        Ok(credential)
    }
}

impl ProjectionProxyWire {
    fn from_proxy(proxy: &SchedulerRuntimeProxyRecord) -> Self {
        let password = proxy.password_secret().map(|secret| ProjectionSecretWire {
            key_id: secret.key_id().to_owned(),
            nonce_base64: BASE64_STANDARD.encode(secret.nonce()),
            ciphertext_base64: BASE64_STANDARD.encode(secret.ciphertext()),
        });
        Self {
            proxy_id: proxy.proxy_id().get(),
            scheme: proxy.scheme().as_str().to_owned(),
            host: proxy.host().to_owned(),
            port: proxy.port(),
            username: proxy.username().map(str::to_owned),
            password,
            trust_proxy_dns: proxy.trust_proxy_dns(),
            version: proxy.version(),
        }
    }

    fn try_into_proxy(
        self,
    ) -> Result<SchedulerRuntimeProxyRecord, SchedulerRuntimeProjectionError> {
        let proxy_id = af_domain::ProxyId::new(self.proxy_id)
            .map_err(|_| SchedulerRuntimeProjectionError::Invariant)?;
        let scheme = crate::CredentialProxyScheme::parse(&self.scheme)
            .map_err(|_| SchedulerRuntimeProjectionError::Invariant)?;
        let password = self
            .password
            .map(ProjectionSecretWire::try_into_envelope)
            .transpose()?;
        SchedulerRuntimeProxyRecord::new(
            proxy_id,
            scheme,
            self.host,
            self.port,
            self.username,
            password,
            self.trust_proxy_dns,
            self.version,
        )
        .map_err(|_| SchedulerRuntimeProjectionError::Invariant)
    }
}

impl ProjectionSecretWire {
    fn try_into_envelope(
        self,
    ) -> Result<EncryptedCredentialEnvelope, SchedulerRuntimeProjectionError> {
        let nonce = BASE64_STANDARD
            .decode(self.nonce_base64)
            .map_err(|_| SchedulerRuntimeProjectionError::Invariant)?
            .try_into()
            .map_err(|_: Vec<u8>| SchedulerRuntimeProjectionError::Invariant)?;
        let ciphertext = BASE64_STANDARD
            .decode(self.ciphertext_base64)
            .map_err(|_| SchedulerRuntimeProjectionError::Invariant)?;
        EncryptedCredentialEnvelope::new(self.key_id, nonce, ciphertext)
            .map_err(|_| SchedulerRuntimeProjectionError::Invariant)
    }
}

fn record_matches_subject(
    record: &SchedulerRuntimeRecord,
    subject: SchedulerCatalogSubject,
) -> bool {
    match subject {
        SchedulerCatalogSubject::Channel(channel_id) => record.ability().channel_id() == channel_id,
        SchedulerCatalogSubject::Group(group_id) => record.ability().group_id() == group_id,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projection_round_trips_without_exposing_sensitive_debug_fields() {
        let channel_id = ChannelId::new(7).unwrap();
        let target = Arc::new(
            SchedulerRuntimeTargetRecord::new_pool_with_request_policy_and_compact_mapping(
                channel_id,
                ChannelType::OpenAi,
                Protocol::OpenAiResponses,
                Some("https://private.example/v1".to_owned()),
                vec![
                    SchedulerRuntimeCredentialRecord::new(
                        9,
                        CredentialKind::ApiKey,
                        EncryptedCredentialEnvelope::new("private-key-id", [1; 24], vec![2; 16])
                            .unwrap(),
                        false,
                    )
                    .unwrap(),
                ],
                ChannelModelMappings::parse(&serde_json::json!({
                    "private-model": "private-upstream-model"
                }))
                .unwrap(),
                ChannelModelMappings::parse(&serde_json::json!({
                    "private-upstream-model": "private-compact-model"
                }))
                .unwrap(),
                ChannelParameterOverrides::default(),
                vec![("x-private-header".to_owned(), "private-value".to_owned())],
            )
            .unwrap()
            .with_auto_ban_rules(
                af_domain::ChannelAutoBanRules::new(
                    vec![503],
                    vec!["private-rule-canary".to_owned()],
                )
                .unwrap(),
            )
            .with_pool_mode(true)
            .with_responses_compact_mode(ResponsesCompactMode::ForceOn)
            .and_then(|target| {
                target.with_responses_compact_probe_result(ResponsesCompactProbeResult::Supported)
            })
            .unwrap(),
        );
        let record = SchedulerRuntimeRecord::new(
            SchedulerAbilityRecord::from_projection_parts(
                GroupId::new(8).unwrap(),
                "private-model".to_owned(),
                channel_id,
                3,
                4,
            )
            .unwrap(),
            target,
        );
        let projection = SchedulerRuntimeProjection::new(
            11,
            SchedulerCatalogSubject::Channel(channel_id),
            vec![record],
        )
        .unwrap();

        let payload = projection.encode().unwrap();
        let decoded = SchedulerRuntimeProjection::decode(&payload).unwrap();
        assert_eq!(decoded.version(), 11);
        assert_eq!(decoded.subject(), projection.subject());
        assert_eq!(decoded.records(), projection.records());
        assert!(decoded.records()[0].target().pool_mode());
        assert_eq!(
            decoded.records()[0]
                .target()
                .responses_compact_probe_result(),
            ResponsesCompactProbeResult::Supported
        );
        assert_eq!(
            decoded.records()[0]
                .target()
                .mapped_responses_compact_model("private-model"),
            "private-compact-model"
        );

        // 普通账号保持旧 wire 形态；旧投影缺少并发字段时继续解码为不限并发。
        let mut old_wire: serde_json::Value = serde_json::from_slice(&payload).unwrap();
        assert!(
            old_wire["targets"][0]["credentials"][0]
                .get("proxy")
                .is_none()
        );
        for target in old_wire["targets"].as_array_mut().unwrap() {
            target.as_object_mut().unwrap().remove("pool_mode");
            target
                .as_object_mut()
                .unwrap()
                .remove("responses_compact_mode");
            target
                .as_object_mut()
                .unwrap()
                .remove("responses_compact_probe_result");
            target
                .as_object_mut()
                .unwrap()
                .remove("responses_compact_model_mapping");
            for credential in target["credentials"].as_array_mut().unwrap() {
                credential.as_object_mut().unwrap().remove("concurrency");
            }
        }
        let old_payload = serde_json::to_vec(&old_wire).unwrap();
        let old_decoded = SchedulerRuntimeProjection::decode(&old_payload).unwrap();
        assert_eq!(
            old_decoded.records()[0].target().credentials()[0].concurrency(),
            None
        );
        assert!(!old_decoded.records()[0].target().pool_mode());
        assert_eq!(
            old_decoded.records()[0].target().responses_compact_mode(),
            ResponsesCompactMode::Auto
        );
        assert_eq!(
            old_decoded.records()[0]
                .target()
                .responses_compact_probe_result(),
            ResponsesCompactProbeResult::Unknown
        );
        assert_eq!(
            old_decoded.records()[0]
                .target()
                .mapped_responses_compact_model("private-model"),
            "private-upstream-model"
        );

        let rendered = format!("{decoded:?}");
        for forbidden in [
            "private-model",
            "private.example",
            "private-key-id",
            "private-value",
            "private-rule-canary",
            "private-upstream-model",
            "private-compact-model",
        ] {
            assert!(!rendered.contains(forbidden));
        }
    }

    #[test]
    fn projection_round_trips_client_simulation_and_old_wire_defaults_off() {
        let channel_id = ChannelId::new(17).unwrap();
        let target = SchedulerRuntimeTargetRecord::new_pool(
            channel_id,
            ChannelType::Anthropic,
            Protocol::Anthropic,
            None,
            vec![
                SchedulerRuntimeCredentialRecord::new(
                    19,
                    CredentialKind::Oauth,
                    EncryptedCredentialEnvelope::new("private-key-id", [3; 24], vec![4; 16])
                        .unwrap(),
                    false,
                )
                .unwrap(),
            ],
            Vec::new(),
        )
        .unwrap()
        .with_client_simulation_profile(Some(ClientSimulationProfile::AnthropicCliHeadersV1))
        .unwrap();
        let record = SchedulerRuntimeRecord::new(
            SchedulerAbilityRecord::from_projection_parts(
                GroupId::new(18).unwrap(),
                "private-model".to_owned(),
                channel_id,
                1,
                1,
            )
            .unwrap(),
            Arc::new(target),
        );
        let projection = SchedulerRuntimeProjection::new(
            20,
            SchedulerCatalogSubject::Channel(channel_id),
            vec![record],
        )
        .unwrap();

        let payload = projection.encode().unwrap();
        let decoded = SchedulerRuntimeProjection::decode(&payload).unwrap();
        assert_eq!(
            decoded.records()[0].target().client_simulation_profile(),
            Some(ClientSimulationProfile::AnthropicCliHeadersV1)
        );

        let mut old_wire: serde_json::Value = serde_json::from_slice(&payload).unwrap();
        old_wire["targets"][0]
            .as_object_mut()
            .unwrap()
            .remove("client_simulation_profile");
        let old_decoded =
            SchedulerRuntimeProjection::decode(&serde_json::to_vec(&old_wire).unwrap()).unwrap();
        assert_eq!(
            old_decoded.records()[0]
                .target()
                .client_simulation_profile(),
            None
        );
    }

    #[test]
    fn projection_rejects_unknown_fields_and_subject_mismatch() {
        assert!(matches!(
            SchedulerRuntimeProjection::decode(
                br#"{"wire_version":1,"projection_version":1,"subject":{"kind":"group","id":1},"abilities":[],"targets":[],"payload":"forbidden"}"#,
            ),
            Err(SchedulerRuntimeProjectionError::Decode)
        ));

        let channel_id = ChannelId::new(3).unwrap();
        let target = Arc::new(
            SchedulerRuntimeTargetRecord::new(
                channel_id,
                ChannelType::OpenAi,
                Protocol::OpenAiChat,
                None,
                SchedulerRuntimeCredentialRecord::new(
                    4,
                    CredentialKind::ApiKey,
                    EncryptedCredentialEnvelope::new("key", [1; 24], vec![2; 16]).unwrap(),
                    false,
                )
                .unwrap(),
                Vec::new(),
            )
            .unwrap(),
        );
        let record = SchedulerRuntimeRecord::new(
            SchedulerAbilityRecord::from_projection_parts(
                GroupId::new(5).unwrap(),
                "model".to_owned(),
                channel_id,
                0,
                0,
            )
            .unwrap(),
            target,
        );
        assert_eq!(
            SchedulerRuntimeProjection::new(
                1,
                SchedulerCatalogSubject::Group(GroupId::new(6).unwrap()),
                vec![record],
            )
            .unwrap_err(),
            SchedulerRuntimeProjectionError::Invariant
        );
    }

    #[test]
    fn projection_round_trips_scoped_proxy_ciphertext() {
        let channel_id = ChannelId::new(17).unwrap();
        let proxy = SchedulerRuntimeProxyRecord::new(
            af_domain::ProxyId::new(23).unwrap(),
            crate::CredentialProxyScheme::Socks5h,
            "proxy.example".to_owned(),
            1080,
            Some("proxy-user".to_owned()),
            Some(EncryptedCredentialEnvelope::new("proxy-key", [7; 24], vec![8; 32]).unwrap()),
            true,
            4,
        )
        .unwrap();
        let credential = SchedulerRuntimeCredentialRecord::new(
            19,
            CredentialKind::ApiKey,
            EncryptedCredentialEnvelope::new("credential-key", [5; 24], vec![6; 32]).unwrap(),
            true,
        )
        .unwrap()
        .with_proxy(proxy)
        .unwrap();
        let target = Arc::new(
            SchedulerRuntimeTargetRecord::new(
                channel_id,
                ChannelType::OpenAi,
                Protocol::OpenAiChat,
                None,
                credential,
                Vec::new(),
            )
            .unwrap(),
        );
        let projection = SchedulerRuntimeProjection::new(
            29,
            SchedulerCatalogSubject::Channel(channel_id),
            vec![SchedulerRuntimeRecord::new(
                SchedulerAbilityRecord::from_projection_parts(
                    GroupId::new(31).unwrap(),
                    "proxy-model".to_owned(),
                    channel_id,
                    0,
                    1,
                )
                .unwrap(),
                target,
            )],
        )
        .unwrap();

        let decoded = SchedulerRuntimeProjection::decode(&projection.encode().unwrap()).unwrap();
        let proxy = decoded.records()[0].target().credentials()[0]
            .proxy()
            .unwrap();
        assert_eq!(proxy.proxy_id().get(), 23);
        assert_eq!(proxy.scheme(), crate::CredentialProxyScheme::Socks5h);
        assert!(proxy.trust_proxy_dns());
    }
}
