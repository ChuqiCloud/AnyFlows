use std::sync::Arc;

use af_admin::{
    AdminVerificationSettings, DatabaseVerificationSettingsService, SessionAuthentication,
    SessionAuthenticator, VerificationSettingsCommand, VerificationSettingsError,
};
use axum::{
    Extension, Json, Router,
    extract::{State, rejection::JsonRejection},
    middleware,
    response::Response,
    routing::get,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_authorization::authorize_management_admin,
    management_error::ManagementError,
    management_session::no_store_json,
};

#[derive(Serialize, ToSchema)]
#[schema(as = AdminVerificationSettings)]
pub(crate) struct AdminVerificationSettingsResponse {
    source: &'static str,
    manual_enabled: bool,
    individual_manual_enabled: bool,
    enterprise_manual_enabled: bool,
    individual_reason_required: bool,
    enterprise_reason_required: bool,
    enabled: bool,
    app_id: Option<String>,
    private_key_configured: bool,
    public_key_configured: bool,
    gateway_url: String,
    biz_code: String,
    timeout_secs: u64,
    version: i64,
}

impl From<AdminVerificationSettings> for AdminVerificationSettingsResponse {
    fn from(value: AdminVerificationSettings) -> Self {
        Self {
            source: value.source,
            manual_enabled: value.manual_enabled,
            individual_manual_enabled: value.individual_manual_enabled,
            enterprise_manual_enabled: value.enterprise_manual_enabled,
            individual_reason_required: value.individual_reason_required,
            enterprise_reason_required: value.enterprise_reason_required,
            enabled: value.enabled,
            app_id: value.app_id,
            private_key_configured: value.private_key_configured,
            public_key_configured: value.public_key_configured,
            gateway_url: value.gateway_url,
            biz_code: value.biz_code,
            timeout_secs: value.timeout_secs,
            version: value.version,
        }
    }
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminVerificationSettingsRequest)]
pub(crate) struct AdminVerificationSettingsRequest {
    expected_version: i64,
    manual_enabled: bool,
    individual_manual_enabled: Option<bool>,
    enterprise_manual_enabled: Option<bool>,
    individual_reason_required: Option<bool>,
    enterprise_reason_required: Option<bool>,
    enabled: bool,
    app_id: Option<String>,
    #[schema(format = Password, write_only)]
    private_key: Option<String>,
    #[schema(format = Password, write_only)]
    public_key: Option<String>,
    gateway_url: String,
    biz_code: String,
    timeout_secs: u64,
}

pub fn build_verification_settings_router(
    service: Arc<DatabaseVerificationSettingsService>,
    auth: Arc<dyn SessionAuthenticator>,
) -> Router {
    let authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(auth),
        authenticate_management_session,
    );
    Router::new()
        .route(
            "/api/admin/account-verification-settings",
            get(read).put(update),
        )
        .route_layer(middleware::from_fn(authorize_management_admin))
        .route_layer(authentication)
        .with_state(service)
}

async fn read(
    State(service): State<Arc<DatabaseVerificationSettingsService>>,
    Extension(auth): Extension<SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let settings = service
        .settings(auth.principal())
        .await
        .map_err(map_error)?;
    Ok(no_store_json(AdminVerificationSettingsResponse::from(
        settings,
    )))
}

async fn update(
    State(service): State<Arc<DatabaseVerificationSettingsService>>,
    Extension(auth): Extension<SessionAuthentication>,
    request: Result<Json<AdminVerificationSettingsRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let settings = service
        .update(
            auth.principal(),
            VerificationSettingsCommand {
                expected_version: request.expected_version,
                manual_enabled: request.manual_enabled,
                individual_manual_enabled: request.individual_manual_enabled,
                enterprise_manual_enabled: request.enterprise_manual_enabled,
                individual_reason_required: request.individual_reason_required,
                enterprise_reason_required: request.enterprise_reason_required,
                enabled: request.enabled,
                app_id: request.app_id,
                private_key: request.private_key,
                public_key: request.public_key,
                gateway_url: request.gateway_url,
                biz_code: request.biz_code,
                timeout_secs: request.timeout_secs,
            },
        )
        .await
        .map_err(map_error)?;
    Ok(no_store_json(AdminVerificationSettingsResponse::from(
        settings,
    )))
}

fn map_error(error: VerificationSettingsError) -> ManagementError {
    match error {
        VerificationSettingsError::Invalid => ManagementError::InvalidRequest,
        VerificationSettingsError::Conflict => ManagementError::AccountVerificationSettingsConflict,
        VerificationSettingsError::Forbidden => ManagementError::Forbidden,
        VerificationSettingsError::Internal => ManagementError::Internal,
    }
}
