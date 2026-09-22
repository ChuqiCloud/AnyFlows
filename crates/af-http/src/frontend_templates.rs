use std::{future::Future, pin::Pin, sync::Arc};

use axum::{
    Extension, Json, Router,
    body::Body,
    extract::{Path, State, rejection::JsonRejection},
    middleware,
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use http::{StatusCode, header};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_authorization::authorize_management_admin,
    management_error::ManagementError,
    management_session::no_store_json,
};

/// 可供管理员选择的内置或外部前端模板摘要。
#[derive(Clone, Debug, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = FrontendTemplateSummary)]
pub struct FrontendTemplateSummary {
    /// 与模板目录一致的稳定标识。
    pub id: String,
    pub name: String,
    pub version: String,
    pub api_contract: String,
    /// 编译进服务二进制的官方前端。
    pub builtin: bool,
    pub description: Option<String>,
    pub author: Option<String>,
    /// 需要管理员会话的静态预览图片地址，不执行模板脚本。
    pub preview_url: Option<String>,
    pub valid: bool,
    pub error: Option<String>,
}

/// 当前模板目录扫描结果。
#[derive(Clone, Debug, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = FrontendTemplateCatalog)]
pub struct FrontendTemplateCatalog {
    /// None 表示默认经典前端；embedded-next 表示新版内置前端。
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

/// 已校验内容格式的只读预览图片。
#[derive(Clone, Debug)]
pub struct FrontendTemplatePreview {
    pub asset: crate::FrontendAsset,
    pub content_type: &'static str,
}

/// 将模板目录扫描、切换和持久化交给服务端编排层实现。
pub trait FrontendTemplateService: Send + Sync + 'static {
    fn list(&self) -> FrontendTemplateFuture<'_, FrontendTemplateCatalog>;
    fn scan(&self) -> FrontendTemplateFuture<'_, FrontendTemplateCatalog>;
    fn preview(&self, template_id: &str) -> Result<FrontendTemplatePreview, FrontendTemplateError>;
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
        .route(
            "/api/admin/frontend-templates/{template_id}/preview",
            get(preview_frontend_template),
        )
        .route_layer(middleware::from_fn(authorize_management_admin))
        .route_layer(authentication)
        .with_state(FrontendTemplateHttpState { service })
}

async fn preview_frontend_template(
    State(state): State<FrontendTemplateHttpState>,
    Path(template_id): Path<String>,
) -> Result<Response, ManagementError> {
    let preview = match state.service.preview(&template_id) {
        Ok(preview) => preview,
        Err(FrontendTemplateError::NotFound) => return Ok(StatusCode::NOT_FOUND.into_response()),
        Err(error) => return Err(map_template_error(error)),
    };
    Ok(Response::builder()
        .header(header::CONTENT_TYPE, preview.content_type)
        .header(header::CACHE_CONTROL, "no-store")
        .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
        .header(
            header::CONTENT_SECURITY_POLICY,
            "default-src 'none'; style-src 'unsafe-inline'; sandbox",
        )
        .header(header::REFERRER_POLICY, "no-referrer")
        .body(Body::from(preview.asset.content()))
        .expect("固定的模板预览响应头必须有效"))
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

#[cfg(test)]
mod tests {
    use super::*;
    use af_admin::{
        SessionAuthentication, SessionAuthenticationError, SessionAuthenticationFuture,
        SessionAuthenticator, SessionLoginFuture, SessionPrincipal, SessionRole,
    };
    use af_domain::{GroupId, UserId};
    use http::Request;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tower::ServiceExt;

    struct Sessions;
    impl SessionAuthenticator for Sessions {
        fn login<'a>(
            &'a self,
            _credentials: &'a af_admin::LoginCredentials,
        ) -> SessionLoginFuture<'a> {
            Box::pin(async { Err(SessionAuthenticationError::InvalidCredentials) })
        }
        fn authenticate<'a>(&'a self, token: &'a str) -> SessionAuthenticationFuture<'a> {
            Box::pin(async move {
                let role = match token {
                    "admin-token" => SessionRole::Admin,
                    "user-token" => SessionRole::User,
                    _ => return Err(SessionAuthenticationError::InvalidSession),
                };
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                Ok(SessionAuthentication::new(
                    SessionPrincipal::new(UserId::new(1).unwrap(), role),
                    GroupId::new(1).unwrap(),
                    now + 60,
                ))
            })
        }
    }

    #[derive(Default)]
    struct Templates {
        previews: AtomicUsize,
        activations: AtomicUsize,
    }
    impl FrontendTemplateService for Templates {
        fn list(&self) -> FrontendTemplateFuture<'_, FrontendTemplateCatalog> {
            Box::pin(async {
                Ok(FrontendTemplateCatalog {
                    active_id: None,
                    templates: Vec::new(),
                })
            })
        }
        fn scan(&self) -> FrontendTemplateFuture<'_, FrontendTemplateCatalog> {
            self.list()
        }
        fn preview(&self, id: &str) -> Result<FrontendTemplatePreview, FrontendTemplateError> {
            self.previews.fetch_add(1, Ordering::SeqCst);
            if id != "embedded" {
                return Err(FrontendTemplateError::NotFound);
            }
            Ok(FrontendTemplatePreview {
                asset: crate::FrontendAsset::from_bytes("preview-image"),
                content_type: "image/png",
            })
        }
        fn activate(
            &self,
            _id: Option<String>,
        ) -> FrontendTemplateFuture<'_, FrontendTemplateCatalog> {
            self.activations.fetch_add(1, Ordering::SeqCst);
            self.list()
        }
    }

    #[tokio::test]
    async fn previews_require_admin_and_do_not_activate_templates() {
        let service = Arc::new(Templates::default());
        let router = build_frontend_template_router(service.clone(), Arc::new(Sessions));
        for (token, status) in [
            (None, StatusCode::UNAUTHORIZED),
            (Some("user-token"), StatusCode::FORBIDDEN),
        ] {
            let mut request =
                Request::builder().uri("/api/admin/frontend-templates/embedded/preview");
            if let Some(token) = token {
                request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
            }
            let response = router
                .clone()
                .oneshot(request.body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), status);
        }
        assert_eq!(service.previews.load(Ordering::SeqCst), 0);
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/admin/frontend-templates/embedded/preview")
                    .header(header::AUTHORIZATION, "Bearer admin-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        assert_eq!(response.headers()[header::CONTENT_TYPE], "image/png");
        assert_eq!(
            response.headers()[header::X_CONTENT_TYPE_OPTIONS],
            "nosniff"
        );
        assert_eq!(
            axum::body::to_bytes(response.into_body(), 1024)
                .await
                .unwrap()
                .as_ref(),
            b"preview-image"
        );
        assert_eq!(service.activations.load(Ordering::SeqCst), 0);
        let missing = router
            .oneshot(
                Request::builder()
                    .uri("/api/admin/frontend-templates/unknown/preview")
                    .header(header::AUTHORIZATION, "Bearer admin-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    }
}
