use std::{collections::BTreeMap, sync::Arc};

use af_db::{SchedulerCatalogSubject, SchedulerRuntimeProjection};
use af_domain::{ChannelId, GroupId};

use crate::SchedulerCandidate;

use super::{
    ChannelIndexCacheError, ChannelIndexSnapshot, ChannelIndexSnapshotError, InMemoryChannelIndex,
    runtime_record_to_source, validate_candidates, validate_total_capacity,
};

/// 一批主体投影原子应用后的聚合结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChannelIndexProjectionApplyReport {
    generation: u64,
    applied_subject_count: usize,
    key_count: usize,
    candidate_count: usize,
}

impl ChannelIndexProjectionApplyReport {
    /// 返回应用完成后的本地快照代数。
    #[must_use]
    pub const fn generation(self) -> u64 {
        self.generation
    }

    /// 返回本批实际推进版本并替换的主体数量。
    #[must_use]
    pub const fn applied_subject_count(self) -> usize {
        self.applied_subject_count
    }

    /// 返回应用完成后的 `(group_id, model)` 键数量。
    #[must_use]
    pub const fn key_count(self) -> usize {
        self.key_count
    }

    /// 返回应用完成后的候选总数。
    #[must_use]
    pub const fn candidate_count(self) -> usize {
        self.candidate_count
    }
}

impl InMemoryChannelIndex {
    /// 在同一刷新门闩下合并并原子发布一批渠道/分组主体投影。
    ///
    /// 同主体只保留最高版本；低于最近全量高水位或已应用主体版本的投影保持幂等忽略。
    pub async fn apply_projections(
        &self,
        projections: Vec<SchedulerRuntimeProjection>,
    ) -> Result<ChannelIndexProjectionApplyReport, ChannelIndexCacheError> {
        let mut newest = BTreeMap::<SchedulerCatalogSubject, SchedulerRuntimeProjection>::new();
        for projection in projections {
            let subject = projection.subject();
            if newest
                .get(&subject)
                .is_none_or(|current| projection.version() > current.version())
            {
                newest.insert(subject, projection);
            }
        }
        validate_total_capacity(newest.len())?;
        let mut projections = newest.into_values().collect::<Vec<_>>();
        projections.sort_unstable_by_key(SchedulerRuntimeProjection::version);

        let _guard = self.inner.refresh_gate.lock().await;
        let current = self.snapshot()?;
        let mut next = current.copy_for_incremental_apply();
        let mut applied_subject_count = 0_usize;
        for projection in projections {
            if next.apply_projection(projection)? {
                applied_subject_count = applied_subject_count
                    .checked_add(1)
                    .ok_or(ChannelIndexSnapshotError::Invariant)?;
            }
        }
        if applied_subject_count == 0 {
            return Ok(projection_report(&current, 0));
        }
        next.generation = current
            .generation()
            .checked_add(1)
            .ok_or(ChannelIndexCacheError::GenerationExhausted)?;
        let next = Arc::new(next);
        let mut published = self
            .inner
            .snapshot
            .write()
            .map_err(|_| ChannelIndexCacheError::StateUnavailable)?;
        *published = Arc::clone(&next);
        Ok(projection_report(&next, applied_subject_count))
    }
}

impl ChannelIndexSnapshot {
    fn copy_for_incremental_apply(&self) -> Self {
        Self {
            generation: self.generation,
            catalog_version: self.catalog_version,
            subject_versions: self.subject_versions.clone(),
            runtime_target_versions: self.runtime_target_versions.clone(),
            entries: self.entries.clone(),
            runtime_targets: self.runtime_targets.clone(),
            key_count: self.key_count,
            candidate_count: self.candidate_count,
        }
    }

    fn apply_projection(
        &mut self,
        projection: SchedulerRuntimeProjection,
    ) -> Result<bool, ChannelIndexSnapshotError> {
        let (version, subject, records) = projection.into_parts();
        if version <= self.catalog_version
            || self
                .subject_versions
                .get(&subject)
                .is_some_and(|current| version <= *current)
        {
            return Ok(false);
        }
        if !self.subject_versions.contains_key(&subject) {
            validate_total_capacity(
                self.subject_versions
                    .len()
                    .checked_add(1)
                    .ok_or(ChannelIndexSnapshotError::Invariant)?,
            )?;
        }
        let records = records
            .into_iter()
            .map(runtime_record_to_source)
            .collect::<Result<Vec<_>, _>>()?;
        let fragment = Self::from_records(records, self.generation)?;

        match subject {
            SchedulerCatalogSubject::Channel(channel_id) => {
                self.apply_channel_fragment(channel_id, version, fragment)?;
            }
            SchedulerCatalogSubject::Group(group_id) => {
                self.apply_group_fragment(group_id, version, fragment)?;
            }
        }
        self.subject_versions.insert(subject, version);
        self.recount()?;
        Ok(true)
    }

