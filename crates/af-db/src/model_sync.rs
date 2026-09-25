use std::{fmt, time::Duration};

use af_domain::{
    ChannelId, ChannelTimeout, ChannelType, CredentialKind, ModelId, Protocol, UserId,
};
use thiserror::Error;

use crate::{
    AdminModelWriteRecord, DatabasePool, EncryptedCredentialEnvelope,
    MAX_ADMIN_MODEL_CONTEXT_WINDOW, MAX_ADMIN_MODEL_DESCRIPTION_BYTES,
    MAX_ADMIN_MODEL_DISPLAY_NAME_BYTES,
};

mod apply;
mod missing;
mod missing_import;
mod preview;
mod target;
mod write;

pub use preview::MODEL_SYNC_PREVIEW_TTL_SECONDS;

/// 单次上游模型预览允许保存的最大候选数。
pub const MAX_MODEL_SYNC_CANDIDATES: usize = 1_000;
/// 单次应用允许原子创建的最大草稿数。
pub const MAX_MODEL_SYNC_APPLY_ITEMS: usize = 100;
/// 缺失模型列表的单页硬上限。
pub const MAX_MISSING_MODEL_PAGE_SIZE: usize = 100;
/// 单个缺失模型响应最多展开的来源渠道数量。
pub const MAX_MISSING_MODEL_CHANNELS: usize = 8;
/// 单个候选最多保存的上游方法数量。
pub const MAX_MODEL_SYNC_METHODS: usize = 32;
/// 单个上游方法名允许占用的最大 UTF-8 字节数。
pub const MAX_MODEL_SYNC_METHOD_BYTES: usize = 128;
/// 同步预览对外 UUID 文本的固定长度。
pub const MODEL_SYNC_PREVIEW_ID_BYTES: usize = 36;

const DEFAULT_MODEL_SYNC_TIMEOUT: Duration = Duration::from_secs(5);

/// 缺失模型引用的脱敏渠道摘要。
#[derive(Clone, Eq, PartialEq)]
pub struct MissingModelChannelRecord {
    channel_id: ChannelId,
    channel_name: String,
}

impl MissingModelChannelRecord {
    #[must_use]
    pub const fn channel_id(&self) -> ChannelId {
        self.channel_id
    }

    #[must_use]
    pub fn channel_name(&self) -> &str {
        &self.channel_name
    }
}

impl fmt::Debug for MissingModelChannelRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MissingModelChannelRecord")
            .field("channel_id", &self.channel_id)
            .field("channel_name", &"<已脱敏>")
            .finish()
    }
}

/// 一条活动渠道已引用但缺少商品元数据的 Canonical 模型。
pub struct MissingModelRecord {
    model: String,
    channel_count: usize,
    channels: Vec<MissingModelChannelRecord>,
}

impl MissingModelRecord {
    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    #[must_use]
    pub const fn channel_count(&self) -> usize {
        self.channel_count
    }

    #[must_use]
    pub fn channels(&self) -> &[MissingModelChannelRecord] {
        &self.channels
    }
}

impl fmt::Debug for MissingModelRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MissingModelRecord")
            .field("channel_count", &self.channel_count)
            .field("expanded_channel_count", &self.channels.len())
            .finish_non_exhaustive()
    }
}

/// 按 Canonical 标识稳定排序的一页缺失模型。
pub struct MissingModelPageRecord {
    models: Vec<MissingModelRecord>,
    next_cursor: Option<String>,
}

impl MissingModelPageRecord {
    #[must_use]
    pub fn models(&self) -> &[MissingModelRecord] {
        &self.models
    }

    #[must_use]
    pub fn next_cursor(&self) -> Option<&str> {
        self.next_cursor.as_deref()
    }
}

impl fmt::Debug for MissingModelPageRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MissingModelPageRecord")
            .field("model_count", &self.models.len())
            .field("has_next_cursor", &self.next_cursor.is_some())
            .finish()
    }
}

/// 上游模型发现需要附加的非认证请求头。
#[derive(Clone, Eq, PartialEq)]
pub struct ModelDiscoveryHeaderRecord {
    name: String,
    value: String,
}

impl ModelDiscoveryHeaderRecord {
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 返回敏感头值，仅允许短暂用于构造上游请求。
    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }
}

impl fmt::Debug for ModelDiscoveryHeaderRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ModelDiscoveryHeaderRecord")
            .field("name", &self.name)
            .field("value", &"<已脱敏>")
            .finish()
    }
}

