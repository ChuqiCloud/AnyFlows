use std::{fmt, future::Future, pin::Pin};

use af_account::{
    CredentialDecryptor, CredentialEncryptor, DecryptedOAuthCredential, PlainCredentialSecret,
};
use af_db::{
    AdminChannelDeleteOutcome, AdminChannelMutationOutcome, AdminChannelRepository,
    AdminChannelRepositoryError, AdminChannelWriteRecord, AdminChannelWriteRepositoryError,
    AdminCredentialCreateOutcome, AdminCredentialDeleteOutcome, AdminCredentialMutationOutcome,
    AdminCredentialSecretRecord, AdminCredentialWriteRecord, ChannelModelMappings,
    ChannelParameterOverrides, MAX_ADMIN_CHANNEL_ABILITIES, MAX_ADMIN_CHANNEL_GROUPS,
    MAX_ADMIN_CHANNEL_JSON_BYTES, MAX_ADMIN_CHANNEL_MODEL_BYTES, MAX_ADMIN_CHANNEL_MODELS,
    validate_channel_header_overrides,
};
use af_domain::{
    ChannelAutoBanRules, ChannelId, ChannelTimeout, ChannelType, ClientSimulationBodyProfile,
    ClientSimulationProfile, CredentialId, CredentialKind, GroupId, Protocol, ResponsesCompactMode,
    Status,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    AdminChannel, AdminCredential, AdminCredentialMultiKeyMode, AdminCredentialQuotaDimension,
    SessionPrincipal, SessionRole,
};

/// 管理写接口允许声明的渠道或凭据状态。
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AdminRoutingWriteStatus {
    /// 启用并允许参与服务或调度。
    Enabled,
    /// 手动禁用或保持待验证状态。
    Disabled,
}

impl AdminRoutingWriteStatus {
    const fn into_domain(self) -> Status {
        match self {
            Self::Enabled => Status::Enabled,
            Self::Disabled => Status::Disabled,
        }
    }
}

/// 管理员创建渠道时使用的完整配置。
pub struct AdminChannelCreateCommand {
    fields: AdminChannelWriteFields,
}

impl AdminChannelCreateCommand {
    /// 设置独立于通信适配器的厂商标识。
    pub fn with_provider(
        mut self,
        provider: Option<String>,
    ) -> Result<Self, AdminChannelWriteError> {
        self.fields = self.fields.with_provider(provider)?;
        Ok(self)
    }

    /// 校验当前生产运行时支持的渠道类型及全部配置边界。
    #[allow(
        clippy::too_many_arguments,
        reason = "字段与管理端渠道写入契约一一对应"
    )]
    pub fn new(
        name: String,
        channel_type: ChannelType,
        protocol: Protocol,
        base_url: Option<String>,
        timeout_secs: Option<u64>,
        status: AdminRoutingWriteStatus,
        weight: i32,
        priority: i32,
        auto_ban: bool,
        models: Vec<String>,
        group_ids: Vec<GroupId>,
        model_mapping: serde_json::Value,
        param_override: serde_json::Value,
        header_override: serde_json::Value,
        settings: serde_json::Value,
        tag: Option<String>,
    ) -> Result<Self, AdminChannelWriteError> {
        Ok(Self {
            fields: AdminChannelWriteFields::new(
                name,
                channel_type,
                protocol,
                base_url,
                timeout_secs,
                status,
                weight,
                priority,
                auto_ban,
                models,
                group_ids,
                model_mapping,
                param_override,
                Some(header_override),
                Some(settings),
                tag,
                true,
            )?,
        })
    }

    fn into_record(self) -> AdminChannelWriteRecord {
        self.fields.into_record()
    }

    /// 显式设置原生 Responses WebSocket 能力。
    pub fn with_responses_websocket_enabled(
        mut self,
        enabled: bool,
    ) -> Result<Self, AdminChannelWriteError> {
        self.fields = self.fields.with_responses_websocket_enabled(enabled)?;
        Ok(self)
    }

    /// 显式设置 Compact 三态能力与专属模型映射。
    pub fn with_responses_compact_configuration(
        mut self,
        mode: ResponsesCompactMode,
        model_mapping: serde_json::Value,
    ) -> Result<Self, AdminChannelWriteError> {
        self.fields = self
            .fields
            .with_responses_compact_configuration(mode, model_mapping)?;
        Ok(self)
    }

    /// 设置创建渠道时是否启用外部账号池模式。
    #[must_use]
    pub fn with_pool_mode(mut self, enabled: bool) -> Self {
        self.fields.pool_mode = Some(enabled);
        self
    }

    /// 创建时显式选择仿真档案；启用档案必须同时确认合规风险。
    pub fn with_client_simulation_profile(
        mut self,
        profile: Option<ClientSimulationProfile>,
        risk_accepted: bool,
    ) -> Result<Self, AdminChannelWriteError> {
        if profile.is_some() && !risk_accepted {
            return Err(AdminChannelWriteError::InvalidInput);
        }
        self.fields = self
            .fields
            .with_client_simulation_profile(profile, risk_accepted)?;
        Ok(self)
    }

    /// 创建时显式选择正文仿真档案；启用档案必须独立确认正文改写风险。
    pub fn with_client_simulation_body_profile(
        mut self,
        profile: Option<ClientSimulationBodyProfile>,
        risk_accepted: bool,
    ) -> Result<Self, AdminChannelWriteError> {
        if profile.is_some() && !risk_accepted {
            return Err(AdminChannelWriteError::InvalidInput);
        }
        self.fields = self
            .fields
            .with_client_simulation_body_profile(profile, risk_accepted)?;
        Ok(self)
    }

    /// 设置创建渠道时使用的自动禁用规则。
    pub fn with_auto_ban_rules(
        mut self,
        rules: ChannelAutoBanRules,
    ) -> Result<Self, AdminChannelWriteError> {
        self.fields.auto_ban_rules = Some(rules);
        Ok(self)
    }
}

impl fmt::Debug for AdminChannelCreateCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminChannelCreateCommand(<已脱敏>)")
    }
}

/// 管理员完整更新渠道时使用的配置；运行累计字段不属于命令。
pub struct AdminChannelUpdateCommand {
    fields: AdminChannelWriteFields,
}

impl AdminChannelUpdateCommand {
    /// 更新厂商标识；省略或 null 时保留原值。
    pub fn with_provider(
        mut self,
        provider: Option<String>,
    ) -> Result<Self, AdminChannelWriteError> {
        self.fields = self.fields.with_provider(provider)?;
        Ok(self)
    }

