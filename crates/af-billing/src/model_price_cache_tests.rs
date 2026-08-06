use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use af_domain::Quota;
use af_protocol::{TokenCount, Usage, UsageDetails, UsageSemantics, UsageSource};
use rust_decimal::Decimal;
use tokio::sync::Notify;

use crate::{
    BillingExpressionDefinition, BillingMode, ModelPriceCache, ModelPriceCacheError,
    ModelPriceMode, ModelPriceSource, ModelPriceSourceError, ModelPriceSourceFuture,
    ModelPriceSourceRecord, PricingContext, PricingRatio, PricingRatios, PricingResolver,
    TokenPrices,
};

type SourceResult = Result<Vec<ModelPriceSourceRecord>, ModelPriceSourceError>;

struct SequenceSource {
    responses: Mutex<VecDeque<SourceResult>>,
    calls: AtomicUsize,
}

impl SequenceSource {
    fn new(responses: impl IntoIterator<Item = SourceResult>) -> Self {
        Self {
            responses: Mutex::new(responses.into_iter().collect()),
            calls: AtomicUsize::new(0),
        }
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::Acquire)
    }
}

impl ModelPriceSource for SequenceSource {
    fn load<'a>(&'a self) -> ModelPriceSourceFuture<'a> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::AcqRel);
            self.responses
                .lock()
                .expect("测试 source 锁不应中毒")
                .pop_front()
                .expect("测试 source 必须预置足够响应")
        })
    }
}

struct BlockingSource {
    records: Vec<ModelPriceSourceRecord>,
    calls: AtomicUsize,
    blocked_call: usize,
    blocked: Notify,
    release: Notify,
}

impl BlockingSource {
    fn new(records: Vec<ModelPriceSourceRecord>, blocked_call: usize) -> Self {
        Self {
            records,
            calls: AtomicUsize::new(0),
            blocked_call,
            blocked: Notify::new(),
            release: Notify::new(),
        }
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::Acquire)
    }

    async fn wait_until_blocked(&self) {
        self.blocked.notified().await;
    }

    fn release(&self) {
        self.release.notify_one();
    }
}

impl ModelPriceSource for BlockingSource {
    fn load<'a>(&'a self) -> ModelPriceSourceFuture<'a> {
        Box::pin(async move {
            let call = self.calls.fetch_add(1, Ordering::AcqRel) + 1;
            if call == self.blocked_call {
                // 精确卡住刷新 IO，让测试可以在加载期间注入新的失效代数。
                self.blocked.notify_one();
                self.release.notified().await;
            }
            Ok(self.records.clone())
        })
    }
}

