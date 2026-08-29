use std::sync::Arc;

use af_admin::{
    OrganizationActivityService, OrganizationAuthorizationError, OrganizationAuthorizationService,
    OrganizationContractPriceCloseCommand, OrganizationContractPriceCreateCommand,
    OrganizationContractPriceRecord, OrganizationContractPriceService,
    OrganizationContractPriceServiceError, SessionAuthentication, SessionAuthenticator,
};
use af_domain::{OrganizationContractPriceId, OrganizationId, OrganizationPermission, Protocol};
use af_telemetry::RequestId;
use axum::{
    Extension, Json, Router,
    extract::{Path, State, rejection::JsonRejection},
    middleware,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use http::{HeaderMap, HeaderValue, StatusCode, header::CACHE_CONTROL};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::str::FromStr;
use utoipa::ToSchema;

use crate::{
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_error::ManagementError,
    middleware::ensure_request_id,
    organization_activity::{
        audit_organization_action_result, authorize_organization_action, record_organization_action,
    },
};

#[derive(Clone)]
struct OrganizationContractPriceHttpState {
    authorization_service: Arc<dyn OrganizationAuthorizationService>,
    service: Arc<dyn OrganizationContractPriceService>,
    activity_service: Arc<dyn OrganizationActivityService>,
}

/// 构建企业合同价管理接口；所有入口均要求有效管理会话。
pub fn build_organization_contract_price_router(
    authorization_service: Arc<dyn OrganizationAuthorizationService>,
    service: Arc<dyn OrganizationContractPriceService>,
    activity_service: Arc<dyn OrganizationActivityService>,
    session_authenticator: Arc<dyn SessionAuthenticator>,
) -> Router {
    let authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(session_authenticator),
        authenticate_management_session,
    );
    Router::new()
        .route(
            "/api/organizations/{organization_id}/contract-prices",
            get(list_contract_prices).post(create_contract_price),
        )
        .route(
            "/api/organizations/{organization_id}/contract-prices/{price_id}",
            get(get_contract_price),
        )
        .route(
            "/api/organizations/{organization_id}/contract-prices/{price_id}/close",
            post(close_contract_price),
        )
        .route_layer(authentication)
        .layer(middleware::from_fn(ensure_request_id))
        .with_state(OrganizationContractPriceHttpState {
            authorization_service,
            service,
            activity_service,
        })
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ContractPriceProtocol {
    OpenaiChat,
    OpenaiResponses,
    OpenaiEmbeddings,
    OpenaiImages,
    OpenaiAudio,
    OpenaiSpeech,
    JinaRerank,
    CohereRerank,
    XaiVideo,
    Anthropic,
    Gemini,
}

impl From<ContractPriceProtocol> for Protocol {
    fn from(value: ContractPriceProtocol) -> Self {
        match value {
            ContractPriceProtocol::OpenaiChat => Self::OpenAiChat,
            ContractPriceProtocol::OpenaiResponses => Self::OpenAiResponses,
            ContractPriceProtocol::OpenaiEmbeddings => Self::OpenAiEmbeddings,
            ContractPriceProtocol::OpenaiImages => Self::OpenAiImages,
            ContractPriceProtocol::OpenaiAudio => Self::OpenAiAudio,
            ContractPriceProtocol::OpenaiSpeech => Self::OpenAiSpeech,
            ContractPriceProtocol::JinaRerank => Self::JinaRerank,
            ContractPriceProtocol::CohereRerank => Self::CohereRerank,
            ContractPriceProtocol::XaiVideo => Self::XaiVideo,
            ContractPriceProtocol::Anthropic => Self::Anthropic,
            ContractPriceProtocol::Gemini => Self::Gemini,
        }
    }
}

impl From<Protocol> for ContractPriceProtocol {
    fn from(value: Protocol) -> Self {
        match value {
            Protocol::OpenAiChat => Self::OpenaiChat,
            Protocol::OpenAiResponses => Self::OpenaiResponses,
            Protocol::OpenAiEmbeddings => Self::OpenaiEmbeddings,
            Protocol::OpenAiImages => Self::OpenaiImages,
            Protocol::OpenAiAudio => Self::OpenaiAudio,
            Protocol::OpenAiSpeech => Self::OpenaiSpeech,
            Protocol::JinaRerank => Self::JinaRerank,
            Protocol::CohereRerank => Self::CohereRerank,
            Protocol::XaiVideo => Self::XaiVideo,
            Protocol::Anthropic => Self::Anthropic,
            Protocol::Gemini => Self::Gemini,
        }
    }
}

#[derive(Deserialize, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ContractPriceValues {
    #[schema(value_type = String, pattern = r"^(0|[1-9][0-9]{0,9})(\.[0-9]{1,28})?$", max_length = 39)]
    input: String,
    #[schema(value_type = String, pattern = r"^(0|[1-9][0-9]{0,9})(\.[0-9]{1,28})?$", max_length = 39)]
    output: String,
    #[schema(value_type = String, pattern = r"^(0|[1-9][0-9]{0,9})(\.[0-9]{1,28})?$", max_length = 39)]
    cache_read: String,
    #[schema(value_type = String, pattern = r"^(0|[1-9][0-9]{0,9})(\.[0-9]{1,28})?$", max_length = 39)]
    cache_creation_5m: String,
    #[schema(value_type = String, pattern = r"^(0|[1-9][0-9]{0,9})(\.[0-9]{1,28})?$", max_length = 39)]
    cache_creation_1h: String,
}

