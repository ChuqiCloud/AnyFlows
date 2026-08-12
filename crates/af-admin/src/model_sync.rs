use std::{
    collections::BTreeSet,
    fmt,
    future::Future,
    pin::Pin,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use af_db::{
    DiscoveredModelRecord, MODEL_SYNC_PREVIEW_TTL_SECONDS, MissingModelImportItemRecord,
    ModelSyncApplyItemRecord, ModelSyncPreviewWrite, ModelSyncRepository, ModelSyncRepositoryError,
};
use af_domain::{ChannelId, ChannelType, Protocol};
use thiserror::Error;

use crate::{
    AdminModel, AdminModelLifecycle, AdminModelModalities, AdminModelUpdateCommand,
    AdminModelVisibility, SessionPrincipal, SessionRole,
};

pub use af_db::{
    MAX_MISSING_MODEL_PAGE_SIZE, MAX_MODEL_SYNC_APPLY_ITEMS, MissingModelChannelRecord,
    MissingModelPageRecord, MissingModelRecord, ModelSyncItemRecord, ModelSyncPreviewRecord,
    ModelSyncRelationRecord,
};

/// 缺失模型列表默认页大小。
pub const DEFAULT_MISSING_MODEL_PAGE_SIZE: usize = 50;

/// 已校验的缺失模型稳定游标查询。
#[derive(Clone, Eq, PartialEq)]
pub struct MissingModelQuery {
    after: Option<String>,
    limit: usize,
}

impl MissingModelQuery {
    pub fn new(after: Option<String>, limit: usize) -> Result<Self, AdminModelSyncError> {
        if !(1..=MAX_MISSING_MODEL_PAGE_SIZE).contains(&limit)
            || after.as_deref().is_some_and(|value| !valid_model(value))
        {
            return Err(AdminModelSyncError::InvalidInput);
        }
        Ok(Self { after, limit })
    }

    #[must_use]
    pub fn after(&self) -> Option<&str> {
        self.after.as_deref()
    }

    #[must_use]
    pub const fn limit(&self) -> usize {
        self.limit
    }
}

impl Default for MissingModelQuery {
    fn default() -> Self {
        Self {
            after: None,
            limit: DEFAULT_MISSING_MODEL_PAGE_SIZE,
        }
    }
}

impl fmt::Debug for MissingModelQuery {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MissingModelQuery")
            .field("has_after", &self.after.is_some())
            .field("limit", &self.limit)
            .finish()
    }
}

/// 受控上游发现返回的来源协议与脱敏候选证据。
pub struct UpstreamModelDiscovery {
    channel_id: ChannelId,
    channel_type: ChannelType,
    protocol: Protocol,
    models: Vec<DiscoveredModelRecord>,
}

impl UpstreamModelDiscovery {
    #[must_use]
    pub fn new(
        channel_id: ChannelId,
        channel_type: ChannelType,
        protocol: Protocol,
        models: Vec<DiscoveredModelRecord>,
    ) -> Self {
        Self {
            channel_id,
            channel_type,
            protocol,
            models,
        }
    }
}

impl fmt::Debug for UpstreamModelDiscovery {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("UpstreamModelDiscovery")
            .field("channel_id", &self.channel_id)
            .field("channel_type", &self.channel_type)
            .field("protocol", &self.protocol)
            .field("model_count", &self.models.len())
            .finish()
    }
}

/// 上游模型发现失败分类，不携带 URL、凭据或响应正文。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum UpstreamModelDiscoveryError {
    #[error("模型发现渠道不存在")]
    ChannelNotFound,
    #[error("模型发现渠道或凭据不可用")]
    ChannelUnavailable,
    #[error("渠道类型或协议不支持模型发现")]
    UnsupportedChannel,
    #[error("上游模型发现请求超时")]
    Timeout,
    #[error("上游拒绝模型发现请求")]
    UpstreamRejected,
    #[error("上游模型发现响应无效")]
    InvalidResponse,
    #[error("上游模型发现候选数量超限")]
    CandidateLimitExceeded,
    #[error("上游模型发现内部失败")]
    Internal,
}

pub type UpstreamModelDiscoveryFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<UpstreamModelDiscovery, UpstreamModelDiscoveryError>>
            + Send
            + 'a,
    >,
>;

/// 只负责受控网络枚举的上游模型发现端口。
pub trait UpstreamModelDiscoverer: Send + Sync {
    fn discover<'a>(&'a self, channel_id: ChannelId) -> UpstreamModelDiscoveryFuture<'a>;
}

/// 管理员为一个预览候选补齐的结构化权威元数据。
pub struct AdminModelSyncApplyItemCommand {
    item_id: i64,
    metadata: AdminModelUpdateCommand,
}

impl AdminModelSyncApplyItemCommand {
    #[allow(
        clippy::too_many_arguments,
        reason = "字段与模型元数据结构化确认表单一一对应"
    )]
    pub fn new(
        item_id: i64,
        display_name: String,
        provider: String,
        description: Option<String>,
        icon_url: Option<String>,
        tags: Vec<String>,
        context_window: Option<i64>,
        input_modalities: AdminModelModalities,
        output_modalities: AdminModelModalities,
        supports_reasoning: bool,
        supports_tool_calls: bool,
    ) -> Result<Self, AdminModelSyncError> {
        if item_id <= 0 {
            return Err(AdminModelSyncError::InvalidInput);
        }
        let metadata = AdminModelUpdateCommand::new(
            display_name,
            provider,
            description,
            icon_url,
            tags,
            context_window,
            input_modalities,
            output_modalities,
            supports_reasoning,
            supports_tool_calls,
            AdminModelVisibility::Hidden,
            AdminModelLifecycle::Draft,
        )
        .map_err(|_| AdminModelSyncError::InvalidInput)?;
        Ok(Self { item_id, metadata })
    }

    fn into_record(self) -> Result<ModelSyncApplyItemRecord, AdminModelSyncError> {
        ModelSyncApplyItemRecord::new(self.item_id, self.metadata.into_record())
            .map_err(|_| AdminModelSyncError::InvalidInput)
    }
}

impl fmt::Debug for AdminModelSyncApplyItemCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AdminModelSyncApplyItemCommand")
            .field("item_id", &self.item_id)
            .field("metadata", &"<已脱敏>")
            .finish()
    }
}

/// 一次同步应用的固定预览标识和明确选择。
pub struct AdminModelSyncApplyCommand {
    preview_id: String,
    items: Vec<AdminModelSyncApplyItemCommand>,
}

impl AdminModelSyncApplyCommand {
    pub fn new(
        preview_id: String,
        items: Vec<AdminModelSyncApplyItemCommand>,
    ) -> Result<Self, AdminModelSyncError> {
        if !valid_preview_id(&preview_id)
            || items.is_empty()
            || items.len() > MAX_MODEL_SYNC_APPLY_ITEMS
        {
            return Err(AdminModelSyncError::InvalidInput);
        }
        Ok(Self { preview_id, items })
    }
}

impl fmt::Debug for AdminModelSyncApplyCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AdminModelSyncApplyCommand")
            .field("item_count", &self.items.len())
            .finish_non_exhaustive()
    }
}

/// 管理员为一个渠道已引用但缺少元数据的 Canonical 补齐结构化字段。
pub struct AdminMissingModelImportItemCommand {
    model: String,
    metadata: AdminModelUpdateCommand,
}

impl AdminMissingModelImportItemCommand {
    #[allow(
        clippy::too_many_arguments,
        reason = "字段与缺失模型快速导入表单一一对应"
    )]
    pub fn new(
        model: String,
        display_name: String,
        provider: String,
        description: Option<String>,
        icon_url: Option<String>,
        tags: Vec<String>,
        context_window: Option<i64>,
        input_modalities: AdminModelModalities,
        output_modalities: AdminModelModalities,
        supports_reasoning: bool,
        supports_tool_calls: bool,
    ) -> Result<Self, AdminModelSyncError> {
        if !valid_model(&model) {
            return Err(AdminModelSyncError::InvalidInput);
        }
        let metadata = AdminModelUpdateCommand::new(
            display_name,
            provider,
            description,
            icon_url,
            tags,
            context_window,
            input_modalities,
            output_modalities,
            supports_reasoning,
            supports_tool_calls,
            AdminModelVisibility::Hidden,
            AdminModelLifecycle::Draft,
        )
        .map_err(|_| AdminModelSyncError::InvalidInput)?;
        Ok(Self { model, metadata })
    }

    fn into_record(self) -> Result<MissingModelImportItemRecord, AdminModelSyncError> {
        MissingModelImportItemRecord::new(self.model, self.metadata.into_record())
            .map_err(|_| AdminModelSyncError::InvalidInput)
    }
}

