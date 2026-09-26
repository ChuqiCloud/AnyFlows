use std::{
    collections::{HashMap, HashSet},
    fmt,
    str::FromStr as _,
};

use af_domain::{
    ChannelAutoBanRules, ChannelId, ChannelTimeout, ChannelType, ClientSimulationBodyProfile,
    ClientSimulationProfile, CredentialId, CredentialKind, CredentialQuotaDimension, GroupId,
    Protocol, ResponsesCompactMode, Status,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseTransaction, DbBackend, EntityTrait,
    QueryFilter, QuerySelect, Set, TransactionTrait,
    entity::prelude::TimeDateTimeWithTimeZone,
    sea_query::{Alias, Expr, LockType, Query},
};
use thiserror::Error;
use tokio::time::timeout;
use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};

use crate::{
    AdminChannelRecord, AdminChannelRepository, AdminChannelRepositoryError, AdminCredentialRecord,
    ChannelModelMappings, ChannelParameterOverrides, EncryptedCredentialEnvelope,
    MAX_ADMIN_CHANNEL_JSON_BYTES, SchedulerCatalogSubject,
    ability_write::{
        AbilityWriteError, ChannelAbilityMetadata, ValidatedChannelRouting,
        delete_channel_abilities, synchronize_channel_routing,
    },
    admin_channel::valid_text,
    channel_settings::{
        auto_ban_rules, clear_responses_compact_probe_record, client_simulation_body_profile,
        client_simulation_profile, pool_mode, responses_compact_mode,
        responses_compact_model_mapping, responses_websocket_enabled, set_auto_ban_rules,
        set_client_simulation_body_profile, set_client_simulation_profile, set_pool_mode,
        set_responses_compact_mode, set_responses_compact_model_mapping,
        set_responses_websocket_enabled, validate_client_simulation_body_capability,
        validate_client_simulation_capability, validate_responses_compact_capability,
        validate_responses_compact_model_mapping, validate_responses_websocket_capability,
    },
    entity::{
        ChannelBaseUrl, EncryptedJson, HeaderOverrides, SensitiveJson, channel_groups,
        channel_models, channels, credentials, proxies,
    },
    scheduler_outbox::enqueue_scheduler_catalog_change,
    validate_channel_header_overrides,
};

/// 单次凭据父子图校验允许读取的最大有效记录数。
pub const MAX_ADMIN_CREDENTIAL_GRAPH_SIZE: usize = 100_000;

const SHADOW_REFERENCE_KEY_ID: &str = "shadow-reference";
const SOFT_DELETE_BATCH_SIZE: usize = 500;

/// 管理端创建或完整更新渠道时写入的业务字段。
pub struct AdminChannelWriteRecord {
    provider: Option<String>,
    name: String,
    channel_type: ChannelType,
    protocol: Protocol,
    base_url: Option<String>,
    timeout: Option<ChannelTimeout>,
    status: Status,
    weight: i32,
    priority: i32,
    auto_ban: bool,
    auto_ban_rules: Option<ChannelAutoBanRules>,
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
    tag: Option<String>,
}

impl AdminChannelWriteRecord {
    /// 组装已经由应用层校验公开边界的渠道写入记录。
    #[allow(
        clippy::too_many_arguments,
        reason = "字段与管理端渠道写入契约一一对应"
    )]
    #[must_use]
    pub fn new(
        name: String,
        channel_type: ChannelType,
        protocol: Protocol,
        base_url: Option<String>,
        timeout: Option<ChannelTimeout>,
        status: Status,
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
    ) -> Self {
        Self {
            name,
            channel_type,
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
            header_override: Some(header_override),
            settings: Some(settings),
            provider: None,
            pool_mode: Some(false),
            client_simulation_profile: Some(None),
            client_simulation_risk_accepted: false,
            client_simulation_body_profile: Some(None),
            client_simulation_body_risk_accepted: false,
            responses_websocket_enabled: Some(false),
            responses_compact_mode: None,
            responses_compact_model_mapping: None,
            auto_ban_rules: Some(ChannelAutoBanRules::default()),
            tag,
        }
    }

    /// 组装更新记录；省略敏感对象时由仓储保留数据库原值。
    #[allow(
        clippy::too_many_arguments,
        reason = "字段与管理端渠道更新契约一一对应"
    )]
    #[must_use]
    pub fn new_update(
        name: String,
        channel_type: ChannelType,
        protocol: Protocol,
        base_url: Option<String>,
        timeout: Option<ChannelTimeout>,
        status: Status,
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
    ) -> Self {
        Self {
            name,
            channel_type,
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
            pool_mode: None,
            provider: None,
            client_simulation_profile: None,
            client_simulation_risk_accepted: false,
            client_simulation_body_profile: None,
            client_simulation_body_risk_accepted: false,
            responses_websocket_enabled: None,
            responses_compact_mode: None,
            responses_compact_model_mapping: None,
            auto_ban_rules: None,
            tag,
        }
    }

    /// 设置非敏感厂商标识；更新时省略则保留。
    #[must_use]
    pub fn with_provider(mut self, provider: Option<String>) -> Self {
        self.provider = provider;
        self
    }

    /// 覆盖受控 Responses WebSocket 能力；调用方必须先完成渠道类型校验。
    #[must_use]
    pub fn with_responses_websocket_enabled(mut self, enabled: bool) -> Self {
        self.responses_websocket_enabled = Some(enabled);
        self
    }

    /// 覆盖受控 Compact 三态能力与专属模型映射。
    #[must_use]
    pub fn with_responses_compact_configuration(
        mut self,
        mode: ResponsesCompactMode,
        model_mapping: serde_json::Value,
    ) -> Self {
        self.responses_compact_mode = Some(mode);
        self.responses_compact_model_mapping = Some(model_mapping);
        self
    }

    /// 覆盖外部账号池模式；省略时更新接口保留原值。
    #[must_use]
    pub fn with_pool_mode(mut self, enabled: bool) -> Self {
        self.pool_mode = Some(enabled);
        self
    }

    /// 覆盖版本化仿真档案与本次写入的显式风险确认。
    #[must_use]
    pub fn with_client_simulation_profile(
        mut self,
        profile: Option<ClientSimulationProfile>,
        risk_accepted: bool,
    ) -> Self {
        self.client_simulation_profile = Some(profile);
        self.client_simulation_risk_accepted = risk_accepted;
        self
    }

    /// 覆盖版本化正文仿真档案与本次写入的独立风险确认。
    #[must_use]
    pub fn with_client_simulation_body_profile(
        mut self,
        profile: Option<ClientSimulationBodyProfile>,
        risk_accepted: bool,
    ) -> Self {
        self.client_simulation_body_profile = Some(profile);
        self.client_simulation_body_risk_accepted = risk_accepted;
        self
    }

    /// 覆盖渠道自动禁用状态码与关键词规则；省略时更新接口保留原值。
    #[must_use]
    pub fn with_auto_ban_rules(mut self, rules: ChannelAutoBanRules) -> Self {
        self.auto_ban_rules = Some(rules);
        self
    }
}

impl fmt::Debug for AdminChannelWriteRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminChannelWriteRecord(<已脱敏>)")
    }
}

/// 管理端创建或完整更新凭据时写入的非明文字段。
pub struct AdminCredentialWriteRecord {
    kind: CredentialKind,
    status: Status,
    multi_key_mode: Option<i16>,
    priority: i32,
    weight: i32,
    concurrency: Option<i32>,
    load_factor_micros: Option<i64>,
    rate_multiplier_micros: Option<i64>,
    schedulable: bool,
    parent_id: Option<CredentialId>,
    quota_dimension: CredentialQuotaDimension,
    proxy_id: Option<i64>,
    oauth_provider: Option<String>,
    /// 创建时可显式声明等待 OAuth 首次授权；更新时为 `None` 表示保留数据库原值。
    oauth_token_pending: Option<bool>,
    oauth_account_key: Option<String>,
    oauth_project_id: Option<String>,
}

impl AdminCredentialWriteRecord {
    /// 组装已经由应用层校验公开边界的凭据元数据。
    #[allow(
        clippy::too_many_arguments,
        reason = "字段与管理端凭据写入契约一一对应"
    )]
    #[must_use]
    pub fn new(
        kind: CredentialKind,
        status: Status,
        multi_key_mode: Option<i16>,
        priority: i32,
        weight: i32,
        concurrency: Option<i32>,
        load_factor_micros: Option<i64>,
        rate_multiplier_micros: Option<i64>,
        schedulable: bool,
        parent_id: Option<CredentialId>,
        quota_dimension: CredentialQuotaDimension,
        proxy_id: Option<i64>,
        oauth_provider: Option<String>,
        oauth_account_key: Option<String>,
        oauth_project_id: Option<String>,
    ) -> Self {
        Self {
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
            oauth_token_pending: None,
            oauth_account_key,
            oauth_project_id,
        }
    }

    /// 返回请求声明的稳定凭据类型。
    #[must_use]
    pub const fn kind(&self) -> CredentialKind {
        self.kind
    }

    /// 返回创建时固定的母凭据；普通凭据没有母级。
    #[must_use]
    pub const fn parent_id(&self) -> Option<CredentialId> {
        self.parent_id
    }

    /// 返回创建时固定的额度维度。
    #[must_use]
    pub const fn quota_dimension(&self) -> CredentialQuotaDimension {
        self.quota_dimension
    }

    /// 设置创建阶段的 OAuth 待授权标记；普通更新不得使用该字段覆盖数据库状态。
    #[must_use]
    pub fn with_oauth_token_pending(mut self, pending: bool) -> Self {
        self.oauth_token_pending = Some(pending);
        self
    }
}

