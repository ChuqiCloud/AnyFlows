use super::*;

impl MerchantPoolRepository {
    pub async fn quote(
        &self,
        owner: UserId,
        write: af_domain::MerchantQuoteWrite,
    ) -> Result<af_domain::MerchantQuote, MerchantPoolError> {
        self.bounded(async {
            let merchant = require_merchant(self.pool.connection(), owner, true).await?;
            let pool = find_pool(self.pool.connection(), write.pool_id).await?;
            if !pool.enabled
                || write.models.is_empty()
                || write.models.len() > 128
                || write.default_ratio_micros <= 0
            {
                return Err(MerchantPoolError::InvalidInput);
            }
            let commission =
                super::terms::resolve_commission(self.pool.connection(), &pool, Some(merchant.id))
                    .await?;
            let mut models = Vec::with_capacity(write.models.len());
            let mut names = std::collections::BTreeSet::new();
            for model in write.models {
                if !af_domain::validate_merchant_text(&model.model, 255, false)
                    || !names.insert(model.model.clone())
                {
                    return Err(MerchantPoolError::InvalidInput);
                }
                let ratio = model.ratio_micros.unwrap_or(write.default_ratio_micros);
                if !(pool.min_ratio_micros..=pool.max_ratio_micros).contains(&ratio) {
                    return Err(MerchantPoolError::InvalidInput);
                }
                let base_prices = model_price(self.pool.connection(), &model.model, 1_000_000)
                    .await?
                    .ok_or(MerchantPoolError::InvalidInput)?;
                let prices = model_price(self.pool.connection(), &model.model, ratio)
                    .await?
                    .ok_or(MerchantPoolError::InvalidInput)?;
                let net_prices = net_prices(&prices, commission)?;
                models.push(af_domain::MerchantQuoteModel {
                    model: model.model,
                    sales_ratio_micros: ratio,
                    base_prices,
                    prices,
                    net_prices,
                });
            }
            Ok(af_domain::MerchantQuote {
                models,
                effective_commission_bps: commission,
            })
        })
        .await
    }

    pub async fn available_supply_pools(
        &self,
        owner: UserId,
        query: MerchantListQuery,
    ) -> Result<MerchantPage<MerchantPool>, MerchantPoolError> {
        let merchant = require_merchant(self.pool.connection(), owner, false).await?;
        let rows = pools::Entity::find()
            .filter(pools::Column::Enabled.eq(true))
            .filter(pools::Column::DeletedAt.is_null())
            .order_by_asc(pools::Column::Id)
            .limit(1001)
            .all(self.pool.connection())
            .await
            .map_err(internal)?;
        if rows.len() > 1000 {
            return Err(MerchantPoolError::LimitReached);
        }
        let mut result = Vec::new();
        for row in rows {
            result.push(self.effective_pool(row, Some(merchant.id)).await?);
        }
        page_items(result, &query)
    }

