use std::{future::Future, pin::Pin, sync::Arc};

use af_admin::{AdminChannelReader, SessionAuthentication, SessionAuthenticator};
use af_domain::CredentialId;
use axum::{
    Extension, Json, Router,
    extract::{Path, State, rejection::JsonRejection},
    middleware,
    response::Response,
    routing::post,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_authorization::authorize_management_admin,
    management_channels::{map_channel_read_error, no_store_json, parse_channel_id},
    management_error::ManagementError,
};

/// 脱敏的凭据调度健康状态，不包含 Key、上游正文或内部 Redis 键。
#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct CredentialHealthSnapshot {
    pub credential_id: i64,
    pub penalty_micros: u64,
    pub cooling: bool,
    pub weight_multiplier_micros: u64,
}

pub type CredentialHealthFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Vec<CredentialHealthSnapshot>, ()>> + Send + 'a>>;

/// 组合根注入的共享调度健康只读端口。
pub trait AdminCredentialHealthReader: Send + Sync {
    fn snapshots<'a>(
        &'a self,
        ids: &'a [(CredentialId, CredentialId)],
    ) -> CredentialHealthFuture<'a>;
}

#[derive(Clone)]
struct CredentialHealthState {
    reader: Arc<dyn AdminChannelReader>,
    health: Option<Arc<dyn AdminCredentialHealthReader>>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct CredentialHealthRequest {
    #[schema(min_items = 1, max_items = 100)]
    credential_ids: Vec<i64>,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct CredentialHealthResponse {
    available: bool,
    credentials: Vec<CredentialHealthSnapshot>,
}

/// 独立装配健康查询，保持缓存实现与 HTTP 端口之间的依赖边界。
pub fn build_credential_health_router(
    auth: Arc<dyn SessionAuthenticator>,
    reader: Arc<dyn AdminChannelReader>,
    health: Option<Arc<dyn AdminCredentialHealthReader>>,
) -> Router {
    let authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(auth),
        authenticate_management_session,
    );
    Router::new()
        .route(
            "/api/admin/channels/{channel_id}/credentials/health",
            post(read),
        )
        .route_layer(middleware::from_fn(authorize_management_admin))
        .route_layer(authentication)
        .with_state(CredentialHealthState { reader, health })
}

async fn read(
    State(state): State<CredentialHealthState>,
    Path(channel_id): Path<String>,
    Extension(auth): Extension<SessionAuthentication>,
    request: Result<Json<CredentialHealthRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let channel_id = parse_channel_id(&channel_id)?;
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    if request.credential_ids.is_empty() || request.credential_ids.len() > 100 {
        return Err(ManagementError::InvalidRequest);
    }
    let mut ids = Vec::with_capacity(request.credential_ids.len());
    let mut seen = std::collections::BTreeSet::new();
    for id in request.credential_ids {
        let id = CredentialId::new(id).map_err(|_| ManagementError::InvalidRequest)?;
        if !seen.insert(id) {
            return Err(ManagementError::InvalidRequest);
        }
        let credential = state
            .reader
            .get_credential(auth.principal(), channel_id, id)
            .await
            .map_err(map_channel_read_error)?;
        ids.push((id, credential.parent_id().unwrap_or(id)));
    }
    let Some(health) = state.health else {
        return Ok(no_store_json(CredentialHealthResponse {
            available: false,
            credentials: Vec::new(),
        }));
    };
    let credentials = health
        .snapshots(&ids)
        .await
        .map_err(|_| ManagementError::Internal)?;
    Ok(no_store_json(CredentialHealthResponse {
        available: true,
        credentials,
    }))
}