#[tokio::test]
async fn initial_snapshot_preserves_modes_prices_and_redaction() {
    let paid = record(
        "gpt-5.5-private",
        BillingMode::PerToken,
        [
            Decimal::from(2),
            Decimal::from(3),
            Decimal::from(4),
            Decimal::from(5),
            Decimal::from(6),
        ],
        7,
    );
    let zero_metered = record(
        "zero-metered-private",
        BillingMode::PerToken,
        [Decimal::ZERO; 5],
        8,
    );
    let free = record("free-private", BillingMode::Free, [Decimal::ZERO; 5], 9);
    let source = Arc::new(SequenceSource::new([Ok(vec![
        paid.clone(),
        zero_metered,
        free,
    ])]));

    let cache = ModelPriceCache::load(source.clone()).await.unwrap();
    let snapshot = cache.snapshot().unwrap();

    assert_eq!(source.calls(), 1);
    assert_eq!(snapshot.generation(), 1);
    assert_eq!(snapshot.len(), 3);
    assert!(snapshot.get("GPT-5.5-private").is_none());
    assert_eq!(
        snapshot
            .entries()
            .map(|(model, _)| model)
            .collect::<Vec<_>>(),
        vec!["free-private", "gpt-5.5-private", "zero-metered-private"]
    );

    let paid_price = snapshot.get("gpt-5.5-private").unwrap();
    assert_eq!(paid_price.mode(), ModelPriceMode::PerToken);
    assert_eq!(paid_price.billing_mode(), Some(BillingMode::PerToken));
    assert_eq!(paid_price.version(), 7);
    let price = paid_price
        .resolver(unit_ratios())
        .unwrap()
        .resolve(&PricingContext::new(&million_input_usage()))
        .unwrap();
    assert_eq!(price.quota(), Quota::new(1_000_000).unwrap());
    assert!(!price.is_free());

    let zero_metered = snapshot.get("zero-metered-private").unwrap();
    let zero_price = zero_metered
        .resolver(unit_ratios())
        .unwrap()
        .resolve(&PricingContext::new(&million_input_usage()))
        .unwrap();
    assert_eq!(zero_price.quota(), Quota::ZERO);
    assert!(!zero_price.is_free());

    let free = snapshot.get("free-private").unwrap();
    let free_price = free
        .resolver(unit_ratios())
        .unwrap()
        .resolve(&PricingContext::new(&million_input_usage()))
        .unwrap();
    assert_eq!(free_price.quota(), Quota::ZERO);
    assert!(free_price.is_free());

    assert_eq!(format!("{paid:?}"), "ModelPriceSourceRecord(<redacted>)");
    let rendered = format!("{paid_price:?}\n{snapshot:?}\n{cache:?}");
    assert!(!rendered.contains("gpt-5.5-private"));
    assert!(!rendered.contains("prices"));
}

