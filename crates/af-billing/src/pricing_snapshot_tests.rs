use std::sync::Arc;

use af_domain::{GroupId, Quota};
use af_protocol::{TokenCount, Usage, UsageDetails, UsageSemantics, UsageSource};
use rust_decimal::Decimal;

use crate::{
    BillingExpressionDefinition, BillingMode, CachedRequestPricingSnapshotSource,
    GroupModelRatioSourceRecord, GroupPricingCache, GroupPricingSource, GroupPricingSourceCatalog,
    GroupPricingSourceFuture, GroupPricingSourceRecord, ModelPriceCache, ModelPriceMode,
    ModelPriceSource, ModelPriceSourceFuture, ModelPriceSourceRecord, PricingContext, PricingRatio,
    PricingRatios, PricingResolver, RatioPricingResolver, RequestPricingSnapshotError,
    RequestPricingSnapshotSource, TokenPrices,
};

struct ModelSource;

impl ModelPriceSource for ModelSource {
    fn load<'a>(&'a self) -> ModelPriceSourceFuture<'a> {
        Box::pin(async {
            Ok(vec![
                ModelPriceSourceRecord::new(
                    "snapshot-model".to_owned(),
                    BillingMode::PerToken,
                    TokenPrices::new(
                        Decimal::ONE,
                        Decimal::ZERO,
                        Decimal::ZERO,
                        Decimal::ZERO,
                        Decimal::ZERO,
                    )
                    .unwrap(),
                    11,
                )
                .unwrap(),
                ModelPriceSourceRecord::expression(
                    "expression-model".to_owned(),
                    BillingExpressionDefinition::new(r#"tier("base", p)"#.to_owned()).unwrap(),
                    12,
                )
                .unwrap(),
            ])
        })
    }
}

struct GroupSource;

impl GroupPricingSource for GroupSource {
    fn load<'a>(&'a self) -> GroupPricingSourceFuture<'a> {
        Box::pin(async {
            let source = GroupId::new(1).unwrap();
            let target = GroupId::new(2).unwrap();
            Ok(GroupPricingSourceCatalog::new(
                vec![
                    GroupPricingSourceRecord::new(source, PricingRatio::ONE, None),
                    GroupPricingSourceRecord::new(
                        target,
                        PricingRatio::new(1_200_000).unwrap(),
                        None,
                    ),
                ],
                vec![GroupModelRatioSourceRecord::new(
                    source,
                    target,
                    PricingRatio::new(750_000).unwrap(),
                )],
            ))
        })
    }
}

#[tokio::test]
async fn capture_fixes_model_version_and_all_ratios_for_both_billing_phases() {
    let model_cache = ModelPriceCache::load(Arc::new(ModelSource)).await.unwrap();
    let group_cache = GroupPricingCache::load(Arc::new(GroupSource))
        .await
        .unwrap();
    let source = CachedRequestPricingSnapshotSource::new(model_cache, group_cache);
    let snapshot = source
        .capture(
            "snapshot-model",
            GroupId::new(1).unwrap(),
            GroupId::new(2).unwrap(),
            0,
        )
        .unwrap();

    assert_eq!(snapshot.model_version(), 11);
    assert_eq!(snapshot.ratios().group().micros(), 1_200_000);
    assert_eq!(snapshot.ratios().group_model().micros(), 750_000);
    let resolver = snapshot.resolver();
    let price = resolver
        .resolve(&PricingContext::new(&million_input_usage()))
        .unwrap();
    assert_eq!(price.quota(), Quota::new(450_000).unwrap());
    assert!(!price.is_free());
}

#[tokio::test]
async fn stale_catalog_and_unknown_model_never_become_free() {
    let model_cache = ModelPriceCache::load(Arc::new(ModelSource)).await.unwrap();
    let group_cache = GroupPricingCache::load(Arc::new(GroupSource))
        .await
        .unwrap();
    let source = CachedRequestPricingSnapshotSource::new(model_cache, group_cache);
    assert!(matches!(
        source.capture(
            "unknown-model",
            GroupId::new(1).unwrap(),
            GroupId::new(2).unwrap(),
            0
        ),
        Err(RequestPricingSnapshotError::ModelNotFound)
    ));
    source.group_pricing().invalidate().unwrap();
    assert!(matches!(
        source.capture(
            "snapshot-model",
            GroupId::new(1).unwrap(),
            GroupId::new(2).unwrap(),
            0
        ),
        Err(RequestPricingSnapshotError::GroupPricingStale)
    ));
}

#[tokio::test]
async fn expression_snapshot_freezes_ratios_and_executes_through_the_unified_resolver() {
    let model_cache = ModelPriceCache::load(Arc::new(ModelSource)).await.unwrap();
    let group_cache = GroupPricingCache::load(Arc::new(GroupSource))
        .await
        .unwrap();
    let source = CachedRequestPricingSnapshotSource::new(model_cache, group_cache);

    let snapshot = source
        .capture(
            "expression-model",
            GroupId::new(1).unwrap(),
            GroupId::new(2).unwrap(),
            0,
        )
        .unwrap();

    assert_eq!(snapshot.model_mode(), ModelPriceMode::Expression);
    assert_eq!(snapshot.billing_mode(), BillingMode::PerToken);
    assert!(snapshot.ratio_resolver().is_none());
    let price = snapshot
        .resolver()
        .resolve(&PricingContext::new(&million_input_usage()))
        .unwrap();
    assert_eq!(price.quota(), Quota::new(450_000).unwrap());
    assert!(!price.is_free());
}

#[test]
fn contract_price_override_does_not_apply_platform_group_ratios() {
    let resolver = RatioPricingResolver::metered(
        TokenPrices::new(
            Decimal::ONE,
            Decimal::ZERO,
            Decimal::ZERO,
            Decimal::ZERO,
            Decimal::ZERO,
        )
        .unwrap(),
        PricingRatios::new(
            PricingRatio::new(1_200_000).unwrap(),
            PricingRatio::new(750_000).unwrap(),
            PricingRatio::ONE,
        ),
    );
    let platform_price = resolver
        .resolve(&PricingContext::new(&million_input_usage()))
        .unwrap();
    assert_eq!(platform_price.quota(), Quota::new(450_000).unwrap());

    let contract_resolver = RatioPricingResolver::metered(
        TokenPrices::new(
            Decimal::ONE,
            Decimal::ZERO,
            Decimal::ZERO,
            Decimal::ZERO,
            Decimal::ZERO,
        )
        .unwrap(),
        PricingRatios::new(PricingRatio::ONE, PricingRatio::ONE, PricingRatio::ONE),
    );
    let contract_price = contract_resolver
        .resolve(&PricingContext::new(&million_input_usage()))
        .unwrap();
    assert_eq!(contract_price.quota(), Quota::new(500_000).unwrap());
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
