use af_admin::{
    AdminModelPrice, AdminModelPriceApplyCommand, AdminModelPriceBillingMode, AdminModelPriceError,
    AdminModelPriceListQuery, AdminModelPricePage, AdminModelPriceWriteCommand,
    ModelPriceSourceCandidate, ModelPriceSourcePreview,
};
use axum::{
    Json,
    extract::{Extension, RawQuery, State, rejection::JsonRejection},
    response::Response,
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    chat_completions::HttpState, management_error::ManagementError,
    management_models::no_store_json,
};

const MAX_MODEL_PRICE_QUERY_BYTES: usize = 1_024;
const MAX_DECIMAL_TEXT_BYTES: usize = 39;

/// HTTP 契约支持的闭合模型计费模式。
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = AdminModelPriceBillingMode)]
pub(crate) enum AdminModelPriceBillingModeValue {
    PerToken,
    Free,
}

impl From<AdminModelPriceBillingMode> for AdminModelPriceBillingModeValue {
    fn from(value: AdminModelPriceBillingMode) -> Self {
        match value {
            AdminModelPriceBillingMode::PerToken => Self::PerToken,
            AdminModelPriceBillingMode::Free => Self::Free,
        }
    }
}

impl From<AdminModelPriceBillingModeValue> for AdminModelPriceBillingMode {
    fn from(value: AdminModelPriceBillingModeValue) -> Self {
        match value {
            AdminModelPriceBillingModeValue::PerToken => Self::PerToken,
            AdminModelPriceBillingModeValue::Free => Self::Free,
        }
    }
}

/// 五类美元/百万 Token 单价；字符串边界避免 JSON 浮点数破坏精度。
#[derive(Deserialize, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminModelPriceValues)]
pub(crate) struct AdminModelPriceValues {
    #[schema(value_type = String, min_length = 1, max_length = 39, pattern = r"^(0|[1-9][0-9]{0,9})(\.[0-9]{1,28})?$")]
    input: String,
    #[schema(value_type = String, min_length = 1, max_length = 39, pattern = r"^(0|[1-9][0-9]{0,9})(\.[0-9]{1,28})?$")]
    output: String,
    #[schema(value_type = String, min_length = 1, max_length = 39, pattern = r"^(0|[1-9][0-9]{0,9})(\.[0-9]{1,28})?$")]
    cache_read: String,
    #[schema(value_type = String, min_length = 1, max_length = 39, pattern = r"^(0|[1-9][0-9]{0,9})(\.[0-9]{1,28})?$")]
    cache_creation_5m: String,
    #[schema(value_type = String, min_length = 1, max_length = 39, pattern = r"^(0|[1-9][0-9]{0,9})(\.[0-9]{1,28})?$")]
    cache_creation_1h: String,
}

impl AdminModelPriceValues {
    fn from_prices(prices: [Decimal; 5]) -> Self {
        let [
            input,
            output,
            cache_read,
            cache_creation_5m,
            cache_creation_1h,
        ] = prices;
        Self {
            input: decimal_text(input),
            output: decimal_text(output),
            cache_read: decimal_text(cache_read),
            cache_creation_5m: decimal_text(cache_creation_5m),
            cache_creation_1h: decimal_text(cache_creation_1h),
        }
    }

    fn into_prices(self) -> Result<[Decimal; 5], ManagementError> {
        Ok([
            parse_decimal(&self.input)?,
            parse_decimal(&self.output)?,
            parse_decimal(&self.cache_read)?,
            parse_decimal(&self.cache_creation_5m)?,
            parse_decimal(&self.cache_creation_1h)?,
        ])
    }
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminModelPrice)]
pub(crate) struct AdminModelPriceResponse {
    #[schema(min_length = 1, max_length = 256)]
    model: String,
    billing_mode: AdminModelPriceBillingModeValue,
    prices: AdminModelPriceValues,
    #[schema(minimum = 1)]
    version: u64,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminModelPriceListResponse)]