    /// 校验完整更新字段，禁止借管理接口写入未实现的适配器语义。
    #[allow(
        clippy::too_many_arguments,
        reason = "字段与管理端渠道写入契约一一对应"
    )]
    pub fn new(
        name: String,
        channel_type: ChannelType,
        protocol: Protocol,
        base_url: Option<String>,
        timeout_secs: Option<u64>,
        status: AdminRoutingWriteStatus,
        weight: i32,
        priority: i32,
        auto_ban: bool,
        models: Vec<String>,
        group_ids: Vec<GroupId>,
        model_mapping: serde_json::Value,
        param_override: serde_json::Value,
        header_override: Option<serde_json::Value>,
        settings: Option<serde_json::Value>,
        tag: Option<String>,
    ) -> Result<Self, AdminChannelWriteError> {
        Ok(Self {
            fields: AdminChannelWriteFields::new(
                name,
                channel_type,
                protocol,
                base_url,
                timeout_secs,
                status,
                weight,
                priority,
                auto_ban,
                models,
                group_ids,
                model_mapping,
                param_override,
                header_override,
                settings,
                tag,
                false,
            )?,
        })
    }

    fn into_record(self) -> AdminChannelWriteRecord {
        self.fields.into_record()
    }

    /// 更新受控 Responses WebSocket 能力；未调用时由仓储保留原值。
    pub fn with_responses_websocket_enabled(
        mut self,
        enabled: bool,
    ) -> Result<Self, AdminChannelWriteError> {
        self.fields = self.fields.with_responses_websocket_enabled(enabled)?;
        Ok(self)
    }

    /// 更新 Compact 三态能力与专属模型映射。
    pub fn with_responses_compact_configuration(
        mut self,
        mode: ResponsesCompactMode,
        model_mapping: serde_json::Value,
    ) -> Result<Self, AdminChannelWriteError> {
        self.fields = self
            .fields
            .with_responses_compact_configuration(mode, model_mapping)?;
        Ok(self)
    }

    /// 更新外部账号池模式；未调用时由仓储保留原值。
    #[must_use]
    pub fn with_pool_mode(mut self, enabled: bool) -> Self {
        self.fields.pool_mode = Some(enabled);
        self
    }

    /// 更新仿真档案选择；仓储只在首次启用或换档案时消费风险确认。
    pub fn with_client_simulation_profile(
        mut self,
        profile: Option<ClientSimulationProfile>,
        risk_accepted: bool,
    ) -> Result<Self, AdminChannelWriteError> {
        self.fields = self
            .fields
            .with_client_simulation_profile(profile, risk_accepted)?;
        Ok(self)
    }

    /// 更新正文仿真档案选择；仓储只在首次启用或换档案时消费风险确认。
    pub fn with_client_simulation_body_profile(
        mut self,
        profile: Option<ClientSimulationBodyProfile>,
        risk_accepted: bool,
    ) -> Result<Self, AdminChannelWriteError> {
        self.fields = self
            .fields
            .with_client_simulation_body_profile(profile, risk_accepted)?;
        Ok(self)
    }

    /// 设置更新渠道时使用的自动禁用规则。
    pub fn with_auto_ban_rules(
        mut self,
        rules: ChannelAutoBanRules,
    ) -> Result<Self, AdminChannelWriteError> {
        self.fields.auto_ban_rules = Some(rules);
        Ok(self)
    }
}

impl fmt::Debug for AdminChannelUpdateCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminChannelUpdateCommand(<已脱敏>)")
    }
}

struct AdminChannelWriteFields {
    provider: Option<String>,
    name: String,
    channel_type: ChannelType,
    protocol: Protocol,
    base_url: Option<String>,
    timeout: Option<ChannelTimeout>,
    status: AdminRoutingWriteStatus,
    weight: i32,
    priority: i32,
    auto_ban: bool,
    models: Vec<String>,
    group_ids: Vec<GroupId>,
    model_mapping: serde_json::Value,
    param_override: serde_json::Value,
    header_override: Option<serde_json::Value>,
    settings: Option<serde_json::Value>,
    pool_mode: Option<bool>,
    client_simulation_profile: Option<Option<ClientSimulationProfile>>,
    client_simulation_risk_accepted: bool,
    client_simulation_body_profile: Option<Option<ClientSimulationBodyProfile>>,
    client_simulation_body_risk_accepted: bool,
    responses_websocket_enabled: Option<bool>,
    responses_compact_mode: Option<ResponsesCompactMode>,
    responses_compact_model_mapping: Option<serde_json::Value>,
    auto_ban_rules: Option<ChannelAutoBanRules>,
    tag: Option<String>,
}

