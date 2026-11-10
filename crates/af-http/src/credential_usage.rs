use std::{future::Future, pin::Pin};

use af_admin::SessionPrincipal;
use af_domain::{ChannelId, CredentialId};
use axum::{
    extract::{Extension, Path, State},
    response::Response,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    chat_completions::HttpState,
    management_channels::{
        map_channel_read_error, no_store_json, parse_channel_id, parse_credential_id,
    },
    management_error::ManagementError,
};

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct CredentialUsageWindow {
    pub window_seconds: i64,
    pub used_percent: f64,
    pub reset_at: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CredentialUsageStatus {
    Available,
    Unsupported,
    Unavailable,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct CredentialUsageSnapshot {
    pub status: CredentialUsageStatus,
    pub windows: Vec<CredentialUsageWindow>,
    pub credits_balance: Option<String>,
    pub fetched_at: Option<i64>,
    pub scope: Option<String>,
    pub provider: Option<String>,
    pub currency: Option<String>,
    pub unit: Option<String>,
    pub total: Option<String>,
    pub used: Option<String>,
    pub remaining: Option<String>,
    #[serde(default)]
    pub unlimited: bool,
    pub expires_at: Option<i64>,
    pub reset_at: Option<i64>,
    pub last_success_at: Option<i64>,
    pub last_checked_at: Option<i64>,
    #[serde(default)]
    pub stale: bool,
    pub error_code: Option<String>,
    pub account_ref: Option<String>,
    pub source: Option<String>,
    pub pause_until: Option<i64>,
    pub alert: Option<String>,
    #[schema(value_type = Option<Object>)]
    pub subscription: Option<serde_json::Value>,
}

impl CredentialUsageSnapshot {
    #[must_use]
    pub fn unavailable() -> Self {
        Self {
            status: CredentialUsageStatus::Unavailable,
            windows: Vec::new(),
            credits_balance: None,
            fetched_at: None,
            scope: None,
            provider: None,
            currency: None,
            unit: None,
            total: None,
            used: None,
            remaining: None,
            unlimited: false,
            expires_at: None,
            reset_at: None,
            last_success_at: None,
            last_checked_at: None,
            stale: true,
            error_code: None,
            account_ref: None,
            source: None,
            pause_until: None,
            alert: None,
            subscription: None,
        }
    }

    #[must_use]
    pub fn unsupported() -> Self {
        Self {
            status: CredentialUsageStatus::Unsupported,
            stale: false,
            ..Self::unavailable()
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

    fn refresh<'a>(
        &'a self,
        principal: SessionPrincipal,
        channel_id: ChannelId,
        credential_id: CredentialId,
    ) -> AdminCredentialUsageFuture<'a> {
        self.probe(principal, channel_id, credential_id)
    }
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
    // Reading through the administrator reader validates nested association and role.
    let _ = credential;
    let Some(probe) = state.admin_credential_usage_probe.as_ref() else {
        return Ok(no_store_json(CredentialUsageSnapshot::unsupported()));
    };
    Ok(no_store_json(
        probe.probe(principal, channel_id, credential_id).await,
    ))
}
