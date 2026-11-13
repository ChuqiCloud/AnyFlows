use std::sync::Arc;

use af_admin::{
    ExternalAccount, ExternalAccountBalancePage, ExternalAccountBalanceSnapshot,
    ExternalAccountBalanceStatus, ExternalAccountError, ExternalAccountLinkCommand,
    SessionAuthentication, SessionAuthenticator, SupportedExternalConnector,
    UserExternalAccountService,
};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Extension, Path, Query, State, rejection::JsonRejection},
    middleware,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use tower_http::limit::RequestBodyLimitLayer;
use utoipa::ToSchema;
use zeroize::Zeroize;

use crate::{
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_error::ManagementError,
    management_session::{no_store_empty, no_store_json},
};

#[derive(Clone)]
struct ExternalAccountState {
    service: Arc<dyn UserExternalAccountService>,
}

pub fn build_user_external_account_router(
    service: Arc<dyn UserExternalAccountService>,
    authenticator: Arc<dyn SessionAuthenticator>,
) -> Router {
    Router::new()
        .route("/api/account/external-connectors", get(connectors))
        .route("/api/account/external-accounts", get(list).post(link))
        .route("/api/account/external-accounts/{id}/refresh", post(refresh))
        .route(
            "/api/account/external-accounts/{id}/balances",
            get(balance_history),
        )
        .route(
            "/api/account/external-accounts/{id}",
            axum::routing::delete(unlink),
        )
        .layer(DefaultBodyLimit::max(16 * 1024))
        .layer(RequestBodyLimitLayer::new(16 * 1024))
        .layer(middleware::from_fn_with_state(
            ManagementAuthenticationState::new(authenticator),
            authenticate_management_session,
        ))
        .with_state(ExternalAccountState { service })
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = UserExternalAccountLinkRequest)]
pub(crate) struct LinkRequest {
    #[schema(min_length = 3, max_length = 128)]
    connector_id: String,
    #[schema(min_length = 9, max_length = 2048)]
    base_url: String,
    #[schema(min_length = 1, max_length = 8192, write_only)]
    credential: String,
}