impl AdminChannelWriteFields {
    #[allow(
        clippy::too_many_arguments,
        reason = "仅在统一校验入口组装完整渠道字段"
    )]
    fn new(
        name: String,
        channel_type: ChannelType,
        protocol: Protocol,
        base_url: Option<String>,
        timeout_secs: Option<u64>,
        status: AdminRoutingWriteStatus,
        weight: i32,
        priority: i32,
        auto_ban: bool,
        models: Vec<String>,
        group_ids: Vec<GroupId>,
        model_mapping: serde_json::Value,
        param_override: serde_json::Value,
        header_override: Option<serde_json::Value>,
        settings: Option<serde_json::Value>,
        tag: Option<String>,
        require_sensitive_fields: bool,
    ) -> Result<Self, AdminChannelWriteError> {
        if !valid_text(&name, 128)
            || weight < 0
            || !matches!(
                (channel_type, protocol),
                (
                    ChannelType::OpenAi,
                    Protocol::OpenAiChat
                        | Protocol::OpenAiResponses
                        | Protocol::OpenAiEmbeddings
                        | Protocol::OpenAiImages
                        | Protocol::OpenAiAudio
                        | Protocol::OpenAiSpeech
                ) | (ChannelType::Anthropic, Protocol::Anthropic)
                    | (ChannelType::Gemini, Protocol::Gemini)
                    | (ChannelType::Jina, Protocol::JinaRerank)
                    | (ChannelType::Cohere, Protocol::CohereRerank)
                    | (ChannelType::Xai, Protocol::XaiVideo)
            )
            || base_url
                .as_deref()
                .is_some_and(|value| value.is_empty() || value.len() > 2_048)
            || tag.as_deref().is_some_and(|value| !valid_text(value, 64))
            || !valid_channel_routing(&models, &group_ids)
            || (require_sensitive_fields && (header_override.is_none() || settings.is_none()))
        {
            return Err(AdminChannelWriteError::InvalidInput);
        }
        validate_json_object(&model_mapping, true)?;
        ChannelModelMappings::parse(&model_mapping)
            .map_err(|_| AdminChannelWriteError::InvalidInput)?;
        validate_json_object(&param_override, false)?;
        let parameter_overrides = ChannelParameterOverrides::parse(&param_override)
            .map_err(|_| AdminChannelWriteError::InvalidInput)?;
        parameter_overrides
            .validate_for_protocol(protocol)
            .map_err(|_| AdminChannelWriteError::InvalidInput)?;
        if let Some(header_override) = &header_override {
            validate_json_object(header_override, false)?;
            validate_channel_header_overrides(header_override)
                .map_err(|_| AdminChannelWriteError::InvalidInput)?;
        }
        if let Some(settings) = &settings {
            validate_json_object(settings, false)?;
        }
        let timeout = timeout_secs
            .map(ChannelTimeout::new)
            .transpose()
            .map_err(|_| AdminChannelWriteError::InvalidInput)?;
        Ok(Self {
            name,
            channel_type,
            provider: None,
            protocol,
            base_url,
            timeout,
            status,
            weight,
            priority,
            auto_ban,
            models,
            group_ids,
            model_mapping,
            param_override,
            header_override,
            settings,
            pool_mode: require_sensitive_fields.then_some(false),
            client_simulation_profile: require_sensitive_fields.then_some(None),
            client_simulation_risk_accepted: false,
            client_simulation_body_profile: require_sensitive_fields.then_some(None),
            client_simulation_body_risk_accepted: false,
            responses_websocket_enabled: require_sensitive_fields.then_some(false),
            responses_compact_mode: None,
            responses_compact_model_mapping: None,
            auto_ban_rules: require_sensitive_fields.then_some(ChannelAutoBanRules::default()),
            tag,
        })
    }

    fn with_provider(mut self, provider: Option<String>) -> Result<Self, AdminChannelWriteError> {
        if provider
            .as_deref()
            .is_some_and(|value| !valid_text(value, 64) || value.trim() != value)
        {
            return Err(AdminChannelWriteError::InvalidInput);
        }
        self.provider = provider;
        Ok(self)
    }

    fn with_responses_websocket_enabled(
        mut self,
        enabled: bool,
    ) -> Result<Self, AdminChannelWriteError> {
        if enabled
            && (self.channel_type != ChannelType::OpenAi
                || self.protocol != Protocol::OpenAiResponses)
        {
            return Err(AdminChannelWriteError::InvalidInput);
        }
        self.responses_websocket_enabled = Some(enabled);
        Ok(self)
    }

    fn with_client_simulation_profile(
        mut self,
        profile: Option<ClientSimulationProfile>,
        risk_accepted: bool,
    ) -> Result<Self, AdminChannelWriteError> {
        if profile.is_some()
            && (self.channel_type != ChannelType::Anthropic || self.protocol != Protocol::Anthropic)
        {
            return Err(AdminChannelWriteError::InvalidInput);
        }
        self.client_simulation_profile = Some(profile);
        self.client_simulation_risk_accepted = risk_accepted;
        Ok(self)
    }

    fn with_client_simulation_body_profile(
        mut self,
        profile: Option<ClientSimulationBodyProfile>,
        risk_accepted: bool,
    ) -> Result<Self, AdminChannelWriteError> {
        if profile.is_some()
            && (self.channel_type != ChannelType::Anthropic || self.protocol != Protocol::Anthropic)
        {
            return Err(AdminChannelWriteError::InvalidInput);
        }
        self.client_simulation_body_profile = Some(profile);
        self.client_simulation_body_risk_accepted = risk_accepted;
        Ok(self)
    }

    fn with_responses_compact_configuration(
        mut self,
        mode: ResponsesCompactMode,
        model_mapping: serde_json::Value,
    ) -> Result<Self, AdminChannelWriteError> {
        validate_json_object(&model_mapping, true)?;
        let mapping = ChannelModelMappings::parse(&model_mapping)
            .map_err(|_| AdminChannelWriteError::InvalidInput)?;
        let native_responses =
            self.channel_type == ChannelType::OpenAi && self.protocol == Protocol::OpenAiResponses;
        if (mode == ResponsesCompactMode::ForceOn || !mapping.is_empty()) && !native_responses {
            return Err(AdminChannelWriteError::InvalidInput);
        }
        self.responses_compact_mode = Some(mode);
        self.responses_compact_model_mapping = Some(model_mapping);
        Ok(self)
    }

    fn into_record(self) -> AdminChannelWriteRecord {
        let pool_mode = self.pool_mode;
        let client_simulation_profile = self.client_simulation_profile;
        let client_simulation_risk_accepted = self.client_simulation_risk_accepted;
        let client_simulation_body_profile = self.client_simulation_body_profile;
        let client_simulation_body_risk_accepted = self.client_simulation_body_risk_accepted;
        let responses_websocket_enabled = self.responses_websocket_enabled;
        let responses_compact_mode = self.responses_compact_mode;
        let responses_compact_model_mapping = self.responses_compact_model_mapping;
        let auto_ban_rules = self.auto_ban_rules;
        let record = AdminChannelWriteRecord::new_update(
            self.name,
            self.channel_type,
            self.protocol,
            self.base_url,
            self.timeout,
            self.status.into_domain(),
            self.weight,
            self.priority,
            self.auto_ban,
            self.models,
            self.group_ids,
            self.model_mapping,
            self.param_override,
            self.header_override,
            self.settings,
            self.tag,
        )
        .with_provider(self.provider);
        let record = match pool_mode {
            Some(enabled) => record.with_pool_mode(enabled),
            None => record,
        };
        let record = match client_simulation_profile {
            Some(profile) => {
                record.with_client_simulation_profile(profile, client_simulation_risk_accepted)
            }
            None => record,
        };
        let record = match client_simulation_body_profile {
            Some(profile) => record
                .with_client_simulation_body_profile(profile, client_simulation_body_risk_accepted),
            None => record,
        };
        let record = match responses_websocket_enabled {
            Some(enabled) => record.with_responses_websocket_enabled(enabled),
            None => record,
        };
        let record = match (responses_compact_mode, responses_compact_model_mapping) {
            (Some(mode), Some(mapping)) => {
                record.with_responses_compact_configuration(mode, mapping)
            }
            (None, None) => record,
            _ => unreachable!("Compact 模式与模型映射必须成对写入"),
        };
        match auto_ban_rules {
            Some(rules) => record.with_auto_ban_rules(rules),
            None => record,
        }
    }
}

/// 管理员创建凭据时使用的完整配置和一次性明文。
pub struct AdminCredentialCreateCommand {
    fields: AdminCredentialWriteFields,
    mode: AdminCredentialCreateMode,
}

enum AdminCredentialCreateMode {
    Encrypted(PlainCredentialSecret),
    PendingOauth,
    SparkShadow,
}