pub(crate) struct AdminModelPriceListResponse {
    #[schema(max_items = 100)]
    prices: Vec<AdminModelPriceResponse>,
    #[schema(min_length = 1, max_length = 256, required = true)]
    next_cursor: Option<String>,
}

/// models.dev 公开来源返回的可选成本证据。
#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminModelPriceSourceCosts)]
pub(crate) struct AdminModelPriceSourceCostsResponse {
    #[schema(value_type = Option<String>, max_length = 64, required = true)]
    input: Option<String>,
    #[schema(value_type = Option<String>, max_length = 64, required = true)]
    output: Option<String>,
    #[schema(value_type = Option<String>, max_length = 64, required = true)]
    cache_read: Option<String>,
    /// 通用缓存写价格只作为证据，不能映射为正式 5m 或 1h 单价。
    #[schema(value_type = Option<String>, max_length = 64, required = true)]
    cache_write: Option<String>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminModelPriceSourceCandidate)]
pub(crate) struct AdminModelPriceSourceCandidateResponse {
    #[schema(min_length = 1, max_length = 256)]
    model: String,
    #[schema(min_length = 1, max_length = 64)]
    provider: String,
    #[schema(min_length = 1, max_length = 128)]
    provider_name: String,
    #[schema(min_length = 1, max_length = 256)]
    source_model: String,
    #[schema(min_length = 1, max_length = 128)]
    source_name: String,
    #[schema(max_length = 32, required = true)]
    last_updated: Option<String>,
    costs: AdminModelPriceSourceCostsResponse,
    has_tiered_pricing: bool,
    source_deprecated: bool,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminModelPriceSourcePreview)]
pub(crate) struct AdminModelPriceSourcePreviewResponse {
    #[schema(value_type = String, pattern = "^models_dev$")]
    source: &'static str,
    #[schema(minimum = 1)]
    fetched_at: i64,
    #[schema(max_length = 128, required = true)]
    revision: Option<String>,
    #[schema(max_items = 10000)]
    candidates: Vec<AdminModelPriceSourceCandidateResponse>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminModelPriceWriteItem)]
pub(crate) struct AdminModelPriceWriteItemRequest {
    #[schema(min_length = 1, max_length = 256)]
    model: String,
    #[schema(minimum = 1, required = true)]
    expected_version: Option<u64>,
    billing_mode: AdminModelPriceBillingModeValue,
    prices: AdminModelPriceValues,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminModelPriceBatchRequest)]
pub(crate) struct AdminModelPriceBatchRequest {
    #[schema(min_items = 1, max_items = 100)]
    items: Vec<AdminModelPriceWriteItemRequest>,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminModelPriceBatchResponse)]
pub(crate) struct AdminModelPriceBatchResponse {
    #[schema(min_items = 1, max_items = 100)]
    prices: Vec<AdminModelPriceResponse>,
}

/// 按 Canonical 稳定游标返回正式模型价格。
pub(crate) async fn list_admin_model_prices(
    State(state): State<HttpState>,
    RawQuery(raw_query): RawQuery,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let service = state
        .admin_model_price_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let page = service
        .list(
            authentication.principal(),
            parse_list_query(raw_query.as_deref())?,
        )
        .await
        .map_err(map_price_error)?;
    Ok(no_store_json(AdminModelPriceListResponse::from_page(page)))
}

/// 从固定 models.dev 端点读取与本地权威身份精确匹配的参考价。
pub(crate) async fn preview_admin_model_prices(
    State(state): State<HttpState>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let service = state
        .admin_model_price_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let preview = service
        .preview(authentication.principal())
        .await
        .map_err(map_price_error)?;
    Ok(no_store_json(
        AdminModelPriceSourcePreviewResponse::from_preview(preview),
    ))
}

