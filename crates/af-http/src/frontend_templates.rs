use std::{future::Future, pin::Pin, sync::Arc};

use axum::{
    Extension, Json, Router,
    extract::{State, rejection::JsonRejection},
    middleware,
    response::Response,
    routing::{get, post, put},
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_authorization::authorize_management_admin,
    management_error::ManagementError,
    management_session::no_store_json,
};

/// 可供管理员选择的外部前端模板摘要。
#[derive(Clone, Debug, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = FrontendTemplateSummary)]
pub struct FrontendTemplateSummary {
    /// 与模板目录一致的稳定标识。
    pub id: String,
    pub name: String,
    pub version: String,
    pub api_contract: String,
    pub valid: bool,
    pub error: Option<String>,
}

/// 当前模板目录扫描结果。
#[derive(Clone, Debug, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = FrontendTemplateCatalog)]
pub struct FrontendTemplateCatalog {
    /// None 表示编译进二进制的内嵌前端。
    pub active_id: Option<String>,
    pub templates: Vec<FrontendTemplateSummary>,
}

/// 模板管理服务的闭合错误分类。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrontendTemplateError {
    InvalidInput,
    NotFound,
    Conflict,
    Internal,
}

pub type FrontendTemplateFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, FrontendTemplateError>> + Send + 'a>>;

/// 将模板目录扫描、切换和持久化交给服务端编排层实现。
pub trait FrontendTemplateService: Send + Sync + 'static {
    fn list(&self) -> FrontendTemplateFuture<'_, FrontendTemplateCatalog>;
    fn scan(&self) -> FrontendTemplateFuture<'_, FrontendTemplateCatalog>;
    fn activate(
        &self,
        template_id: Option<String>,
    ) -> FrontendTemplateFuture<'_, FrontendTemplateCatalog>;
}

#[derive(Clone)]
struct FrontendTemplateHttpState {
    service: Arc<dyn FrontendTemplateService>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = FrontendTemplateActivationRequest)]
pub(crate) struct FrontendTemplateActivationRequest {
    /// None 或空字符串表示恢复内嵌前端。
    template_id: Option<String>,
}

/// 构建仅管理员可访问的模板扫描和切换接口。
pub fn build_frontend_template_router(
    service: Arc<dyn FrontendTemplateService>,
    session_authenticator: Arc<dyn af_admin::SessionAuthenticator>,
) -> Router {
    let authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(session_authenticator),
        authenticate_management_session,
    );
    Router::new()
        .route(
            "/api/admin/frontend-templates",
            get(list_frontend_templates),
        )
        .route(
            "/api/admin/frontend-templates/scan",
            post(scan_frontend_templates),
        )
        .route(
            "/api/admin/frontend-templates/active",
            put(activate_frontend_template),
        )
        .route_layer(middleware::from_fn(authorize_management_admin))
        .route_layer(authentication)
        .with_state(FrontendTemplateHttpState { service })
}

async fn list_frontend_templates(
    State(state): State<FrontendTemplateHttpState>,
    Extension(_authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let catalog = state.service.list().await.map_err(map_template_error)?;
    Ok(no_store_json(catalog))
}

async fn scan_frontend_templates(
    State(state): State<FrontendTemplateHttpState>,
    Extension(_authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let catalog = state.service.scan().await.map_err(map_template_error)?;
    Ok(no_store_json(catalog))
}

async fn activate_frontend_template(
    State(state): State<FrontendTemplateHttpState>,
    Extension(_authentication): Extension<af_admin::SessionAuthentication>,
    request: Result<Json<FrontendTemplateActivationRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let template_id = request
        .template_id
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    let catalog = state
        .service
        .activate(template_id)
        .await
        .map_err(map_template_error)?;
    Ok(no_store_json(catalog))
}

fn map_template_error(error: FrontendTemplateError) -> ManagementError {
    match error {
        FrontendTemplateError::InvalidInput | FrontendTemplateError::NotFound => {
            ManagementError::InvalidRequest
        }
        FrontendTemplateError::Conflict => ManagementError::SiteSettingsConflict,
        FrontendTemplateError::Internal => ManagementError::Internal,
    }
}