impl AdminCredentialCreateCommand {
    /// 校验类型化 secret 及非敏感调度配置。
    #[allow(
        clippy::too_many_arguments,
        reason = "字段与管理端凭据写入契约一一对应"
    )]
    pub fn new(
        kind: CredentialKind,
        secret: PlainCredentialSecret,
        status: AdminRoutingWriteStatus,
        multi_key_mode: Option<AdminCredentialMultiKeyMode>,
        priority: i32,
        weight: i32,
        concurrency: Option<i32>,
        load_factor_micros: Option<i64>,
        rate_multiplier_micros: Option<i64>,
        schedulable: bool,
        parent_id: Option<CredentialId>,
        quota_dimension: AdminCredentialQuotaDimension,
        proxy_id: Option<i64>,
        oauth_provider: Option<String>,
        oauth_account_key: Option<String>,
        oauth_project_id: Option<String>,
    ) -> Result<Self, AdminChannelWriteError> {
        if parent_id.is_some() || quota_dimension != AdminCredentialQuotaDimension::Global {
            return Err(AdminChannelWriteError::InvalidInput);
        }
        let fields = AdminCredentialWriteFields::new(
            kind,
            status,
            multi_key_mode,
            priority,
            weight,
            concurrency,
            load_factor_micros,
            rate_multiplier_micros,
            schedulable,
            parent_id,
            quota_dimension,
            proxy_id,
            oauth_provider,
            oauth_account_key,
            oauth_project_id,
        )?;
        if secret.kind() != kind {
            return Err(AdminChannelWriteError::InvalidInput);
        }
        Ok(Self {
            fields,
            mode: AdminCredentialCreateMode::Encrypted(secret),
        })
    }

    /// 创建等待首次 OAuth 授权的凭据；该模式不接收伪造 token。
    #[allow(
        clippy::too_many_arguments,
        reason = "字段与管理端凭据写入契约一一对应"
    )]
    pub fn new_pending_oauth(
        kind: CredentialKind,
        status: AdminRoutingWriteStatus,
        multi_key_mode: Option<AdminCredentialMultiKeyMode>,
        priority: i32,
        weight: i32,
        concurrency: Option<i32>,
        load_factor_micros: Option<i64>,
        rate_multiplier_micros: Option<i64>,
        schedulable: bool,
        parent_id: Option<CredentialId>,
        quota_dimension: AdminCredentialQuotaDimension,
        proxy_id: Option<i64>,
        oauth_provider: Option<String>,
        oauth_account_key: Option<String>,
        oauth_project_id: Option<String>,
    ) -> Result<Self, AdminChannelWriteError> {
        if kind != CredentialKind::Oauth
            || oauth_provider.is_none()
            || parent_id.is_some()
            || quota_dimension != AdminCredentialQuotaDimension::Global
        {
            return Err(AdminChannelWriteError::InvalidInput);
        }
        let fields = AdminCredentialWriteFields::new(
            kind,
            status,
            multi_key_mode,
            priority,
            weight,
            concurrency,
            load_factor_micros,
            rate_multiplier_micros,
            schedulable,
            parent_id,
            quota_dimension,
            proxy_id,
            oauth_provider,
            oauth_account_key,
            oauth_project_id,
        )?;
        Ok(Self {
            fields,
            mode: AdminCredentialCreateMode::PendingOauth,
        })
    }

    /// 创建只继承母凭据密钥和代理的 Spark 调度影子。
    #[allow(
        clippy::too_many_arguments,
        reason = "字段与管理端凭据调度配置一一对应"
    )]
    pub fn new_spark_shadow(
        status: AdminRoutingWriteStatus,
        multi_key_mode: Option<AdminCredentialMultiKeyMode>,
        priority: i32,
        weight: i32,
        concurrency: Option<i32>,
        load_factor_micros: Option<i64>,
        rate_multiplier_micros: Option<i64>,
        schedulable: bool,
        parent_id: CredentialId,
    ) -> Result<Self, AdminChannelWriteError> {
        if concurrency.is_some() {
            return Err(AdminChannelWriteError::InvalidInput);
        }
        let fields = AdminCredentialWriteFields::new(
            CredentialKind::Oauth,
            status,
            multi_key_mode,
            priority,
            weight,
            None,
            load_factor_micros,
            rate_multiplier_micros,
            schedulable,
            Some(parent_id),
            AdminCredentialQuotaDimension::Spark,
            None,
            None,
            None,
            None,
        )?;
        Ok(Self {
            fields,
            mode: AdminCredentialCreateMode::SparkShadow,
        })
    }

    fn into_parts(self) -> (AdminCredentialWriteRecord, AdminCredentialCreateMode) {
        let oauth_token_pending = matches!(&self.mode, AdminCredentialCreateMode::PendingOauth);
        (
            self.fields
                .into_record()
                .with_oauth_token_pending(oauth_token_pending),
            self.mode,
        )
    }
}

impl fmt::Debug for AdminCredentialCreateCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminCredentialCreateCommand(<已脱敏>)")
    }
}

/// 管理员完整更新凭据时使用的配置；空 secret 表示保留原密文。
pub struct AdminCredentialUpdateCommand {
    fields: AdminCredentialWriteFields,
    secret: Option<PlainCredentialSecret>,
}

impl AdminCredentialUpdateCommand {
    /// 校验更新字段；所属渠道和凭据类型只用于确认归属，不允许转移。
    #[allow(
        clippy::too_many_arguments,
        reason = "字段与管理端凭据写入契约一一对应"
    )]
    pub fn new(
        kind: CredentialKind,
        secret: Option<PlainCredentialSecret>,
        status: AdminRoutingWriteStatus,
        multi_key_mode: Option<AdminCredentialMultiKeyMode>,
        priority: i32,
        weight: i32,
        concurrency: Option<i32>,
        load_factor_micros: Option<i64>,
        rate_multiplier_micros: Option<i64>,
        schedulable: bool,
        parent_id: Option<CredentialId>,
        quota_dimension: AdminCredentialQuotaDimension,
        proxy_id: Option<i64>,
        oauth_provider: Option<String>,
        oauth_account_key: Option<String>,
        oauth_project_id: Option<String>,
    ) -> Result<Self, AdminChannelWriteError> {
        let fields = AdminCredentialWriteFields::new(
            kind,
            status,
            multi_key_mode,
            priority,
            weight,
            concurrency,
            load_factor_micros,
            rate_multiplier_micros,
            schedulable,
            parent_id,
            quota_dimension,
            proxy_id,
            oauth_provider,
            oauth_account_key,
            oauth_project_id,
        )?;
        if secret.as_ref().is_some_and(|secret| secret.kind() != kind)
            || (quota_dimension == AdminCredentialQuotaDimension::Spark && secret.is_some())
        {
            return Err(AdminChannelWriteError::InvalidInput);
        }
        Ok(Self { fields, secret })
    }

    fn into_parts(self) -> (AdminCredentialWriteRecord, Option<PlainCredentialSecret>) {
        (self.fields.into_record(), self.secret)
    }
}

impl fmt::Debug for AdminCredentialUpdateCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminCredentialUpdateCommand(<已脱敏>)")
    }
}