/// 原子创建或更新管理员明确确认的一批正式模型价格。
pub(crate) async fn apply_admin_model_prices(
    State(state): State<HttpState>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
    request: Result<Json<AdminModelPriceBatchRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let request = request.map_err(|_| ManagementError::InvalidRequest)?.0;
    let command = AdminModelPriceApplyCommand::new(
        request
            .items
            .into_iter()
            .map(AdminModelPriceWriteItemRequest::into_command)
            .collect::<Result<Vec<_>, _>>()?,
    )
    .map_err(map_price_error)?;
    let service = state
        .admin_model_price_service
        .as_ref()
        .ok_or(ManagementError::Internal)?;
    let prices = service
        .apply(authentication.principal(), command)
        .await
        .map_err(map_price_error)?;
    Ok(no_store_json(AdminModelPriceBatchResponse {
        prices: prices
            .iter()
            .map(AdminModelPriceResponse::from_price)
            .collect(),
    }))
}

impl AdminModelPriceResponse {
    fn from_price(price: &AdminModelPrice) -> Self {
        Self {
            model: price.model().to_owned(),
            billing_mode: price.billing_mode().into(),
            prices: AdminModelPriceValues::from_prices(price.prices()),
            version: price.version(),
        }
    }
}

impl AdminModelPriceListResponse {
    fn from_page(page: AdminModelPricePage) -> Self {
        Self {
            prices: page
                .prices()
                .iter()
                .map(AdminModelPriceResponse::from_price)
                .collect(),
            next_cursor: page.next_cursor().map(str::to_owned),
        }
    }
}

impl AdminModelPriceSourcePreviewResponse {
    fn from_preview(preview: ModelPriceSourcePreview) -> Self {
        Self {
            source: preview.source(),
            fetched_at: preview.fetched_at(),
            revision: preview.revision().map(str::to_owned),
            candidates: preview
                .candidates()
                .iter()
                .map(AdminModelPriceSourceCandidateResponse::from_candidate)
                .collect(),
        }
    }
}

impl AdminModelPriceSourceCandidateResponse {
    fn from_candidate(candidate: &ModelPriceSourceCandidate) -> Self {
        let [input, output, cache_read, cache_write] = candidate.costs();
        Self {
            model: candidate.model().to_owned(),
            provider: candidate.provider().to_owned(),
            provider_name: candidate.provider_name().to_owned(),
            source_model: candidate.source_model().to_owned(),
            source_name: candidate.source_name().to_owned(),
            last_updated: candidate.last_updated().map(str::to_owned),
            costs: AdminModelPriceSourceCostsResponse {
                input: input.map(decimal_text),
                output: output.map(decimal_text),
                cache_read: cache_read.map(decimal_text),
                cache_write: cache_write.map(decimal_text),
            },
            has_tiered_pricing: candidate.has_tiered_pricing(),
            source_deprecated: candidate.source_deprecated(),
        }
    }
}

impl AdminModelPriceWriteItemRequest {
    fn into_command(self) -> Result<AdminModelPriceWriteCommand, ManagementError> {
        AdminModelPriceWriteCommand::new(
            self.model,
            self.expected_version,
            self.billing_mode.into(),
            self.prices.into_prices()?,
        )
        .map_err(map_price_error)
    }
}

fn parse_list_query(raw_query: Option<&str>) -> Result<AdminModelPriceListQuery, ManagementError> {
    let Some(raw_query) = raw_query else {
        return Ok(AdminModelPriceListQuery::default());
    };
    if raw_query.is_empty() {
        return Ok(AdminModelPriceListQuery::default());
    }
    if raw_query.len() > MAX_MODEL_PRICE_QUERY_BYTES {
        return Err(ManagementError::InvalidRequest);
    }
    validate_percent_encoding(raw_query)?;
    let mut after = None;
    let mut limit = None;
    for (key, value) in url::form_urlencoded::parse(raw_query.as_bytes()) {
        if key.contains('\u{fffd}') || value.contains('\u{fffd}') {
            return Err(ManagementError::InvalidRequest);
        }
        match key.as_ref() {
            "after" if after.is_none() => after = Some(value.into_owned()),
            "limit" if limit.is_none() => limit = Some(parse_limit(&value)?),
            _ => return Err(ManagementError::InvalidRequest),
        }
    }
    AdminModelPriceListQuery::new(
        after,
        limit.unwrap_or(af_admin::DEFAULT_ADMIN_MODEL_PRICE_PAGE_SIZE),
    )
    .map_err(map_price_error)
}

