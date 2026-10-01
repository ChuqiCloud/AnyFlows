use af_admin::{
    AdminMissingModelImportCommand, AdminMissingModelImportItemCommand, AdminModelSyncApplyCommand,
    AdminModelSyncApplyItemCommand, AdminModelSyncError, MissingModelPageRecord, MissingModelQuery,
    ModelSyncItemRecord, ModelSyncPreviewRecord, ModelSyncRelationRecord,
};
use af_domain::{ChannelId, ModelId};
use axum::{
    Json,
    extract::{Extension, Path, RawQuery, State, rejection::JsonRejection},
    response::Response,
};
use http::StatusCode;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    chat_completions::HttpState,
    management_error::ManagementError,
    management_model_writes::parse_modalities,
    management_models::{AdminModelModalityValue, AdminModelResponse, no_store_json},
};

const MAX_MODEL_SYNC_QUERY_BYTES: usize = 1_024;

/// 管理 API 使用的闭合同步候选关系。
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = AdminModelSyncRelation)]
pub(crate) enum AdminModelSyncRelationValue {
    MissingMetadata,
    DiscoveredUnconfigured,
    Existing,
    NotReported,
}

impl From<ModelSyncRelationRecord> for AdminModelSyncRelationValue {
    fn from(value: ModelSyncRelationRecord) -> Self {
        match value {
            ModelSyncRelationRecord::MissingMetadata => Self::MissingMetadata,
            ModelSyncRelationRecord::DiscoveredUnconfigured => Self::DiscoveredUnconfigured,
            ModelSyncRelationRecord::Existing => Self::Existing,
            ModelSyncRelationRecord::NotReported => Self::NotReported,
        }
    }
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminMissingModelChannel)]
pub(crate) struct AdminMissingModelChannelResponse {
    #[schema(minimum = 1)]
    channel_id: i64,
    #[schema(min_length = 1, max_length = 128)]
    channel_name: String,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminMissingModel)]
pub(crate) struct AdminMissingModelResponse {
    #[schema(min_length = 1, max_length = 256)]
    model: String,
    #[schema(minimum = 1)]
    channel_count: usize,
    #[schema(max_items = 8)]
    channels: Vec<AdminMissingModelChannelResponse>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminMissingModelListResponse)]
pub(crate) struct AdminMissingModelListResponse {
    #[schema(max_items = 100)]
    models: Vec<AdminMissingModelResponse>,
    #[schema(min_length = 1, max_length = 256, required = true)]
    next_cursor: Option<String>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminModelSyncPreviewRequest)]
pub(crate) struct AdminModelSyncPreviewRequest {
    #[schema(minimum = 1)]
    channel_id: i64,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminModelSyncPreviewItem)]
pub(crate) struct AdminModelSyncPreviewItemResponse {
    #[schema(minimum = 1)]
    item_id: i64,
    #[schema(min_length = 1, max_length = 256)]
    canonical_model: String,
    #[schema(min_length = 1, max_length = 256, required = true)]
    upstream_model: Option<String>,
    relation: AdminModelSyncRelationValue,
    #[schema(min_length = 1, max_length = 128, required = true)]
    display_name_hint: Option<String>,
    #[schema(min_length = 1, max_length = 4096, required = true)]
    description_hint: Option<String>,
    #[schema(minimum = 1, maximum = 2147483647, required = true)]
    context_window_hint: Option<i64>,
    #[schema(minimum = 1, maximum = 2147483647, required = true)]
    input_token_limit_hint: Option<i64>,
    #[schema(minimum = 1, maximum = 2147483647, required = true)]
    output_token_limit_hint: Option<i64>,
    #[schema(max_items = 32)]
    supported_methods: Vec<String>,
    #[schema(minimum = 1, required = true)]
    applied_model_id: Option<i64>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminModelSyncPreview)]
pub(crate) struct AdminModelSyncPreviewResponse {
    #[schema(min_length = 36, max_length = 36, format = "uuid")]
    preview_id: String,
    #[schema(minimum = 1)]
    channel_id: i64,
    #[serde(rename = "type")]
    #[schema(value_type = crate::openapi::schema::AdminChannelTypeSchema)]
    channel_type: af_domain::ChannelType,
    #[schema(value_type = crate::openapi::schema::AdminChannelProtocolSchema)]
    protocol: af_domain::Protocol,
    /// 预览失效时间，使用 Unix 秒时间戳。
    #[schema(minimum = 1)]
    expires_at: i64,
    #[schema(max_items = 1000)]
    items: Vec<AdminModelSyncPreviewItemResponse>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminModelSyncApplyItemRequest)]
pub(crate) struct AdminModelSyncApplyItemRequest {
    #[schema(minimum = 1)]
    item_id: i64,
    #[schema(min_length = 1, max_length = 128)]
    display_name: String,
    #[schema(min_length = 1, max_length = 64)]
    provider: String,
    #[schema(max_length = 4096)]
    description: Option<String>,
    #[schema(max_length = 2048)]
    icon_url: Option<String>,
    #[schema(max_items = 32)]
    tags: Vec<String>,
    #[schema(minimum = 1, maximum = 2147483647)]
    context_window: Option<i64>,
    #[schema(min_items = 1, max_items = 4)]
    input_modalities: Vec<AdminModelModalityValue>,
    #[schema(min_items = 1, max_items = 4)]
    output_modalities: Vec<AdminModelModalityValue>,
    supports_reasoning: bool,
    supports_tool_calls: bool,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminModelSyncApplyRequest)]
pub(crate) struct AdminModelSyncApplyRequest {
    #[schema(min_items = 1, max_items = 100)]
    items: Vec<AdminModelSyncApplyItemRequest>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminModelSyncApplyResponse)]
pub(crate) struct AdminModelSyncApplyResponse {
    #[schema(min_items = 1, max_items = 100)]
    models: Vec<AdminModelResponse>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminMissingModelImportItemRequest)]
pub(crate) struct AdminMissingModelImportItemRequest {
    #[schema(min_length = 1, max_length = 256)]
    model: String,
    #[schema(min_length = 1, max_length = 128)]
    display_name: String,
    #[schema(min_length = 1, max_length = 64)]
    provider: String,
    #[schema(max_length = 4096)]
    description: Option<String>,
    #[schema(max_length = 2048)]
    icon_url: Option<String>,
    #[schema(max_items = 32)]
    tags: Vec<String>,
    #[schema(minimum = 1, maximum = 2147483647)]
    context_window: Option<i64>,
    #[schema(min_items = 1, max_items = 4)]
    input_modalities: Vec<AdminModelModalityValue>,
    #[schema(min_items = 1, max_items = 4)]
    output_modalities: Vec<AdminModelModalityValue>,
    supports_reasoning: bool,
    supports_tool_calls: bool,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminMissingModelImportRequest)]
pub(crate) struct AdminMissingModelImportRequest {
    #[schema(min_items = 1, max_items = 100)]
    items: Vec<AdminMissingModelImportItemRequest>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminMissingModelImportResponse)]
pub(crate) struct AdminMissingModelImportResponse {
    #[schema(min_items = 1, max_items = 100)]
    models: Vec<AdminModelResponse>,
}

/// 按稳定 Canonical 游标列出活动渠道引用但缺少元数据的模型。
pub(crate) async fn list_missing_admin_models(
    State(state): State<HttpState>,
    RawQuery(raw_query): RawQuery,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let query = parse_missing_query(raw_query.as_deref())?;
    let service = state
        .admin_model_sync_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let page = service
        .list_missing(authentication.principal(), &query)
        .await
        .map_err(map_sync_error)?;
    Ok(no_store_json(AdminMissingModelListResponse::from_page(
        page,
    )))
}

/// 原子导入明确选择的缺失 Canonical，并固定创建为隐藏草稿。
pub(crate) async fn import_missing_admin_models(
    State(state): State<HttpState>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
    request: Result<Json<AdminMissingModelImportRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let request = request.map_err(|_| ManagementError::InvalidRequest)?.0;
    let command = AdminMissingModelImportCommand::new(
        request
            .items
            .into_iter()
            .map(AdminMissingModelImportItemRequest::into_command)
            .collect::<Result<Vec<_>, _>>()?,
    )
    .map_err(map_sync_error)?;
    let service = state
        .admin_model_sync_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let models = service
        .import_missing(authentication.principal(), command)
        .await
        .map_err(map_sync_error)?;
    Ok(status_json(
        StatusCode::CREATED,
        AdminMissingModelImportResponse {
            models: models.iter().map(AdminModelResponse::from_model).collect(),
        },
    ))
}