struct AdminCredentialWriteFields {
    kind: CredentialKind,
    status: AdminRoutingWriteStatus,
    multi_key_mode: Option<AdminCredentialMultiKeyMode>,
    priority: i32,
    weight: i32,
    concurrency: Option<i32>,
    load_factor_micros: Option<i64>,
    rate_multiplier_micros: Option<i64>,
    schedulable: bool,
    parent_id: Option<CredentialId>,
    quota_dimension: AdminCredentialQuotaDimension,
    proxy_id: Option<i64>,
    oauth_provider: Option<String>,
    oauth_account_key: Option<String>,
    oauth_project_id: Option<String>,
}

impl AdminCredentialWriteFields {
    #[allow(
        clippy::too_many_arguments,
        reason = "仅在统一校验入口组装完整凭据字段"
    )]
    fn new(
        kind: CredentialKind,
        status: AdminRoutingWriteStatus,
        multi_key_mode: Option<AdminCredentialMultiKeyMode>,
        priority: i32,
        weight: i32,
        concurrency: Option<i32>,
        load_factor_micros: Option<i64>,
        rate_multiplier_micros: Option<i64>,
        schedulable: bool,
        parent_id: Option<CredentialId>,
        quota_dimension: AdminCredentialQuotaDimension,
        proxy_id: Option<i64>,
        oauth_provider: Option<String>,
        oauth_account_key: Option<String>,
        oauth_project_id: Option<String>,
    ) -> Result<Self, AdminChannelWriteError> {
        let oauth_fields_present =
            oauth_provider.is_some() || oauth_account_key.is_some() || oauth_project_id.is_some();
        let valid_shape = match quota_dimension {
            AdminCredentialQuotaDimension::Global => parent_id.is_none(),
            AdminCredentialQuotaDimension::Spark => {
                kind == CredentialKind::Oauth
                    && parent_id.is_some()
                    && concurrency.is_none()
                    && proxy_id.is_none()
                    && !oauth_fields_present
            }
        };
        if !matches!(
            kind,
            CredentialKind::ApiKey
                | CredentialKind::Oauth
                | CredentialKind::Bedrock
                | CredentialKind::ServiceAccount
        ) || weight < 0
            || concurrency.is_some_and(|value| value < 0)
            || load_factor_micros.is_some_and(|value| value < 0)
            || rate_multiplier_micros.is_some_and(|value| value < 0)
            || proxy_id.is_some_and(|value| value <= 0)
            || !valid_optional_text(oauth_provider.as_deref(), 64)
            || !valid_optional_text(oauth_account_key.as_deref(), 255)
            || !valid_optional_text(oauth_project_id.as_deref(), 255)
            || (kind != CredentialKind::Oauth && oauth_fields_present)
            || !valid_shape
        {
            return Err(AdminChannelWriteError::InvalidInput);
        }
        Ok(Self {
            kind,
            status,
            multi_key_mode,
            priority,
            weight,
            concurrency,
            load_factor_micros,
            rate_multiplier_micros,
            schedulable,
            parent_id,
            quota_dimension,
            proxy_id,
            oauth_provider,
            oauth_account_key,
            oauth_project_id,
        })
    }

    fn into_record(self) -> AdminCredentialWriteRecord {
        AdminCredentialWriteRecord::new(
            self.kind,
            self.status.into_domain(),
            self.multi_key_mode.map(|mode| match mode {
                AdminCredentialMultiKeyMode::Random => 1,
                AdminCredentialMultiKeyMode::RoundRobin => 2,
            }),
            self.priority,
            self.weight,
            self.concurrency,
            self.load_factor_micros,
            self.rate_multiplier_micros,
            self.schedulable,
            self.parent_id,
            self.quota_dimension,
            self.proxy_id,
            self.oauth_provider,
            self.oauth_account_key,
            self.oauth_project_id,
        )
    }
}

/// 管理渠道或凭据写入失败分类。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AdminChannelWriteError {
    /// 请求字段、类型或父凭据引用无效。
    #[error("管理渠道写入参数无效")]
    InvalidInput,
    /// 当前会话不是管理员。
    #[error("管理渠道写入权限不足")]
    Forbidden,
    /// 渠道不存在或已经软删除。
    #[error("管理渠道不存在")]
    ChannelNotFound,
    /// 凭据不存在、已删除或不属于父渠道。
    #[error("管理凭据不存在")]
    CredentialNotFound,
    /// 加密、数据库或持久化状态发生内部故障。
    #[error("管理渠道写入内部失败")]
    Internal,
}

pub type AdminChannelCreateFuture<'a> =
    Pin<Box<dyn Future<Output = Result<AdminChannel, AdminChannelWriteError>> + Send + 'a>>;
pub type AdminChannelUpdateFuture<'a> =
    Pin<Box<dyn Future<Output = Result<AdminChannel, AdminChannelWriteError>> + Send + 'a>>;
pub type AdminChannelDeleteFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), AdminChannelWriteError>> + Send + 'a>>;
pub type AdminCredentialCreateFuture<'a> =
    Pin<Box<dyn Future<Output = Result<AdminCredential, AdminChannelWriteError>> + Send + 'a>>;
pub type AdminCredentialUpdateFuture<'a> =
    Pin<Box<dyn Future<Output = Result<AdminCredential, AdminChannelWriteError>> + Send + 'a>>;
pub type AdminCredentialDeleteFuture<'a> =
    Pin<Box<dyn Future<Output = Result<(), AdminChannelWriteError>> + Send + 'a>>;
pub type AdminCredentialExportFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<Vec<AdminCredentialExport>, AdminChannelWriteError>> + Send + 'a,
    >,
>;

/// 管理员 OAuth 导出项；令牌仅在调用链中以受保护类型传递。
pub struct AdminCredentialExport {
    credential_id: CredentialId,
    oauth_account_key: Option<String>,
    oauth_project_id: Option<String>,
    secret: DecryptedOAuthCredential,
}

impl AdminCredentialExport {
    #[must_use]
    pub const fn credential_id(&self) -> CredentialId {
        self.credential_id
    }

    #[must_use]
    pub fn oauth_account_key(&self) -> Option<&str> {
        self.oauth_account_key.as_deref()
    }

    #[must_use]
    pub fn oauth_project_id(&self) -> Option<&str> {
        self.oauth_project_id.as_deref()
    }

    #[must_use]
    pub const fn secret(&self) -> &DecryptedOAuthCredential {
        &self.secret
    }
}