impl fmt::Debug for AdminMissingModelImportItemCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AdminMissingModelImportItemCommand")
            .field("model", &self.model)
            .field("metadata", &"<已脱敏>")
            .finish()
    }
}

/// 一次缺失模型快速导入的明确选择，拒绝重复 Canonical 和超大批次。
pub struct AdminMissingModelImportCommand {
    items: Vec<AdminMissingModelImportItemCommand>,
}

impl AdminMissingModelImportCommand {
    pub fn new(
        items: Vec<AdminMissingModelImportItemCommand>,
    ) -> Result<Self, AdminModelSyncError> {
        if items.is_empty()
            || items.len() > MAX_MODEL_SYNC_APPLY_ITEMS
            || items
                .iter()
                .map(|item| item.model.as_str())
                .collect::<BTreeSet<_>>()
                .len()
                != items.len()
        {
            return Err(AdminModelSyncError::InvalidInput);
        }
        Ok(Self { items })
    }
}

impl fmt::Debug for AdminMissingModelImportCommand {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AdminMissingModelImportCommand")
            .field("item_count", &self.items.len())
            .finish()
    }
}

/// 模型同步管理用例的稳定失败分类。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AdminModelSyncError {
    #[error("模型同步请求参数无效")]
    InvalidInput,
    #[error("模型同步权限不足")]
    Forbidden,
    #[error("模型同步渠道不存在")]
    ChannelNotFound,
    #[error("模型同步渠道不可用")]
    ChannelUnavailable,
    #[error("模型同步不支持当前渠道")]
    UnsupportedChannel,
    #[error("模型同步上游请求超时")]
    UpstreamTimeout,
    #[error("模型同步上游拒绝枚举请求")]
    UpstreamRejected,
    #[error("模型同步上游响应无效")]
    InvalidResponse,
    #[error("模型同步候选数量超限")]
    CandidateLimitExceeded,
    #[error("模型同步预览不存在")]
    PreviewNotFound,
    #[error("模型同步预览已过期")]
    PreviewExpired,
    #[error("模型同步预览已应用")]
    PreviewAlreadyApplied,
    #[error("模型同步发生并发冲突")]
    Conflict,
    #[error("模型同步内部失败")]
    Internal,
}

pub type MissingModelListFuture<'a> =
    Pin<Box<dyn Future<Output = Result<MissingModelPageRecord, AdminModelSyncError>> + Send + 'a>>;
pub type ModelSyncPreviewFuture<'a> =
    Pin<Box<dyn Future<Output = Result<ModelSyncPreviewRecord, AdminModelSyncError>> + Send + 'a>>;
pub type ModelSyncApplyFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Vec<AdminModel>, AdminModelSyncError>> + Send + 'a>>;
pub type MissingModelImportFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Vec<AdminModel>, AdminModelSyncError>> + Send + 'a>>;

/// 管理员缺失检测、真实预览与原子应用的组合端口。
pub trait AdminModelSyncService: Send + Sync {
    fn list_missing<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: &'a MissingModelQuery,
    ) -> MissingModelListFuture<'a>;

    fn preview<'a>(
        &'a self,
        principal: SessionPrincipal,
        channel_id: ChannelId,
    ) -> ModelSyncPreviewFuture<'a>;

    fn apply<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: AdminModelSyncApplyCommand,
    ) -> ModelSyncApplyFuture<'a>;

    fn import_missing<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: AdminMissingModelImportCommand,
    ) -> MissingModelImportFuture<'a>;
}

/// 使用数据库审计仓储和受控网络端口实现模型同步管理用例。
pub struct DatabaseAdminModelSyncService {
    repository: ModelSyncRepository,
    discoverer: Arc<dyn UpstreamModelDiscoverer>,
}

