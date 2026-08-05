use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    sync::Arc,
};

use af_domain::{ChannelId, ChannelTimeout, ChannelType, CredentialKind, GroupId, Protocol};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use sea_orm::entity::prelude::Json;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    SchedulerAbilityRecord, SchedulerCatalogSubject, credential_probe::EncryptedCredentialEnvelope,
};

use super::{
    ChannelModelMappings, ChannelParameterOverrides, SchedulerRuntimeCredentialRecord,
    SchedulerRuntimeRecord, SchedulerRuntimeTargetRecord,
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
    responses_websocket_enabled: bool,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ProjectionCredentialWire {
    credential_id: i64,
    credential_kind: String,
    key_id: String,
    nonce_base64: String,
    ciphertext_base64: String,
    proxy_required: bool,
    priority: i32,
    weight: u32,
    #[serde(default)]
    concurrency: Option<u32>,
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
            responses_websocket_enabled: target.responses_websocket_enabled(),
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
        SchedulerRuntimeTargetRecord::new_pool_with_request_policy(
            channel_id,
            channel_type,
            protocol,
            self.base_url,
            credentials,
            model_mappings,
            parameter_overrides,
            headers,
        )
        .map_err(|_| SchedulerRuntimeProjectionError::Invariant)?
        .with_timeout(timeout)
        .with_auto_ban_rules(auto_ban_rules)
        .with_responses_websocket_enabled(self.responses_websocket_enabled)
        .map_err(|_| SchedulerRuntimeProjectionError::Invariant)
    }
}

impl ProjectionCredentialWire {
    fn from_credential(credential: &SchedulerRuntimeCredentialRecord) -> Self {
        Self {
            credential_id: credential.credential_id(),
            credential_kind: credential.credential_kind().to_string(),
            key_id: credential.envelope().key_id().to_owned(),
            nonce_base64: BASE64_STANDARD.encode(credential.envelope().nonce()),
            ciphertext_base64: BASE64_STANDARD.encode(credential.envelope().ciphertext()),
            proxy_required: credential.proxy_required(),
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
        SchedulerRuntimeCredentialRecord::with_scheduling_and_concurrency(
            self.credential_id,
            credential_kind,
            envelope,
            self.proxy_required,
            self.priority,
            self.weight,
            concurrency,
        )
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
            SchedulerRuntimeTargetRecord::new_pool(
                channel_id,
                ChannelType::OpenAi,
                Protocol::OpenAiChat,
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
                vec![("x-private-header".to_owned(), "private-value".to_owned())],
            )
            .unwrap()
            .with_auto_ban_rules(
                af_domain::ChannelAutoBanRules::new(
                    vec![503],
                    vec!["private-rule-canary".to_owned()],
                )
                .unwrap(),
            ),
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

        // 旧版 wire 没有账号并发字段，serde default 必须继续解码为不限并发。
        let mut old_wire: serde_json::Value = serde_json::from_slice(&payload).unwrap();
        for target in old_wire["targets"].as_array_mut().unwrap() {
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

        let rendered = format!("{decoded:?}");
        for forbidden in [
            "private-model",
            "private.example",
            "private-key-id",
            "private-value",
            "private-rule-canary",
        ] {
            assert!(!rendered.contains(forbidden));
        }
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
}