impl Drop for LinkRequest {
    fn drop(&mut self) {
        self.credential.zeroize();
    }
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
#[schema(as = UserExternalConnectorResponse)]
pub(crate) struct ConnectorResponse {
    id: String,
    display_name: String,
    authentication_modes: Vec<String>,
}

impl From<SupportedExternalConnector> for ConnectorResponse {
    fn from(value: SupportedExternalConnector) -> Self {
        Self {
            id: value.id,
            display_name: value.display_name,
            authentication_modes: value.authentication_modes,
        }
    }
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
#[schema(as = UserExternalAccountBalanceResponse)]
pub(crate) struct BalanceResponse {
    amount: Option<String>,
    unit: Option<String>,
    status: &'static str,
    /// Unix epoch milliseconds.
    observed_at: i64,
}

impl From<ExternalAccountBalanceSnapshot> for BalanceResponse {
    fn from(value: ExternalAccountBalanceSnapshot) -> Self {
        Self {
            amount: value.amount,
            unit: value.unit,
            status: match value.status {
                ExternalAccountBalanceStatus::Available => "available",
                ExternalAccountBalanceStatus::Unavailable => "unavailable",
                ExternalAccountBalanceStatus::AuthenticationRequired => "authentication_required",
            },
            observed_at: value.observed_at,
        }
    }
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
#[schema(as = UserExternalAccountResponse)]
pub(crate) struct AccountResponse {
    id: i64,
    connector_id: String,
    external_subject: String,
    display_name: Option<String>,
    base_url: String,
    /// Unix epoch seconds.
    created_at: i64,
    /// Unix epoch seconds.
    updated_at: i64,
    latest_balance: Option<BalanceResponse>,
}

impl From<ExternalAccount> for AccountResponse {
    fn from(value: ExternalAccount) -> Self {
        Self {
            id: value.id,
            connector_id: value.connector_id,
            external_subject: value.external_subject,
            display_name: value.display_name,
            base_url: value.base_url,
            created_at: value.created_at,
            updated_at: value.updated_at,
            latest_balance: value.latest_balance.map(BalanceResponse::from),
        }
    }
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
#[schema(as = UserExternalAccountListResponse)]
pub(crate) struct AccountListResponse {
    items: Vec<AccountResponse>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
#[schema(as = UserExternalAccountBalanceHistoryResponse)]
pub(crate) struct BalanceHistoryResponse {
    connection_id: i64,
    items: Vec<BalanceResponse>,
}

impl From<ExternalAccountBalancePage> for BalanceHistoryResponse {
    fn from(value: ExternalAccountBalancePage) -> Self {
        Self {
            connection_id: value.connection_id,
            items: value.items.into_iter().map(BalanceResponse::from).collect(),
        }
    }
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct BalanceHistoryQuery {
    limit: Option<u32>,
}

#[derive(Debug)]
struct ExternalAccountHttpError(ExternalAccountError);

impl From<ExternalAccountError> for ExternalAccountHttpError {
    fn from(value: ExternalAccountError) -> Self {
        Self(value)
    }
}

impl IntoResponse for ExternalAccountHttpError {
    fn into_response(self) -> Response {
        let error = match self.0 {
            ExternalAccountError::InvalidInput => ManagementError::ExternalAccountInvalidRequest,
            ExternalAccountError::Unsupported => ManagementError::ExternalAccountUnsupported,
            ExternalAccountError::Conflict => ManagementError::ExternalAccountConflict,
            ExternalAccountError::NotFound => ManagementError::ExternalAccountNotFound,
            ExternalAccountError::AuthenticationRequired => {
                ManagementError::ExternalAccountAuthenticationRequired
            }
            ExternalAccountError::Unavailable => ManagementError::ExternalAccountUnavailable,
            ExternalAccountError::RateLimited => ManagementError::ExternalAccountRateLimited,
            ExternalAccountError::LimitReached => ManagementError::ExternalAccountLimitReached,
            ExternalAccountError::Internal => ManagementError::Internal,
        };
        error.into_response()
    }
}

async fn connectors(
    State(state): State<ExternalAccountState>,
    Extension(_auth): Extension<SessionAuthentication>,
) -> Response {
    no_store_json(
        state
            .service
            .connectors()
            .into_iter()
            .map(ConnectorResponse::from)
            .collect::<Vec<_>>(),
    )
}

async fn list(
    State(state): State<ExternalAccountState>,
    Extension(auth): Extension<SessionAuthentication>,
) -> Result<Response, ExternalAccountHttpError> {
    let items = state
        .service
        .list(auth.principal())
        .await?
        .into_iter()
        .map(AccountResponse::from)
        .collect();
    Ok(no_store_json(AccountListResponse { items }))
}

async fn link(
    State(state): State<ExternalAccountState>,
    Extension(auth): Extension<SessionAuthentication>,
    body: Result<Json<LinkRequest>, JsonRejection>,
) -> Result<Response, ExternalAccountHttpError> {
    let mut body = body.map_err(|_| ExternalAccountError::InvalidInput)?.0;
    let command = ExternalAccountLinkCommand::new(
        std::mem::take(&mut body.connector_id),
        std::mem::take(&mut body.base_url),
        std::mem::take(&mut body.credential),
    )?;
    let account = state.service.link(auth.principal(), command).await?;
    Ok(no_store_json(AccountResponse::from(account)))
}

async fn refresh(
    State(state): State<ExternalAccountState>,
    Extension(auth): Extension<SessionAuthentication>,
    Path(id): Path<i64>,
) -> Result<Response, ExternalAccountHttpError> {
    if id <= 0 {
        return Err(ExternalAccountError::InvalidInput.into());
    }
    let account = state.service.refresh(auth.principal(), id).await?;
    Ok(no_store_json(AccountResponse::from(account)))
}

async fn unlink(
    State(state): State<ExternalAccountState>,
    Extension(auth): Extension<SessionAuthentication>,
    Path(id): Path<i64>,
) -> Result<Response, ExternalAccountHttpError> {
    if id <= 0 {
        return Err(ExternalAccountError::InvalidInput.into());
    }
    state.service.unlink(auth.principal(), id).await?;
    Ok(no_store_empty(axum::http::StatusCode::NO_CONTENT))
}

async fn balance_history(
    State(state): State<ExternalAccountState>,
    Extension(auth): Extension<SessionAuthentication>,
    Path(id): Path<i64>,
    Query(query): Query<BalanceHistoryQuery>,
) -> Result<Response, ExternalAccountHttpError> {
    if id <= 0 {
        return Err(ExternalAccountError::InvalidInput.into());
    }
    let limit = query.limit.unwrap_or(20);
    let page = state
        .service
        .balance_history(auth.principal(), id, limit)
        .await?;
    Ok(no_store_json(BalanceHistoryResponse::from(page)))
}