/// 管理渠道与凭据写入应用端口；角色校验必须先于加密和仓储访问。
pub trait AdminChannelWriter: Send + Sync {
    /// 创建渠道并返回非敏感管理快照。
    fn create_channel<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: AdminChannelCreateCommand,
    ) -> AdminChannelCreateFuture<'a>;

    /// 完整更新渠道配置，同时保留运行累计字段。
    fn update_channel<'a>(
        &'a self,
        principal: SessionPrincipal,
        channel_id: ChannelId,
        command: AdminChannelUpdateCommand,
    ) -> AdminChannelUpdateFuture<'a>;

    /// 安全软删除渠道、凭据和直接运行时关系。
    fn delete_channel<'a>(
        &'a self,
        principal: SessionPrincipal,
        channel_id: ChannelId,
    ) -> AdminChannelDeleteFuture<'a>;

    /// 创建凭据；明文只在生成绑定真实 ID 的密文时短暂暴露。
    fn create_credential<'a>(
        &'a self,
        principal: SessionPrincipal,
        channel_id: ChannelId,
        command: AdminCredentialCreateCommand,
    ) -> AdminCredentialCreateFuture<'a>;

    /// 完整更新凭据配置，可选轮换 secret。
    fn update_credential<'a>(
        &'a self,
        principal: SessionPrincipal,
        channel_id: ChannelId,
        credential_id: CredentialId,
        command: AdminCredentialUpdateCommand,
    ) -> AdminCredentialUpdateFuture<'a>;

    /// 安全软删除凭据及其影子后代。
    fn delete_credential<'a>(
        &'a self,
        principal: SessionPrincipal,
        channel_id: ChannelId,
        credential_id: CredentialId,
    ) -> AdminCredentialDeleteFuture<'a>;

    /// 导出指定渠道的 OAuth 凭据明文，仅允许管理员调用。
    fn export_oauth_credentials<'a>(
        &'a self,
        principal: SessionPrincipal,
        channel_id: ChannelId,
    ) -> AdminCredentialExportFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            let _ = channel_id;
            Err(AdminChannelWriteError::Internal)
        })
    }
}

/// 使用数据库仓储和启动密钥实现管理员渠道写入。
pub struct DatabaseAdminChannelWriter {
    repository: AdminChannelRepository,
    encryptor: CredentialEncryptor,
    decryptor: Option<CredentialDecryptor>,
}

impl DatabaseAdminChannelWriter {
    /// 绑定渠道仓储和已校验的凭据加密器。
    #[must_use]
    pub const fn new(repository: AdminChannelRepository, encryptor: CredentialEncryptor) -> Self {
        Self {
            repository,
            encryptor,
            decryptor: None,
        }
    }

    /// 注入与加密器共享密钥的解密器，供管理员安全导出账号池令牌。
    #[must_use]
    pub fn with_decryptor(mut self, decryptor: CredentialDecryptor) -> Self {
        self.decryptor = Some(decryptor);
        self
    }
}

impl AdminChannelWriter for DatabaseAdminChannelWriter {
    fn create_channel<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: AdminChannelCreateCommand,
    ) -> AdminChannelCreateFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            let channel = self
                .repository
                .create_channel(command.into_record())
                .await
                .map(AdminChannel::from_record)
                .map_err(map_repository_error)?;
            audit_client_simulation_write("create", principal, &channel);
            Ok(channel)
        })
    }

    fn update_channel<'a>(
        &'a self,
        principal: SessionPrincipal,
        channel_id: ChannelId,
        command: AdminChannelUpdateCommand,
    ) -> AdminChannelUpdateFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            match self
                .repository
                .update_channel(channel_id, command.into_record())
                .await
                .map_err(map_repository_error)?
            {
                AdminChannelMutationOutcome::Mutated(record) => {
                    let channel = AdminChannel::from_record(*record);
                    audit_client_simulation_write("update", principal, &channel);
                    Ok(channel)
                }
                AdminChannelMutationOutcome::NotFound => {
                    Err(AdminChannelWriteError::ChannelNotFound)
                }
            }
        })
    }

    fn delete_channel<'a>(
        &'a self,
        principal: SessionPrincipal,
        channel_id: ChannelId,
    ) -> AdminChannelDeleteFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            match self
                .repository
                .delete_channel(channel_id)
                .await
                .map_err(map_repository_error)?
            {
                AdminChannelDeleteOutcome::Deleted => Ok(()),
                AdminChannelDeleteOutcome::NotFound => Err(AdminChannelWriteError::ChannelNotFound),
            }
        })
    }

    fn create_credential<'a>(
        &'a self,
        principal: SessionPrincipal,
        channel_id: ChannelId,
        command: AdminCredentialCreateCommand,
    ) -> AdminCredentialCreateFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            let (record, mode) = command.into_parts();
            let outcome = match mode {
                AdminCredentialCreateMode::Encrypted(secret) => self
                    .repository
                    .create_credential(channel_id, record, |credential_id| {
                        self.encryptor
                            .encrypt(channel_id, credential_id.get(), &secret)
                            .map_err(|_| ())
                    })
                    .await
                    .map_err(map_repository_error)?,
                AdminCredentialCreateMode::PendingOauth => self
                    .repository
                    .create_pending_oauth_credential(channel_id, record)
                    .await
                    .map_err(map_repository_error)?,
                AdminCredentialCreateMode::SparkShadow => self
                    .repository
                    .create_spark_shadow_credential(channel_id, record)
                    .await
                    .map_err(map_repository_error)?,
            };
            match outcome {
                AdminCredentialCreateOutcome::Created(record) => map_credential(*record),
                AdminCredentialCreateOutcome::ChannelNotFound => {
                    Err(AdminChannelWriteError::ChannelNotFound)
                }
            }
        })
    }

    fn update_credential<'a>(
        &'a self,
        principal: SessionPrincipal,
        channel_id: ChannelId,
        credential_id: CredentialId,
        command: AdminCredentialUpdateCommand,
    ) -> AdminCredentialUpdateFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            let (record, secret) = command.into_parts();
            let replacement = secret
                .as_ref()
                .map(|secret| {
                    self.encryptor
                        .encrypt(channel_id, credential_id.get(), secret)
                })
                .transpose()
                .map_err(|_| AdminChannelWriteError::Internal)?;
            match self
                .repository
                .update_credential(channel_id, credential_id, record, replacement)
                .await
                .map_err(map_repository_error)?
            {
                AdminCredentialMutationOutcome::Mutated(record) => map_credential(*record),
                AdminCredentialMutationOutcome::ChannelNotFound => {
                    Err(AdminChannelWriteError::ChannelNotFound)
                }
                AdminCredentialMutationOutcome::CredentialNotFound => {
                    Err(AdminChannelWriteError::CredentialNotFound)
                }
            }
        })
    }

    fn delete_credential<'a>(
        &'a self,
        principal: SessionPrincipal,
        channel_id: ChannelId,
        credential_id: CredentialId,
    ) -> AdminCredentialDeleteFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            match self
                .repository
                .delete_credential(channel_id, credential_id)
                .await
                .map_err(map_repository_error)?
            {
                AdminCredentialDeleteOutcome::Deleted => Ok(()),
                AdminCredentialDeleteOutcome::ChannelNotFound => {
                    Err(AdminChannelWriteError::ChannelNotFound)
                }
                AdminCredentialDeleteOutcome::CredentialNotFound => {
                    Err(AdminChannelWriteError::CredentialNotFound)
                }
            }
        })
    }

    fn export_oauth_credentials<'a>(
        &'a self,
        principal: SessionPrincipal,
        channel_id: ChannelId,
    ) -> AdminCredentialExportFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            let decryptor = self
                .decryptor
                .as_ref()
                .ok_or(AdminChannelWriteError::Internal)?;
            let records = self
                .repository
                .list_oauth_credential_secrets(channel_id)
                .await
                .map_err(map_repository_read_error)?
                .ok_or(AdminChannelWriteError::ChannelNotFound)?;
            records
                .into_iter()
                .map(|record: AdminCredentialSecretRecord| {
                    let credential = record.credential();
                    let secret = decryptor
                        .decrypt_oauth_envelope(
                            channel_id,
                            credential.credential_id().get(),
                            record.envelope(),
                        )
                        .map_err(|_| AdminChannelWriteError::Internal)?;
                    Ok(AdminCredentialExport {
                        credential_id: credential.credential_id(),
                        oauth_account_key: credential.oauth_account_key().map(str::to_owned),
                        oauth_project_id: credential.oauth_project_id().map(str::to_owned),
                        secret,
                    })
                })
                .collect()
        })
    }
}