/// 渠道中一条 Canonical 到上游模型的精确映射。
#[derive(Clone, Eq, PartialEq)]
pub struct ModelDiscoveryMappingRecord {
    canonical_model: String,
    upstream_model: String,
}

impl ModelDiscoveryMappingRecord {
    #[must_use]
    pub fn canonical_model(&self) -> &str {
        &self.canonical_model
    }

    #[must_use]
    pub fn upstream_model(&self) -> &str {
        &self.upstream_model
    }
}

impl fmt::Debug for ModelDiscoveryMappingRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ModelDiscoveryMappingRecord(<已脱敏>)")
    }
}

/// 已从数据库读取并完成类型校验的上游模型发现目标。
#[derive(Clone, Eq, PartialEq)]
pub struct ModelDiscoveryTargetRecord {
    channel_id: ChannelId,
    channel_type: ChannelType,
    protocol: Protocol,
    base_url: Option<String>,
    timeout: Option<ChannelTimeout>,
    credential_id: i64,
    credential_kind: CredentialKind,
    oauth_provider: Option<String>,
    oauth_account_key: Option<String>,
    envelope: EncryptedCredentialEnvelope,
    headers: Vec<ModelDiscoveryHeaderRecord>,
    mappings: Vec<ModelDiscoveryMappingRecord>,
    proxy_required: bool,
}

impl ModelDiscoveryTargetRecord {
    #[must_use]
    pub const fn channel_id(&self) -> ChannelId {
        self.channel_id
    }

    #[must_use]
    pub const fn channel_type(&self) -> ChannelType {
        self.channel_type
    }

    #[must_use]
    pub const fn protocol(&self) -> Protocol {
        self.protocol
    }

    #[must_use]
    pub fn base_url(&self) -> Option<&str> {
        self.base_url.as_deref()
    }

    #[must_use]
    pub const fn timeout(&self) -> Option<ChannelTimeout> {
        self.timeout
    }

    #[must_use]
    pub const fn credential_id(&self) -> i64 {
        self.credential_id
    }

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

    #[must_use]
    pub const fn envelope(&self) -> &EncryptedCredentialEnvelope {
        &self.envelope
    }

    #[must_use]
    pub fn headers(&self) -> &[ModelDiscoveryHeaderRecord] {
        &self.headers
    }

    #[must_use]
    pub fn mappings(&self) -> &[ModelDiscoveryMappingRecord] {
        &self.mappings
    }

    #[must_use]
    pub const fn proxy_required(&self) -> bool {
        self.proxy_required
    }
}

impl fmt::Debug for ModelDiscoveryTargetRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ModelDiscoveryTargetRecord")
            .field("channel_id", &self.channel_id)
            .field("channel_type", &self.channel_type)
            .field("protocol", &self.protocol)
            .field("base_url", &self.base_url.as_ref().map(|_| "<已脱敏>"))
            .field("timeout", &self.timeout)
            .field("credential_id", &self.credential_id)
            .field("credential_kind", &self.credential_kind)
            .field("oauth_provider", &self.oauth_provider)
            .field(
                "oauth_account_key",
                &self.oauth_account_key.as_ref().map(|_| "<已脱敏>"),
            )
            .field("header_count", &self.headers.len())
            .field("mapping_count", &self.mappings.len())
            .field("proxy_required", &self.proxy_required)
            .finish()
    }
}

/// 渠道发现目标的有界读取结果。
#[allow(clippy::large_enum_variant)]
pub enum ModelDiscoveryTargetLookup {
    /// 渠道不存在或已经软删除。
    NotFound,
    /// 渠道存在，但没有可用于发现的有效凭据。
    Unavailable,
    /// 发现目标已经完成持久化校验。
    Found(ModelDiscoveryTargetRecord),
}

impl fmt::Debug for ModelDiscoveryTargetLookup {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => formatter.write_str("ModelDiscoveryTargetLookup::NotFound"),
            Self::Unavailable => formatter.write_str("ModelDiscoveryTargetLookup::Unavailable"),
            Self::Found(_) => formatter.write_str("ModelDiscoveryTargetLookup::Found(<已脱敏>)"),
        }
    }
}

/// 同步预览中候选与当前配置和元数据的关系。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModelSyncRelationRecord {
    MissingMetadata,
    DiscoveredUnconfigured,
    Existing,
    NotReported,
}

