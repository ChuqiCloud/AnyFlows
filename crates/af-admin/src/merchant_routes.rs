//! Consumer merchant routing never grants merchant management privileges.
use crate::SessionPrincipal;
pub use af_db::{
    MerchantConsumerAttempt, MerchantRouteRecord, MerchantRouteWrite, MerchantServiceStatistics,
    MerchantServiceTimeline,
};
use af_db::{MerchantPoolRepository, UserRouteError, UserRouteRepository};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::{future::Future, pin::Pin};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MerchantRoutePreviewRequest {
    pub route: MerchantRouteWrite,
    pub model: String,
    pub protocol: String,
    pub streaming: bool,
    pub input_tokens: i64,
    pub output_tokens: i64,
    #[serde(default)]
    pub duration_seconds: Option<u8>,
    #[serde(default)]
    pub resolution: Option<String>,
}
#[derive(Serialize)]
pub struct MerchantRoutePreviewItem {
    pub merchant_id: i64,
    pub merchant_name: String,
    pub service_id: i64,
    pub service_name: String,
    pub sales_ratio_micros: i64,
    pub estimated_quota: Option<i64>,
    pub eligible: bool,
    pub reason: Option<String>,
}
#[derive(Serialize)]
pub struct MerchantRoutePreview {
    pub items: Vec<MerchantRoutePreviewItem>,
    pub quota_per_usd: i64,
    pub as_of: i64,
}

