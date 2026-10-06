use af_admin::{
    AdminModel, AdminModelLifecycle, AdminModelListQuery, AdminModelModalities, AdminModelPage,
    AdminModelReadError, AdminModelVisibility,
};
use af_domain::ModelId;
use axum::{
    Json,
    extract::{Extension, Path, RawQuery, State},
    response::{IntoResponse, Response},
};
use http::{HeaderValue, header::CACHE_CONTROL};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{chat_completions::HttpState, management_error::ManagementError};

/// 管理 API 使用的闭合模型可见范围。
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = AdminModelVisibility)]
pub(crate) enum AdminModelVisibilityValue {
    Public,
    Authenticated,
    Hidden,
}

impl From<AdminModelVisibility> for AdminModelVisibilityValue {
    fn from(value: AdminModelVisibility) -> Self {
        match value {
            AdminModelVisibility::Public => Self::Public,
            AdminModelVisibility::Authenticated => Self::Authenticated,
            AdminModelVisibility::Hidden => Self::Hidden,
        }
    }
}

impl From<AdminModelVisibilityValue> for AdminModelVisibility {
    fn from(value: AdminModelVisibilityValue) -> Self {
        match value {
            AdminModelVisibilityValue::Public => Self::Public,
            AdminModelVisibilityValue::Authenticated => Self::Authenticated,
            AdminModelVisibilityValue::Hidden => Self::Hidden,
        }
    }
}

/// 管理 API 使用的闭合模型生命周期。
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = AdminModelLifecycle)]
pub(crate) enum AdminModelLifecycleValue {
    Draft,
    Active,
    Deprecated,
    Retired,
}

impl From<AdminModelLifecycle> for AdminModelLifecycleValue {
    fn from(value: AdminModelLifecycle) -> Self {
        match value {
            AdminModelLifecycle::Draft => Self::Draft,
            AdminModelLifecycle::Active => Self::Active,
            AdminModelLifecycle::Deprecated => Self::Deprecated,
            AdminModelLifecycle::Retired => Self::Retired,
        }
    }
}

impl From<AdminModelLifecycleValue> for AdminModelLifecycle {
    fn from(value: AdminModelLifecycleValue) -> Self {
        match value {
            AdminModelLifecycleValue::Draft => Self::Draft,
            AdminModelLifecycleValue::Active => Self::Active,
            AdminModelLifecycleValue::Deprecated => Self::Deprecated,
            AdminModelLifecycleValue::Retired => Self::Retired,
        }
    }
}

/// 管理 API 使用的闭合模型模态。
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = AdminModelModality)]
pub(crate) enum AdminModelModalityValue {
    Text,
    Image,
    Audio,
    Video,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminModel)]
pub(crate) struct AdminModelResponse {
    #[schema(minimum = 1)]
    id: i64,
    #[schema(min_length = 1, max_length = 256)]
    model: String,
    #[schema(min_length = 1, max_length = 128)]
    display_name: String,
    #[schema(min_length = 1, max_length = 64)]
    provider: String,
    #[schema(max_length = 4096, required = true)]
    description: Option<String>,
    #[schema(max_length = 2048, required = true)]
    icon_url: Option<String>,
    #[schema(max_items = 32)]
    tags: Vec<String>,
    #[schema(minimum = 1, maximum = 2147483647, required = true)]
    context_window: Option<i64>,
    #[schema(min_items = 1, max_items = 4)]
    input_modalities: Vec<AdminModelModalityValue>,
    #[schema(min_items = 1, max_items = 4)]
    output_modalities: Vec<AdminModelModalityValue>,
    supports_reasoning: bool,
    supports_tool_calls: bool,
    visibility: AdminModelVisibilityValue,
    lifecycle: AdminModelLifecycleValue,
    created_at: i64,
    updated_at: i64,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminModelListResponse)]
pub(crate) struct AdminModelListResponse {
    #[schema(max_items = 100)]
    models: Vec<AdminModelResponse>,
    #[schema(minimum = 1, required = true)]
    next_cursor: Option<i64>,
}

/// 返回管理员可见的模型商品元数据列表。
pub(crate) async fn list_admin_models(
    State(state): State<HttpState>,
    RawQuery(raw_query): RawQuery,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let reader = state
        .admin_model_reader
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let page = reader
        .list(
            authentication.principal(),
            parse_list_query(raw_query.as_deref())?,
        )
        .await
        .map_err(map_read_error)?;
    Ok(no_store_json(AdminModelListResponse::from_page(page)))
}

