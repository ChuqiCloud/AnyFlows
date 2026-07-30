use af_domain::{ChannelId, ChannelType, CredentialKind, Protocol, Status};
use sea_orm::entity::prelude::TimeDateTimeWithTimeZone;

use crate::{
    credential_probe::EncryptedCredentialEnvelope,
    entity::{channels, credentials},
};

use super::{ChannelModelMappings, ChannelParameterOverrides};

/// 将持久化实体转换为运行时选择策略可理解的内部来源对象时发生的不变量错误。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SchedulerRuntimeSourceError {
    Invariant,
}

/// 已从数据库实体中提取并校验的渠道运行时来源。
pub(super) struct SchedulerRuntimeChannelSource {
    channel_id: ChannelId,
    channel_type: ChannelType,
    protocol: Protocol,
    base_url: Option<String>,
    model_mappings: ChannelModelMappings,
    parameter_overrides: ChannelParameterOverrides,
    headers: Vec<(String, String)>,
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
        Ok(Some(Self {
            channel_id,
            channel_type,
            protocol,
            base_url: channel.base_url.map(|value| value.as_str().to_owned()),
            model_mappings,
            parameter_overrides,
            headers,
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
}

/// 已从数据库实体中提取并校验的凭据运行时来源。
pub(super) struct SchedulerRuntimeCredentialSource {
    credential_id: i64,
    channel_id: ChannelId,
    credential_kind: CredentialKind,
    envelope: EncryptedCredentialEnvelope,
    proxy_required: bool,
    priority: i32,
    weight: u32,
    rate_limit_reset_at: Option<TimeDateTimeWithTimeZone>,
    overload_until: Option<TimeDateTimeWithTimeZone>,
    temp_unschedulable_until: Option<TimeDateTimeWithTimeZone>,
}

impl SchedulerRuntimeCredentialSource {
    /// 从 SeaORM 凭据实体提取运行时所需字段；禁用、删除或不可调度凭据直接 fail-closed 跳过。
    pub(super) fn from_entity(
        credential: credentials::Model,
    ) -> Result<Option<Self>, SchedulerRuntimeSourceError> {
        if credential.id <= 0 {
            return Err(SchedulerRuntimeSourceError::Invariant);
        }
        let status = Status::try_from(credential.status)
            .map_err(|_| SchedulerRuntimeSourceError::Invariant)?;
        if !status.is_enabled() || !credential.schedulable || credential.deleted_at.is_some() {
            return Ok(None);
        }

        let channel_id = ChannelId::new(credential.channel_id)
            .map_err(|_| SchedulerRuntimeSourceError::Invariant)?;
        let credential_kind = credential
            .kind
            .parse()
            .map_err(|_| SchedulerRuntimeSourceError::Invariant)?;
        let (key_id, nonce, ciphertext) = credential
            .secret
            .envelope_parts()
            .map_err(|_| SchedulerRuntimeSourceError::Invariant)?;
        let envelope = EncryptedCredentialEnvelope::new(key_id, nonce, ciphertext)
            .map_err(|_| SchedulerRuntimeSourceError::Invariant)?;
        let weight =
            u32::try_from(credential.weight).map_err(|_| SchedulerRuntimeSourceError::Invariant)?;
        Ok(Some(Self {
            credential_id: credential.id,
            channel_id,
            credential_kind,
            envelope,
            proxy_required: credential.proxy_id.is_some(),
            priority: credential.priority,
            weight,
            rate_limit_reset_at: credential.rate_limit_reset_at,
            overload_until: credential.overload_until,
            temp_unschedulable_until: credential.temp_unschedulable_until,
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
        ]
        .into_iter()
        .flatten()
        .any(|until| until > now)
    }

    /// 消费来源对象，返回构造运行时凭据记录所需的安全字段。
    pub(super) fn into_runtime_parts(
        self,
    ) -> (
        i64,
        CredentialKind,
        EncryptedCredentialEnvelope,
        bool,
        i32,
        u32,
    ) {
        (
            self.credential_id,
            self.credential_kind,
            self.envelope,
            self.proxy_required,
            self.priority,
            self.weight,
        )
    }
}