impl ModelSyncRelationRecord {
    pub(super) const fn database_value(self) -> i16 {
        match self {
            Self::MissingMetadata => 1,
            Self::DiscoveredUnconfigured => 2,
            Self::Existing => 3,
            Self::NotReported => 4,
        }
    }

    pub(super) const fn try_from_database(value: i16) -> Result<Self, ModelSyncRepositoryError> {
        match value {
            1 => Ok(Self::MissingMetadata),
            2 => Ok(Self::DiscoveredUnconfigured),
            3 => Ok(Self::Existing),
            4 => Ok(Self::NotReported),
            _ => Err(ModelSyncRepositoryError::Invariant),
        }
    }

    #[must_use]
    pub const fn is_applicable(self) -> bool {
        matches!(self, Self::MissingMetadata | Self::DiscoveredUnconfigured)
    }
}

/// 上游明确返回、但尚未被管理员确认为权威元数据的候选证据。
pub struct DiscoveredModelRecord {
    canonical_model: String,
    upstream_model: String,
    display_name_hint: Option<String>,
    description_hint: Option<String>,
    context_window_hint: Option<i64>,
    input_token_limit_hint: Option<i64>,
    output_token_limit_hint: Option<i64>,
    supported_methods: Vec<String>,
}

impl DiscoveredModelRecord {
    #[allow(
        clippy::too_many_arguments,
        reason = "字段与三种上游的公共证据投影一一对应"
    )]
    pub fn new(
        canonical_model: String,
        upstream_model: String,
        display_name_hint: Option<String>,
        description_hint: Option<String>,
        context_window_hint: Option<i64>,
        input_token_limit_hint: Option<i64>,
        output_token_limit_hint: Option<i64>,
        supported_methods: Vec<String>,
    ) -> Result<Self, ModelSyncRepositoryError> {
        let record = Self {
            canonical_model,
            upstream_model,
            display_name_hint,
            description_hint,
            context_window_hint,
            input_token_limit_hint,
            output_token_limit_hint,
            supported_methods,
        };
        if !record.is_valid() {
            return Err(ModelSyncRepositoryError::Invariant);
        }
        Ok(record)
    }

    pub(super) fn is_valid(&self) -> bool {
        valid_model(&self.canonical_model)
            && valid_model(&self.upstream_model)
            && valid_optional_text(
                self.display_name_hint.as_deref(),
                MAX_ADMIN_MODEL_DISPLAY_NAME_BYTES,
            )
            && valid_optional_text(
                self.description_hint.as_deref(),
                MAX_ADMIN_MODEL_DESCRIPTION_BYTES,
            )
            && [
                self.context_window_hint,
                self.input_token_limit_hint,
                self.output_token_limit_hint,
            ]
            .into_iter()
            .all(|value| {
                value.is_none_or(|value| (1..=MAX_ADMIN_MODEL_CONTEXT_WINDOW).contains(&value))
            })
            && valid_methods(&self.supported_methods)
    }
}

impl fmt::Debug for DiscoveredModelRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DiscoveredModelRecord")
            .field("has_display_name_hint", &self.display_name_hint.is_some())
            .field("has_description_hint", &self.description_hint.is_some())
            .field(
                "has_context_window_hint",
                &self.context_window_hint.is_some(),
            )
            .field("supported_method_count", &self.supported_methods.len())
            .finish_non_exhaustive()
    }
}

/// 已保存的固定同步预览条目。
pub struct ModelSyncItemRecord {
    item_id: i64,
    canonical_model: String,
    upstream_model: Option<String>,
    relation: ModelSyncRelationRecord,
    display_name_hint: Option<String>,
    description_hint: Option<String>,
    context_window_hint: Option<i64>,
    input_token_limit_hint: Option<i64>,
    output_token_limit_hint: Option<i64>,
    supported_methods: Vec<String>,
    applied_model_id: Option<ModelId>,
}

impl ModelSyncItemRecord {
    #[must_use]
    pub const fn item_id(&self) -> i64 {
        self.item_id
    }

    #[must_use]
    pub fn canonical_model(&self) -> &str {
        &self.canonical_model
    }

    #[must_use]
    pub fn upstream_model(&self) -> Option<&str> {
        self.upstream_model.as_deref()
    }

    #[must_use]
    pub const fn relation(&self) -> ModelSyncRelationRecord {
        self.relation
    }

    #[must_use]
    pub fn display_name_hint(&self) -> Option<&str> {
        self.display_name_hint.as_deref()
    }

    #[must_use]
    pub fn description_hint(&self) -> Option<&str> {
        self.description_hint.as_deref()
    }

