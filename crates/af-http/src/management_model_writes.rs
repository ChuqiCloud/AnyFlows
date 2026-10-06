use std::collections::HashSet;

use af_admin::{
    AdminModelCreateCommand, AdminModelLifecycle, AdminModelModalities, AdminModelUpdateCommand,
    AdminModelVisibility, AdminModelWriteError,
};
use axum::{
    Json,
    extract::{Extension, Path, State, rejection::JsonRejection},
    response::{IntoResponse, Response},
};
use http::{HeaderValue, StatusCode, header::CACHE_CONTROL};
use serde::Deserialize;
use utoipa::ToSchema;

use crate::{
    chat_completions::HttpState,
    management_error::ManagementError,
    management_models::{
        AdminModelLifecycleValue, AdminModelModalityValue, AdminModelResponse,
        AdminModelVisibilityValue, no_store_json, parse_model_id,
    },
};

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminModelCreateRequest)]
/// 管理端创建模型商品元数据时使用的正文。
pub(crate) struct AdminModelCreateRequest {
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
    visibility: AdminModelVisibilityValue,
    lifecycle: AdminModelLifecycleValue,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminModelUpdateRequest)]
/// 管理端更新模型商品元数据时使用的正文；Canonical 标识不可修改。
pub(crate) struct AdminModelUpdateRequest {
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
    visibility: AdminModelVisibilityValue,
    lifecycle: AdminModelLifecycleValue,
}

/// 创建模型商品元数据，并返回完整管理快照。
pub(crate) async fn create_admin_model(
    State(state): State<HttpState>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
    request: Result<Json<AdminModelCreateRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let writer = state
        .admin_model_writer
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let model = writer
        .create(
            authentication.principal(),
            request
                .map_err(|_| ManagementError::InvalidRequest)?
                .0
                .into_command()?,
        )
        .await
        .map_err(map_write_error)?;
    Ok(status_json(
        StatusCode::CREATED,
        AdminModelResponse::from_model(&model),
    ))
}

/// 完整更新模型商品的可变元数据字段。
pub(crate) async fn update_admin_model(
    State(state): State<HttpState>,
    Path(model_id): Path<String>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
    request: Result<Json<AdminModelUpdateRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let writer = state
        .admin_model_writer
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let model = writer
        .update(
            authentication.principal(),
            parse_model_id(&model_id)?,
            request
                .map_err(|_| ManagementError::InvalidRequest)?
                .0
                .into_command()?,
        )
        .await
        .map_err(map_write_error)?;
    Ok(no_store_json(AdminModelResponse::from_model(&model)))
}

/// 软删除模型商品元数据，不影响价格、渠道能力和上游映射。
pub(crate) async fn delete_admin_model(
    State(state): State<HttpState>,
    Path(model_id): Path<String>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let writer = state
        .admin_model_writer
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    writer
        .delete(authentication.principal(), parse_model_id(&model_id)?)
        .await
        .map_err(map_write_error)?;
    Ok(no_store_empty(StatusCode::NO_CONTENT))
}

impl AdminModelCreateRequest {
    fn into_command(self) -> Result<AdminModelCreateCommand, ManagementError> {
        let input_modalities = parse_modalities(self.input_modalities)?;
        let output_modalities = parse_modalities(self.output_modalities)?;
        AdminModelCreateCommand::new(
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
            map_visibility(self.visibility),
            map_lifecycle(self.lifecycle),
        )
        .map_err(map_write_error)
    }
}

impl AdminModelUpdateRequest {
    fn into_command(self) -> Result<AdminModelUpdateCommand, ManagementError> {
        let input_modalities = parse_modalities(self.input_modalities)?;
        let output_modalities = parse_modalities(self.output_modalities)?;
        AdminModelUpdateCommand::new(
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
            map_visibility(self.visibility),
            map_lifecycle(self.lifecycle),
        )
        .map_err(map_write_error)
    }
}

pub(crate) fn parse_modalities(
    values: Vec<AdminModelModalityValue>,
) -> Result<AdminModelModalities, ManagementError> {
    if values.is_empty() || values.len() > 4 {
        return Err(ManagementError::InvalidRequest);
    }
    let mut unique = HashSet::with_capacity(values.len());
    if values.iter().any(|value| !unique.insert(*value)) {
        return Err(ManagementError::InvalidRequest);
    }
    Ok(AdminModelModalities::new(
        unique.contains(&AdminModelModalityValue::Text),
        unique.contains(&AdminModelModalityValue::Image),
        unique.contains(&AdminModelModalityValue::Audio),
        unique.contains(&AdminModelModalityValue::Video),
    ))
}

const fn map_visibility(value: AdminModelVisibilityValue) -> AdminModelVisibility {
    match value {
        AdminModelVisibilityValue::Public => AdminModelVisibility::Public,
        AdminModelVisibilityValue::Authenticated => AdminModelVisibility::Authenticated,
        AdminModelVisibilityValue::Hidden => AdminModelVisibility::Hidden,
    }
}

const fn map_lifecycle(value: AdminModelLifecycleValue) -> AdminModelLifecycle {
    match value {
        AdminModelLifecycleValue::Draft => AdminModelLifecycle::Draft,
        AdminModelLifecycleValue::Active => AdminModelLifecycle::Active,
        AdminModelLifecycleValue::Deprecated => AdminModelLifecycle::Deprecated,
        AdminModelLifecycleValue::Retired => AdminModelLifecycle::Retired,
    }
}

fn map_write_error(error: AdminModelWriteError) -> ManagementError {
    match error {
        AdminModelWriteError::InvalidInput => ManagementError::InvalidRequest,
        AdminModelWriteError::Forbidden => ManagementError::Forbidden,
        AdminModelWriteError::Conflict => ManagementError::ModelConflict,
        AdminModelWriteError::NotFound => ManagementError::ModelNotFound,
        AdminModelWriteError::Internal => ManagementError::Internal,
    }
}

fn status_json(status: StatusCode, value: impl serde::Serialize) -> Response {
    let mut response = no_store_json(value);
    *response.status_mut() = status;
    response
}

fn no_store_empty(status: StatusCode) -> Response {
    let mut response = status.into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modalities_reject_empty_and_duplicate_values() {
        assert!(parse_modalities(Vec::new()).is_err());
        assert!(
            parse_modalities(vec![
                AdminModelModalityValue::Text,
                AdminModelModalityValue::Text,
            ])
            .is_err()
        );
        let modalities = parse_modalities(vec![
            AdminModelModalityValue::Text,
            AdminModelModalityValue::Image,
        ])
        .unwrap();
        assert!(modalities.text());
        assert!(modalities.image());
        assert!(!modalities.audio());
    }
}