/// 对单个真实渠道执行受控上游枚举，并保存十五分钟固定预览。
pub(crate) async fn create_admin_model_sync_preview(
    State(state): State<HttpState>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
    request: Result<Json<AdminModelSyncPreviewRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let channel_id = request
        .map_err(|_| ManagementError::InvalidRequest)?
        .0
        .channel_id;
    let channel_id = ChannelId::new(channel_id).map_err(|_| ManagementError::InvalidRequest)?;
    let service = state
        .admin_model_sync_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let preview = service
        .preview(authentication.principal(), channel_id)
        .await
        .map_err(map_sync_error)?;
    Ok(status_json(
        StatusCode::CREATED,
        AdminModelSyncPreviewResponse::from_preview(preview),
    ))
}

/// 原子应用固定预览中的明确选择，并返回新建的隐藏草稿；适用候选同时加入来源渠道路由。
pub(crate) async fn apply_admin_model_sync_preview(
    State(state): State<HttpState>,
    Path(preview_id): Path<String>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
    request: Result<Json<AdminModelSyncApplyRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let request = request.map_err(|_| ManagementError::InvalidRequest)?.0;
    let command = AdminModelSyncApplyCommand::new(
        preview_id,
        request
            .items
            .into_iter()
            .map(AdminModelSyncApplyItemRequest::into_command)
            .collect::<Result<Vec<_>, _>>()?,
    )
    .map_err(map_sync_error)?;
    let service = state
        .admin_model_sync_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let models = service
        .apply(authentication.principal(), command)
        .await
        .map_err(map_sync_error)?;
    Ok(no_store_json(AdminModelSyncApplyResponse {
        models: models.iter().map(AdminModelResponse::from_model).collect(),
    }))
}

impl AdminMissingModelListResponse {
    fn from_page(page: MissingModelPageRecord) -> Self {
        Self {
            models: page
                .models()
                .iter()
                .map(|model| AdminMissingModelResponse {
                    model: model.model().to_owned(),
                    channel_count: model.channel_count(),
                    channels: model
                        .channels()
                        .iter()
                        .map(|channel| AdminMissingModelChannelResponse {
                            channel_id: channel.channel_id().get(),
                            channel_name: channel.channel_name().to_owned(),
                        })
                        .collect(),
                })
                .collect(),
            next_cursor: page.next_cursor().map(str::to_owned),
        }
    }
}

impl AdminModelSyncPreviewResponse {
    fn from_preview(preview: ModelSyncPreviewRecord) -> Self {
        Self {
            preview_id: preview.preview_id().to_owned(),
            channel_id: preview.channel_id().get(),
            channel_type: preview.channel_type(),
            protocol: preview.protocol(),
            expires_at: preview.expires_at(),
            items: preview
                .items()
                .iter()
                .map(AdminModelSyncPreviewItemResponse::from_item)
                .collect(),
        }
    }
}

impl AdminModelSyncPreviewItemResponse {
    fn from_item(item: &ModelSyncItemRecord) -> Self {
        Self {
            item_id: item.item_id(),
            canonical_model: item.canonical_model().to_owned(),
            upstream_model: item.upstream_model().map(str::to_owned),
            relation: item.relation().into(),
            display_name_hint: item.display_name_hint().map(str::to_owned),
            description_hint: item.description_hint().map(str::to_owned),
            context_window_hint: item.context_window_hint(),
            input_token_limit_hint: item.input_token_limit_hint(),
            output_token_limit_hint: item.output_token_limit_hint(),
            supported_methods: item.supported_methods().to_vec(),
            applied_model_id: item.applied_model_id().map(ModelId::get),
        }
    }
}

impl AdminModelSyncApplyItemRequest {
    fn into_command(self) -> Result<AdminModelSyncApplyItemCommand, ManagementError> {
        let input_modalities = parse_modalities(self.input_modalities)?;
        let output_modalities = parse_modalities(self.output_modalities)?;
        AdminModelSyncApplyItemCommand::new(
            self.item_id,
            self.display_name,
            self.provider,
            self.description,
            self.icon_url,
            self.tags,
            self.context_window,
            input_modalities,
            output_modalities,
            self.supports_reasoning,
            self.supports_tool_calls,
        )
        .map_err(map_sync_error)
    }
}