fn parse_limit(value: &str) -> Result<usize, ManagementError> {
    if value.is_empty()
        || value.starts_with(['+', '-'])
        || value.chars().any(|character| !character.is_ascii_digit())
    {
        return Err(ManagementError::InvalidRequest);
    }
    value
        .parse::<usize>()
        .map_err(|_| ManagementError::InvalidRequest)
}

fn validate_percent_encoding(raw_query: &str) -> Result<(), ManagementError> {
    let bytes = raw_query.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len()
                || !bytes[index + 1].is_ascii_hexdigit()
                || !bytes[index + 2].is_ascii_hexdigit()
            {
                return Err(ManagementError::InvalidRequest);
            }
            index += 3;
        } else {
            index += 1;
        }
    }
    Ok(())
}

fn parse_decimal(value: &str) -> Result<Decimal, ManagementError> {
    let mut parts = value.split('.');
    let integer = parts.next().unwrap_or_default();
    let fraction = parts.next();
    if value.is_empty()
        || value.len() > MAX_DECIMAL_TEXT_BYTES
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
    Decimal::from_str_exact(value).map_err(|_| ManagementError::InvalidRequest)
}

fn decimal_text(value: Decimal) -> String {
    value.normalize().to_string()
}

fn map_price_error(error: AdminModelPriceError) -> ManagementError {
    match error {
        AdminModelPriceError::InvalidInput => ManagementError::InvalidRequest,
        AdminModelPriceError::Forbidden => ManagementError::Forbidden,
        AdminModelPriceError::ModelNotFound => ManagementError::ModelNotFound,
        AdminModelPriceError::Conflict => ManagementError::ModelPriceConflict,
        AdminModelPriceError::SourceTimeout => ManagementError::ModelPriceSourceTimeout,
        AdminModelPriceError::SourceUnavailable => ManagementError::ModelPriceSourceUnavailable,
        AdminModelPriceError::SourceResponseTooLarge => {
            ManagementError::ModelPriceSourceResponseTooLarge
        }
        AdminModelPriceError::SourceInvalidResponse => {
            ManagementError::ModelPriceSourceInvalidResponse
        }
        AdminModelPriceError::SourceCandidateLimitExceeded => {
            ManagementError::ModelPriceSourceCandidateLimitExceeded
        }
        AdminModelPriceError::RuntimeRefreshFailed | AdminModelPriceError::Internal => {
            ManagementError::Internal
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimal_boundary_accepts_plain_exact_values_only() {
        assert_eq!(
            parse_decimal("1.234500").unwrap().normalize().to_string(),
            "1.2345"
        );
        for value in [
            "",
            "-1",
            "+1",
            ".5",
            "1.",
            "1e-3",
            " 1",
            "1..2",
            "01",
            "10000000000",
            "0.12345678901234567890123456789",
        ] {
            assert!(parse_decimal(value).is_err(), "{value}");
        }
    }

    #[test]
    fn list_query_rejects_duplicates_unknown_keys_and_invalid_cursors() {
        assert_eq!(parse_list_query(None).unwrap().limit(), 50);
        for query in [
            "after=a&after=b",
            "limit=1&limit=2",
            "unknown=1",
            "after=%zz",
            "limit=0",
        ] {
            assert!(parse_list_query(Some(query)).is_err(), "{query}");
        }
    }
}
