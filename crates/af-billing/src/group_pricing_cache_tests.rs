use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use crate::{
    GroupModelRatioSourceRecord, GroupPeakPricing, GroupPricingCache, GroupPricingCacheError,
    GroupPricingSource, GroupPricingSourceCatalog, GroupPricingSourceError,
    GroupPricingSourceFuture, GroupPricingSourceRecord, PricingRatio,
};
use af_domain::GroupId;

type SourceResult = Result<GroupPricingSourceCatalog, GroupPricingSourceError>;

struct SequenceSource {
    responses: Mutex<VecDeque<SourceResult>>,
}

impl SequenceSource {
    fn new(responses: impl IntoIterator<Item = SourceResult>) -> Self {
        Self {
            responses: Mutex::new(responses.into_iter().collect()),
        }
    }
}

impl GroupPricingSource for SequenceSource {
    fn load<'a>(&'a self) -> GroupPricingSourceFuture<'a> {
        Box::pin(async move {
            self.responses
                .lock()
                .expect("测试 source 锁不应中毒")
                .pop_front()
                .expect("测试 source 必须预置足够响应")
        })
    }
}

#[tokio::test]
async fn snapshot_applies_base_special_and_cross_midnight_peak_ratios() {
    let source_group = GroupId::new(1).unwrap();
    let target_group = GroupId::new(2).unwrap();
    let peak =
        GroupPeakPricing::new(PricingRatio::new(2_000_000).unwrap(), 23 * 3_600, 1_800).unwrap();
    let source = Arc::new(SequenceSource::new([Ok(GroupPricingSourceCatalog::new(
        vec![
            GroupPricingSourceRecord::new(
                source_group,
                PricingRatio::new(1_000_000).unwrap(),
                None,
            ),
            GroupPricingSourceRecord::new(
                target_group,
                PricingRatio::new(1_200_000).unwrap(),
                Some(peak),
            ),
        ],
        vec![GroupModelRatioSourceRecord::new(
            source_group,
            target_group,
            PricingRatio::new(750_000).unwrap(),
        )],
    ))]));
    let cache = GroupPricingCache::load(source).await.unwrap();
    let snapshot = cache.snapshot().unwrap();

    let peak_ratios = snapshot
        .ratios_for_request(source_group, target_group, 23 * 3_600 + 30 * 60)
        .unwrap();
    assert_eq!(peak_ratios.group().micros(), 1_200_000);
    assert_eq!(peak_ratios.group_model().micros(), 750_000);
    assert_eq!(peak_ratios.applied_peak().micros(), 2_000_000);

    let quiet_ratios = snapshot
        .ratios_for_request(source_group, target_group, 12 * 3_600)
        .unwrap();
    assert_eq!(quiet_ratios.applied_peak(), PricingRatio::ONE);

    let after_midnight = snapshot
        .ratios_for_request(source_group, target_group, 1_200)
        .unwrap();
    assert_eq!(after_midnight.applied_peak().micros(), 2_000_000);
}

#[tokio::test]
async fn invalid_or_duplicate_catalog_entries_fail_closed() {
    let group = GroupId::new(1).unwrap();
    let duplicate = GroupPricingSourceCatalog::new(
        vec![
            GroupPricingSourceRecord::new(group, PricingRatio::ONE, None),
            GroupPricingSourceRecord::new(group, PricingRatio::ONE, None),
        ],
        Vec::new(),
    );
    assert!(matches!(
        GroupPricingCache::load(Arc::new(SequenceSource::new([Ok(duplicate)]))).await,
        Err(GroupPricingCacheError::DuplicateGroup)
    ));

    let orphan = GroupPricingSourceCatalog::new(
        vec![GroupPricingSourceRecord::new(
            group,
            PricingRatio::ONE,
            None,
        )],
        vec![GroupModelRatioSourceRecord::new(
            group,
            GroupId::new(2).unwrap(),
            PricingRatio::ONE,
        )],
    );
    assert!(matches!(
        GroupPricingCache::load(Arc::new(SequenceSource::new([Ok(orphan)]))).await,
        Err(GroupPricingCacheError::OrphanGroupModelRatio)
    ));
}

#[tokio::test]
async fn failed_refresh_keeps_the_previous_immutable_snapshot() {
    let group = GroupId::new(1).unwrap();
    let source = Arc::new(SequenceSource::new([
        Ok(GroupPricingSourceCatalog::new(
            vec![GroupPricingSourceRecord::new(
                group,
                PricingRatio::ONE,
                None,
            )],
            Vec::new(),
        )),
        Err(GroupPricingSourceError::Unavailable),
    ]));
    let cache = GroupPricingCache::load(source).await.unwrap();
    let previous = cache.snapshot().unwrap();
    cache.invalidate().unwrap();
    assert!(matches!(
        cache.refresh_if_stale().await,
        Err(GroupPricingCacheError::Source(
            GroupPricingSourceError::Unavailable
        ))
    ));
    let current = cache.snapshot().unwrap();
    assert!(Arc::ptr_eq(&previous, &current));
    assert!(cache.is_stale());
}