impl AdminMissingModelImportItemRequest {
    fn into_command(self) -> Result<AdminMissingModelImportItemCommand, ManagementError> {
        let input_modalities = parse_modalities(self.input_modalities)?;
        let output_modalities = parse_modalities(self.output_modalities)?;
        AdminMissingModelImportItemCommand::new(
            self.model,
            self.display_name,
            self.provider,
            self.description,
            self.icon_url,
            self.tags,
            self.context_window,
            input_modalities,
            output_modalities,
            self.supports_reasoning,
            self.supports_tool_calls,
        )
        .map_err(map_sync_error)
    }
}

fn parse_missing_query(raw_query: Option<&str>) -> Result<MissingModelQuery, ManagementError> {
    let Some(raw_query) = raw_query else {
        return Ok(MissingModelQuery::default());
    };
    if raw_query.is_empty() {
        return Ok(MissingModelQuery::default());
    }
    if raw_query.len() > MAX_MODEL_SYNC_QUERY_BYTES {
        return Err(ManagementError::InvalidRequest);
    }
    validate_percent_encoding(raw_query)?;
    let mut after = None;
    let mut limit = None;
    for (key, value) in url::form_urlencoded::parse(raw_query.as_bytes()) {
        if key.contains('\u{fffd}') || value.contains('\u{fffd}') {
            return Err(ManagementError::InvalidRequest);
        }
        match key.as_ref() {
            "after" if after.is_none() => after = Some(value.into_owned()),
            "limit" if limit.is_none() => limit = Some(parse_limit(&value)?),
            _ => return Err(ManagementError::InvalidRequest),
        }
    }
    MissingModelQuery::new(
        after,
        limit.unwrap_or(af_admin::DEFAULT_MISSING_MODEL_PAGE_SIZE),
    )
    .map_err(map_sync_error)
}

fn parse_limit(value: &str) -> Result<usize, ManagementError> {
    if value.is_empty()
        || value.starts_with(['+', '-'])
        || value.chars().any(|character| !character.is_ascii_digit())
    {
        return Err(ManagementError::InvalidRequest);
    }
    value
        .parse::<usize>()
        .map_err(|_| ManagementError::InvalidRequest)
}

fn validate_percent_encoding(raw_query: &str) -> Result<(), ManagementError> {
    let bytes = raw_query.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len()
                || !bytes[index + 1].is_ascii_hexdigit()
                || !bytes[index + 2].is_ascii_hexdigit()
            {
                return Err(ManagementError::InvalidRequest);
            }
            index += 3;
        } else {
            index += 1;
        }
    }
    Ok(())
}

fn map_sync_error(error: AdminModelSyncError) -> ManagementError {
    match error {
        AdminModelSyncError::InvalidInput => ManagementError::InvalidRequest,
        AdminModelSyncError::Forbidden => ManagementError::Forbidden,
        AdminModelSyncError::ChannelNotFound => ManagementError::ChannelNotFound,
        AdminModelSyncError::ChannelUnavailable => ManagementError::ModelSyncChannelUnavailable,
        AdminModelSyncError::UnsupportedChannel => ManagementError::ModelSyncUnsupportedChannel,
        AdminModelSyncError::UpstreamTimeout => ManagementError::ModelSyncUpstreamTimeout,
        AdminModelSyncError::UpstreamRejected => ManagementError::ModelSyncUpstreamRejected,
        AdminModelSyncError::InvalidResponse => ManagementError::ModelSyncInvalidResponse,
        AdminModelSyncError::CandidateLimitExceeded => {
            ManagementError::ModelSyncCandidateLimitExceeded
        }
        AdminModelSyncError::PreviewNotFound => ManagementError::ModelSyncPreviewNotFound,
        AdminModelSyncError::PreviewExpired => ManagementError::ModelSyncPreviewExpired,
        AdminModelSyncError::PreviewAlreadyApplied => {
            ManagementError::ModelSyncPreviewAlreadyApplied
        }
        AdminModelSyncError::Conflict => ManagementError::ModelSyncConflict,
        AdminModelSyncError::Internal => ManagementError::Internal,
    }
}