    fn apply_channel_fragment(
        &mut self,
        channel_id: ChannelId,
        version: u64,
        mut fragment: ChannelIndexSnapshot,
    ) -> Result<(), ChannelIndexSnapshotError> {
        let protected_groups = newer_groups(&self.subject_versions, version);
        for (group_id, models) in &mut self.entries {
            if !protected_groups.contains(group_id) {
                retain_other_channels(models, channel_id);
            }
        }
        self.entries.retain(|_, models| !models.is_empty());
        fragment
            .entries
            .retain(|group_id, _| !protected_groups.contains(group_id));

        let target = fragment.runtime_targets.remove(&channel_id);
        if !fragment.runtime_targets.is_empty() {
            return Err(ChannelIndexSnapshotError::Invariant);
        }
        self.update_runtime_target(channel_id, version, target)?;
        self.merge_entries(fragment.entries)?;
        self.reconcile_runtime_targets()
    }

    fn apply_group_fragment(
        &mut self,
        group_id: GroupId,
        version: u64,
        mut fragment: ChannelIndexSnapshot,
    ) -> Result<(), ChannelIndexSnapshotError> {
        let protected_channels = newer_channels(&self.subject_versions, version);
        if let Some(models) = self.entries.get_mut(&group_id) {
            models.retain(|_, candidates| {
                let retained = candidates
                    .iter()
                    .copied()
                    .filter(|candidate| protected_channels.contains(&candidate.channel_id()))
                    .collect::<Vec<_>>();
                if retained.is_empty() {
                    return false;
                }
                *candidates = Arc::from(retained);
                true
            });
        }
        self.entries.retain(|_, models| !models.is_empty());
        if fragment
            .entries
            .keys()
            .any(|fragment_group_id| *fragment_group_id != group_id)
        {
            return Err(ChannelIndexSnapshotError::Invariant);
        }
        for models in fragment.entries.values_mut() {
            models.retain(|_, candidates| {
                let retained = candidates
                    .iter()
                    .copied()
                    .filter(|candidate| !protected_channels.contains(&candidate.channel_id()))
                    .collect::<Vec<_>>();
                if retained.is_empty() {
                    return false;
                }
                *candidates = Arc::from(retained);
                true
            });
        }
        fragment.entries.retain(|_, models| !models.is_empty());
        for (channel_id, target) in fragment.runtime_targets {
            self.update_runtime_target(channel_id, version, Some(target))?;
        }
        self.merge_entries(fragment.entries)?;
        self.reconcile_runtime_targets()
    }

    fn update_runtime_target(
        &mut self,
        channel_id: ChannelId,
        version: u64,
        target: Option<Arc<af_db::SchedulerRuntimeTargetRecord>>,
    ) -> Result<(), ChannelIndexSnapshotError> {
        let current_version = self
            .runtime_target_versions
            .get(&channel_id)
            .copied()
            .unwrap_or(self.catalog_version);
        if version <= current_version {
            return Ok(());
        }
        if !self.runtime_target_versions.contains_key(&channel_id) {
            validate_total_capacity(
                self.runtime_target_versions
                    .len()
                    .checked_add(1)
                    .ok_or(ChannelIndexSnapshotError::Invariant)?,
            )?;
        }
        self.runtime_target_versions.insert(channel_id, version);
        match target {
            Some(target) if target.channel_id() == channel_id => {
                self.runtime_targets.insert(channel_id, target);
            }
            Some(_) => return Err(ChannelIndexSnapshotError::Invariant),
            None => {
                self.runtime_targets.remove(&channel_id);
            }
        }
        Ok(())
    }

    fn merge_entries(
        &mut self,
        entries: BTreeMap<GroupId, BTreeMap<String, Arc<[SchedulerCandidate]>>>,
    ) -> Result<(), ChannelIndexSnapshotError> {
        for (group_id, models) in entries {
            let destination = self.entries.entry(group_id).or_default();
            for (model, candidates) in models {
                let mut merged = destination
                    .get(&model)
                    .map(|current| current.to_vec())
                    .unwrap_or_default();
                merged.extend_from_slice(&candidates);
                validate_candidates(&merged)?;
                merged.sort_unstable_by(|left, right| {
                    right
                        .priority()
                        .cmp(&left.priority())
                        .then_with(|| left.channel_id().cmp(&right.channel_id()))
                });
                destination.insert(model, Arc::from(merged));
            }
        }
        Ok(())
    }

    fn reconcile_runtime_targets(&mut self) -> Result<(), ChannelIndexSnapshotError> {
        let referenced = self
            .entries
            .values()
            .flat_map(|models| models.values())
            .flat_map(|candidates| candidates.iter().map(|candidate| candidate.channel_id()))
            .collect::<std::collections::BTreeSet<_>>();
        self.runtime_targets
            .retain(|channel_id, _| referenced.contains(channel_id));
        if referenced
            .iter()
            .any(|channel_id| !self.runtime_targets.contains_key(channel_id))
        {
            return Err(ChannelIndexSnapshotError::Invariant);
        }
        Ok(())
    }