impl fmt::Debug for AdminCredentialWriteRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AdminCredentialWriteRecord(<已脱敏>)")
    }
}

/// 渠道完整更新结果。
pub enum AdminChannelMutationOutcome {
    /// 已更新并返回非敏感快照。
    Mutated(Box<AdminChannelRecord>),
    /// 渠道不存在或已经软删除。
    NotFound,
}

impl fmt::Debug for AdminChannelMutationOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Mutated(_) => {
                formatter.write_str("AdminChannelMutationOutcome::Mutated(<已脱敏>)")
            }
            Self::NotFound => formatter.write_str("AdminChannelMutationOutcome::NotFound"),
        }
    }
}

/// 渠道安全软删除结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdminChannelDeleteOutcome {
    /// 渠道、凭据和直接运行时关系已在同一事务内删除。
    Deleted,
    /// 渠道不存在或已经软删除。
    NotFound,
}

/// 凭据创建结果；父渠道缺失不会伪装成通用数据库错误。
pub enum AdminCredentialCreateOutcome {
    /// 已创建并返回非敏感凭据快照。
    Created(Box<AdminCredentialRecord>),
    /// 父渠道不存在或已经软删除。
    ChannelNotFound,
}

impl fmt::Debug for AdminCredentialCreateOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Created(_) => {
                formatter.write_str("AdminCredentialCreateOutcome::Created(<已脱敏>)")
            }
            Self::ChannelNotFound => {
                formatter.write_str("AdminCredentialCreateOutcome::ChannelNotFound")
            }
        }
    }
}

/// 凭据完整更新结果。
pub enum AdminCredentialMutationOutcome {
    /// 已更新并返回非敏感凭据快照。
    Mutated(Box<AdminCredentialRecord>),
    /// 父渠道不存在或已经软删除。
    ChannelNotFound,
    /// 凭据不存在、已删除或不属于父渠道。
    CredentialNotFound,
}

impl fmt::Debug for AdminCredentialMutationOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Mutated(_) => {
                formatter.write_str("AdminCredentialMutationOutcome::Mutated(<已脱敏>)")
            }
            Self::ChannelNotFound => {
                formatter.write_str("AdminCredentialMutationOutcome::ChannelNotFound")
            }
            Self::CredentialNotFound => {
                formatter.write_str("AdminCredentialMutationOutcome::CredentialNotFound")
            }
        }
    }
}

/// 凭据安全软删除结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdminCredentialDeleteOutcome {
    /// 凭据及其全部有效影子后代已软删除。
    Deleted,
    /// 父渠道不存在或已经软删除。
    ChannelNotFound,
    /// 凭据不存在、已删除或不属于父渠道。
    CredentialNotFound,
}

/// 管理渠道与凭据写仓储错误；不携带配置、密文或数据库诊断。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AdminChannelWriteRepositoryError {
    /// 写入字段违反持久化边界。
    #[error("管理渠道写入参数无效")]
    InvalidInput,
    /// 父凭据引用无效或会破坏父子图。
    #[error("管理凭据引用无效")]
    InvalidReference,
    /// 应用层未能为真实凭据 ID 生成密文封套。
    #[error("管理凭据密文准备失败")]
    SecretPreparation,
    /// 获取连接、执行查询或提交事务失败。
    #[error("管理渠道数据库写入失败")]
    Query,
    /// 完整写事务超过硬截止时间。
    #[error("管理渠道数据库写入超时")]
    Timeout,
    /// 持久化状态、父子图或影响行数违反不变量。
    #[error("管理渠道持久化状态损坏")]
    Invariant,
}

impl AdminChannelRepository {
    /// 创建渠道；累计用量和探测余额始终由持久化默认值初始化。
    pub async fn create_channel(
        &self,
        record: AdminChannelWriteRecord,
    ) -> Result<AdminChannelRecord, AdminChannelWriteRepositoryError> {
        match timeout(self.lookup_timeout, self.create_channel_inner(record)).await {
            Ok(result) => result,
            Err(_) => Err(internal_error(AdminChannelWriteRepositoryError::Timeout)),
        }
    }

    /// 完整更新渠道配置，同时保留余额和累计用量等运行字段。
    pub async fn update_channel(
        &self,
        channel_id: ChannelId,
        record: AdminChannelWriteRecord,
    ) -> Result<AdminChannelMutationOutcome, AdminChannelWriteRepositoryError> {
        match timeout(
            self.lookup_timeout,
            self.update_channel_inner(channel_id, record),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(internal_error(AdminChannelWriteRepositoryError::Timeout)),
        }
    }

    /// 软删除渠道及其凭据，并物理清理可重建的运行时关系。
    pub async fn delete_channel(
        &self,
        channel_id: ChannelId,
    ) -> Result<AdminChannelDeleteOutcome, AdminChannelWriteRepositoryError> {
        match timeout(self.lookup_timeout, self.delete_channel_inner(channel_id)).await {
            Ok(result) => result,
            Err(_) => Err(internal_error(AdminChannelWriteRepositoryError::Timeout)),
        }
    }

    /// 创建凭据；回调只接收事务内生成的真实 ID，并且必须返回密文封套。
    pub async fn create_credential<F>(
        &self,
        channel_id: ChannelId,
        record: AdminCredentialWriteRecord,
        prepare_secret: F,
    ) -> Result<AdminCredentialCreateOutcome, AdminChannelWriteRepositoryError>
    where
        F: FnOnce(CredentialId) -> Result<EncryptedCredentialEnvelope, ()> + Send,
    {
        match timeout(
            self.lookup_timeout,
            self.create_credential_inner(
                channel_id,
                record,
                Some(prepare_secret),
                CredentialPlaceholder::TransactionPending,
            ),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(internal_error(AdminChannelWriteRepositoryError::Timeout)),
        }
    }

    /// 创建尚未完成首次 OAuth 交换的凭据；占位密文不会调用加密回调。
    pub async fn create_pending_oauth_credential(
        &self,
        channel_id: ChannelId,
        record: AdminCredentialWriteRecord,
    ) -> Result<AdminCredentialCreateOutcome, AdminChannelWriteRepositoryError> {
        match timeout(
            self.lookup_timeout,
            self.create_credential_inner(
                channel_id,
                record,
                None::<fn(CredentialId) -> Result<EncryptedCredentialEnvelope, ()>>,
                CredentialPlaceholder::OauthPending,
            ),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(internal_error(AdminChannelWriteRepositoryError::Timeout)),
        }
    }

    /// 创建不持有密钥的 Spark 影子；运行时必须只读取母凭据密文。
    pub async fn create_spark_shadow_credential(
        &self,
        channel_id: ChannelId,
        record: AdminCredentialWriteRecord,
    ) -> Result<AdminCredentialCreateOutcome, AdminChannelWriteRepositoryError> {
        match timeout(
            self.lookup_timeout,
            self.create_credential_inner(
                channel_id,
                record,
                None::<fn(CredentialId) -> Result<EncryptedCredentialEnvelope, ()>>,
                CredentialPlaceholder::SparkShadow,
            ),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(internal_error(AdminChannelWriteRepositoryError::Timeout)),
        }
    }

    /// 完整更新凭据元数据；仅在给出新封套时轮换 secret。
    pub async fn update_credential(
        &self,
        channel_id: ChannelId,
        credential_id: CredentialId,
        record: AdminCredentialWriteRecord,
        replacement_secret: Option<EncryptedCredentialEnvelope>,
    ) -> Result<AdminCredentialMutationOutcome, AdminChannelWriteRepositoryError> {
        match timeout(
            self.lookup_timeout,
            self.update_credential_inner(channel_id, credential_id, record, replacement_secret),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(internal_error(AdminChannelWriteRepositoryError::Timeout)),
        }
    }

    /// 软删除凭据及其影子后代；损坏或成环父子图失败关闭。
    pub async fn delete_credential(
        &self,
        channel_id: ChannelId,
        credential_id: CredentialId,
    ) -> Result<AdminCredentialDeleteOutcome, AdminChannelWriteRepositoryError> {
        match timeout(
            self.lookup_timeout,
            self.delete_credential_inner(channel_id, credential_id),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => Err(internal_error(AdminChannelWriteRepositoryError::Timeout)),
        }
    }