impl ContractPriceValues {
    fn into_prices(self) -> Result<[Decimal; 5], ManagementError> {
        Ok([
            parse_decimal(&self.input)?,
            parse_decimal(&self.output)?,
            parse_decimal(&self.cache_read)?,
            parse_decimal(&self.cache_creation_5m)?,
            parse_decimal(&self.cache_creation_1h)?,
        ])
    }

    fn from_prices(values: [Decimal; 5]) -> Self {
        Self {
            input: decimal_text(values[0]),
            output: decimal_text(values[1]),
            cache_read: decimal_text(values[2]),
            cache_creation_5m: decimal_text(values[3]),
            cache_creation_1h: decimal_text(values[4]),
        }
    }
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ContractPriceCreateRequest {
    #[schema(min_length = 1, max_length = 256)]
    model: String,
    protocol: ContractPriceProtocol,
    prices: ContractPriceValues,
    #[schema(minimum = 0)]
    effective_from: i64,
    #[schema(minimum = 0, required = true)]
    effective_until: Option<i64>,
    #[schema(minimum = 0, required = true)]
    expected_version: Option<i64>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ContractPriceCloseRequest {
    #[schema(minimum = 0)]
    effective_until: i64,
    #[schema(minimum = 1)]
    expected_version: i64,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ContractPriceResponse {
    id: i64,
    organization_id: i64,
    model: String,
    protocol: ContractPriceProtocol,
    prices: ContractPriceValues,
    effective_from: i64,
    #[schema(required = true)]
    effective_until: Option<i64>,
    version: i64,
    created_by_user_id: i64,
    created_at: i64,
    updated_at: i64,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct ContractPriceListResponse {
    #[schema(max_items = 1000)]
    prices: Vec<ContractPriceResponse>,
}

async fn list_contract_prices(
    State(state): State<OrganizationContractPriceHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Path(organization_id): Path<i64>,
) -> Result<Response, ManagementError> {
    let context = authorize(
        &state,
        &authentication,
        organization_id,
        OrganizationPermission::ContractPricesRead,
    )
    .await?;
    let records = state.service.list(context).await.map_err(map_error)?;
    let prices = records
        .iter()
        .map(ContractPriceResponse::from_record)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(no_store_status_json(
        StatusCode::OK,
        ContractPriceListResponse { prices },
    ))
}

async fn get_contract_price(
    State(state): State<OrganizationContractPriceHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Path((organization_id, price_id)): Path<(i64, i64)>,
) -> Result<Response, ManagementError> {
    let context = authorize(
        &state,
        &authentication,
        organization_id,
        OrganizationPermission::ContractPricesRead,
    )
    .await?;
    let price_id =
        OrganizationContractPriceId::new(price_id).map_err(|_| ManagementError::InvalidRequest)?;
    let record = state
        .service
        .get(context, price_id)
        .await
        .map_err(map_error)?;
    Ok(no_store_status_json(
        StatusCode::OK,
        ContractPriceResponse::from_record(&record)?,
    ))
}

async fn create_contract_price(
    State(state): State<OrganizationContractPriceHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    Path(organization_id): Path<i64>,
    headers: HeaderMap,
    request: Result<Json<ContractPriceCreateRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let organization_id =
        OrganizationId::new(organization_id).map_err(|_| ManagementError::InvalidRequest)?;
    let context = authorize_organization_action(
        &state.authorization_service,
        &state.activity_service,
        &authentication,
        organization_id,
        OrganizationPermission::ContractPricesManage,
        &request_id,
        &headers,
        "organization.contract_price.create",
        "contract_price",
        "new".to_owned(),
    )
    .await?;
    let Json(request) = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &request_id,
        &headers,
        OrganizationPermission::ContractPricesManage,
        "organization.contract_price.create",
        "contract_price",
        "new".to_owned(),
        request.map_err(|_| ManagementError::InvalidRequest),
    )
    .await?;
    let command = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &request_id,
        &headers,
        OrganizationPermission::ContractPricesManage,
        "organization.contract_price.create",
        "contract_price",
        "new".to_owned(),
        into_create_command(request),
    )
    .await?;
    let record = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &request_id,
        &headers,
        OrganizationPermission::ContractPricesManage,
        "organization.contract_price.create",
        "contract_price",
        "new".to_owned(),
        state
            .service
            .create(context.clone(), command)
            .await
            .map_err(map_error),
    )
    .await?;
    record_organization_action(
        &state.activity_service,
        context,
        &request_id,
        &headers,
        OrganizationPermission::ContractPricesManage.code(),
        "organization.contract_price.create",
        "contract_price",
        record.id().get().to_string(),
        af_admin::OrganizationAuditOutcome::Succeeded,
        Some(json!({"protocol": record.protocol().as_str(), "version": record.version()})),
    )
    .await?;
    Ok(no_store_status_json(
        StatusCode::CREATED,
        ContractPriceResponse::from_record(&record)?,
    ))
}

async fn close_contract_price(
    State(state): State<OrganizationContractPriceHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(request_id): Extension<RequestId>,
    Path((organization_id, price_id)): Path<(i64, i64)>,
    headers: HeaderMap,
    request: Result<Json<ContractPriceCloseRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let organization_id =
        OrganizationId::new(organization_id).map_err(|_| ManagementError::InvalidRequest)?;
    let price_id =
        OrganizationContractPriceId::new(price_id).map_err(|_| ManagementError::InvalidRequest)?;
    let context = authorize_organization_action(
        &state.authorization_service,
        &state.activity_service,
        &authentication,
        organization_id,
        OrganizationPermission::ContractPricesManage,
        &request_id,
        &headers,
        "organization.contract_price.close",
        "contract_price",
        price_id.get().to_string(),
    )
    .await?;
    let Json(request) = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &request_id,
        &headers,
        OrganizationPermission::ContractPricesManage,
        "organization.contract_price.close",
        "contract_price",
        price_id.get().to_string(),
        request.map_err(|_| ManagementError::InvalidRequest),
    )
    .await?;
    let command = OrganizationContractPriceCloseCommand::new(
        price_id,
        request.effective_until,
        request.expected_version,
    )
    .map_err(|_| ManagementError::InvalidRequest)?;
    let record = audit_organization_action_result(
        &state.activity_service,
        context.clone(),
        &request_id,
        &headers,
        OrganizationPermission::ContractPricesManage,
        "organization.contract_price.close",
        "contract_price",
        price_id.get().to_string(),
        state
            .service
            .close(context.clone(), command)
            .await
            .map_err(map_error),
    )
    .await?;
    record_organization_action(&state.activity_service, context, &request_id, &headers, OrganizationPermission::ContractPricesManage.code(), "organization.contract_price.close", "contract_price", price_id.get().to_string(), af_admin::OrganizationAuditOutcome::Succeeded, Some(json!({"effective_until": record.effective_until().map(|value| value.unix_timestamp()), "version": record.version()}))).await?;
    Ok(no_store_status_json(
        StatusCode::OK,
        ContractPriceResponse::from_record(&record)?,
    ))
}