fn status_json(status: StatusCode, value: impl Serialize) -> Response {
    let mut response = no_store_json(value);
    *response.status_mut() = status;
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_query_rejects_duplicate_unknown_and_malformed_values() {
        let query = parse_missing_query(Some("after=gpt-4&limit=48")).unwrap();
        assert_eq!(query.after(), Some("gpt-4"));
        assert_eq!(query.limit(), 48);

        for raw_query in [
            "after=",
            "after=gpt&after=claude",
            "limit=0",
            "limit=101",
            "unknown=value",
            "after=%",
            "after=%ff",
        ] {
            assert_eq!(
                parse_missing_query(Some(raw_query)),
                Err(ManagementError::InvalidRequest),
                "{raw_query}"
            );
        }
    }

    #[test]
    fn apply_request_rejects_lifecycle_and_empty_modalities() {
        let raw = serde_json::json!({
            "items": [{
                "item_id": 1,
                "display_name": "Model",
                "provider": "provider",
                "description": null,
                "icon_url": null,
                "tags": [],
                "context_window": null,
                "input_modalities": ["text"],
                "output_modalities": ["text"],
                "supports_reasoning": false,
                "supports_tool_calls": false,
                "lifecycle": "active"
            }]
        });
        assert!(serde_json::from_value::<AdminModelSyncApplyRequest>(raw).is_err());

        let invalid = AdminModelSyncApplyItemRequest {
            item_id: 1,
            display_name: "Model".to_owned(),
            provider: "provider".to_owned(),
            description: None,
            icon_url: None,
            tags: Vec::new(),
            context_window: None,
            input_modalities: Vec::new(),
            output_modalities: vec![AdminModelModalityValue::Text],
            supports_reasoning: false,
            supports_tool_calls: false,
        };
        assert!(matches!(
            invalid.into_command(),
            Err(ManagementError::InvalidRequest)
        ));
    }

    #[test]
    fn missing_import_request_rejects_visibility_and_empty_modalities() {
        let raw = serde_json::json!({
            "items": [{
                "model": "model-a",
                "display_name": "Model A",
                "provider": "provider",
                "description": null,
                "icon_url": null,
                "tags": [],
                "context_window": null,
                "input_modalities": ["text"],
                "output_modalities": ["text"],
                "supports_reasoning": false,
                "supports_tool_calls": false,
                "visibility": "public"
            }]
        });
        assert!(serde_json::from_value::<AdminMissingModelImportRequest>(raw).is_err());

        let invalid = AdminMissingModelImportItemRequest {
            model: "model-a".to_owned(),
            display_name: "Model A".to_owned(),
            provider: "provider".to_owned(),
            description: None,
            icon_url: None,
            tags: Vec::new(),
            context_window: None,
            input_modalities: Vec::new(),
            output_modalities: vec![AdminModelModalityValue::Text],
            supports_reasoning: false,
            supports_tool_calls: false,
        };
        assert!(matches!(
            invalid.into_command(),
            Err(ManagementError::InvalidRequest)
        ));
    }

    #[test]
    fn sync_errors_keep_actionable_http_categories() {
        let cases = [
            (
                AdminModelSyncError::ChannelUnavailable,
                ManagementError::ModelSyncChannelUnavailable,
            ),
            (
                AdminModelSyncError::UnsupportedChannel,
                ManagementError::ModelSyncUnsupportedChannel,
            ),
            (
                AdminModelSyncError::UpstreamTimeout,
                ManagementError::ModelSyncUpstreamTimeout,
            ),
            (
                AdminModelSyncError::UpstreamRejected,
                ManagementError::ModelSyncUpstreamRejected,
            ),
            (
                AdminModelSyncError::InvalidResponse,
                ManagementError::ModelSyncInvalidResponse,
            ),
            (
                AdminModelSyncError::CandidateLimitExceeded,
                ManagementError::ModelSyncCandidateLimitExceeded,
            ),
            (
                AdminModelSyncError::PreviewExpired,
                ManagementError::ModelSyncPreviewExpired,
            ),
            (
                AdminModelSyncError::PreviewAlreadyApplied,
                ManagementError::ModelSyncPreviewAlreadyApplied,
            ),
        ];
        for (source, expected) in cases {
            assert_eq!(map_sync_error(source), expected);
        }
    }
}