    async fn create_channel_inner(
        &self,
        record: AdminChannelWriteRecord,
    ) -> Result<AdminChannelRecord, AdminChannelWriteRepositoryError> {
        let fields = validate_channel_fields(record, true)?;
        let transaction = begin_transaction(self).await?;
        let now = TimeDateTimeWithTimeZone::now_utc();
        let ability_tag = fields.tag.clone();
        let ability_metadata = ChannelAbilityMetadata {
            enabled: fields.status.is_enabled(),
            priority: fields.priority,
            weight: fields.weight,
            tag: ability_tag.as_deref(),
        };
        let header_override = fields
            .header_override
            .ok_or_else(|| internal_error(AdminChannelWriteRepositoryError::Invariant))?;
        let mut settings = fields
            .settings
            .ok_or_else(|| internal_error(AdminChannelWriteRepositoryError::Invariant))?;
        set_provider(&mut settings, fields.provider)?;
        let pool_mode = fields
            .pool_mode
            .ok_or_else(|| internal_error(AdminChannelWriteRepositoryError::Invariant))?;
        set_pool_mode(&mut settings, pool_mode)
            .map_err(|_| AdminChannelWriteRepositoryError::InvalidInput)?;
        let client_simulation_profile = fields
            .client_simulation_profile
            .ok_or_else(|| internal_error(AdminChannelWriteRepositoryError::Invariant))?;
        if client_simulation_profile.is_some() && !fields.client_simulation_risk_accepted {
            return Err(AdminChannelWriteRepositoryError::InvalidInput);
        }
        validate_client_simulation_capability(
            fields.channel_type,
            fields.protocol,
            client_simulation_profile,
        )
        .map_err(|_| AdminChannelWriteRepositoryError::InvalidInput)?;
        set_client_simulation_profile(&mut settings, client_simulation_profile)
            .map_err(|_| AdminChannelWriteRepositoryError::InvalidInput)?;
        let client_simulation_body_profile = fields
            .client_simulation_body_profile
            .ok_or_else(|| internal_error(AdminChannelWriteRepositoryError::Invariant))?;
        if client_simulation_body_profile.is_some() && !fields.client_simulation_body_risk_accepted
        {
            return Err(AdminChannelWriteRepositoryError::InvalidInput);
        }
        validate_client_simulation_body_capability(
            fields.channel_type,
            fields.protocol,
            client_simulation_profile,
            client_simulation_body_profile,
        )
        .map_err(|_| AdminChannelWriteRepositoryError::InvalidInput)?;
        set_client_simulation_body_profile(&mut settings, client_simulation_body_profile)
            .map_err(|_| AdminChannelWriteRepositoryError::InvalidInput)?;
        let responses_websocket_enabled = fields
            .responses_websocket_enabled
            .ok_or_else(|| internal_error(AdminChannelWriteRepositoryError::Invariant))?;
        set_responses_websocket_enabled(&mut settings, responses_websocket_enabled)
            .map_err(|_| AdminChannelWriteRepositoryError::InvalidInput)?;
        let responses_compact_mode = responses_compact_mode(&settings)
            .map_err(|_| AdminChannelWriteRepositoryError::InvalidInput)?;
        let responses_compact_mode = fields
            .responses_compact_mode
            .unwrap_or(responses_compact_mode);
        validate_responses_compact_capability(
            fields.channel_type,
            fields.protocol,
            responses_compact_mode,
        )
        .map_err(|_| AdminChannelWriteRepositoryError::InvalidInput)?;
        set_responses_compact_mode(&mut settings, responses_compact_mode)
            .map_err(|_| AdminChannelWriteRepositoryError::InvalidInput)?;
        let responses_compact_model_mapping = match fields.responses_compact_model_mapping {
            Some(mapping) => mapping,
            None => responses_compact_model_mapping(&settings)
                .map_err(|_| AdminChannelWriteRepositoryError::InvalidInput)?,
        };
        validate_responses_compact_model_mapping(
            fields.channel_type,
            fields.protocol,
            &responses_compact_model_mapping,
        )
        .map_err(|_| AdminChannelWriteRepositoryError::InvalidInput)?;
        set_responses_compact_model_mapping(&mut settings, &responses_compact_model_mapping)
            .map_err(|_| AdminChannelWriteRepositoryError::InvalidInput)?;
        // 探测事实只允许运行时写入，创建接口不得通过原始 settings 伪造支持状态。
        clear_responses_compact_probe_record(&mut settings)
            .map_err(|_| AdminChannelWriteRepositoryError::InvalidInput)?;
        let auto_ban_rules = fields
            .auto_ban_rules
            .as_ref()
            .ok_or_else(|| internal_error(AdminChannelWriteRepositoryError::Invariant))?;
        set_auto_ban_rules(&mut settings, auto_ban_rules)
            .map_err(|_| AdminChannelWriteRepositoryError::InvalidInput)?;
        let settings = SensitiveJson::from(settings);
        let inserted = channels::ActiveModel {
            name: Set(fields.name),
            r#type: Set(fields.channel_type.as_str().to_owned()),
            protocol: Set(fields.protocol.as_str().to_owned()),
            base_url: Set(fields.base_url),
            timeout_secs: Set(fields.timeout_secs),
            status: Set(fields.status.code()),
            weight: Set(fields.weight),
            priority: Set(fields.priority),
            auto_ban: Set(fields.auto_ban),
            model_mapping: Set(fields.model_mapping),
            param_override: Set(fields.param_override),
            header_override: Set(header_override),
            settings: Set(settings),
            tag: Set(fields.tag),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(map_db_error)?;
        let channel_id = ChannelId::new(inserted.id)
            .map_err(|_| internal_error(AdminChannelWriteRepositoryError::Invariant))?;
        synchronize_channel_routing(
            &transaction,
            channel_id,
            &fields.routing,
            ability_metadata,
            now,
        )
        .await
        .map_err(map_ability_write_error)?;
        let snapshot = AdminChannelRecord::try_from_model(inserted)
            .map_err(map_read_error)?
            .with_routing(fields.routing.snapshot());
        commit_transaction(transaction).await?;
        Ok(snapshot)
    }

    async fn update_channel_inner(
        &self,
        channel_id: ChannelId,
        record: AdminChannelWriteRecord,
    ) -> Result<AdminChannelMutationOutcome, AdminChannelWriteRepositoryError> {
        let fields = validate_channel_fields(record, false)?;
        let transaction = begin_transaction(self).await?;
        let Some(current) = find_active_channel(&transaction, channel_id).await? else {
            return Ok(AdminChannelMutationOutcome::NotFound);
        };
        let current_settings = current.settings.into_inner();
        let current_client_simulation_profile = client_simulation_profile(&current_settings)
            .map_err(|_| internal_error(AdminChannelWriteRepositoryError::Invariant))?;
        let effective_client_simulation_profile = fields
            .client_simulation_profile
            .unwrap_or(current_client_simulation_profile);
        if effective_client_simulation_profile.is_some()
            && effective_client_simulation_profile != current_client_simulation_profile
            && !fields.client_simulation_risk_accepted
        {
            return Err(AdminChannelWriteRepositoryError::InvalidInput);
        }
        validate_client_simulation_capability(
            fields.channel_type,
            fields.protocol,
            effective_client_simulation_profile,
        )
        .map_err(|_| AdminChannelWriteRepositoryError::InvalidInput)?;
        let current_client_simulation_body_profile =
            client_simulation_body_profile(&current_settings)
                .map_err(|_| internal_error(AdminChannelWriteRepositoryError::Invariant))?;
        let effective_client_simulation_body_profile = fields
            .client_simulation_body_profile
            .unwrap_or(current_client_simulation_body_profile);
        if effective_client_simulation_body_profile.is_some()
            && effective_client_simulation_body_profile != current_client_simulation_body_profile
            && !fields.client_simulation_body_risk_accepted
        {
            return Err(AdminChannelWriteRepositoryError::InvalidInput);
        }
        validate_client_simulation_body_capability(
            fields.channel_type,
            fields.protocol,
            effective_client_simulation_profile,
            effective_client_simulation_body_profile,
        )
        .map_err(|_| AdminChannelWriteRepositoryError::InvalidInput)?;
        let effective_pool_mode = match fields.pool_mode {
            Some(enabled) => enabled,
            None => pool_mode(&current_settings)
                .map_err(|_| internal_error(AdminChannelWriteRepositoryError::Invariant))?,
        };
        let effective_responses_websocket_enabled = match fields.responses_websocket_enabled {
            Some(enabled) => enabled,
            None => responses_websocket_enabled(&current_settings)
                .map_err(|_| internal_error(AdminChannelWriteRepositoryError::Invariant))?,
        };
        validate_responses_websocket_capability(
            fields.channel_type,
            fields.protocol,
            effective_responses_websocket_enabled,
        )
        .map_err(|_| AdminChannelWriteRepositoryError::InvalidInput)?;
        let effective_auto_ban_rules = match fields.auto_ban_rules {
            Some(rules) => rules,
            None => auto_ban_rules(&current_settings)
                .map_err(|_| internal_error(AdminChannelWriteRepositoryError::Invariant))?,
        };
        let provider = fields.provider.or_else(|| {
            current_settings
                .get("provider")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        });
        let mut settings = fields.settings.unwrap_or(current_settings);
        set_provider(&mut settings, provider)?;
        set_pool_mode(&mut settings, effective_pool_mode)
            .map_err(|_| AdminChannelWriteRepositoryError::InvalidInput)?;
        set_client_simulation_profile(&mut settings, effective_client_simulation_profile)
            .map_err(|_| AdminChannelWriteRepositoryError::InvalidInput)?;
        set_client_simulation_body_profile(&mut settings, effective_client_simulation_body_profile)
            .map_err(|_| AdminChannelWriteRepositoryError::InvalidInput)?;
        set_responses_websocket_enabled(&mut settings, effective_responses_websocket_enabled)
            .map_err(|_| AdminChannelWriteRepositoryError::InvalidInput)?;
        let responses_compact_mode = responses_compact_mode(&settings)
            .map_err(|_| AdminChannelWriteRepositoryError::InvalidInput)?;
        let responses_compact_mode = fields
            .responses_compact_mode
            .unwrap_or(responses_compact_mode);
        validate_responses_compact_capability(
            fields.channel_type,
            fields.protocol,
            responses_compact_mode,
        )
        .map_err(|_| AdminChannelWriteRepositoryError::InvalidInput)?;
        set_responses_compact_mode(&mut settings, responses_compact_mode)
            .map_err(|_| AdminChannelWriteRepositoryError::InvalidInput)?;
        let responses_compact_model_mapping = match fields.responses_compact_model_mapping {
            Some(mapping) => mapping,
            None => responses_compact_model_mapping(&settings)
                .map_err(|_| AdminChannelWriteRepositoryError::InvalidInput)?,
        };
        validate_responses_compact_model_mapping(
            fields.channel_type,
            fields.protocol,
            &responses_compact_model_mapping,
        )
        .map_err(|_| AdminChannelWriteRepositoryError::InvalidInput)?;
        set_responses_compact_model_mapping(&mut settings, &responses_compact_model_mapping)
            .map_err(|_| AdminChannelWriteRepositoryError::InvalidInput)?;
        // 任意管理更新都使既有探测事实失效，避免旧端点或模型结论继续参与调度。
        clear_responses_compact_probe_record(&mut settings)
            .map_err(|_| AdminChannelWriteRepositoryError::InvalidInput)?;
        set_auto_ban_rules(&mut settings, &effective_auto_ban_rules)
            .map_err(|_| AdminChannelWriteRepositoryError::InvalidInput)?;
        let now = TimeDateTimeWithTimeZone::now_utc();
        let ability_tag = fields.tag.clone();
        let ability_metadata = ChannelAbilityMetadata {
            enabled: fields.status.is_enabled(),
            priority: fields.priority,
            weight: fields.weight,
            tag: ability_tag.as_deref(),
        };
        let mut update = channels::Entity::update_many()
            .filter(channels::Column::Id.eq(channel_id.get()))
            .filter(channels::Column::DeletedAt.is_null())
            .col_expr(channels::Column::Name, Expr::value(fields.name))
            .col_expr(
                channels::Column::Type,
                Expr::value(fields.channel_type.as_str()),
            )
            .col_expr(
                channels::Column::Protocol,
                Expr::value(fields.protocol.as_str()),
            )
            .col_expr(channels::Column::BaseUrl, Expr::value(fields.base_url))
            .col_expr(
                channels::Column::TimeoutSecs,
                Expr::value(fields.timeout_secs),
            )
            .col_expr(channels::Column::Status, Expr::value(fields.status.code()))
            .col_expr(channels::Column::Weight, Expr::value(fields.weight))
            .col_expr(channels::Column::Priority, Expr::value(fields.priority))
            .col_expr(channels::Column::AutoBan, Expr::value(fields.auto_ban))
            .col_expr(
                channels::Column::ModelMapping,
                Expr::value(fields.model_mapping),
            )
            .col_expr(
                channels::Column::ParamOverride,
                Expr::value(fields.param_override),
            )
            .col_expr(channels::Column::Tag, Expr::value(fields.tag));
        if let Some(header_override) = fields.header_override {
            update = update.col_expr(
                channels::Column::HeaderOverride,
                Expr::value(header_override),
            );
        }
        update = update.col_expr(
            channels::Column::Settings,
            Expr::value(SensitiveJson::from(settings)),
        );
        let result = update
            .col_expr(channels::Column::UpdatedAt, Expr::value(now))
            .exec(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(map_db_error)?;
        require_rows(result.rows_affected, 1)?;
        synchronize_channel_routing(
            &transaction,
            channel_id,
            &fields.routing,
            ability_metadata,
            now,
        )
        .await
        .map_err(map_ability_write_error)?;
        let model = fetch_channel_model(&transaction, channel_id).await?;
        let snapshot = AdminChannelRecord::try_from_model(model)
            .map_err(map_read_error)?
            .with_routing(fields.routing.snapshot());
        commit_transaction(transaction).await?;
        Ok(AdminChannelMutationOutcome::Mutated(Box::new(snapshot)))
    }

    async fn delete_channel_inner(
        &self,
        channel_id: ChannelId,
    ) -> Result<AdminChannelDeleteOutcome, AdminChannelWriteRepositoryError> {
        let transaction = begin_transaction(self).await?;
        if !active_channel_exists(&transaction, channel_id).await? {
            return Ok(AdminChannelDeleteOutcome::NotFound);
        }
        let now = TimeDateTimeWithTimeZone::now_utc();

        // 能力项同时引用模型和分组关系，必须先于两张关系表删除。
        delete_channel_abilities(&transaction, channel_id, now)
            .await
            .map_err(map_ability_write_error)?;
        channel_models::Entity::delete_many()
            .filter(channel_models::Column::ChannelId.eq(channel_id.get()))
            .exec(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(map_db_error)?;
        channel_groups::Entity::delete_many()
            .filter(channel_groups::Column::ChannelId.eq(channel_id.get()))
            .exec(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(map_db_error)?;
        credentials::Entity::update_many()
            .filter(credentials::Column::ChannelId.eq(channel_id.get()))
            .filter(credentials::Column::DeletedAt.is_null())
            .col_expr(credentials::Column::DeletedAt, Expr::value(now))
            .col_expr(credentials::Column::UpdatedAt, Expr::value(now))
            .exec(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(map_db_error)?;
        let result = channels::Entity::update_many()
            .filter(channels::Column::Id.eq(channel_id.get()))
            .filter(channels::Column::DeletedAt.is_null())
            .col_expr(channels::Column::DeletedAt, Expr::value(now))
            .col_expr(channels::Column::UpdatedAt, Expr::value(now))
            .exec(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(map_db_error)?;
        require_rows(result.rows_affected, 1)?;
        commit_transaction(transaction).await?;
        Ok(AdminChannelDeleteOutcome::Deleted)
    }

    async fn create_credential_inner<F>(
        &self,
        channel_id: ChannelId,
        record: AdminCredentialWriteRecord,
        prepare_secret: Option<F>,
        placeholder_kind: CredentialPlaceholder,
    ) -> Result<AdminCredentialCreateOutcome, AdminChannelWriteRepositoryError>
    where
        F: FnOnce(CredentialId) -> Result<EncryptedCredentialEnvelope, ()>,
    {
        let fields = validate_credential_fields(record)?;
        let oauth_token_pending = fields.oauth_token_pending.unwrap_or(false);
        let valid_secret_path = matches!(
            (
                placeholder_kind,
                prepare_secret.is_some(),
                oauth_token_pending,
                fields.quota_dimension
            ),
            (
                CredentialPlaceholder::TransactionPending,
                true,
                false,
                CredentialQuotaDimension::Global
            ) | (
                CredentialPlaceholder::OauthPending,
                false,
                true,
                CredentialQuotaDimension::Global
            ) | (
                CredentialPlaceholder::SparkShadow,
                false,
                false,
                CredentialQuotaDimension::Spark
            )
        );
        if !valid_secret_path {
            return Err(AdminChannelWriteRepositoryError::InvalidInput);
        }
        let transaction = begin_transaction(self).await?;
        let Some(channel) = find_active_channel(&transaction, channel_id).await? else {
            return Ok(AdminCredentialCreateOutcome::ChannelNotFound);
        };
        validate_credential_channel_compatibility(&channel, fields.kind)?;
        if fields.quota_dimension == CredentialQuotaDimension::Spark {
            validate_spark_shadow_channel(&channel)?;
            lock_and_validate_spark_parent(&transaction, channel_id, &fields, None).await?;
        }
        validate_proxy_binding(&transaction, fields.proxy_id).await?;
        let placeholder = match placeholder_kind {
            CredentialPlaceholder::TransactionPending => pending_encrypted_json()?,
            CredentialPlaceholder::OauthPending => oauth_token_pending_encrypted_json()?,
            CredentialPlaceholder::SparkShadow => spark_shadow_reference_encrypted_json()?,
        };
        let inserted = credentials::ActiveModel {
            channel_id: Set(channel_id.get()),
            kind: Set(fields.kind.as_str().to_owned()),
            secret: Set(placeholder),
            status: Set(fields.status.code()),
            multi_key_mode: Set(fields.multi_key_mode),
            priority: Set(fields.priority),
            weight: Set(fields.weight),
            concurrency: Set(fields.concurrency),
            load_factor_micros: Set(fields.load_factor_micros),
            rate_multiplier_micros: Set(fields.rate_multiplier_micros),
            schedulable: Set(fields.schedulable),
            parent_id: Set(fields.parent_id.map(CredentialId::get)),
            quota_dimension: Set(fields.quota_dimension.as_str().to_owned()),
            proxy_id: Set(fields.proxy_id),
            oauth_provider: Set(fields.oauth_provider),
            oauth_token_pending: Set(oauth_token_pending),
            oauth_account_key: Set(fields.oauth_account_key),
            oauth_project_id: Set(fields.oauth_project_id),
            ..Default::default()
        }
        .insert(&transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(map_db_error)?;
        let credential_id = CredentialId::new(inserted.id)
            .map_err(|_| internal_error(AdminChannelWriteRepositoryError::Invariant))?;
        if let Some(prepare_secret) = prepare_secret {
            let envelope = prepare_secret(credential_id).map_err(|()| {
                internal_error(AdminChannelWriteRepositoryError::SecretPreparation)
            })?;
            let encrypted = encrypted_json(envelope)?;
            let result = credentials::Entity::update_many()
                .filter(credentials::Column::Id.eq(credential_id.get()))
                .filter(credentials::Column::ChannelId.eq(channel_id.get()))
                .filter(credentials::Column::DeletedAt.is_null())
                .col_expr(credentials::Column::Secret, Expr::value(encrypted))
                .exec(&transaction)
                .with_subscriber(NoSubscriber::default())
                .await
                .map_err(map_db_error)?;
            require_rows(result.rows_affected, 1)?;
        }
        let model = fetch_credential_model(&transaction, channel_id, credential_id).await?;
        let snapshot = AdminCredentialRecord::try_from_model(model).map_err(map_read_error)?;
        enqueue_scheduler_catalog_change(
            &transaction,
            SchedulerCatalogSubject::Channel(channel_id),
            TimeDateTimeWithTimeZone::now_utc(),
        )
        .await
        .map_err(map_db_error)?;
        commit_transaction(transaction).await?;
        Ok(AdminCredentialCreateOutcome::Created(Box::new(snapshot)))
    }

    async fn update_credential_inner(
        &self,
        channel_id: ChannelId,
        credential_id: CredentialId,
        record: AdminCredentialWriteRecord,
        replacement_secret: Option<EncryptedCredentialEnvelope>,
    ) -> Result<AdminCredentialMutationOutcome, AdminChannelWriteRepositoryError> {
        let fields = validate_credential_fields(record)?;
        if fields.oauth_token_pending.is_some() {
            return Err(AdminChannelWriteRepositoryError::InvalidInput);
        }
        let replacement_secret = replacement_secret.map(encrypted_json).transpose()?;
        let has_replacement_secret = replacement_secret.is_some();
        let transaction = begin_transaction(self).await?;
        let Some(channel) = find_active_channel(&transaction, channel_id).await? else {
            return Ok(AdminCredentialMutationOutcome::ChannelNotFound);
        };
        validate_credential_channel_compatibility(&channel, fields.kind)?;
        let Some(current) = find_active_credential(&transaction, channel_id, credential_id).await?
        else {
            return Ok(AdminCredentialMutationOutcome::CredentialNotFound);
        };
        let current_kind = CredentialKind::from_str(&current.kind)
            .map_err(|_| internal_error(AdminChannelWriteRepositoryError::Invariant))?;
        let current_quota_dimension = CredentialQuotaDimension::from_str(&current.quota_dimension)
            .map_err(|_| internal_error(AdminChannelWriteRepositoryError::Invariant))?;
        if current_kind != fields.kind
            || current.parent_id != fields.parent_id.map(CredentialId::get)
            || current_quota_dimension != fields.quota_dimension
        {
            return Err(AdminChannelWriteRepositoryError::InvalidInput);
        }
        if current_quota_dimension == CredentialQuotaDimension::Spark {
            validate_spark_shadow_channel(&channel)?;
            if has_replacement_secret || !is_spark_shadow_reference(&current.secret)? {
                return Err(AdminChannelWriteRepositoryError::InvalidInput);
            }
            lock_and_validate_spark_parent(&transaction, channel_id, &fields, Some(credential_id))
                .await?;
        }
        let oauth_contract_changed = current_kind == CredentialKind::Oauth
            && (has_replacement_secret
                || current.oauth_provider.as_deref() != fields.oauth_provider.as_deref());
        if oauth_contract_changed
            && (current.oauth_revision < 0 || current.oauth_revision == i64::MAX)
        {
            return Err(internal_error(AdminChannelWriteRepositoryError::Invariant));
        }
        validate_proxy_binding(&transaction, fields.proxy_id).await?;

        let now = TimeDateTimeWithTimeZone::now_utc();
        let mut update = credentials::Entity::update_many()
            .filter(credentials::Column::Id.eq(credential_id.get()))
            .filter(credentials::Column::ChannelId.eq(channel_id.get()))
            .filter(credentials::Column::DeletedAt.is_null())
            .col_expr(
                credentials::Column::Status,
                Expr::value(fields.status.code()),
            )
            .col_expr(
                credentials::Column::MultiKeyMode,
                Expr::value(fields.multi_key_mode),
            )
            .col_expr(credentials::Column::Priority, Expr::value(fields.priority))
            .col_expr(credentials::Column::Weight, Expr::value(fields.weight))
            .col_expr(
                credentials::Column::Concurrency,
                Expr::value(fields.concurrency),
            )
            .col_expr(
                credentials::Column::LoadFactorMicros,
                Expr::value(fields.load_factor_micros),
            )
            .col_expr(
                credentials::Column::RateMultiplierMicros,
                Expr::value(fields.rate_multiplier_micros),
            )
            .col_expr(
                credentials::Column::Schedulable,
                Expr::value(fields.schedulable),
            )
            .col_expr(credentials::Column::ProxyId, Expr::value(fields.proxy_id))
            .col_expr(
                credentials::Column::OauthProvider,
                Expr::value(fields.oauth_provider),
            )
            .col_expr(
                credentials::Column::OauthAccountKey,
                Expr::value(fields.oauth_account_key),
            )
            .col_expr(
                credentials::Column::OauthProjectId,
                Expr::value(fields.oauth_project_id),
            )
            .col_expr(credentials::Column::UpdatedAt, Expr::value(now));
        if let Some(secret) = replacement_secret {
            update = update
                .col_expr(credentials::Column::Secret, Expr::value(secret))
                .col_expr(credentials::Column::OauthTokenPending, Expr::value(false));
        }
        if oauth_contract_changed {
            // 手工换 token 或切换 Provider 必须使并发后台刷新失效，并清除旧到期投影。
            update = update
                .filter(credentials::Column::OauthRevision.eq(current.oauth_revision))
                .filter(credentials::Column::Secret.eq(current.secret))
                .col_expr(
                    credentials::Column::OauthRevision,
                    Expr::col(credentials::Column::OauthRevision).add(1_i64),
                )
                .col_expr(
                    credentials::Column::OauthExpiresAtEpochSeconds,
                    Expr::value(Option::<i64>::None),
                );
        }
        let result = update
            .exec(&transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(map_db_error)?;
        require_rows(result.rows_affected, 1)?;
        let model = fetch_credential_model(&transaction, channel_id, credential_id).await?;
        let snapshot = AdminCredentialRecord::try_from_model(model).map_err(map_read_error)?;
        enqueue_scheduler_catalog_change(
            &transaction,
            SchedulerCatalogSubject::Channel(channel_id),
            now,
        )
        .await
        .map_err(map_db_error)?;
        commit_transaction(transaction).await?;
        Ok(AdminCredentialMutationOutcome::Mutated(Box::new(snapshot)))
    }

    async fn delete_credential_inner(
        &self,
        channel_id: ChannelId,
        credential_id: CredentialId,
    ) -> Result<AdminCredentialDeleteOutcome, AdminChannelWriteRepositoryError> {
        let transaction = begin_transaction(self).await?;
        if !active_channel_exists(&transaction, channel_id).await? {
            return Ok(AdminCredentialDeleteOutcome::ChannelNotFound);
        }
        let graph = load_active_credential_graph(&transaction, channel_id).await?;
        if !graph.contains_key(&credential_id.get()) {
            return Ok(AdminCredentialDeleteOutcome::CredentialNotFound);
        }
        validate_credential_graph(&graph)?;
        let affected_ids = collect_descendants(&graph, credential_id.get())?;
        let now = TimeDateTimeWithTimeZone::now_utc();
        let mut affected_rows = 0_u64;
        for batch in affected_ids.chunks(SOFT_DELETE_BATCH_SIZE) {
            let result = credentials::Entity::update_many()
                .filter(credentials::Column::ChannelId.eq(channel_id.get()))
                .filter(credentials::Column::Id.is_in(batch.iter().copied()))
                .filter(credentials::Column::DeletedAt.is_null())
                .col_expr(credentials::Column::DeletedAt, Expr::value(now))
                .col_expr(credentials::Column::UpdatedAt, Expr::value(now))
                .exec(&transaction)
                .with_subscriber(NoSubscriber::default())
                .await
                .map_err(map_db_error)?;
            affected_rows = affected_rows
                .checked_add(result.rows_affected)
                .ok_or_else(|| internal_error(AdminChannelWriteRepositoryError::Invariant))?;
        }
        require_rows(affected_rows, affected_ids.len() as u64)?;
        enqueue_scheduler_catalog_change(
            &transaction,
            SchedulerCatalogSubject::Channel(channel_id),
            now,
        )
        .await
        .map_err(map_db_error)?;
        commit_transaction(transaction).await?;
        Ok(AdminCredentialDeleteOutcome::Deleted)
    }
}

fn set_provider(
    settings: &mut serde_json::Value,
    provider: Option<String>,
) -> Result<(), AdminChannelWriteRepositoryError> {
    let object = settings
        .as_object_mut()
        .ok_or(AdminChannelWriteRepositoryError::InvalidInput)?;
    if let Some(provider) = provider {
        object.insert("provider".to_owned(), serde_json::Value::String(provider));
    }
    if object.get("provider").is_some_and(|value| {
        value
            .as_str()
            .is_none_or(|value| !valid_trimmed_text(value, 64))
    }) {
        return Err(AdminChannelWriteRepositoryError::InvalidInput);
    }
    validate_json_object(settings, false)
}

struct ValidatedChannelFields {
    provider: Option<String>,
    name: String,
    channel_type: ChannelType,
    protocol: Protocol,
    base_url: Option<ChannelBaseUrl>,
    timeout_secs: Option<i32>,
    status: Status,
    weight: i32,
    priority: i32,
    auto_ban: bool,
    auto_ban_rules: Option<ChannelAutoBanRules>,
    routing: ValidatedChannelRouting,
    model_mapping: serde_json::Value,
    param_override: serde_json::Value,
    header_override: Option<HeaderOverrides>,
    settings: Option<serde_json::Value>,
    pool_mode: Option<bool>,
    client_simulation_profile: Option<Option<ClientSimulationProfile>>,
    client_simulation_risk_accepted: bool,
    client_simulation_body_profile: Option<Option<ClientSimulationBodyProfile>>,
    client_simulation_body_risk_accepted: bool,
    responses_websocket_enabled: Option<bool>,
    responses_compact_mode: Option<ResponsesCompactMode>,
    responses_compact_model_mapping: Option<ChannelModelMappings>,
    tag: Option<String>,
}

fn validate_channel_fields(
    record: AdminChannelWriteRecord,
    require_sensitive_fields: bool,
) -> Result<ValidatedChannelFields, AdminChannelWriteRepositoryError> {
    let codex_oauth = record
        .provider
        .as_deref()
        .is_some_and(|provider| provider.eq_ignore_ascii_case("codex"));
    if !valid_trimmed_text(&record.name, 128)
        || record
            .provider
            .as_deref()
            .is_some_and(|value| !valid_trimmed_text(value, 64))
        || record.weight < 0
        || !matches!(record.status, Status::Enabled | Status::Disabled)
        || !matches!(
            (record.channel_type, record.protocol),
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
        || (codex_oauth
            && (record.channel_type != ChannelType::OpenAi
                || record.protocol != Protocol::OpenAiResponses
                || record.base_url.is_some()
                || record.responses_websocket_enabled == Some(true)))
        || record
            .tag
            .as_deref()
            .is_some_and(|value| !valid_trimmed_text(value, 64))
        || (require_sensitive_fields
            && (record.header_override.is_none() || record.settings.is_none()))
    {
        return Err(AdminChannelWriteRepositoryError::InvalidInput);
    }
    validate_json_object(&record.model_mapping, true)?;
    ChannelModelMappings::parse(&record.model_mapping)
        .map_err(|_| AdminChannelWriteRepositoryError::InvalidInput)?;
    validate_json_object(&record.param_override, false)?;
    let parameter_overrides = ChannelParameterOverrides::parse(&record.param_override)
        .map_err(|_| AdminChannelWriteRepositoryError::InvalidInput)?;
    parameter_overrides
        .validate_for_protocol(record.protocol)
        .map_err(|_| AdminChannelWriteRepositoryError::InvalidInput)?;
    if let Some(header_override) = &record.header_override {
        validate_json_object(header_override, false)?;
        validate_channel_header_overrides(header_override)
            .map_err(|_| AdminChannelWriteRepositoryError::InvalidInput)?;
    }
    if let Some(settings) = &record.settings {
        validate_json_object(settings, false)?;
    }
    if let Some(enabled) = record.responses_websocket_enabled {
        validate_responses_websocket_capability(record.channel_type, record.protocol, enabled)
            .map_err(|_| AdminChannelWriteRepositoryError::InvalidInput)?;
    }
    if let Some(profile) = record.client_simulation_profile {
        validate_client_simulation_capability(record.channel_type, record.protocol, profile)
            .map_err(|_| AdminChannelWriteRepositoryError::InvalidInput)?;
    }
    if let (Some(profile), Some(body_profile)) = (
        record.client_simulation_profile,
        record.client_simulation_body_profile,
    ) {
        validate_client_simulation_body_capability(
            record.channel_type,
            record.protocol,
            profile,
            body_profile,
        )
        .map_err(|_| AdminChannelWriteRepositoryError::InvalidInput)?;
    }
    if let Some(mode) = record.responses_compact_mode {
        validate_responses_compact_capability(record.channel_type, record.protocol, mode)
            .map_err(|_| AdminChannelWriteRepositoryError::InvalidInput)?;
    }
    let responses_compact_model_mapping = record
        .responses_compact_model_mapping
        .map(|mapping| {
            validate_json_object(&mapping, true)?;
            let mapping = ChannelModelMappings::parse(&mapping)
                .map_err(|_| AdminChannelWriteRepositoryError::InvalidInput)?;
            validate_responses_compact_model_mapping(
                record.channel_type,
                record.protocol,
                &mapping,
            )
            .map_err(|_| AdminChannelWriteRepositoryError::InvalidInput)?;
            Ok(mapping)
        })
        .transpose()?;
    let base_url = record
        .base_url
        .as_deref()
        .map(ChannelBaseUrl::parse)
        .transpose()
        .map_err(|_| AdminChannelWriteRepositoryError::InvalidInput)?;
    let timeout_secs = record
        .timeout
        .map(|timeout| {
            <i32 as std::convert::TryFrom<u64>>::try_from(timeout.seconds())
                .map_err(|_| AdminChannelWriteRepositoryError::InvalidInput)
        })
        .transpose()?;
    let header_override = record
        .header_override
        .map(HeaderOverrides::validate)
        .transpose()
        .map_err(|_| AdminChannelWriteRepositoryError::InvalidInput)?;
    let routing = ValidatedChannelRouting::new(record.models, record.group_ids)
        .map_err(map_ability_write_error)?;
    Ok(ValidatedChannelFields {
        name: record.name,
        channel_type: record.channel_type,
        protocol: record.protocol,
        base_url,
        timeout_secs,
        status: record.status,
        weight: record.weight,
        priority: record.priority,
        auto_ban: record.auto_ban,
        auto_ban_rules: record.auto_ban_rules,
        routing,
        model_mapping: record.model_mapping,
        param_override: record.param_override,
        header_override,
        settings: record.settings,
        provider: record.provider,
        pool_mode: record.pool_mode,
        client_simulation_profile: record.client_simulation_profile,
        client_simulation_risk_accepted: record.client_simulation_risk_accepted,
        client_simulation_body_profile: record.client_simulation_body_profile,
        client_simulation_body_risk_accepted: record.client_simulation_body_risk_accepted,
        responses_websocket_enabled: record.responses_websocket_enabled,
        responses_compact_mode: record.responses_compact_mode,
        responses_compact_model_mapping,
        tag: record.tag,
    })
}

struct ValidatedCredentialFields {
    kind: CredentialKind,
    status: Status,
    multi_key_mode: Option<i16>,
    priority: i32,
    weight: i32,
    concurrency: Option<i32>,
    load_factor_micros: Option<i64>,
    rate_multiplier_micros: Option<i64>,
    schedulable: bool,
    parent_id: Option<CredentialId>,
    quota_dimension: CredentialQuotaDimension,
    proxy_id: Option<i64>,
    oauth_provider: Option<String>,
    oauth_token_pending: Option<bool>,
    oauth_account_key: Option<String>,
    oauth_project_id: Option<String>,
}

#[derive(Clone, Copy)]
enum CredentialPlaceholder {
    TransactionPending,
    OauthPending,
    SparkShadow,
}

fn validate_credential_fields(
    record: AdminCredentialWriteRecord,
) -> Result<ValidatedCredentialFields, AdminChannelWriteRepositoryError> {
    let oauth_fields_present = record.oauth_provider.is_some()
        || record.oauth_account_key.is_some()
        || record.oauth_project_id.is_some();
    let oauth_token_pending = record.oauth_token_pending;
    let valid_shape = match record.quota_dimension {
        CredentialQuotaDimension::Global => record.parent_id.is_none(),
        CredentialQuotaDimension::Spark => {
            record.kind == CredentialKind::Oauth
                && record.parent_id.is_some()
                && record.concurrency.is_none()
                && record.proxy_id.is_none()
                && !oauth_fields_present
                && oauth_token_pending != Some(true)
        }
    };
    if !matches!(
        record.kind,
        CredentialKind::ApiKey
            | CredentialKind::Oauth
            | CredentialKind::Bedrock
            | CredentialKind::ServiceAccount
    ) || !matches!(record.status, Status::Enabled | Status::Disabled)
        || record
            .multi_key_mode
            .is_some_and(|value| !matches!(value, 1 | 2))
        || record.weight < 0
        || record.concurrency.is_some_and(|value| value < 0)
        || record.load_factor_micros.is_some_and(|value| value < 0)
        || record.rate_multiplier_micros.is_some_and(|value| value < 0)
        || record.proxy_id.is_some_and(|value| value <= 0)
        || !valid_optional_text(record.oauth_provider.as_deref(), 64)
        || !valid_optional_text(record.oauth_account_key.as_deref(), 255)
        || !valid_optional_text(record.oauth_project_id.as_deref(), 255)
        || (oauth_token_pending == Some(true)
            && (record.kind != CredentialKind::Oauth || record.oauth_provider.is_none()))
        || (record.kind != CredentialKind::Oauth && oauth_fields_present)
        || !valid_shape
    {
        return Err(AdminChannelWriteRepositoryError::InvalidInput);
    }
    Ok(ValidatedCredentialFields {
        kind: record.kind,
        status: record.status,
        multi_key_mode: record.multi_key_mode,
        priority: record.priority,
        weight: record.weight,
        concurrency: record.concurrency,
        load_factor_micros: record.load_factor_micros,
        rate_multiplier_micros: record.rate_multiplier_micros,
        schedulable: record.schedulable,
        parent_id: record.parent_id,
        quota_dimension: record.quota_dimension,
        proxy_id: record.proxy_id,
        oauth_provider: record.oauth_provider,
        oauth_token_pending,
        oauth_account_key: record.oauth_account_key,
        oauth_project_id: record.oauth_project_id,
    })
}

fn validate_credential_channel_compatibility(
    channel: &channels::Model,
    kind: CredentialKind,
) -> Result<(), AdminChannelWriteRepositoryError> {
    let channel_type = channel
        .r#type
        .parse::<ChannelType>()
        .map_err(|_| internal_error(AdminChannelWriteRepositoryError::Invariant))?;
    if matches!(
        channel_type,
        ChannelType::Jina | ChannelType::Cohere | ChannelType::Xai
    ) && kind != CredentialKind::ApiKey
    {
        return Err(AdminChannelWriteRepositoryError::InvalidInput);
    }
    let is_codex_oauth = channel
        .settings
        .clone()
        .into_inner()
        .get("provider")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|provider| provider.eq_ignore_ascii_case("codex"));
    if is_codex_oauth
        && (channel_type != ChannelType::OpenAi
            || channel.protocol.parse::<Protocol>().ok() != Some(Protocol::OpenAiResponses)
            || kind != CredentialKind::Oauth)
    {
        return Err(AdminChannelWriteRepositoryError::InvalidInput);
    }
    Ok(())
}

fn validate_spark_shadow_channel(
    channel: &channels::Model,
) -> Result<(), AdminChannelWriteRepositoryError> {
    let channel_type = channel
        .r#type
        .parse::<ChannelType>()
        .map_err(|_| internal_error(AdminChannelWriteRepositoryError::Invariant))?;
    let protocol = channel
        .protocol
        .parse::<Protocol>()
        .map_err(|_| internal_error(AdminChannelWriteRepositoryError::Invariant))?;
    if channel_type != ChannelType::OpenAi || protocol != Protocol::OpenAiResponses {
        return Err(AdminChannelWriteRepositoryError::InvalidInput);
    }
    Ok(())
}

async fn lock_and_validate_spark_parent(
    transaction: &DatabaseTransaction,
    channel_id: ChannelId,
    fields: &ValidatedCredentialFields,
    current_shadow_id: Option<CredentialId>,
) -> Result<(), AdminChannelWriteRepositoryError> {
    let parent_id = fields
        .parent_id
        .ok_or(AdminChannelWriteRepositoryError::InvalidReference)?;
    let Some(parent) = lock_active_credential(transaction, channel_id, parent_id).await? else {
        return Err(AdminChannelWriteRepositoryError::InvalidReference);
    };
    let parent_dimension = CredentialQuotaDimension::from_str(&parent.quota_dimension)
        .map_err(|_| internal_error(AdminChannelWriteRepositoryError::Invariant))?;
    if parent.parent_id.is_some()
        || parent.kind != CredentialKind::Oauth.as_str()
        || parent_dimension != CredentialQuotaDimension::Global
        || parent.oauth_token_pending
    {
        return Err(AdminChannelWriteRepositoryError::InvalidReference);
    }
    if active_other_child_exists(transaction, channel_id, parent_id, current_shadow_id).await? {
        return Err(AdminChannelWriteRepositoryError::InvalidReference);
    }
    Ok(())
}

async fn lock_active_credential(
    transaction: &DatabaseTransaction,
    channel_id: ChannelId,
    credential_id: CredentialId,
) -> Result<Option<credentials::Model>, AdminChannelWriteRepositoryError> {
    if transaction.get_database_backend() == DbBackend::Sqlite {
        // SQLite 没有 FOR UPDATE，恒等写入先固定母凭据与影子唯一性检查。
        credentials::Entity::update_many()
            .filter(credentials::Column::Id.eq(credential_id.get()))
            .filter(credentials::Column::ChannelId.eq(channel_id.get()))
            .filter(credentials::Column::DeletedAt.is_null())
            .col_expr(
                credentials::Column::Priority,
                Expr::col(credentials::Column::Priority).into(),
            )
            .exec(transaction)
            .with_subscriber(NoSubscriber::default())
            .await
            .map_err(map_db_error)?;
    }
    let mut query = credentials::Entity::find_by_id(credential_id.get())
        .filter(credentials::Column::ChannelId.eq(channel_id.get()))
        .filter(credentials::Column::DeletedAt.is_null());
    if transaction.get_database_backend() != DbBackend::Sqlite {
        query = query.lock(LockType::Update);
    }
    query
        .one(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(map_db_error)
}

/// 凭据只能绑定当前启用且未软删除的真实代理目录项。
async fn validate_proxy_binding(
    transaction: &DatabaseTransaction,
    proxy_id: Option<i64>,
) -> Result<(), AdminChannelWriteRepositoryError> {
    let Some(proxy_id) = proxy_id else {
        return Ok(());
    };
    let exists = proxies::Entity::find_by_id(proxy_id)
        .select_only()
        .column(proxies::Column::Id)
        .filter(proxies::Column::Enabled.eq(true))
        .filter(proxies::Column::DeletedAt.is_null())
        .limit(1)
        .into_tuple::<i64>()
        .one(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(map_db_error)?
        .is_some();
    if !exists {
        return Err(AdminChannelWriteRepositoryError::InvalidReference);
    }
    Ok(())
}

async fn active_other_child_exists(
    transaction: &DatabaseTransaction,
    channel_id: ChannelId,
    parent_id: CredentialId,
    current_shadow_id: Option<CredentialId>,
) -> Result<bool, AdminChannelWriteRepositoryError> {
    let mut query = credentials::Entity::find()
        .select_only()
        .column(credentials::Column::Id)
        .filter(credentials::Column::ChannelId.eq(channel_id.get()))
        .filter(credentials::Column::ParentId.eq(parent_id.get()))
        .filter(credentials::Column::DeletedAt.is_null());
    if let Some(current_shadow_id) = current_shadow_id {
        query = query.filter(credentials::Column::Id.ne(current_shadow_id.get()));
    }
    let result = query
        .limit(1)
        .into_tuple::<i64>()
        .one(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(map_db_error)?;
    Ok(result.is_some())
}

async fn active_channel_exists(
    transaction: &DatabaseTransaction,
    channel_id: ChannelId,
) -> Result<bool, AdminChannelWriteRepositoryError> {
    let result = channels::Entity::find()
        .select_only()
        .column(channels::Column::Id)
        .filter(channels::Column::Id.eq(channel_id.get()))
        .filter(channels::Column::DeletedAt.is_null())
        .into_tuple::<i64>()
        .one(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(map_db_error)?;
    Ok(result.is_some())
}

async fn find_active_channel(
    transaction: &DatabaseTransaction,
    channel_id: ChannelId,
) -> Result<Option<channels::Model>, AdminChannelWriteRepositoryError> {
    channels::Entity::find_by_id(channel_id.get())
        .filter(channels::Column::DeletedAt.is_null())
        .one(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(map_db_error)
}

async fn find_active_credential(
    transaction: &DatabaseTransaction,
    channel_id: ChannelId,
    credential_id: CredentialId,
) -> Result<Option<credentials::Model>, AdminChannelWriteRepositoryError> {
    credentials::Entity::find_by_id(credential_id.get())
        .filter(credentials::Column::ChannelId.eq(channel_id.get()))
        .filter(credentials::Column::DeletedAt.is_null())
        .one(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(map_db_error)
}

async fn fetch_channel_model(
    transaction: &DatabaseTransaction,
    channel_id: ChannelId,
) -> Result<channels::Model, AdminChannelWriteRepositoryError> {
    channels::Entity::find_by_id(channel_id.get())
        .filter(channels::Column::DeletedAt.is_null())
        .one(transaction)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(map_db_error)?
        .ok_or_else(|| internal_error(AdminChannelWriteRepositoryError::Invariant))
}

async fn fetch_credential_model(
    transaction: &DatabaseTransaction,
    channel_id: ChannelId,
    credential_id: CredentialId,
) -> Result<credentials::Model, AdminChannelWriteRepositoryError> {
    find_active_credential(transaction, channel_id, credential_id)
        .await?
        .ok_or_else(|| internal_error(AdminChannelWriteRepositoryError::Invariant))
}

async fn load_active_credential_graph(
    transaction: &DatabaseTransaction,
    channel_id: ChannelId,
) -> Result<HashMap<i64, Option<i64>>, AdminChannelWriteRepositoryError> {
    let query = Query::select()
        .expr_as(
            Expr::col((credentials::Entity, credentials::Column::Id)),
            Alias::new("credential_id"),
        )
        .expr_as(
            Expr::col((credentials::Entity, credentials::Column::ParentId)),
            Alias::new("parent_id"),
        )
        .from(credentials::Entity)
        .and_where(
            Expr::col((credentials::Entity, credentials::Column::ChannelId)).eq(channel_id.get()),
        )
        .and_where(Expr::col((credentials::Entity, credentials::Column::DeletedAt)).is_null())
        .limit((MAX_ADMIN_CREDENTIAL_GRAPH_SIZE + 1) as u64)
        .to_owned();
    let statement = transaction.get_database_backend().build(&query);
    let rows = transaction
        .query_all(statement)
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(map_db_error)?;
    if rows.len() > MAX_ADMIN_CREDENTIAL_GRAPH_SIZE {
        return Err(internal_error(AdminChannelWriteRepositoryError::Invariant));
    }
    let mut graph = HashMap::with_capacity(rows.len());
    for row in rows {
        let id = row
            .try_get::<i64>("", "credential_id")
            .map_err(|_| internal_error(AdminChannelWriteRepositoryError::Invariant))?;
        let parent_id = row
            .try_get::<Option<i64>>("", "parent_id")
            .map_err(|_| internal_error(AdminChannelWriteRepositoryError::Invariant))?;
        if id <= 0 || graph.insert(id, parent_id).is_some() {
            return Err(internal_error(AdminChannelWriteRepositoryError::Invariant));
        }
    }
    Ok(graph)
}

fn validate_credential_graph(
    graph: &HashMap<i64, Option<i64>>,
) -> Result<(), AdminChannelWriteRepositoryError> {
    for start in graph.keys().copied() {
        let mut path = HashSet::new();
        let mut current = start;
        while let Some(parent_id) = graph.get(&current).copied().flatten() {
            if !path.insert(current) || !graph.contains_key(&parent_id) {
                return Err(internal_error(AdminChannelWriteRepositoryError::Invariant));
            }
            current = parent_id;
        }
    }
    Ok(())
}

fn collect_descendants(
    graph: &HashMap<i64, Option<i64>>,
    root: i64,
) -> Result<Vec<i64>, AdminChannelWriteRepositoryError> {
    let mut children: HashMap<i64, Vec<i64>> = HashMap::new();
    for (&id, &parent_id) in graph {
        if let Some(parent_id) = parent_id {
            children.entry(parent_id).or_default().push(id);
        }
    }
    let mut selected = HashSet::new();
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        if !selected.insert(id) {
            return Err(internal_error(AdminChannelWriteRepositoryError::Invariant));
        }
        if let Some(child_ids) = children.get(&id) {
            stack.extend(child_ids.iter().copied());
        }
    }
    let mut selected = selected.into_iter().collect::<Vec<_>>();
    selected.sort_unstable();
    Ok(selected)
}

fn pending_encrypted_json() -> Result<EncryptedJson, AdminChannelWriteRepositoryError> {
    EncryptedJson::from_envelope(serde_json::json!({
        "version": 1,
        "algorithm": "xchacha20poly1305",
        "key_id": "transaction-pending",
        "nonce": URL_SAFE_NO_PAD.encode([0_u8; 24]),
        "ciphertext": URL_SAFE_NO_PAD.encode([0_u8; 16]),
    }))
    .map_err(|_| internal_error(AdminChannelWriteRepositoryError::Invariant))
}

/// 待授权凭据使用独立占位标识，避免被误认为可解密的真实 token。
fn oauth_token_pending_encrypted_json() -> Result<EncryptedJson, AdminChannelWriteRepositoryError> {
    EncryptedJson::from_envelope(serde_json::json!({
        "version": 1,
        "algorithm": "xchacha20poly1305",
        "key_id": "oauth-token-pending",
        "nonce": URL_SAFE_NO_PAD.encode([0_u8; 24]),
        "ciphertext": URL_SAFE_NO_PAD.encode([0_u8; 16]),
    }))
    .map_err(|_| internal_error(AdminChannelWriteRepositoryError::Invariant))
}

/// Spark 影子只保存不可解密标记，任何解密路径都会因未知 key ID 失败关闭。
fn spark_shadow_reference_encrypted_json() -> Result<EncryptedJson, AdminChannelWriteRepositoryError>
{
    EncryptedJson::from_envelope(serde_json::json!({
        "version": 1,
        "algorithm": "xchacha20poly1305",
        "key_id": SHADOW_REFERENCE_KEY_ID,
        "nonce": URL_SAFE_NO_PAD.encode([0_u8; 24]),
        "ciphertext": URL_SAFE_NO_PAD.encode([0_u8; 16]),
    }))
    .map_err(|_| internal_error(AdminChannelWriteRepositoryError::Invariant))
}

fn is_spark_shadow_reference(
    secret: &EncryptedJson,
) -> Result<bool, AdminChannelWriteRepositoryError> {
    secret
        .envelope_parts()
        .map(|(key_id, _, _)| key_id == SHADOW_REFERENCE_KEY_ID)
        .map_err(|_| internal_error(AdminChannelWriteRepositoryError::Invariant))
}

fn encrypted_json(
    envelope: EncryptedCredentialEnvelope,
) -> Result<EncryptedJson, AdminChannelWriteRepositoryError> {
    EncryptedJson::from_envelope(serde_json::json!({
        "version": 1,
        "algorithm": "xchacha20poly1305",
        "key_id": envelope.key_id(),
        "nonce": URL_SAFE_NO_PAD.encode(envelope.nonce()),
        "ciphertext": URL_SAFE_NO_PAD.encode(envelope.ciphertext()),
    }))
    .map_err(|_| internal_error(AdminChannelWriteRepositoryError::Invariant))
}

fn validate_json_object(
    value: &serde_json::Value,
    string_values_only: bool,
) -> Result<(), AdminChannelWriteRepositoryError> {
    let serde_json::Value::Object(object) = value else {
        return Err(AdminChannelWriteRepositoryError::InvalidInput);
    };
    if serde_json::to_vec(value)
        .ok()
        .is_none_or(|encoded| encoded.len() > MAX_ADMIN_CHANNEL_JSON_BYTES as usize)
        || (string_values_only
            && object.iter().any(|(key, value)| {
                !valid_trimmed_text(key, 256)
                    || value
                        .as_str()
                        .is_none_or(|value| !valid_trimmed_text(value, 256))
            }))
    {
        return Err(AdminChannelWriteRepositoryError::InvalidInput);
    }
    Ok(())
}

fn valid_optional_text(value: Option<&str>, maximum_bytes: usize) -> bool {
    value.is_none_or(|value| valid_trimmed_text(value, maximum_bytes))
}

fn valid_trimmed_text(value: &str, maximum_bytes: usize) -> bool {
    valid_text(value, maximum_bytes) && value.trim() == value
}

async fn begin_transaction(
    repository: &AdminChannelRepository,
) -> Result<DatabaseTransaction, AdminChannelWriteRepositoryError> {
    repository
        .pool
        .connection()
        .begin()
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(map_db_error)
}

async fn commit_transaction(
    transaction: DatabaseTransaction,
) -> Result<(), AdminChannelWriteRepositoryError> {
    transaction
        .commit()
        .with_subscriber(NoSubscriber::default())
        .await
        .map_err(map_db_error)
}

fn require_rows(actual: u64, expected: u64) -> Result<(), AdminChannelWriteRepositoryError> {
    if actual == expected {
        Ok(())
    } else {
        Err(internal_error(AdminChannelWriteRepositoryError::Invariant))
    }
}

fn map_read_error(error: AdminChannelRepositoryError) -> AdminChannelWriteRepositoryError {
    match error {
        AdminChannelRepositoryError::Timeout => AdminChannelWriteRepositoryError::Timeout,
        AdminChannelRepositoryError::Query | AdminChannelRepositoryError::Invariant => {
            internal_error(AdminChannelWriteRepositoryError::Invariant)
        }
    }
}

fn map_db_error(_error: sea_orm::DbErr) -> AdminChannelWriteRepositoryError {
    internal_error(AdminChannelWriteRepositoryError::Query)
}

fn map_ability_write_error(error: AbilityWriteError) -> AdminChannelWriteRepositoryError {
    match error {
        AbilityWriteError::InvalidInput | AbilityWriteError::CapacityExceeded => {
            AdminChannelWriteRepositoryError::InvalidInput
        }
        AbilityWriteError::InvalidReference => AdminChannelWriteRepositoryError::InvalidReference,
        AbilityWriteError::Query => internal_error(AdminChannelWriteRepositoryError::Query),
        AbilityWriteError::Invariant => internal_error(AdminChannelWriteRepositoryError::Invariant),
    }
}

fn internal_error(error: AdminChannelWriteRepositoryError) -> AdminChannelWriteRepositoryError {
    let error_kind = match error {
        AdminChannelWriteRepositoryError::InvalidInput => "admin_channel_write_invalid_input",
        AdminChannelWriteRepositoryError::InvalidReference => {
            "admin_channel_write_invalid_reference"
        }
        AdminChannelWriteRepositoryError::SecretPreparation => {
            "admin_channel_write_secret_preparation"
        }
        AdminChannelWriteRepositoryError::Query => "admin_channel_write_query",
        AdminChannelWriteRepositoryError::Timeout => "admin_channel_write_timeout",
        AdminChannelWriteRepositoryError::Invariant => "admin_channel_write_invariant",
    };
    tracing::error!(
        target: "af_db::admin_channel_write",
        error_kind,
        "管理渠道写仓储发生内部错误"
    );
    error
}