    /// This is an execution capability, not a substitute for caller pool authorization.
    pub async fn published_candidates(
        &self,
        pool_id: i64,
        merchant_id: Option<i64>,
        model: &str,
    ) -> Result<Vec<MerchantRuntimeCandidate>, MerchantPoolError> {
        self.bounded(async {
            let pool = find_pool(self.pool.connection(), pool_id).await?;
            if !pool.enabled {
                return Ok(Vec::new());
            }
            let mut selection = services::Entity::find()
                .filter(services::Column::PoolId.eq(pool_id))
                .filter(services::Column::DeletedAt.is_null())
                .filter(services::Column::Status.eq("published"));
            if let Some(id) = merchant_id {
                selection = selection.filter(services::Column::MerchantId.eq(id));
            }
            let rows = selection
                .order_by_asc(services::Column::Id)
                .limit(10_001)
                .all(self.pool.connection())
                .await
                .map_err(internal)?;
            if rows.len() > 10_000 {
                return Err(MerchantPoolError::LimitReached);
            }
            let mut candidates = Vec::new();
            for row in rows {
                self.apply_due_service_prices(row.id, DatabaseTimestamp::now_utc())
                    .await?;
                let maintenance = super::policies::active_maintenance(
                    self.pool.connection(),
                    row.id,
                    DatabaseTimestamp::now_utc().unix_timestamp(),
                )
                .await?;
                if maintenance.iter().any(|notice| notice.blocks_requests) {
                    continue;
                }
                let Some(revision_id) = row.published_revision_id else {
                    continue;
                };
                let merchant = profiles::Entity::find_by_id(row.merchant_id)
                    .filter(profiles::Column::Status.eq("approved"))
                    .filter(profiles::Column::DeletedAt.is_null())
                    .one(self.pool.connection())
                    .await
                    .map_err(internal)?;
                let Some(merchant) = merchant else { continue };
                match active_owner(
                    self.pool.connection(),
                    UserId::new(merchant.owner_user_id).map_err(internal)?,
                )
                .await
                {
                    Ok(_) => {}
                    Err(MerchantPoolError::Forbidden) => continue,
                    Err(error) => return Err(error),
                }
                let revision = load_revision(self.pool.connection(), revision_id, row.id).await?;
                if revision.status != "published" {
                    continue;
                }
                let Some(price) = super::policies::active_price(
                    self.pool.connection(),
                    row.id,
                    Some(revision_id),
                )
                .await?
                else {
                    continue;
                };
                let Some(model) = super::policies::price_models(&price)?
                    .into_iter()
                    .find(|item| item.model == model)
                else {
                    continue;
                };
                let ratio = model.ratio_micros.unwrap_or(price.default_ratio_micros);
                if !(pool.min_ratio_micros..=pool.max_ratio_micros).contains(&ratio) {
                    continue;
                }
                if model_price(self.pool.connection(), &model.model, ratio)
                    .await?
                    .is_none()
                {
                    continue;
                }
                let channel = channels::Entity::find_by_id(revision.channel_id)
                    .filter(channels::Column::DeletedAt.is_null())
                    .filter(channels::Column::Status.eq(Status::Enabled.code()))
                    .one(self.pool.connection())
                    .await
                    .map_err(internal)?;
                if channel.is_none() {
                    continue;
                }
                candidates.push(MerchantRuntimeCandidate {
                    pool_id,
                    group_id: pool.group_id,
                    merchant_id: row.merchant_id,
                    service_id: row.id,
                    revision_id,
                    price_version: price.id,
                    channel_id: revision.channel_id,
                    sales_ratio_micros: ratio,
                    commission_bps: super::terms::resolve_commission(
                        self.pool.connection(),
                        &pool,
                        Some(row.merchant_id),
                    )
                    .await?,
                    settlement_delay_secs: pool
                        .settlement_delay_secs
                        .try_into()
                        .map_err(internal)?,
                    protocol: revision.protocol,
                    model,
                });
            }
            Ok(candidates)
        })
        .await
    }