fn into_create_command(
    request: ContractPriceCreateRequest,
) -> Result<OrganizationContractPriceCreateCommand, ManagementError> {
    let prices = request.prices.into_prices()?;
    OrganizationContractPriceCreateCommand::new(
        request.model,
        request.protocol.into(),
        prices,
        request.effective_from,
        request.effective_until,
        request.expected_version,
    )
    .map_err(|_| ManagementError::InvalidRequest)
}

impl ContractPriceResponse {
    fn from_record(record: &OrganizationContractPriceRecord) -> Result<Self, ManagementError> {
        Ok(Self {
            id: record.id().get(),
            organization_id: record.organization_id().get(),
            model: record.model().to_owned(),
            protocol: record.protocol().into(),
            prices: ContractPriceValues::from_prices(record.prices()),
            effective_from: record.effective_from().unix_timestamp(),
            effective_until: record.effective_until().map(|value| value.unix_timestamp()),
            version: record.version(),
            created_by_user_id: record.created_by_user_id().get(),
            created_at: record.created_at().unix_timestamp(),
            updated_at: record.updated_at().unix_timestamp(),
        })
    }
}

async fn authorize(
    state: &OrganizationContractPriceHttpState,
    authentication: &SessionAuthentication,
    organization_id: i64,
    permission: OrganizationPermission,
) -> Result<af_admin::OrganizationContext, ManagementError> {
    let organization_id =
        OrganizationId::new(organization_id).map_err(|_| ManagementError::InvalidRequest)?;
    state
        .authorization_service
        .context(
            authentication.principal(),
            organization_id,
            OrganizationPermission::OverviewRead,
        )
        .await
        .map_err(map_authorization_error)
        .and_then(|context| {
            context
                .allows(permission)
                .then_some(context)
                .ok_or(ManagementError::Forbidden)
        })
}