impl fmt::Debug for DatabaseAdminChannelWriter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseAdminChannelWriter(<已脱敏>)")
    }
}

fn require_admin(principal: SessionPrincipal) -> Result<(), AdminChannelWriteError> {
    if principal.role() == SessionRole::Admin {
        Ok(())
    } else {
        Err(AdminChannelWriteError::Forbidden)
    }
}

/// 记录平台渠道写入后的闭合仿真状态，不展开任何敏感设置。
fn audit_client_simulation_write(
    operation: &'static str,
    principal: SessionPrincipal,
    channel: &AdminChannel,
) {
    tracing::info!(
        target: "af_admin::channel_audit",
        audit_event = "channel_client_simulation_write",
        operation,
        actor_user_id = principal.user_id().get(),
        channel_id = channel.channel_id().get(),
        client_simulation_profile = channel
            .client_simulation_profile()
            .map_or("disabled", af_domain::ClientSimulationProfile::as_str),
        client_simulation_body_profile = channel
            .client_simulation_body_profile()
            .map_or("disabled", af_domain::ClientSimulationBodyProfile::as_str),
        "管理员已写入渠道客户端仿真配置"
    );
}

fn map_repository_error(error: AdminChannelWriteRepositoryError) -> AdminChannelWriteError {
    match error {
        AdminChannelWriteRepositoryError::InvalidInput
        | AdminChannelWriteRepositoryError::InvalidReference => {
            AdminChannelWriteError::InvalidInput
        }
        AdminChannelWriteRepositoryError::SecretPreparation
        | AdminChannelWriteRepositoryError::Query
        | AdminChannelWriteRepositoryError::Timeout
        | AdminChannelWriteRepositoryError::Invariant => AdminChannelWriteError::Internal,
    }
}

fn map_repository_read_error(error: AdminChannelRepositoryError) -> AdminChannelWriteError {
    match error {
        AdminChannelRepositoryError::Query
        | AdminChannelRepositoryError::Timeout
        | AdminChannelRepositoryError::Invariant => AdminChannelWriteError::Internal,
    }
}

fn map_credential(
    record: af_db::AdminCredentialRecord,
) -> Result<AdminCredential, AdminChannelWriteError> {
    AdminCredential::from_record(record).map_err(|_| AdminChannelWriteError::Internal)
}

fn validate_json_object(
    value: &serde_json::Value,
    string_values_only: bool,
) -> Result<(), AdminChannelWriteError> {
    let serde_json::Value::Object(object) = value else {
        return Err(AdminChannelWriteError::InvalidInput);
    };
    if serde_json::to_vec(value)
        .ok()
        .is_none_or(|encoded| encoded.len() > MAX_ADMIN_CHANNEL_JSON_BYTES as usize)
        || (string_values_only
            && object.iter().any(|(key, value)| {
                !valid_text(key, 256) || value.as_str().is_none_or(|value| !valid_text(value, 256))
            }))
    {
        return Err(AdminChannelWriteError::InvalidInput);
    }
    Ok(())
}

fn valid_optional_text(value: Option<&str>, maximum_bytes: usize) -> bool {
    value.is_none_or(|value| valid_text(value, maximum_bytes))
}

