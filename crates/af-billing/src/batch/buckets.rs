use std::collections::BTreeMap;

use af_domain::{BillingReservationId, ChannelId, QuotaDelta, TokenId, UserId};

use super::types::{
    BillingBatch, BillingBatchError, BillingBatchEvent, BillingWriterId, ChannelBillingDelta,
    TokenBillingDelta, UserBillingDelta,
};

const MAX_PERSISTED_SEQUENCE: u64 = i64::MAX as u64;

#[derive(Clone, Copy, Default)]
struct UserBucket {
    quota_delta: i64,
    used_quota_delta: i64,
    request_count_delta: i64,
}

impl UserBucket {
    fn checked_add(self, delta: UserBillingDelta) -> Result<Self, BillingBatchError> {
        Ok(Self {
            quota_delta: checked_quota_sum(self.quota_delta, delta.quota_delta().units())?,
            used_quota_delta: checked_quota_sum(
                self.used_quota_delta,
                delta.used_quota_delta().units(),
            )?,
            request_count_delta: self
                .request_count_delta
                .checked_add(delta.request_count_delta())
                .ok_or(BillingBatchError::AggregateOverflow)?,
        })
    }

    const fn is_empty(self) -> bool {
        self.quota_delta == 0 && self.used_quota_delta == 0 && self.request_count_delta == 0
    }
}

#[derive(Clone, Copy, Default)]
struct TokenBucket {
    remain_quota_delta: i64,
    used_quota_delta: i64,
}

impl TokenBucket {
    fn checked_add(self, delta: TokenBillingDelta) -> Result<Self, BillingBatchError> {
        Ok(Self {
            remain_quota_delta: checked_quota_sum(
                self.remain_quota_delta,
                delta.remain_quota_delta().units(),
            )?,
            used_quota_delta: checked_quota_sum(
                self.used_quota_delta,
                delta.used_quota_delta().units(),
            )?,
        })
    }

    const fn is_empty(self) -> bool {
        self.remain_quota_delta == 0 && self.used_quota_delta == 0
    }
}

#[derive(Clone, Copy, Default)]
struct ChannelBucket {
    used_quota_delta: i64,
}

impl ChannelBucket {
    fn checked_add(self, delta: ChannelBillingDelta) -> Result<Self, BillingBatchError> {
        Ok(Self {
            used_quota_delta: checked_quota_sum(
                self.used_quota_delta,
                delta.used_quota_delta().units(),
            )?,
        })
    }

    const fn is_empty(self) -> bool {
        self.used_quota_delta == 0
    }
}

#[derive(Clone, Default)]
pub(super) struct BillingBuckets {
    users: BTreeMap<UserId, UserBucket>,
    tokens: BTreeMap<TokenId, TokenBucket>,
    channels: BTreeMap<ChannelId, ChannelBucket>,
}

impl BillingBuckets {
    pub(super) fn preview(
        &self,
        event: BillingBatchEvent,
    ) -> Result<BucketPatch, BillingBatchError> {
        Ok(BucketPatch {
            user: event
                .user()
                .map(|delta| {
                    self.users
                        .get(&delta.user_id())
                        .copied()
                        .unwrap_or_default()
                        .checked_add(delta)
                        .map(|bucket| (delta.user_id(), bucket))
                })
                .transpose()?,
            token: event
                .token()
                .map(|delta| {
                    self.tokens
                        .get(&delta.token_id())
                        .copied()
                        .unwrap_or_default()
                        .checked_add(delta)
                        .map(|bucket| (delta.token_id(), bucket))
                })
                .transpose()?,
            channel: event
                .channel()
                .map(|delta| {
                    self.channels
                        .get(&delta.channel_id())
                        .copied()
                        .unwrap_or_default()
                        .checked_add(delta)
                        .map(|bucket| (delta.channel_id(), bucket))
                })
                .transpose()?,
        })
    }

    pub(super) fn apply(&mut self, patch: BucketPatch) {
        if let Some((id, bucket)) = patch.user {
            self.users.insert(id, bucket);
        }
        if let Some((id, bucket)) = patch.token {
            self.tokens.insert(id, bucket);
        }
        if let Some((id, bucket)) = patch.channel {
            self.channels.insert(id, bucket);
        }
    }

    pub(super) fn into_batch(
        self,
        writer_id: BillingWriterId,
        start_sequence: u64,
        end_sequence: u64,
        event_ids: Vec<BillingReservationId>,
    ) -> Result<BillingBatch, BillingBatchError> {
        let users = self
            .users
            .into_iter()
            .filter(|(_, bucket)| !bucket.is_empty())
            .map(|(user_id, bucket)| {
                UserBillingDelta::new(
                    user_id,
                    QuotaDelta::new(bucket.quota_delta)
                        .map_err(|_| BillingBatchError::AggregateOverflow)?,
                    QuotaDelta::new(bucket.used_quota_delta)
                        .map_err(|_| BillingBatchError::AggregateOverflow)?,
                    u64::try_from(bucket.request_count_delta)
                        .map_err(|_| BillingBatchError::AggregateOverflow)?,
                )
            })
            .collect::<Result<Vec<_>, BillingBatchError>>()?;
        let tokens = self
            .tokens
            .into_iter()
            .filter(|(_, bucket)| !bucket.is_empty())
            .map(|(token_id, bucket)| {
                TokenBillingDelta::new(
                    token_id,
                    QuotaDelta::new(bucket.remain_quota_delta)
                        .map_err(|_| BillingBatchError::AggregateOverflow)?,
                    QuotaDelta::new(bucket.used_quota_delta)
                        .map_err(|_| BillingBatchError::AggregateOverflow)?,
                )
            })
            .collect::<Result<Vec<_>, BillingBatchError>>()?;
        let channels = self
            .channels
            .into_iter()
            .filter(|(_, bucket)| !bucket.is_empty())
            .map(|(channel_id, bucket)| {
                ChannelBillingDelta::new(
                    channel_id,
                    QuotaDelta::new(bucket.used_quota_delta)
                        .map_err(|_| BillingBatchError::AggregateOverflow)?,
                )
            })
            .collect::<Result<Vec<_>, BillingBatchError>>()?;
        Ok(BillingBatch::from_parts(
            writer_id,
            start_sequence,
            end_sequence,
            event_ids,
            users,
            tokens,
            channels,
        ))
    }
}

pub(super) struct BucketPatch {
    user: Option<(UserId, UserBucket)>,
    token: Option<(TokenId, TokenBucket)>,
    channel: Option<(ChannelId, ChannelBucket)>,
}

pub(super) fn checked_next_sequence(current: u64) -> Result<u64, BillingBatchError> {
    let next = current
        .checked_add(1)
        .ok_or(BillingBatchError::SequenceExhausted)?;
    if next > MAX_PERSISTED_SEQUENCE {
        return Err(BillingBatchError::SequenceExhausted);
    }
    Ok(next)
}

fn checked_quota_sum(lhs: i64, rhs: i64) -> Result<i64, BillingBatchError> {
    let sum = lhs
        .checked_add(rhs)
        .ok_or(BillingBatchError::AggregateOverflow)?;
    QuotaDelta::new(sum).map_err(|_| BillingBatchError::AggregateOverflow)?;
    Ok(sum)
}