    pub async fn catalog(
        &self,
        owner: UserId,
        pool_id: i64,
        query: MerchantListQuery,
    ) -> Result<MerchantPage<MerchantCatalogService>, MerchantPoolError> {
        if !self.check_pool_access(owner, pool_id).await? {
            return Err(MerchantPoolError::NotFound);
        }
        self.bounded(async {
            pagination(&query)?;
            let rows = services::Entity::find()
                .filter(services::Column::PoolId.eq(pool_id))
                .filter(services::Column::DeletedAt.is_null())
                .filter(services::Column::PublishedRevisionId.is_not_null())
                .order_by_asc(services::Column::Id)
                .limit(10_001)
                .all(self.pool.connection())
                .await
                .map_err(internal)?;
            if rows.len() > 10_000 {
                return Err(MerchantPoolError::LimitReached);
            }
            let pool = find_pool(self.pool.connection(), pool_id).await?;
            let mut items = Vec::new();
            for row in rows {
                self.apply_due_service_prices(row.id, DatabaseTimestamp::now_utc())
                    .await?;
                let merchant = profiles::Entity::find_by_id(row.merchant_id)
                    .filter(profiles::Column::DeletedAt.is_null())
                    .one(self.pool.connection())
                    .await
                    .map_err(internal)?
                    .ok_or(MerchantPoolError::Unavailable)?;
                if let Some(search) = &query.search {
                    let needle = search.to_lowercase();
                    if !row.name.to_lowercase().contains(&needle)
                        && !merchant.name.to_lowercase().contains(&needle)
                    {
                        continue;
                    }
                }
                let Some(id) = row.published_revision_id else {
                    continue;
                };
                let revision = load_revision(self.pool.connection(), id, row.id).await?;
                let Some(price) =
                    super::policies::active_price(self.pool.connection(), row.id, Some(id)).await?
                else {
                    continue;
                };
                let maintenance = super::policies::active_maintenance(
                    self.pool.connection(),
                    row.id,
                    DatabaseTimestamp::now_utc().unix_timestamp(),
                )
                .await?;
                if query
                    .protocol
                    .as_ref()
                    .is_some_and(|protocol| protocol != &revision.protocol)
                {
                    continue;
                }
                let mut models = Vec::new();
                for model in super::policies::price_models(&price)? {
                    if query
                        .model
                        .as_ref()
                        .is_some_and(|name| name != &model.model)
                    {
                        continue;
                    }
                    let ratio = model.ratio_micros.unwrap_or(price.default_ratio_micros);
                    if query.min_ratio_micros.is_some_and(|min| ratio < min)
                        || query.max_ratio_micros.is_some_and(|max| ratio > max)
                    {
                        continue;
                    }
                    let capable = match query.capability.as_deref() {
                        Some("streaming") => model.supports_streaming,
                        Some("tools") => model.supports_tools,
                        Some("vision") => model.supports_vision,
                        _ => true,
                    };
                    if !capable {
                        continue;
                    }
                    models.push(MerchantModelOffer {
                        prices: model_price(self.pool.connection(), &model.model, ratio).await?,
                        model: model.model,
                        sales_ratio_micros: ratio,
                        context_window: model.context_window,
                        max_output_tokens: model.max_output_tokens,
                        supports_streaming: model.supports_streaming,
                        supports_tools: model.supports_tools,
                        supports_vision: model.supports_vision,
                    });
                }
                if models.is_empty() {
                    continue;
                }
                let merchant_active = match active_owner(
                    self.pool.connection(),
                    UserId::new(merchant.owner_user_id).map_err(internal)?,
                )
                .await
                {
                    Ok(_) => true,
                    Err(MerchantPoolError::Forbidden) => false,
                    Err(error) => return Err(error),
                };
                let channel_active = channels::Entity::find_by_id(revision.channel_id)
                    .filter(channels::Column::Status.eq(Status::Enabled.code()))
                    .filter(channels::Column::DeletedAt.is_null())
                    .one(self.pool.connection())
                    .await
                    .map_err(internal)?
                    .is_some();
                let status = if merchant.status != "approved"
                    || !merchant_active
                    || row.platform_suspended
                {
                    "suspended".to_owned()
                } else if row.merchant_paused {
                    "paused".to_owned()
                } else if row.status != "published" {
                    row.status
                } else if maintenance.iter().any(|notice| notice.blocks_requests) {
                    "maintenance".to_owned()
                } else if !channel_active
                    || revision.status != "published"
                    || models.iter().any(|m| {
                        m.prices.is_none()
                            || !(pool.min_ratio_micros..=pool.max_ratio_micros)
                                .contains(&m.sales_ratio_micros)
                    })
                {
                    "unavailable".to_owned()
                } else {
                    "published".to_owned()
                };
                if query
                    .status
                    .as_ref()
                    .is_some_and(|filter| filter != &status)
                {
                    continue;
                }
                items.push(MerchantCatalogService {
                    id: row.id,
                    pool_id,
                    merchant_id: merchant.id,
                    merchant_name: merchant.name,
                    name: row.name,
                    description: row.description,
                    status,
                    protocol: revision.protocol,
                    revision: revision.revision,
                    price_version: price.id,
                    maintenance,
                    models,
                    updated_at: revision
                        .updated_at
                        .unix_timestamp()
                        .max(price.updated_at.unix_timestamp()),
                });
            }
            if matches!(query.sort.as_deref(), Some("success_rate" | "latency")) {
                let statistics = crate::UserRouteRepository::new(self.pool.clone(), self.timeout)
                    .map_err(internal)?
                    .merchant_statistics_filtered(
                        pool_id,
                        query.model.as_deref(),
                        query.window_seconds.unwrap_or(86400),
                        query.protocol.as_deref(),
                        query.streaming,
                    )
                    .await
                    .map_err(internal)?;
                let by_service = statistics
                    .into_iter()
                    .map(|row| (row.service_id, row))
                    .collect::<std::collections::HashMap<_, _>>();
                items.sort_by(|left, right| {
                    let l = by_service
                        .get(&left.id)
                        .filter(|row| row.sample_count >= 20);
                    let r = by_service
                        .get(&right.id)
                        .filter(|row| row.sample_count >= 20);
                    let ranked = match (l, r) {
                        (Some(l), Some(r)) => {
                            if query.sort.as_deref() == Some("success_rate") {
                                // Compare a smoothed estimate with integer arithmetic; low samples never lead.
                                let lv = u128::from(l.success_count + 1)
                                    * u128::from(r.sample_count + 2);
                                let rv = u128::from(r.success_count + 1)
                                    * u128::from(l.sample_count + 2);
                                rv.cmp(&lv)
                            } else {
                                let delay = |row: &crate::MerchantServiceStatistics| {
                                    if query.streaming == Some(true) {
                                        row.ttft_p50_ms
                                    } else {
                                        row.latency_p50_ms
                                    }
                                };
                                match (delay(l), delay(r)) {
                                    (Some(l), Some(r)) => l.cmp(&r),
                                    (Some(_), None) => std::cmp::Ordering::Less,
                                    (None, Some(_)) => std::cmp::Ordering::Greater,
                                    _ => std::cmp::Ordering::Equal,
                                }
                            }
                        }
                        (Some(_), None) => std::cmp::Ordering::Less,
                        (None, Some(_)) => std::cmp::Ordering::Greater,
                        _ => std::cmp::Ordering::Equal,
                    };
                    ranked.then_with(|| left.id.cmp(&right.id))
                });
            } else if query.sort.as_deref() == Some("price") && query.model.is_some() {
                items.sort_by_key(|row| {
                    (
                        row.models
                            .first()
                            .map(|m| m.sales_ratio_micros)
                            .unwrap_or(i64::MAX),
                        row.id,
                    )
                });
            } else {
                items.sort_by(|a, b| {
                    a.merchant_name
                        .cmp(&b.merchant_name)
                        .then_with(|| a.name.cmp(&b.name))
                        .then_with(|| a.id.cmp(&b.id))
                });
            }
            page_items(items, &query)
        })
        .await
    }