#[tokio::test]
async fn expression_definition_is_retained_without_leaking_source() {
    let definition =
        BillingExpressionDefinition::new(r#"v1:tier("base", p * 2.5)"#.to_owned()).unwrap();
    let source = Arc::new(SequenceSource::new([Ok(vec![
        ModelPriceSourceRecord::expression("expression-model".to_owned(), definition, 3).unwrap(),
    ])]));
    let cache = ModelPriceCache::load(source).await.unwrap();
    let price = cache.snapshot().unwrap().get("expression-model").unwrap();

    assert_eq!(price.mode(), ModelPriceMode::Expression);
    assert_eq!(price.billing_mode(), None);
    assert!(price.resolver(unit_ratios()).is_none());
    let resolved = price
        .request_resolver(unit_ratios())
        .unwrap()
        .resolve(&PricingContext::new(&million_input_usage()))
        .unwrap();
    assert_eq!(resolved.quota(), Quota::new(1_250_000).unwrap());
    assert!(!resolved.is_free());
    assert!(!format!("{price:?}").contains("expression-model"));
}

#[tokio::test]
async fn failed_refresh_keeps_the_previous_snapshot_and_stale_state() {
    let source = Arc::new(SequenceSource::new([
        Ok(vec![record(
            "stable-model",
            BillingMode::PerToken,
            [Decimal::ONE; 5],
            1,
        )]),
        Err(ModelPriceSourceError::Unavailable),
    ]));
    let cache = ModelPriceCache::load(source.clone()).await.unwrap();
    let previous = cache.snapshot().unwrap();

    assert_eq!(cache.invalidate().unwrap(), 2);
    assert!(matches!(
        cache.refresh_if_stale().await,
        Err(ModelPriceCacheError::Source(
            ModelPriceSourceError::Unavailable
        ))
    ));

    let current = cache.snapshot().unwrap();
    assert!(Arc::ptr_eq(&previous, &current));
    assert!(current.get("stable-model").is_some());
    assert!(cache.is_stale());
    assert_eq!(source.calls(), 2);
}

#[tokio::test]
async fn successful_refresh_replaces_the_complete_catalog() {
    let source = Arc::new(SequenceSource::new([
        Ok(vec![record(
            "removed-model",
            BillingMode::PerToken,
            [Decimal::ONE; 5],
            1,
        )]),
        Ok(vec![record(
            "replacement-model",
            BillingMode::Free,
            [Decimal::ZERO; 5],
            2,
        )]),
    ]));
    let cache = ModelPriceCache::load(source).await.unwrap();
    let previous = cache.snapshot().unwrap();

    cache.invalidate().unwrap();
    let current = cache.refresh_if_stale().await.unwrap();

    assert_eq!(current.generation(), 2);
    assert!(current.get("removed-model").is_none());
    assert!(current.get("replacement-model").is_some());
    assert!(previous.get("removed-model").is_some());
    assert!(previous.get("replacement-model").is_none());
    assert!(!cache.is_stale());
}

#[tokio::test]
async fn duplicate_model_rejects_the_entire_initial_load() {
    let source = Arc::new(SequenceSource::new([Ok(vec![
        record(
            "duplicate-model",
            BillingMode::PerToken,
            [Decimal::ONE; 5],
            1,
        ),
        record("duplicate-model", BillingMode::Free, [Decimal::ZERO; 5], 2),
    ])]));

    assert!(matches!(
        ModelPriceCache::load(source).await,
        Err(ModelPriceCacheError::DuplicateModel)
    ));
}

#[tokio::test]
async fn invalidation_during_refresh_remains_pending_for_the_next_refresh() {
    let source = Arc::new(BlockingSource::new(
        vec![record(
            "generation-model",
            BillingMode::PerToken,
            [Decimal::ONE; 5],
            1,
        )],
        2,
    ));
    let cache = ModelPriceCache::load(source.clone()).await.unwrap();
    assert_eq!(cache.invalidate().unwrap(), 2);

    let refreshing = tokio::spawn({
        let cache = cache.clone();
        async move { cache.refresh_if_stale().await }
    });
    source.wait_until_blocked().await;
    assert_eq!(cache.invalidate().unwrap(), 3);
    source.release();

    let first_refresh = refreshing.await.unwrap().unwrap();
    assert_eq!(first_refresh.generation(), 2);
    assert!(cache.is_stale());

    let second_refresh = cache.refresh_if_stale().await.unwrap();
    assert_eq!(second_refresh.generation(), 3);
    assert!(!cache.is_stale());
    assert_eq!(source.calls(), 3);
}

#[tokio::test]
async fn concurrent_stale_refreshes_share_one_source_load() {
    let source = Arc::new(BlockingSource::new(
        vec![record(
            "shared-refresh-model",
            BillingMode::PerToken,
            [Decimal::ONE; 5],
            1,
        )],
        2,
    ));
    let cache = ModelPriceCache::load(source.clone()).await.unwrap();
    cache.invalidate().unwrap();

    let tasks = (0..8)
        .map(|_| {
            let cache = cache.clone();
            tokio::spawn(async move { cache.refresh_if_stale().await })
        })
        .collect::<Vec<_>>();
    source.wait_until_blocked().await;
    source.release();

    for task in tasks {
        let snapshot = task.await.unwrap().unwrap();
        assert_eq!(snapshot.generation(), 2);
    }
    assert_eq!(source.calls(), 2);
    assert!(!cache.is_stale());
}

fn record(
    model: &str,
    billing_mode: BillingMode,
    values: [Decimal; 5],
    version: u64,
) -> ModelPriceSourceRecord {
    ModelPriceSourceRecord::new(
        model.to_owned(),
        billing_mode,
        TokenPrices::new(values[0], values[1], values[2], values[3], values[4]).unwrap(),
        version,
    )
    .unwrap()
}

fn unit_ratios() -> PricingRatios {
    PricingRatios::new(PricingRatio::ONE, PricingRatio::ONE, PricingRatio::ONE)
}

fn million_input_usage() -> Usage {
    Usage::new(
        TokenCount::new(1_000_000).unwrap(),
        TokenCount::ZERO,
        UsageDetails::new(
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::ZERO,
        ),
        UsageSource::Upstream,
        UsageSemantics::Inclusive,
    )
    .unwrap()
}