pub type MerchantRouteFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, UserRouteError>> + Send + 'a>>;
pub trait MerchantRouteService: Send + Sync {
    fn preview(
        &self,
        principal: SessionPrincipal,
        request: MerchantRoutePreviewRequest,
    ) -> MerchantRouteFuture<'_, MerchantRoutePreview>;
    fn own_statistics(
        &self,
        principal: SessionPrincipal,
        model: Option<String>,
        window_seconds: u32,
        protocol: Option<String>,
        streaming: Option<bool>,
    ) -> MerchantRouteFuture<'_, Vec<MerchantServiceStatistics>>;
    fn timeline(
        &self,
        principal: SessionPrincipal,
        pool_id: i64,
        model: Option<String>,
        window_seconds: u32,
        protocol: Option<String>,
        streaming: Option<bool>,
    ) -> MerchantRouteFuture<'_, Vec<MerchantServiceTimeline>>;
    fn attempts(
        &self,
        principal: SessionPrincipal,
        request_id: String,
    ) -> MerchantRouteFuture<'_, Vec<MerchantConsumerAttempt>>;
    fn list(
        &self,
        principal: SessionPrincipal,
    ) -> MerchantRouteFuture<'_, Vec<MerchantRouteRecord>>;
    fn get(
        &self,
        principal: SessionPrincipal,
        id: i64,
    ) -> MerchantRouteFuture<'_, MerchantRouteRecord>;
    fn save(
        &self,
        principal: SessionPrincipal,
        id: Option<i64>,
        write: MerchantRouteWrite,
    ) -> MerchantRouteFuture<'_, MerchantRouteRecord>;
    fn delete(&self, principal: SessionPrincipal, id: i64) -> MerchantRouteFuture<'_, ()>;
    fn statistics(
        &self,
        principal: SessionPrincipal,
        pool_id: i64,
        model: Option<String>,
        window_seconds: u32,
        protocol: Option<String>,
        streaming: Option<bool>,
    ) -> MerchantRouteFuture<'_, Vec<MerchantServiceStatistics>>;
}
pub struct DatabaseMerchantRouteService {
    routes: UserRouteRepository,
    pools: MerchantPoolRepository,
}
impl DatabaseMerchantRouteService {
    #[must_use]
    pub const fn new(routes: UserRouteRepository, pools: MerchantPoolRepository) -> Self {
        Self { routes, pools }
    }
}
impl MerchantRouteService for DatabaseMerchantRouteService {
    fn preview(
        &self,
        p: SessionPrincipal,
        request: MerchantRoutePreviewRequest,
    ) -> MerchantRouteFuture<'_, MerchantRoutePreview> {
        Box::pin(async move {
            request.route.validate()?;
            if request.model.is_empty()
                || request.model.len() > 255
                || !matches!(
                    request.protocol.as_str(),
                    "openai_chat"
                        | "openai_responses"
                        | "openai_images"
                        | "openai_audio"
                        | "openai_speech"
                        | "xai_video"
                )
                || request.input_tokens < 0
                || request.output_tokens < 0
                || request.input_tokens > 10_000_000
                || request.output_tokens > 10_000_000
                || request
                    .duration_seconds
                    .is_some_and(|n| !(1..=15).contains(&n))
                || request
                    .resolution
                    .as_deref()
                    .is_some_and(|n| !matches!(n, "480p" | "720p" | "1080p"))
            {
                return Err(UserRouteError::InvalidInput);
            }
            let mut page = 1;
            let mut services = Vec::new();
            loop {
                let list = self
                    .pools
                    .catalog(
                        p.user_id(),
                        request.route.pool_id,
                        af_domain::MerchantListQuery {
                            page: Some(page),
                            page_size: Some(100),
                            ..Default::default()
                        },
                    )
                    .await
                    .map_err(|_| UserRouteError::Unavailable)?;
                services.extend(list.items);
                if u64::from(page) * 100 >= list.total {
                    break;
                }
                page += 1;
            }
            let stats = self
                .routes
                .merchant_statistics_filtered(
                    request.route.pool_id,
                    Some(&request.model),
                    900,
                    Some(&request.protocol),
                    Some(request.streaming),
                )
                .await?;
            let mut rows = Vec::new();
            for service in services {
                let member = request
                    .route
                    .members
                    .iter()
                    .find(|m| m.merchant_id == service.merchant_id);
                let model = service.models.iter().find(|m| m.model == request.model);
                let reason =
                    if request
                        .route
                        .excluded_merchant_ids
                        .contains(&service.merchant_id)
                    {
                        Some("excluded")
                    } else if member.is_none() && !request.route.allow_pool_fallback {
                        Some("not_selected")
                    } else if service.status != "published" {
                        Some("unavailable")
                    } else if service.protocol != request.protocol
                        && !(matches!(
                            service.protocol.as_str(),
                            "openai_chat" | "openai_responses"
                        ) && matches!(
                            request.protocol.as_str(),
                            "openai_chat" | "openai_responses"
                        ))
                    {
                        Some("protocol_unsupported")
                    } else if model.is_none() {
                        Some("model_unavailable")
                    } else if request.streaming && model.is_some_and(|m| !m.supports_streaming) {
                        Some("streaming_unsupported")
                    } else if request.route.max_price_ratio_micros.is_some_and(|maximum| {
                        model.is_some_and(|m| m.sales_ratio_micros > maximum)
                    }) {
                        Some("price_exceeded")
                    } else {
                        None
                    };
                let estimated = if let Some(prices) = model.and_then(|m| m.prices.as_ref()) {
                    let parse = |value: &str| {
                        value
                            .parse::<Decimal>()
                            .map_err(|_| UserRouteError::Internal)
                    };
                    let input = parse(&prices.input)?
                        .checked_mul(Decimal::from(request.input_tokens))
                        .ok_or(UserRouteError::Internal)?;
                    let output = parse(&prices.output)?
                        .checked_mul(Decimal::from(request.output_tokens))
                        .ok_or(UserRouteError::Internal)?;
                    let mut usd = input
                        .checked_add(output)
                        .and_then(|n| n.checked_div(Decimal::from(1_000_000)))
                        .ok_or(UserRouteError::Internal)?;
                    if let Some(rates) = &prices.video_per_second {
                        let rate = match request.resolution.as_deref().unwrap_or("480p") {
                            "480p" => Some(rates.p480.as_str()),
                            "720p" => Some(rates.p720.as_str()),
                            _ => rates.p1080.as_deref(),
                        }
                        .ok_or(UserRouteError::InvalidInput)?;
                        usd = parse(rate)?
                            .checked_mul(Decimal::from(request.duration_seconds.unwrap_or(8)))
                            .ok_or(UserRouteError::Internal)?;
                    }
                    Some(
                        af_billing::quota_math::quota_from_usd(usd)
                            .map_err(|_| UserRouteError::Internal)?
                            .units(),
                    )
                } else {
                    None
                };
                rows.push((
                    member.map_or(i32::MAX, |m| m.priority),
                    member.map_or(1, |m| m.weight),
                    MerchantRoutePreviewItem {
                        merchant_id: service.merchant_id,
                        merchant_name: service.merchant_name,
                        service_id: service.id,
                        service_name: service.name,
                        sales_ratio_micros: model.map_or(0, |m| m.sales_ratio_micros),
                        estimated_quota: estimated,
                        eligible: reason.is_none() && estimated.is_some(),
                        reason: reason.map(str::to_owned),
                    },
                ));
            }
            let mut by_merchant: std::collections::BTreeMap<
                i64,
                (i32, u32, MerchantRoutePreviewItem),
            > = std::collections::BTreeMap::new();
            for row in rows {
                let id = row.2.merchant_id;
                let replace = by_merchant.get(&id).is_none_or(|old| {
                    (!old.2.eligible && row.2.eligible)
                        || (old.2.eligible == row.2.eligible
                            && row.2.estimated_quota.unwrap_or(i64::MAX)
                                < old.2.estimated_quota.unwrap_or(i64::MAX))
                });
                if replace {
                    by_merchant.insert(id, row);
                }
            }
            let mut rows = by_merchant.into_values().collect::<Vec<_>>();
            rows.sort_by(|left, right| {
                (!left.2.eligible)
                    .cmp(&(!right.2.eligible))
                    .then_with(|| match request.route.strategy {
                        af_domain::UserRouteStrategy::PriceFirst => {
                            left.2.estimated_quota.cmp(&right.2.estimated_quota)
                        }
                        af_domain::UserRouteStrategy::StableFirst => {
                            let score = |id| {
                                stats
                                    .iter()
                                    .find(|s| s.service_id == id && s.sample_count >= 20)
                                    .map(|s| (s.success_count + 1, s.sample_count + 2))
                            };
                            match (score(left.2.service_id), score(right.2.service_id)) {
                                (Some((a, b)), Some((c, d))) => (u128::from(c) * u128::from(b))
                                    .cmp(&(u128::from(a) * u128::from(d))),
                                (Some(_), None) => std::cmp::Ordering::Less,
                                (None, Some(_)) => std::cmp::Ordering::Greater,
                                _ => std::cmp::Ordering::Equal,
                            }
                        }
                        af_domain::UserRouteStrategy::Weighted => right.1.cmp(&left.1),
                        _ => left.0.cmp(&right.0),
                    })
                    .then_with(|| left.2.merchant_id.cmp(&right.2.merchant_id))
            });
            Ok(MerchantRoutePreview {
                items: rows.into_iter().map(|(_, _, item)| item).collect(),
                quota_per_usd: af_billing::quota_math::QUOTA_PER_USD,
                as_of: time::OffsetDateTime::now_utc().unix_timestamp(),
            })
        })
    }
    fn own_statistics(
        &self,
        p: SessionPrincipal,
        model: Option<String>,
        window_seconds: u32,
        protocol: Option<String>,
        streaming: Option<bool>,
    ) -> MerchantRouteFuture<'_, Vec<MerchantServiceStatistics>> {
        Box::pin(async move {
            let merchant = self
                .pools
                .owner_merchant(p.user_id())
                .await
                .map_err(|_| UserRouteError::Internal)?
                .ok_or(UserRouteError::NotFound)?;
            let mut page = 1;
            let mut pools = std::collections::BTreeSet::new();
            loop {
                let list = self
                    .pools
                    .list_services(
                        Some(p.user_id()),
                        af_domain::MerchantListQuery {
                            page: Some(page),
                            page_size: Some(100),
                            ..Default::default()
                        },
                    )
                    .await
                    .map_err(|_| UserRouteError::Internal)?;
                for service in list.items {
                    pools.insert(service.pool_id);
                }
                if u64::from(page) * 100 >= list.total {
                    break;
                }
                page += 1;
            }
            let mut result = Vec::new();
            for pool in pools {
                let statistics = self
                    .routes
                    .merchant_statistics_filtered(
                        pool,
                        model.as_deref(),
                        window_seconds,
                        protocol.as_deref(),
                        streaming,
                    )
                    .await?;
                result.extend(
                    statistics
                        .into_iter()
                        .filter(|row| row.merchant_id == merchant.id),
                );
            }
            Ok(result)
        })
    }
    fn timeline(
        &self,
        p: SessionPrincipal,
        pool_id: i64,
        model: Option<String>,
        window_seconds: u32,
        protocol: Option<String>,
        streaming: Option<bool>,
    ) -> MerchantRouteFuture<'_, Vec<MerchantServiceTimeline>> {
        Box::pin(async move {
            if !self
                .pools
                .check_pool_access(p.user_id(), pool_id)
                .await
                .map_err(|_| UserRouteError::Internal)?
            {
                return Err(UserRouteError::NotFound);
            }
            self.routes
                .merchant_timeline(
                    pool_id,
                    model.as_deref(),
                    window_seconds,
                    protocol.as_deref(),
                    streaming,
                )
                .await
        })
    }
    fn attempts(
        &self,
        p: SessionPrincipal,
        request_id: String,
    ) -> MerchantRouteFuture<'_, Vec<MerchantConsumerAttempt>> {
        Box::pin(async move {
            self.routes
                .merchant_attempts(p.user_id(), &request_id)
                .await
        })
    }
    fn list(&self, p: SessionPrincipal) -> MerchantRouteFuture<'_, Vec<MerchantRouteRecord>> {
        Box::pin(async move { self.routes.merchant_routes(p.user_id()).await })
    }
    fn get(&self, p: SessionPrincipal, id: i64) -> MerchantRouteFuture<'_, MerchantRouteRecord> {
        Box::pin(async move { self.routes.merchant_route(p.user_id(), id).await })
    }
    fn save(
        &self,
        p: SessionPrincipal,
        id: Option<i64>,
        write: MerchantRouteWrite,
    ) -> MerchantRouteFuture<'_, MerchantRouteRecord> {
        Box::pin(async move {
            self.routes
                .write_merchant_route(p.user_id(), id, write)
                .await
        })
    }
    fn delete(&self, p: SessionPrincipal, id: i64) -> MerchantRouteFuture<'_, ()> {
        Box::pin(async move {
            self.routes.merchant_route(p.user_id(), id).await?;
            self.routes.delete(p.user_id(), id).await
        })
    }
    fn statistics(
        &self,
        p: SessionPrincipal,
        pool_id: i64,
        model: Option<String>,
        window_seconds: u32,
        protocol: Option<String>,
        streaming: Option<bool>,
    ) -> MerchantRouteFuture<'_, Vec<MerchantServiceStatistics>> {
        Box::pin(async move {
            if !self
                .pools
                .check_pool_access(p.user_id(), pool_id)
                .await
                .map_err(|_| UserRouteError::Internal)?
            {
                return Err(UserRouteError::NotFound);
            }
            self.routes
                .merchant_statistics_filtered(
                    pool_id,
                    model.as_deref(),
                    window_seconds,
                    protocol.as_deref(),
                    streaming,
                )
                .await
        })
    }
}