    pub async fn catalog_service(
        &self,
        owner: UserId,
        pool_id: i64,
        service_id: i64,
    ) -> Result<MerchantCatalogService, MerchantPoolError> {
        if !self.check_pool_access(owner, pool_id).await? {
            return Err(MerchantPoolError::NotFound);
        }
        let row = find_service(self.pool.connection(), service_id).await?;
        if row.pool_id != pool_id || row.published_revision_id.is_none() {
            return Err(MerchantPoolError::NotFound);
        }
        // Search is not used; filter and paginate through the stable catalog projection.
        let query = MerchantListQuery {
            page_size: Some(100),
            ..Default::default()
        };
        let mut page = 1;
        loop {
            let response = self
                .catalog(
                    owner,
                    pool_id,
                    MerchantListQuery {
                        page: Some(page),
                        ..query.clone()
                    },
                )
                .await?;
            if let Some(item) = response
                .items
                .into_iter()
                .find(|item| item.id == service_id)
            {
                return Ok(item);
            }
            if u64::from(page) * 100 >= response.total {
                return Err(MerchantPoolError::NotFound);
            }
            page += 1;
        }
    }
}

fn net_prices(
    prices: &MerchantModelPrices,
    commission: u16,
) -> Result<MerchantModelPrices, MerchantPoolError> {
    let net_bps = 10_000_u16
        .checked_sub(commission)
        .ok_or(MerchantPoolError::Unavailable)?;
    let ratio = Decimal::from(net_bps)
        .checked_div(Decimal::from(10_000))
        .ok_or(MerchantPoolError::Unavailable)?;
    let net = |value: &str| -> Result<String, MerchantPoolError> {
        value
            .parse::<Decimal>()
            .map_err(internal)?
            .checked_mul(ratio)
            .map(|value| value.normalize().to_string())
            .ok_or(MerchantPoolError::Unavailable)
    };
    Ok(MerchantModelPrices {
        currency: prices.currency.clone(),
        unit: prices.unit.clone(),
        billing_mode: prices.billing_mode.clone(),
        video_per_second: prices
            .video_per_second
            .as_ref()
            .map(|rates| {
                Ok(af_domain::MerchantVideoPrices {
                    p480: net(&rates.p480)?,
                    p720: net(&rates.p720)?,
                    p1080: rates.p1080.as_deref().map(net).transpose()?,
                })
            })
            .transpose()?,
        input: net(&prices.input)?,
        output: net(&prices.output)?,
        cache_read: net(&prices.cache_read)?,
        cache_write_5m: net(&prices.cache_write_5m)?,
        cache_write_1h: net(&prices.cache_write_1h)?,
        version: prices.version,
    })
}