fn parse_decimal(value: &str) -> Result<Decimal, ManagementError> {
    let mut parts = value.split('.');
    let integer = parts.next().unwrap_or_default();
    let fraction = parts.next();
    if value.is_empty()
        || value.len() > 39
        || parts.next().is_some()
        || integer.is_empty()
        || integer.len() > 10
        || (integer.len() > 1 && integer.starts_with('0'))
        || !integer.bytes().all(|byte| byte.is_ascii_digit())
        || fraction.is_some_and(|part| {
            part.is_empty() || part.len() > 28 || !part.bytes().all(|byte| byte.is_ascii_digit())
        })
    {
        return Err(ManagementError::InvalidRequest);
    }
    let decimal = Decimal::from_str(value).map_err(|_| ManagementError::InvalidRequest)?;
    if decimal.is_sign_negative() {
        return Err(ManagementError::InvalidRequest);
    }
    Ok(decimal)
}

fn decimal_text(value: Decimal) -> String {
    value.normalize().to_string()
}
fn map_authorization_error(error: OrganizationAuthorizationError) -> ManagementError {
    match error {
        OrganizationAuthorizationError::NotFound => ManagementError::OrganizationNotFound,
        OrganizationAuthorizationError::Forbidden => ManagementError::Forbidden,
        OrganizationAuthorizationError::Internal => ManagementError::Internal,
    }
}
fn map_error(error: OrganizationContractPriceServiceError) -> ManagementError {
    match error {
        OrganizationContractPriceServiceError::InvalidRequest => ManagementError::InvalidRequest,
        OrganizationContractPriceServiceError::Forbidden => ManagementError::Forbidden,
        OrganizationContractPriceServiceError::NotFound => {
            ManagementError::OrganizationContractPriceNotFound
        }
        OrganizationContractPriceServiceError::IntervalConflict => {
            ManagementError::OrganizationContractPriceIntervalConflict
        }
        OrganizationContractPriceServiceError::Conflict => {
            ManagementError::OrganizationContractPriceConflict
        }
        OrganizationContractPriceServiceError::InvalidTransition => {
            ManagementError::OrganizationContractPriceTransitionInvalid
        }
        OrganizationContractPriceServiceError::Internal => ManagementError::Internal,
    }
}
fn no_store_status_json(status: StatusCode, value: impl Serialize) -> Response {
    let mut response = (status, Json(value)).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn decimal_values_reject_float_shape_and_keep_exact_text() {
        assert!(parse_decimal("01.2").is_err());
        assert_eq!(
            decimal_text(parse_decimal("0.0000000000000000000000000001").unwrap()),
            "0.0000000000000000000000000001"
        );
        assert_eq!(
            serde_json::to_value(ContractPriceProtocol::Anthropic).unwrap(),
            json!("anthropic")
        );
    }
}
