use std::{future::Future, pin::Pin};

use af_admin::SessionPrincipal;
use af_domain::{ChannelId, CredentialId};
use axum::{
    extract::{Extension, Path, State},
    response::Response,
};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{
    chat_completions::HttpState,
    management_channels::{
        map_channel_read_error, no_store_json, parse_channel_id, parse_credential_id,
    },
    management_error::ManagementError,
};

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct CredentialUsageWindow {
    pub window_seconds: i64,
    pub used_percent: f64,
    pub reset_at: Option<i64>,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CredentialUsageStatus {
    Available,
    Unsupported,
    Unavailable,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct CredentialUsageSnapshot {
    pub status: CredentialUsageStatus,
    pub windows: Vec<CredentialUsageWindow>,
    pub credits_balance: Option<String>,
    pub fetched_at: Option<i64>,
}

impl CredentialUsageSnapshot {
    #[must_use]
    pub fn unavailable() -> Self {
        Self {
            status: CredentialUsageStatus::Unavailable,
            windows: Vec::new(),
            credits_balance: None,
            fetched_at: None,
        }
    }

    #[must_use]
    pub fn unsupported() -> Self {
        Self {
            status: CredentialUsageStatus::Unsupported,
            windows: Vec::new(),
            credits_balance: None,
            fetched_at: None,
        }
    }
}

pub type AdminCredentialUsageFuture<'a> =
    Pin<Box<dyn Future<Output = CredentialUsageSnapshot> + Send + 'a>>;

pub trait AdminCredentialUsageProbe: Send + Sync {
    fn probe<'a>(
        &'a self,
        principal: SessionPrincipal,
        channel_id: ChannelId,
        credential_id: CredentialId,
    ) -> AdminCredentialUsageFuture<'a>;
}

pub(crate) async fn get_admin_credential_usage(
    State(state): State<HttpState>,
    Path((channel_id, credential_id)): Path<(String, String)>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let channel_id = parse_channel_id(&channel_id)?;
    let credential_id = parse_credential_id(&credential_id)?;
    let principal = authentication.principal();
    let reader = state
        .admin_channel_reader
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let credential = reader
        .get_credential(principal, channel_id, credential_id)
        .await
        .map_err(map_channel_read_error)?;
    if credential.kind() != af_domain::CredentialKind::Oauth
        || credential.oauth_provider() != Some("codex")
        || credential.oauth_token_pending()
        || credential.parent_id().is_some()
    {
        return Ok(no_store_json(CredentialUsageSnapshot::unsupported()));
    }
    if credential.proxy_id().is_some() {
        return Ok(no_store_json(CredentialUsageSnapshot::unsupported()));
    }
    let probe = state
        .admin_credential_usage_probe
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    Ok(no_store_json(
        probe.probe(principal, channel_id, credential_id).await,
    ))
}