pub(super) async fn model_price<C: ConnectionTrait>(
    connection: &C,
    model: &str,
    ratio: i64,
) -> Result<Option<MerchantModelPrices>, MerchantPoolError> {
    if model == "grok-imagine-video"
        || model == "grok-imagine-video-1.5"
        || model.starts_with("grok-imagine-video-1.5-")
    {
        let ratio = Decimal::from(ratio)
            .checked_div(Decimal::from(1_000_000))
            .ok_or(MerchantPoolError::Unavailable)?;
        let scale = |micros| {
            Decimal::new(micros, 6)
                .checked_mul(ratio)
                .map(|n| n.normalize().to_string())
                .ok_or(MerchantPoolError::Unavailable)
        };
        let v15 = model != "grok-imagine-video";
        return Ok(Some(MerchantModelPrices {
            currency: "USD".to_owned(),
            unit: "second".to_owned(),
            billing_mode: "video".to_owned(),
            video_per_second: Some(af_domain::MerchantVideoPrices {
                p480: scale(if v15 { 80_000 } else { 50_000 })?,
                p720: scale(if v15 { 140_000 } else { 70_000 })?,
                p1080: if v15 { Some(scale(250_000)?) } else { None },
            }),
            input: "0".to_owned(),
            output: "0".to_owned(),
            cache_read: "0".to_owned(),
            cache_write_5m: "0".to_owned(),
            cache_write_1h: "0".to_owned(),
            version: 1,
        }));
    }
    let Some(price) = model_prices::Entity::find_by_id(SensitiveString::from(model))
        .one(connection)
        .await
        .map_err(internal)?
    else {
        return Ok(None);
    };
    // Media uses the same platform-normalized, deterministic token tariff.
    if !matches!(price.billing_mode, 1 | 2) || price.billing_expression.is_some() {
        return Ok(None);
    }
    let multiplier = Decimal::from(ratio)
        .checked_div(Decimal::from(1_000_000))
        .ok_or(MerchantPoolError::Unavailable)?;
    let final_price = |base: Decimal| -> Result<String, MerchantPoolError> {
        base.checked_mul(multiplier)
            .map(|value| value.normalize().to_string())
            .ok_or(MerchantPoolError::Unavailable)
    };
    Ok(Some(MerchantModelPrices {
        currency: "USD".to_owned(),
        unit: "million_tokens".to_owned(),
        billing_mode: if price.billing_mode == 2 {
            "free"
        } else {
            "token"
        }
        .to_owned(),
        video_per_second: None,
        input: final_price(price.input_price.expose())?,
        output: final_price(price.output_price.expose())?,
        cache_read: final_price(price.cache_read_price.expose())?,
        cache_write_5m: final_price(price.cache_creation_5m_price.expose())?,
        cache_write_1h: final_price(price.cache_creation_1h_price.expose())?,
        version: price.version,
    }))
}