fn valid_text(value: &str, maximum_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum_bytes
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

fn valid_channel_routing(models: &[String], group_ids: &[GroupId]) -> bool {
    if models.len() > MAX_ADMIN_CHANNEL_MODELS
        || group_ids.len() > MAX_ADMIN_CHANNEL_GROUPS
        || models
            .len()
            .checked_mul(group_ids.len())
            .is_none_or(|count| count > MAX_ADMIN_CHANNEL_ABILITIES)
    {
        return false;
    }
    let mut seen_models = std::collections::HashSet::with_capacity(models.len());
    if models.iter().any(|model| {
        !valid_text(model, MAX_ADMIN_CHANNEL_MODEL_BYTES) || !seen_models.insert(model.as_str())
    }) {
        return false;
    }
    let mut seen_groups = std::collections::HashSet::with_capacity(group_ids.len());
    !group_ids
        .iter()
        .any(|group_id| !seen_groups.insert(*group_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_routing_uses_the_database_model_byte_limit() {
        let group_ids = [GroupId::new(1).unwrap()];
        assert!(valid_channel_routing(
            &["x".repeat(MAX_ADMIN_CHANNEL_MODEL_BYTES)],
            &group_ids,
        ));
        assert!(!valid_channel_routing(
            &["x".repeat(MAX_ADMIN_CHANNEL_MODEL_BYTES + 1)],
            &group_ids,
        ));
    }

    #[test]
    fn commands_reject_unsupported_types_and_redact_sensitive_fields() {
        let channel = AdminChannelCreateCommand::new(
            "private".to_owned(),
            ChannelType::OpenAi,
            Protocol::OpenAiChat,
            Some("https://private.example.com".to_owned()),
            Some(60),
            AdminRoutingWriteStatus::Disabled,
            10,
            20,
            true,
            vec!["gpt-test".to_owned()],
            vec![GroupId::new(1).unwrap()],
            serde_json::json!({"model": "upstream"}),
            serde_json::json!({}),
            serde_json::json!({"x-private": "header-secret"}),
            serde_json::json!({"private": "settings-secret"}),
            None,
        )
        .unwrap();
        let credential = AdminCredentialCreateCommand::new(
            CredentialKind::ApiKey,
            PlainCredentialSecret::api_key("sk-private".to_owned()).unwrap(),
            AdminRoutingWriteStatus::Disabled,
            None,
            0,
            0,
            None,
            None,
            None,
            false,
            None,
            AdminCredentialQuotaDimension::Global,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        let rendered = format!("{channel:?}{credential:?}");
        for canary in [
            "private.example.com",
            "header-secret",
            "settings-secret",
            "sk-private",
        ] {
            assert!(!rendered.contains(canary));
        }

        assert!(
            AdminChannelCreateCommand::new(
                "anthropic".to_owned(),
                ChannelType::Anthropic,
                Protocol::Anthropic,
                None,
                None,
                AdminRoutingWriteStatus::Disabled,
                10,
                0,
                true,
                Vec::new(),
                Vec::new(),
                serde_json::json!({}),
                serde_json::json!({}),
                serde_json::json!({}),
                serde_json::json!({}),
                None,
            )
            .is_ok()
        );
        assert!(
            AdminChannelCreateCommand::new(
                "gemini".to_owned(),
                ChannelType::Gemini,
                Protocol::Gemini,
                None,
                None,
                AdminRoutingWriteStatus::Disabled,
                10,
                0,
                true,
                Vec::new(),
                Vec::new(),
                serde_json::json!({}),
                serde_json::json!({}),
                serde_json::json!({}),
                serde_json::json!({}),
                None,
            )
            .is_ok()
        );
        assert!(
            AdminChannelCreateCommand::new(
                "mismatch".to_owned(),
                ChannelType::Anthropic,
                Protocol::OpenAiChat,
                None,
                None,
                AdminRoutingWriteStatus::Disabled,
                10,
                0,
                true,
                Vec::new(),
                Vec::new(),
                serde_json::json!({}),
                serde_json::json!({}),
                serde_json::json!({}),
                serde_json::json!({}),
                None,
            )
            .is_err()
        );
        assert!(
            AdminCredentialCreateCommand::new(
                CredentialKind::Bedrock,
                PlainCredentialSecret::api_key("private".to_owned()).unwrap(),
                AdminRoutingWriteStatus::Disabled,
                None,
                0,
                0,
                None,
                None,
                None,
                false,
                None,
                AdminCredentialQuotaDimension::Global,
                None,
                None,
                None,
                None,
            )
            .is_err()
        );
    }

    #[test]
    fn credential_commands_keep_spark_shadow_shape_closed() {
        let parent_id = CredentialId::new(41).unwrap();
        let shadow = AdminCredentialCreateCommand::new_spark_shadow(
            AdminRoutingWriteStatus::Enabled,
            Some(AdminCredentialMultiKeyMode::RoundRobin),
            10,
            20,
            None,
            Some(1_000_000),
            Some(900_000),
            true,
            parent_id,
        )
        .unwrap();
        let (record, mode) = shadow.into_parts();
        assert_eq!(record.kind(), CredentialKind::Oauth);
        assert_eq!(record.parent_id(), Some(parent_id));
        assert_eq!(
            record.quota_dimension(),
            AdminCredentialQuotaDimension::Spark
        );
        assert!(matches!(mode, AdminCredentialCreateMode::SparkShadow));

        assert!(
            AdminCredentialCreateCommand::new_spark_shadow(
                AdminRoutingWriteStatus::Enabled,
                None,
                10,
                20,
                Some(2),
                None,
                None,
                true,
                parent_id,
            )
            .is_err()
        );

        assert!(
            AdminCredentialCreateCommand::new(
                CredentialKind::Oauth,
                PlainCredentialSecret::oauth_access_token("spark-token".to_owned()).unwrap(),
                AdminRoutingWriteStatus::Enabled,
                None,
                0,
                1,
                None,
                None,
                None,
                true,
                Some(parent_id),
                AdminCredentialQuotaDimension::Spark,
                None,
                None,
                None,
                None,
            )
            .is_err()
        );
        assert!(
            AdminCredentialUpdateCommand::new(
                CredentialKind::Oauth,
                Some(PlainCredentialSecret::oauth_access_token("spark-token".to_owned()).unwrap(),),
                AdminRoutingWriteStatus::Enabled,
                None,
                0,
                1,
                None,
                None,
                None,
                true,
                Some(parent_id),
                AdminCredentialQuotaDimension::Spark,
                None,
                None,
                None,
                None,
            )
            .is_err()
        );
        assert!(
            AdminCredentialUpdateCommand::new(
                CredentialKind::Oauth,
                None,
                AdminRoutingWriteStatus::Enabled,
                None,
                0,
                1,
                None,
                None,
                None,
                true,
                Some(parent_id),
                AdminCredentialQuotaDimension::Global,
                None,
                None,
                None,
                None,
            )
            .is_err()
        );
    }

    #[test]
    fn responses_channels_accept_supported_overrides_and_reject_stop_sequences() {
        let build = |parameters| {
            AdminChannelCreateCommand::new(
                "responses".to_owned(),
                ChannelType::OpenAi,
                Protocol::OpenAiResponses,
                None,
                None,
                AdminRoutingWriteStatus::Disabled,
                10,
                0,
                true,
                Vec::new(),
                Vec::new(),
                serde_json::json!({}),
                parameters,
                serde_json::json!({}),
                serde_json::json!({}),
                None,
            )
        };

        assert!(build(serde_json::json!({"max_output_tokens": 128})).is_ok());
        assert!(build(serde_json::json!({"stop_sequences": ["private"]})).is_err());
    }

    #[test]
    fn anthropic_channels_accept_messages_parameters_and_reject_hot_temperature() {
        let build = |parameters| {
            AdminChannelCreateCommand::new(
                "anthropic".to_owned(),
                ChannelType::Anthropic,
                Protocol::Anthropic,
                None,
                None,
                AdminRoutingWriteStatus::Disabled,
                10,
                0,
                true,
                Vec::new(),
                Vec::new(),
                serde_json::json!({}),
                parameters,
                serde_json::json!({}),
                serde_json::json!({}),
                None,
            )
        };

        assert!(
            build(serde_json::json!({
                "temperature": 1.0,
                "stop_sequences": ["private"]
            }))
            .is_ok()
        );
        assert!(build(serde_json::json!({"temperature": 1.01})).is_err());
    }

    #[test]
    fn channel_timeout_is_optional_and_bounded() {
        let build = |timeout_secs| {
            AdminChannelCreateCommand::new(
                "timeout".to_owned(),
                ChannelType::OpenAi,
                Protocol::OpenAiChat,
                None,
                timeout_secs,
                AdminRoutingWriteStatus::Disabled,
                10,
                0,
                true,
                Vec::new(),
                Vec::new(),
                serde_json::json!({}),
                serde_json::json!({}),
                serde_json::json!({}),
                serde_json::json!({}),
                None,
            )
        };

        assert!(build(None).is_ok());
        assert!(build(Some(1)).is_ok());
        assert!(build(Some(900)).is_ok());
        assert!(build(Some(0)).is_err());
        assert!(build(Some(901)).is_err());
    }
}