    #[must_use]
    pub const fn context_window_hint(&self) -> Option<i64> {
        self.context_window_hint
    }

    #[must_use]
    pub const fn input_token_limit_hint(&self) -> Option<i64> {
        self.input_token_limit_hint
    }

    #[must_use]
    pub const fn output_token_limit_hint(&self) -> Option<i64> {
        self.output_token_limit_hint
    }

    #[must_use]
    pub fn supported_methods(&self) -> &[String] {
        &self.supported_methods
    }

    #[must_use]
    pub const fn applied_model_id(&self) -> Option<ModelId> {
        self.applied_model_id
    }
}

impl fmt::Debug for ModelSyncItemRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ModelSyncItemRecord")
            .field("item_id", &self.item_id)
            .field("relation", &self.relation)
            .field("supported_method_count", &self.supported_methods.len())
            .field("applied_model_id", &self.applied_model_id)
            .finish_non_exhaustive()
    }
}

/// 一次已经持久化的同步预览快照。
pub struct ModelSyncPreviewRecord {
    preview_id: String,
    channel_id: ChannelId,
    channel_type: ChannelType,
    protocol: Protocol,
    expires_at: i64,
    items: Vec<ModelSyncItemRecord>,
}

impl ModelSyncPreviewRecord {
    #[must_use]
    pub fn preview_id(&self) -> &str {
        &self.preview_id
    }

    #[must_use]
    pub const fn channel_id(&self) -> ChannelId {
        self.channel_id
    }

    #[must_use]
    pub const fn channel_type(&self) -> ChannelType {
        self.channel_type
    }

    #[must_use]
    pub const fn protocol(&self) -> Protocol {
        self.protocol
    }

    #[must_use]
    pub const fn expires_at(&self) -> i64 {
        self.expires_at
    }

    #[must_use]
    pub fn items(&self) -> &[ModelSyncItemRecord] {
        &self.items
    }
}

impl fmt::Debug for ModelSyncPreviewRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ModelSyncPreviewRecord")
            .field("channel_id", &self.channel_id)
            .field("channel_type", &self.channel_type)
            .field("protocol", &self.protocol)
            .field("expires_at", &self.expires_at)
            .field("item_count", &self.items.len())
            .finish_non_exhaustive()
    }
}

/// 创建同步预览所需的固定来源和发现结果。
pub struct ModelSyncPreviewWrite {
    preview_id: String,
    actor_user_id: UserId,
    channel_id: ChannelId,
    channel_type: ChannelType,
    protocol: Protocol,
    expires_at: i64,
    discovered_models: Vec<DiscoveredModelRecord>,
}

impl ModelSyncPreviewWrite {
    #[allow(
        clippy::too_many_arguments,
        reason = "字段与审计运行及候选输入一一对应"
    )]
    pub fn new(
        preview_id: String,
        actor_user_id: UserId,
        channel_id: ChannelId,
        channel_type: ChannelType,
        protocol: Protocol,
        expires_at: i64,
        discovered_models: Vec<DiscoveredModelRecord>,
    ) -> Result<Self, ModelSyncRepositoryError> {
        if !valid_preview_id(&preview_id)
            || expires_at <= 0
            || discovered_models.len() > MAX_MODEL_SYNC_CANDIDATES
        {
            return Err(ModelSyncRepositoryError::Invariant);
        }
        Ok(Self {
            preview_id,
            actor_user_id,
            channel_id,
            channel_type,
            protocol,
            expires_at,
            discovered_models,
        })
    }
}

impl fmt::Debug for ModelSyncPreviewWrite {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ModelSyncPreviewWrite")
            .field("actor_user_id", &self.actor_user_id)
            .field("channel_id", &self.channel_id)
            .field("channel_type", &self.channel_type)
            .field("protocol", &self.protocol)
            .field("expires_at", &self.expires_at)
            .field("discovered_model_count", &self.discovered_models.len())
            .finish_non_exhaustive()
    }
}

/// 管理员为一个预览候选补齐的隐藏草稿字段。
pub struct ModelSyncApplyItemRecord {
    item_id: i64,
    fields: AdminModelWriteRecord,
}

/// 管理员从缺失模型清单中选择的 Canonical 与隐藏草稿字段。
pub struct MissingModelImportItemRecord {
    model: String,
    fields: AdminModelWriteRecord,
}