impl DatabaseAdminModelSyncService {
    #[must_use]
    pub fn new(
        repository: ModelSyncRepository,
        discoverer: Arc<dyn UpstreamModelDiscoverer>,
    ) -> Self {
        Self {
            repository,
            discoverer,
        }
    }
}

impl AdminModelSyncService for DatabaseAdminModelSyncService {
    fn list_missing<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: &'a MissingModelQuery,
    ) -> MissingModelListFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            self.repository
                .list_missing_models(query.after(), query.limit())
                .await
                .map_err(map_repository_error)
        })
    }

    fn preview<'a>(
        &'a self,
        principal: SessionPrincipal,
        channel_id: ChannelId,
    ) -> ModelSyncPreviewFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            let discovery = self
                .discoverer
                .discover(channel_id)
                .await
                .map_err(map_discovery_error)?;
            if discovery.channel_id != channel_id {
                return Err(AdminModelSyncError::Internal);
            }
            let now = current_unix().ok_or(AdminModelSyncError::Internal)?;
            let expires_at = now
                .checked_add(MODEL_SYNC_PREVIEW_TTL_SECONDS)
                .ok_or(AdminModelSyncError::Internal)?;
            let write = ModelSyncPreviewWrite::new(
                generate_preview_id().ok_or(AdminModelSyncError::Internal)?,
                principal.user_id(),
                discovery.channel_id,
                discovery.channel_type,
                discovery.protocol,
                expires_at,
                discovery.models,
            )
            .map_err(|_| AdminModelSyncError::Internal)?;
            self.repository
                .create_preview(write)
                .await
                .map_err(map_repository_error)
        })
    }

    fn apply<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: AdminModelSyncApplyCommand,
    ) -> ModelSyncApplyFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            let now = current_unix().ok_or(AdminModelSyncError::Internal)?;
            let items = command
                .items
                .into_iter()
                .map(AdminModelSyncApplyItemCommand::into_record)
                .collect::<Result<Vec<_>, _>>()?;
            self.repository
                .apply_preview(&command.preview_id, principal.user_id(), now, items)
                .await
                .map_err(map_repository_error)?
                .into_iter()
                .map(|record| Ok(AdminModel::from_record(record)))
                .collect()
        })
    }

    fn import_missing<'a>(
        &'a self,
        principal: SessionPrincipal,
        command: AdminMissingModelImportCommand,
    ) -> MissingModelImportFuture<'a> {
        Box::pin(async move {
            require_admin(principal)?;
            let items = command
                .items
                .into_iter()
                .map(AdminMissingModelImportItemCommand::into_record)
                .collect::<Result<Vec<_>, _>>()?;
            self.repository
                .import_missing_models(items)
                .await
                .map_err(map_repository_error)?
                .into_iter()
                .map(|record| Ok(AdminModel::from_record(record)))
                .collect()
        })
    }
}

impl fmt::Debug for DatabaseAdminModelSyncService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseAdminModelSyncService(<已脱敏>)")
    }
}

fn require_admin(principal: SessionPrincipal) -> Result<(), AdminModelSyncError> {
    if principal.role() == SessionRole::Admin {
        Ok(())
    } else {
        Err(AdminModelSyncError::Forbidden)
    }
}

fn map_discovery_error(error: UpstreamModelDiscoveryError) -> AdminModelSyncError {
    match error {
        UpstreamModelDiscoveryError::ChannelNotFound => AdminModelSyncError::ChannelNotFound,
        UpstreamModelDiscoveryError::ChannelUnavailable => AdminModelSyncError::ChannelUnavailable,
        UpstreamModelDiscoveryError::UnsupportedChannel => AdminModelSyncError::UnsupportedChannel,
        UpstreamModelDiscoveryError::Timeout => AdminModelSyncError::UpstreamTimeout,
        UpstreamModelDiscoveryError::UpstreamRejected => AdminModelSyncError::UpstreamRejected,
        UpstreamModelDiscoveryError::InvalidResponse => AdminModelSyncError::InvalidResponse,
        UpstreamModelDiscoveryError::CandidateLimitExceeded => {
            AdminModelSyncError::CandidateLimitExceeded
        }
        UpstreamModelDiscoveryError::Internal => AdminModelSyncError::Internal,
    }
}

