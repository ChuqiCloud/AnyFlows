use crate::{
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_authorization::authorize_management_admin,
    management_session::no_store_json,
};
use af_admin::{
    CredentialModelError, CredentialModelService, SessionAuthentication, SessionAuthenticator,
    UpstreamModelPolicySettings,
};
use af_domain::{ChannelId, CredentialId};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Extension, Path, State, rejection::JsonRejection},
    middleware,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use http::StatusCode;
use std::sync::Arc;

#[derive(Clone)]
struct ModelState {
    service: Arc<dyn CredentialModelService>,
}

pub fn build_credential_models_router(
    service: Arc<dyn CredentialModelService>,
    authenticator: Arc<dyn SessionAuthenticator>,
) -> Router {
    let auth = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(authenticator),
        authenticate_management_session,
    );
    Router::new()
        .route(
            "/api/admin/channels/{channel_id}/model-policy",
            get(channel_policy).put(write_channel_policy),
        )
        .route(
            "/api/admin/channels/{channel_id}/credentials/{credential_id}/model-policy",
            get(credential_policy).put(write_credential_policy),
        )
        .route(
            "/api/admin/channels/{channel_id}/credentials/{credential_id}/models",
            get(models),
        )
        .route(
            "/api/admin/channels/{channel_id}/credentials/{credential_id}/models/refresh",
            post(refresh),
        )
        .route_layer(middleware::from_fn(authorize_management_admin))
        .route_layer(auth)
        .layer(DefaultBodyLimit::max(512 * 1024))
        .with_state(ModelState { service })
}

struct ModelError(CredentialModelError);
impl From<CredentialModelError> for ModelError {
    fn from(error: CredentialModelError) -> Self {
        Self(error)
    }
}
impl IntoResponse for ModelError {
    fn into_response(self) -> Response {
        let (status, code) = match self.0 {
            CredentialModelError::InvalidInput => (StatusCode::BAD_REQUEST, "invalid_request"),
            CredentialModelError::NotFound => (StatusCode::NOT_FOUND, "credential_not_found"),
            CredentialModelError::Conflict => (StatusCode::CONFLICT, "model_policy_conflict"),
            CredentialModelError::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
            CredentialModelError::Internal => {
                (StatusCode::SERVICE_UNAVAILABLE, "model_policy_unavailable")
            }
        };
        let mut response =
            no_store_json(serde_json::json!({"code":code,"message":self.0.to_string()}));
        *response.status_mut() = status;
        response
    }
}
fn channel(id: &str) -> Result<ChannelId, ModelError> {
    id.parse::<i64>()
        .ok()
        .and_then(|id| ChannelId::new(id).ok())
        .ok_or_else(|| CredentialModelError::InvalidInput.into())
}
fn credential(id: &str) -> Result<CredentialId, ModelError> {
    id.parse::<i64>()
        .ok()
        .and_then(|id| CredentialId::new(id).ok())
        .ok_or_else(|| CredentialModelError::InvalidInput.into())
}
async fn channel_policy(
    State(state): State<ModelState>,
    Path(id): Path<String>,
    Extension(auth): Extension<SessionAuthentication>,
) -> Result<Response, ModelError> {
    Ok(no_store_json(
        state
            .service
            .policy(auth.principal(), channel(&id)?, None)
            .await?,
    ))
}
async fn write_channel_policy(
    State(state): State<ModelState>,
    Path(id): Path<String>,
    Extension(auth): Extension<SessionAuthentication>,
    body: Result<Json<UpstreamModelPolicySettings>, JsonRejection>,
) -> Result<Response, ModelError> {
    let Json(settings) = body.map_err(|_| CredentialModelError::InvalidInput)?;
    Ok(no_store_json(
        state
            .service
            .write_policy(auth.principal(), channel(&id)?, None, settings)
            .await?,
    ))
}
async fn credential_policy(
    State(state): State<ModelState>,
    Path((c, k)): Path<(String, String)>,
    Extension(auth): Extension<SessionAuthentication>,
) -> Result<Response, ModelError> {
    Ok(no_store_json(
        state
            .service
            .policy(auth.principal(), channel(&c)?, Some(credential(&k)?))
            .await?,
    ))
}
async fn write_credential_policy(
    State(state): State<ModelState>,
    Path((c, k)): Path<(String, String)>,
    Extension(auth): Extension<SessionAuthentication>,
    body: Result<Json<UpstreamModelPolicySettings>, JsonRejection>,
) -> Result<Response, ModelError> {
    let Json(settings) = body.map_err(|_| CredentialModelError::InvalidInput)?;
    Ok(no_store_json(
        state
            .service
            .write_policy(
                auth.principal(),
                channel(&c)?,
                Some(credential(&k)?),
                settings,
            )
            .await?,
    ))
}
async fn models(
    State(state): State<ModelState>,
    Path((c, k)): Path<(String, String)>,
    Extension(auth): Extension<SessionAuthentication>,
) -> Result<Response, ModelError> {
    Ok(no_store_json(
        state
            .service
            .models(auth.principal(), channel(&c)?, credential(&k)?)
            .await?,
    ))
}
async fn refresh(
    State(state): State<ModelState>,
    Path((c, k)): Path<(String, String)>,
    Extension(auth): Extension<SessionAuthentication>,
) -> Result<Response, ModelError> {
    Ok(no_store_json(
        state
            .service
            .refresh(auth.principal(), channel(&c)?, credential(&k)?)
            .await?,
    ))
}