impl MissingModelImportItemRecord {
    /// 组装已经由应用层完成字段校验的缺失模型导入项。
    pub fn new(
        model: String,
        fields: AdminModelWriteRecord,
    ) -> Result<Self, ModelSyncRepositoryError> {
        if !valid_model(&model) {
            return Err(ModelSyncRepositoryError::Invariant);
        }
        Ok(Self { model, fields })
    }
}

impl fmt::Debug for MissingModelImportItemRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MissingModelImportItemRecord")
            .field("model", &self.model)
            .field("fields", &"<已脱敏>")
            .finish()
    }
}

impl ModelSyncApplyItemRecord {
    pub fn new(
        item_id: i64,
        fields: AdminModelWriteRecord,
    ) -> Result<Self, ModelSyncRepositoryError> {
        if item_id <= 0 {
            return Err(ModelSyncRepositoryError::Invariant);
        }
        Ok(Self { item_id, fields })
    }
}

impl fmt::Debug for ModelSyncApplyItemRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ModelSyncApplyItemRecord")
            .field("item_id", &self.item_id)
            .field("fields", &"<已脱敏>")
            .finish()
    }
}

/// 模型同步仓储构造错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ModelSyncRepositoryConfigError {
    #[error("模型同步仓储超时必须大于零")]
    ZeroTimeout,
}

/// 模型同步持久化失败分类，不携带 URL、凭据、模型或上游正文。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ModelSyncRepositoryError {
    #[error("模型同步仓储查询失败")]
    Query,
    #[error("模型同步仓储操作超时")]
    Timeout,
    #[error("模型同步持久化状态损坏")]
    Invariant,
    #[error("模型同步预览不存在")]
    NotFound,
    #[error("模型同步预览已过期")]
    Expired,
    #[error("模型同步预览已应用")]
    AlreadyApplied,
    #[error("模型同步应用发生并发冲突")]
    Conflict,
}

/// 缺失检测、发现目标、预览审计与原子应用共用的数据库仓储。
#[derive(Clone)]
pub struct ModelSyncRepository {
    pub(super) pool: DatabasePool,
    pub(super) operation_timeout: Duration,
}

impl ModelSyncRepository {
    #[must_use]
    pub fn new(pool: DatabasePool) -> Self {
        Self {
            pool,
            operation_timeout: DEFAULT_MODEL_SYNC_TIMEOUT,
        }
    }

    pub fn with_timeout(
        pool: DatabasePool,
        operation_timeout: Duration,
    ) -> Result<Self, ModelSyncRepositoryConfigError> {
        if operation_timeout.is_zero() {
            return Err(ModelSyncRepositoryConfigError::ZeroTimeout);
        }
        Ok(Self {
            pool,
            operation_timeout,
        })
    }
}

impl fmt::Debug for ModelSyncRepository {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ModelSyncRepository")
            .field("operation_timeout", &self.operation_timeout)
            .finish_non_exhaustive()
    }
}

pub(super) fn valid_model(value: &str) -> bool {
    crate::model_price::is_valid_model_name(value)
}

pub(super) fn valid_text(value: &str, maximum_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum_bytes
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

pub(super) fn valid_optional_text(value: Option<&str>, maximum_bytes: usize) -> bool {
    value.is_none_or(|value| valid_text(value, maximum_bytes))
}

pub(super) fn valid_methods(methods: &[String]) -> bool {
    methods.len() <= MAX_MODEL_SYNC_METHODS
        && methods
            .iter()
            .all(|method| valid_text(method, MAX_MODEL_SYNC_METHOD_BYTES))
        && methods.windows(2).all(|pair| pair[0] < pair[1])
}

pub(super) fn valid_preview_id(value: &str) -> bool {
    value.len() == MODEL_SYNC_PREVIEW_ID_BYTES
        && value.bytes().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()
            }
        })
}

pub(super) fn record_internal_error(error: ModelSyncRepositoryError) -> ModelSyncRepositoryError {
    let error_kind = match error {
        ModelSyncRepositoryError::Query => "model_sync_query",
        ModelSyncRepositoryError::Timeout => "model_sync_timeout",
        ModelSyncRepositoryError::Invariant => "model_sync_invariant",
        ModelSyncRepositoryError::NotFound
        | ModelSyncRepositoryError::Expired
        | ModelSyncRepositoryError::AlreadyApplied
        | ModelSyncRepositoryError::Conflict => return error,
    };
    tracing::error!(
        target: "af_db::model_sync",
        error_kind,
        "模型同步仓储发生内部错误"
    );
    error
}