fn map_repository_error(error: ModelSyncRepositoryError) -> AdminModelSyncError {
    match error {
        ModelSyncRepositoryError::NotFound => AdminModelSyncError::PreviewNotFound,
        ModelSyncRepositoryError::Expired => AdminModelSyncError::PreviewExpired,
        ModelSyncRepositoryError::AlreadyApplied => AdminModelSyncError::PreviewAlreadyApplied,
        ModelSyncRepositoryError::Conflict => AdminModelSyncError::Conflict,
        ModelSyncRepositoryError::Query
        | ModelSyncRepositoryError::Timeout
        | ModelSyncRepositoryError::Invariant => AdminModelSyncError::Internal,
    }
}

fn current_unix() -> Option<i64> {
    let seconds = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
    i64::try_from(seconds).ok()
}

fn generate_preview_id() -> Option<String> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).ok()?;
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Some(format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15]
    ))
}

fn valid_preview_id(value: &str) -> bool {
    value.len() == af_db::MODEL_SYNC_PREVIEW_ID_BYTES
        && value.bytes().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()
            }
        })
}

fn valid_model(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= af_domain::MAX_MODEL_NAME_BYTES
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use af_domain::UserId;

    use super::*;

    #[test]
    fn preview_ids_are_v4_shaped_and_commands_force_draft_visibility() {
        let preview_id = generate_preview_id().expect("测试环境必须提供安全随机源");
        assert!(valid_preview_id(&preview_id));
        assert_eq!(&preview_id[14..15], "4");
        assert!(matches!(&preview_id[19..20], "8" | "9" | "a" | "b"));

        let item = AdminModelSyncApplyItemCommand::new(
            1,
            "Model".to_owned(),
            "provider".to_owned(),
            None,
            None,
            Vec::new(),
            None,
            AdminModelModalities::new(true, false, false, false),
            AdminModelModalities::new(true, false, false, false),
            false,
            false,
        )
        .unwrap();
        assert!(!format!("{item:?}").contains("provider"));

        let missing = AdminMissingModelImportItemCommand::new(
            "model-a".to_owned(),
            "Model A".to_owned(),
            "provider".to_owned(),
            None,
            None,
            Vec::new(),
            None,
            AdminModelModalities::new(true, false, false, false),
            AdminModelModalities::new(true, false, false, false),
            false,
            false,
        )
        .unwrap();
        assert!(!format!("{missing:?}").contains("provider"));
    }

    #[test]
    fn missing_import_rejects_duplicate_canonical_models() {
        let item = || {
            AdminMissingModelImportItemCommand::new(
                "model-a".to_owned(),
                "Model A".to_owned(),
                "provider".to_owned(),
                None,
                None,
                Vec::new(),
                None,
                AdminModelModalities::new(true, false, false, false),
                AdminModelModalities::new(true, false, false, false),
                false,
                false,
            )
            .unwrap()
        };
        assert_eq!(
            AdminMissingModelImportCommand::new(vec![item(), item()]).unwrap_err(),
            AdminModelSyncError::InvalidInput
        );
    }

    #[test]
    fn normal_users_are_rejected_before_io() {
        let principal = SessionPrincipal::new(UserId::new(1).unwrap(), SessionRole::User);
        assert_eq!(
            require_admin(principal),
            Err(AdminModelSyncError::Forbidden)
        );
    }

    #[test]
    fn discovery_failures_keep_distinct_operator_actions() {
        assert_eq!(
            map_discovery_error(UpstreamModelDiscoveryError::UpstreamRejected),
            AdminModelSyncError::UpstreamRejected
        );
        assert_eq!(
            map_discovery_error(UpstreamModelDiscoveryError::InvalidResponse),
            AdminModelSyncError::InvalidResponse
        );
        assert_eq!(
            map_discovery_error(UpstreamModelDiscoveryError::CandidateLimitExceeded),
            AdminModelSyncError::CandidateLimitExceeded
        );
    }
}