/// 返回单个未软删除模型商品元数据快照。
pub(crate) async fn get_admin_model(
    State(state): State<HttpState>,
    Path(model_id): Path<String>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let reader = state
        .admin_model_reader
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let model = reader
        .get(authentication.principal(), parse_model_id(&model_id)?)
        .await
        .map_err(map_read_error)?;
    Ok(no_store_json(AdminModelResponse::from_model(&model)))
}

impl AdminModelListResponse {
    fn from_page(page: AdminModelPage) -> Self {
        Self {
            models: page
                .models()
                .iter()
                .map(AdminModelResponse::from_model)
                .collect(),
            next_cursor: page.next_cursor().map(ModelId::get),
        }
    }
}

impl AdminModelResponse {
    pub(crate) fn from_model(model: &AdminModel) -> Self {
        Self {
            id: model.model_id().get(),
            model: model.model().to_owned(),
            display_name: model.display_name().to_owned(),
            provider: model.provider().to_owned(),
            description: model.description().map(str::to_owned),
            icon_url: model.icon_url().map(str::to_owned),
            tags: model.tags().to_vec(),
            context_window: model.context_window(),
            input_modalities: modality_values(model.input_modalities()),
            output_modalities: modality_values(model.output_modalities()),
            supports_reasoning: model.supports_reasoning(),
            supports_tool_calls: model.supports_tool_calls(),
            visibility: model.visibility().into(),
            lifecycle: model.lifecycle().into(),
            created_at: model.created_at(),
            updated_at: model.updated_at(),
        }
    }
}

pub(crate) fn modality_values(modalities: AdminModelModalities) -> Vec<AdminModelModalityValue> {
    [
        (modalities.text(), AdminModelModalityValue::Text),
        (modalities.image(), AdminModelModalityValue::Image),
        (modalities.audio(), AdminModelModalityValue::Audio),
        (modalities.video(), AdminModelModalityValue::Video),
    ]
    .into_iter()
    .filter_map(|(enabled, value)| enabled.then_some(value))
    .collect()
}

fn parse_list_query(raw_query: Option<&str>) -> Result<AdminModelListQuery, ManagementError> {
    let Some(raw_query) = raw_query else {
        return Ok(AdminModelListQuery::default());
    };
    if raw_query.is_empty() {
        return Ok(AdminModelListQuery::default());
    }
    validate_percent_encoding(raw_query)?;
    let mut after = None;
    let mut limit = None;
    for (key, value) in url::form_urlencoded::parse(raw_query.as_bytes()) {
        if key.contains('\u{fffd}') || value.contains('\u{fffd}') {
            return Err(ManagementError::InvalidRequest);
        }
        match key.as_ref() {
            "after" if after.is_none() => after = Some(parse_model_id(&value)?),
            "limit" if limit.is_none() => limit = Some(parse_limit(&value)?),
            _ => return Err(ManagementError::InvalidRequest),
        }
    }
    AdminModelListQuery::new(
        after,
        limit.unwrap_or(af_admin::DEFAULT_ADMIN_MODEL_PAGE_SIZE),
    )
    .map_err(map_read_error)
}

pub(crate) fn parse_model_id(value: &str) -> Result<ModelId, ManagementError> {
    if value.is_empty()
        || value.starts_with(['+', '-'])
        || value.chars().any(|character| !character.is_ascii_digit())
    {
        return Err(ManagementError::InvalidRequest);
    }
    ModelId::new(
        value
            .parse::<i64>()
            .map_err(|_| ManagementError::InvalidRequest)?,
    )
    .map_err(|_| ManagementError::InvalidRequest)
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

fn map_read_error(error: AdminModelReadError) -> ManagementError {
    match error {
        AdminModelReadError::InvalidPagination => ManagementError::InvalidRequest,
        AdminModelReadError::Forbidden => ManagementError::Forbidden,
        AdminModelReadError::NotFound => ManagementError::ModelNotFound,
        AdminModelReadError::Internal => ManagementError::Internal,
    }
}

pub(crate) fn no_store_json(value: impl Serialize) -> Response {
    let mut response = Json(value).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_query_rejects_duplicate_unknown_and_malformed_values() {
        assert_eq!(parse_list_query(None).unwrap().limit(), 50);
        for query in [
            "after=1&after=2",
            "limit=1&limit=2",
            "unknown=1",
            "after=-1",
            "limit=0",
            "after=%zz",
        ] {
            assert!(parse_list_query(Some(query)).is_err(), "{query}");
        }
    }
}