    fn recount(&mut self) -> Result<(), ChannelIndexSnapshotError> {
        let mut key_count = 0_usize;
        let mut candidate_count = 0_usize;
        for candidates in self.entries.values().flat_map(|models| models.values()) {
            key_count = key_count
                .checked_add(1)
                .ok_or(ChannelIndexSnapshotError::Invariant)?;
            candidate_count = candidate_count
                .checked_add(candidates.len())
                .ok_or(ChannelIndexSnapshotError::Invariant)?;
        }
        validate_total_capacity(candidate_count)?;
        self.key_count = key_count;
        self.candidate_count = candidate_count;
        Ok(())
    }
}

fn newer_groups(
    versions: &BTreeMap<SchedulerCatalogSubject, u64>,
    version: u64,
) -> std::collections::BTreeSet<GroupId> {
    versions
        .iter()
        .filter_map(|(subject, current)| match subject {
            SchedulerCatalogSubject::Group(group_id) if *current > version => Some(*group_id),
            _ => None,
        })
        .collect()
}

fn newer_channels(
    versions: &BTreeMap<SchedulerCatalogSubject, u64>,
    version: u64,
) -> std::collections::BTreeSet<ChannelId> {
    versions
        .iter()
        .filter_map(|(subject, current)| match subject {
            SchedulerCatalogSubject::Channel(channel_id) if *current > version => Some(*channel_id),
            _ => None,
        })
        .collect()
}

fn retain_other_channels(
    models: &mut BTreeMap<String, Arc<[SchedulerCandidate]>>,
    channel_id: ChannelId,
) {
    models.retain(|_, candidates| {
        let retained = candidates
            .iter()
            .copied()
            .filter(|candidate| candidate.channel_id() != channel_id)
            .collect::<Vec<_>>();
        if retained.len() == candidates.len() {
            return true;
        }
        if retained.is_empty() {
            return false;
        }
        *candidates = Arc::from(retained);
        true
    });
}

fn projection_report(
    snapshot: &ChannelIndexSnapshot,
    applied_subject_count: usize,
) -> ChannelIndexProjectionApplyReport {
    ChannelIndexProjectionApplyReport {
        generation: snapshot.generation(),
        applied_subject_count,
        key_count: snapshot.key_count(),
        candidate_count: snapshot.candidate_count(),
    }
}

#[cfg(test)]
mod tests {
    use crate::ChannelIndexSourceRecord;
    use af_db::{
        EncryptedCredentialEnvelope, SchedulerRuntimeCredentialRecord, SchedulerRuntimeTargetRecord,
    };
    use af_domain::{ChannelType, CredentialKind, Protocol};

    use super::*;

    #[test]
    fn newer_group_version_blocks_late_channel_membership() {
        let group_id = GroupId::new(1).unwrap();
        let channel_id = ChannelId::new(2).unwrap();
        let mut current = ChannelIndexSnapshot::from_records(Vec::new(), 1).unwrap();
        current
            .subject_versions
            .insert(SchedulerCatalogSubject::Group(group_id), 6);
        let late_channel =
            ChannelIndexSnapshot::from_records(vec![record(group_id, "gpt-5", channel_id)], 1)
                .unwrap();

        current
            .apply_channel_fragment(channel_id, 5, late_channel)
            .unwrap();

        assert!(current.candidates(group_id, "gpt-5").unwrap().is_empty());
        assert_eq!(current.runtime_target_count(), 0);
    }

    #[test]
    fn newer_channel_version_survives_late_group_tombstone() {
        let group_id = GroupId::new(1).unwrap();
        let channel_id = ChannelId::new(2).unwrap();
        let mut current =
            ChannelIndexSnapshot::from_records(vec![record(group_id, "gpt-5", channel_id)], 1)
                .unwrap();
        current
            .subject_versions
            .insert(SchedulerCatalogSubject::Channel(channel_id), 7);
        current.runtime_target_versions.insert(channel_id, 7);

        current
            .apply_group_fragment(
                group_id,
                6,
                ChannelIndexSnapshot::from_records(Vec::new(), 1).unwrap(),
            )
            .unwrap();

        assert_eq!(
            current.candidates(group_id, "gpt-5").unwrap()[0].channel_id(),
            channel_id
        );
        assert!(current.runtime_target(channel_id).is_some());
    }

    fn record(group_id: GroupId, model: &str, channel_id: ChannelId) -> ChannelIndexSourceRecord {
        let credential = SchedulerRuntimeCredentialRecord::new(
            10,
            CredentialKind::ApiKey,
            EncryptedCredentialEnvelope::new("test-key", [1; 24], vec![2; 16]).unwrap(),
            false,
        )
        .unwrap();
        let target = SchedulerRuntimeTargetRecord::new(
            channel_id,
            ChannelType::OpenAi,
            Protocol::OpenAiChat,
            None,
            credential,
            Vec::new(),
        )
        .unwrap();
        ChannelIndexSourceRecord::with_runtime_target(group_id, model, 0, 0, Arc::new(target))
            .unwrap()
    }
}
