use std::collections::HashSet;

use af_domain::GroupId;

use super::{AbilityWriteError, ChannelRoutingSnapshot};

/// 单个渠道允许声明的 Canonical 模型数量上限。
pub const MAX_ADMIN_CHANNEL_MODELS: usize = 512;
/// 渠道关联表模型列允许的 UTF-8 字节数上限。
pub const MAX_ADMIN_CHANNEL_MODEL_BYTES: usize = 255;
/// 单个渠道允许关联的有效分组数量上限。
pub const MAX_ADMIN_CHANNEL_GROUPS: usize = 64;
/// 单个渠道允许物化的能力笛卡尔积上限。
pub const MAX_ADMIN_CHANNEL_ABILITIES: usize = 4_096;

/// 已校验、去除输入顺序影响的渠道模型与分组集合。
pub(crate) struct ValidatedChannelRouting {
    models: Vec<String>,
    group_ids: Vec<GroupId>,
}

impl ValidatedChannelRouting {
    /// 校验模型、分组、重复项和笛卡尔积容量，并固定存储顺序。
    pub(crate) fn new(
        mut models: Vec<String>,
        mut group_ids: Vec<GroupId>,
    ) -> Result<Self, AbilityWriteError> {
        if models.len() > MAX_ADMIN_CHANNEL_MODELS
            || group_ids.len() > MAX_ADMIN_CHANNEL_GROUPS
            || models.iter().any(|model| !valid_model(model))
            || has_duplicate_models(&models)
            || has_duplicate_groups(&group_ids)
            || models
                .len()
                .checked_mul(group_ids.len())
                .is_none_or(|count| count > MAX_ADMIN_CHANNEL_ABILITIES)
        {
            return Err(AbilityWriteError::InvalidInput);
        }
        models.sort_unstable();
        group_ids.sort_unstable_by_key(|group_id| group_id.get());
        Ok(Self { models, group_ids })
    }

    pub(super) fn models(&self) -> &[String] {
        &self.models
    }

    pub(super) fn group_ids(&self) -> &[GroupId] {
        &self.group_ids
    }

    pub(crate) fn snapshot(&self) -> ChannelRoutingSnapshot {
        ChannelRoutingSnapshot {
            models: self.models.clone(),
            group_ids: self.group_ids.clone(),
        }
    }
}

pub(super) fn valid_model(model: &str) -> bool {
    !model.is_empty()
        && model.len() <= MAX_ADMIN_CHANNEL_MODEL_BYTES
        && model.trim() == model
        && !model.chars().any(char::is_control)
}

pub(super) fn query_limit(maximum: usize) -> Result<u64, AbilityWriteError> {
    u64::try_from(maximum)
        .ok()
        .and_then(|maximum| maximum.checked_add(1))
        .ok_or(AbilityWriteError::Invariant)
}

pub(super) fn relation_page_limit(
    channel_count: usize,
    per_channel: usize,
) -> Result<u64, AbilityWriteError> {
    channel_count
        .checked_mul(per_channel)
        .and_then(|maximum| maximum.checked_add(1))
        .and_then(|limit| u64::try_from(limit).ok())
        .ok_or(AbilityWriteError::Invariant)
}

fn has_duplicate_models(models: &[String]) -> bool {
    let mut seen = HashSet::with_capacity(models.len());
    models.iter().any(|model| !seen.insert(model.as_str()))
}

fn has_duplicate_groups(group_ids: &[GroupId]) -> bool {
    let mut seen = HashSet::with_capacity(group_ids.len());
    group_ids.iter().any(|group_id| !seen.insert(*group_id))
}
