use std::{collections::BTreeSet, fmt};

use af_domain::{BillingReservationId, ChannelId, QuotaDelta, TokenId, UserId};
use sha2::{Digest, Sha256};
use thiserror::Error;

const WRITER_BYTES: usize = 16;
const FINGERPRINT_BYTES: usize = 32;
const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";

/// 单个用户行需要原子应用的计费增量。
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct UserBillingWrite {
    user_id: UserId,
    quota_delta: QuotaDelta,
    used_quota_delta: QuotaDelta,
    request_count_delta: i64,
}

impl UserBillingWrite {
    /// 构造至少包含一个非零字段的用户增量。
    pub fn new(
        user_id: UserId,
        quota_delta: QuotaDelta,
        used_quota_delta: QuotaDelta,
        request_count_delta: i64,
    ) -> Result<Self, BillingBatchWriteError> {
        if request_count_delta < 0 {
            return Err(BillingBatchWriteError::InvalidRequestCount);
        }
        if quota_delta.is_zero() && used_quota_delta.is_zero() && request_count_delta == 0 {
            return Err(BillingBatchWriteError::EmptyDelta);
        }
        Ok(Self {
            user_id,
            quota_delta,
            used_quota_delta,
            request_count_delta,
        })
    }

    pub(crate) const fn user_id(self) -> UserId {
        self.user_id
    }

    pub(crate) const fn quota_delta(self) -> QuotaDelta {
        self.quota_delta
    }

    pub(crate) const fn used_quota_delta(self) -> QuotaDelta {
        self.used_quota_delta
    }

    pub(crate) const fn request_count_delta(self) -> i64 {
        self.request_count_delta
    }
}

impl fmt::Debug for UserBillingWrite {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("UserBillingWrite(<redacted>)")
    }
}

/// 单个令牌行需要原子应用的计费增量。
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct TokenBillingWrite {
    token_id: TokenId,
    remain_quota_delta: QuotaDelta,
    used_quota_delta: QuotaDelta,
}

impl TokenBillingWrite {
    /// 构造至少包含一个非零字段的令牌增量。
    pub fn new(
        token_id: TokenId,
        remain_quota_delta: QuotaDelta,
        used_quota_delta: QuotaDelta,
    ) -> Result<Self, BillingBatchWriteError> {
        if remain_quota_delta.is_zero() && used_quota_delta.is_zero() {
            return Err(BillingBatchWriteError::EmptyDelta);
        }
        Ok(Self {
            token_id,
            remain_quota_delta,
            used_quota_delta,
        })
    }

    pub(crate) const fn token_id(self) -> TokenId {
        self.token_id
    }

    pub(crate) const fn remain_quota_delta(self) -> QuotaDelta {
        self.remain_quota_delta
    }

    pub(crate) const fn used_quota_delta(self) -> QuotaDelta {
        self.used_quota_delta
    }
}

impl fmt::Debug for TokenBillingWrite {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("TokenBillingWrite(<redacted>)")
    }
}

/// 单个渠道行需要原子应用的累计消耗增量。
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct ChannelBillingWrite {
    channel_id: ChannelId,
    used_quota_delta: QuotaDelta,
}

impl ChannelBillingWrite {
    /// 构造非零的渠道累计消耗增量。
    pub fn new(
        channel_id: ChannelId,
        used_quota_delta: QuotaDelta,
    ) -> Result<Self, BillingBatchWriteError> {
        if used_quota_delta.is_zero() {
            return Err(BillingBatchWriteError::EmptyDelta);
        }
        Ok(Self {
            channel_id,
            used_quota_delta,
        })
    }

    pub(crate) const fn channel_id(self) -> ChannelId {
        self.channel_id
    }

    pub(crate) const fn used_quota_delta(self) -> QuotaDelta {
        self.used_quota_delta
    }
}

impl fmt::Debug for ChannelBillingWrite {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ChannelBillingWrite(<redacted>)")
    }
}

/// 已完成结构校验并按主体标识排序的数据库批量写入快照。
#[derive(Clone, Eq, PartialEq)]
pub struct BillingBatchWrite {
    writer_id: [u8; WRITER_BYTES],
    start_sequence: i64,
    end_sequence: i64,
    event_ids: Vec<BillingReservationId>,
    users: Vec<UserBillingWrite>,
    tokens: Vec<TokenBillingWrite>,
    channels: Vec<ChannelBillingWrite>,
    fingerprint: [u8; FINGERPRINT_BYTES],
}

impl BillingBatchWrite {
    /// 构造连续批次并固化内容指纹；主体增量会按标识排序以固定锁顺序。
    pub fn new(
        writer_id: [u8; WRITER_BYTES],
        start_sequence: u64,
        end_sequence: u64,
        event_ids: Vec<BillingReservationId>,
        mut users: Vec<UserBillingWrite>,
        mut tokens: Vec<TokenBillingWrite>,
        mut channels: Vec<ChannelBillingWrite>,
    ) -> Result<Self, BillingBatchWriteError> {
        if writer_id.iter().all(|byte| *byte == 0) {
            return Err(BillingBatchWriteError::InvalidWriter);
        }
        let start_sequence =
            i64::try_from(start_sequence).map_err(|_| BillingBatchWriteError::InvalidSequence)?;
        let end_sequence =
            i64::try_from(end_sequence).map_err(|_| BillingBatchWriteError::InvalidSequence)?;
        if start_sequence <= 0 || end_sequence < start_sequence {
            return Err(BillingBatchWriteError::InvalidSequence);
        }
        let event_count = i64::try_from(event_ids.len())
            .map_err(|_| BillingBatchWriteError::InvalidEventCount)?;
        if event_count <= 0
            || end_sequence
                .checked_sub(start_sequence)
                .and_then(|length| length.checked_add(1))
                != Some(event_count)
        {
            return Err(BillingBatchWriteError::InvalidEventCount);
        }
        if !all_unique(event_ids.iter().copied()) {
            return Err(BillingBatchWriteError::DuplicateEvent);
        }

        users.sort_unstable_by_key(|delta| delta.user_id());
        tokens.sort_unstable_by_key(|delta| delta.token_id());
        channels.sort_unstable_by_key(|delta| delta.channel_id());
        if has_duplicate_adjacent(users.iter().map(|delta| delta.user_id()))
            || has_duplicate_adjacent(tokens.iter().map(|delta| delta.token_id()))
            || has_duplicate_adjacent(channels.iter().map(|delta| delta.channel_id()))
        {
            return Err(BillingBatchWriteError::DuplicateSubject);
        }

        let fingerprint = fingerprint(
            writer_id,
            start_sequence,
            end_sequence,
            &event_ids,
            &users,
            &tokens,
            &channels,
        );
        Ok(Self {
            writer_id,
            start_sequence,
            end_sequence,
            event_ids,
            users,
            tokens,
            channels,
            fingerprint,
        })
    }

    pub(crate) fn writer_key(&self) -> String {
        encode_hex(&self.writer_id)
    }

    pub(crate) const fn start_sequence(&self) -> i64 {
        self.start_sequence
    }

    pub(crate) const fn end_sequence(&self) -> i64 {
        self.end_sequence
    }

    pub(crate) fn event_count(&self) -> i64 {
        i64::try_from(self.event_ids.len()).expect("已校验事件数量必须可表示为 i64")
    }

    pub(crate) fn fingerprint_key(&self) -> String {
        encode_hex(&self.fingerprint)
    }

    pub(crate) fn users(&self) -> &[UserBillingWrite] {
        &self.users
    }

    pub(crate) fn tokens(&self) -> &[TokenBillingWrite] {
        &self.tokens
    }

    pub(crate) fn channels(&self) -> &[ChannelBillingWrite] {
        &self.channels
    }
}

impl fmt::Debug for BillingBatchWrite {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BillingBatchWrite")
            .field("event_count", &self.event_ids.len())
            .field("user_bucket_count", &self.users.len())
            .field("token_bucket_count", &self.tokens.len())
            .field("channel_bucket_count", &self.channels.len())
            .finish_non_exhaustive()
    }
}

/// 批量写入快照的闭合构造错误；不携带标识或额度原值。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum BillingBatchWriteError {
    /// writer 标识不能全零。
    #[error("计费批量 writer 标识无效")]
    InvalidWriter,
    /// sequence 必须位于数据库正整数范围内并保持连续区间。
    #[error("计费批量序列范围无效")]
    InvalidSequence,
    /// 事件数量必须与连续 sequence 范围一致。
    #[error("计费批量事件数量无效")]
    InvalidEventCount,
    /// 同一批次不能重复携带事件标识。
    #[error("计费批量包含重复事件")]
    DuplicateEvent,
    /// 同一批次的单个主体只能出现一个聚合增量。
    #[error("计费批量包含重复主体")]
    DuplicateSubject,
    /// 请求数增量必须是非负整数。
    #[error("计费批量请求数增量无效")]
    InvalidRequestCount,
    /// 单个主体增量必须至少修改一个字段。
    #[error("计费批量主体增量不能为空")]
    EmptyDelta,
}

/// 数据库应用批次后的 exactly-once 结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BillingBatchWriteOutcome {
    /// 本次事务首次应用了该批次。
    Applied,
    /// 相同 writer、sequence 范围与内容指纹已经提交。
    Existing,
}

/// 批量落盘仓储错误；不携带 SQL、标识、路径或额度原值。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum BillingBatchRepositoryError {
    /// 仓储操作截止时间配置无效。
    #[error("计费批量仓储配置无效")]
    InvalidConfiguration,
    /// 批次不是 checkpoint 的相同重放或连续下一段。
    #[error("计费批量序列冲突")]
    SequenceConflict,
    /// 主体、计数器或 checkpoint 持久化状态违反不变量。
    #[error("计费批量持久化状态损坏")]
    Invariant,
    /// 获取连接或执行尚未提交的数据库操作失败。
    #[error("计费批量数据库操作失败")]
    Query,
    /// 超时或提交失败导致事务结果无法确认。
    #[error("计费批量数据库结果未知")]
    OutcomeUnknown,
}

fn fingerprint(
    writer_id: [u8; WRITER_BYTES],
    start_sequence: i64,
    end_sequence: i64,
    event_ids: &[BillingReservationId],
    users: &[UserBillingWrite],
    tokens: &[TokenBillingWrite],
    channels: &[ChannelBillingWrite],
) -> [u8; FINGERPRINT_BYTES] {
    let mut digest = Sha256::new();
    digest.update(b"AFDBBATCH01");
    digest.update(writer_id);
    digest.update(start_sequence.to_le_bytes());
    digest.update(end_sequence.to_le_bytes());
    update_length(&mut digest, event_ids.len());
    for event_id in event_ids {
        digest.update(event_id.bytes());
    }
    update_length(&mut digest, users.len());
    for delta in users {
        digest.update(delta.user_id().get().to_le_bytes());
        digest.update(delta.quota_delta().units().to_le_bytes());
        digest.update(delta.used_quota_delta().units().to_le_bytes());
        digest.update(delta.request_count_delta().to_le_bytes());
    }
    update_length(&mut digest, tokens.len());
    for delta in tokens {
        digest.update(delta.token_id().get().to_le_bytes());
        digest.update(delta.remain_quota_delta().units().to_le_bytes());
        digest.update(delta.used_quota_delta().units().to_le_bytes());
    }
    update_length(&mut digest, channels.len());
    for delta in channels {
        digest.update(delta.channel_id().get().to_le_bytes());
        digest.update(delta.used_quota_delta().units().to_le_bytes());
    }
    digest.finalize().into()
}

fn update_length(digest: &mut Sha256, length: usize) {
    let length = u64::try_from(length).expect("已校验集合长度必须可表示为 u64");
    digest.update(length.to_le_bytes());
}

fn all_unique<T>(values: impl IntoIterator<Item = T>) -> bool
where
    T: Ord,
{
    let mut seen = BTreeSet::new();
    values.into_iter().all(|value| seen.insert(value))
}

fn has_duplicate_adjacent<T>(values: impl IntoIterator<Item = T>) -> bool
where
    T: Eq,
{
    let mut previous = None;
    for value in values {
        if previous.as_ref() == Some(&value) {
            return true;
        }
        previous = Some(value);
    }
    false
}

fn encode_hex(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(HEX_DIGITS[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX_DIGITS[usize::from(byte & 0x0f)]));
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(marker: u8) -> BillingReservationId {
        BillingReservationId::new([marker; 16]).unwrap()
    }

    #[test]
    fn batch_validation_sorts_subjects_and_rejects_invalid_ranges() {
        let users = vec![
            UserBillingWrite::new(
                UserId::new(2).unwrap(),
                QuotaDelta::ZERO,
                QuotaDelta::new(1).unwrap(),
                1,
            )
            .unwrap(),
            UserBillingWrite::new(
                UserId::new(1).unwrap(),
                QuotaDelta::new(-1).unwrap(),
                QuotaDelta::new(1).unwrap(),
                1,
            )
            .unwrap(),
        ];
        let batch = BillingBatchWrite::new(
            [1; 16],
            1,
            2,
            vec![event(1), event(2)],
            users,
            Vec::new(),
            Vec::new(),
        )
        .unwrap();

        assert_eq!(batch.users()[0].user_id().get(), 1);
        assert_eq!(batch.users()[1].user_id().get(), 2);
        assert_eq!(batch.writer_key(), "01".repeat(16));
        assert_eq!(batch.fingerprint_key().len(), 64);
        assert_eq!(
            BillingBatchWrite::new(
                [1; 16],
                1,
                2,
                vec![event(1)],
                Vec::new(),
                Vec::new(),
                Vec::new(),
            ),
            Err(BillingBatchWriteError::InvalidEventCount)
        );
    }

    #[test]
    fn duplicate_events_subjects_and_empty_deltas_are_rejected() {
        assert_eq!(
            UserBillingWrite::new(
                UserId::new(1).unwrap(),
                QuotaDelta::ZERO,
                QuotaDelta::ZERO,
                0,
            ),
            Err(BillingBatchWriteError::EmptyDelta)
        );
        assert_eq!(
            BillingBatchWrite::new(
                [1; 16],
                1,
                2,
                vec![event(1), event(1)],
                Vec::new(),
                Vec::new(),
                Vec::new(),
            ),
            Err(BillingBatchWriteError::DuplicateEvent)
        );
        let duplicate = UserBillingWrite::new(
            UserId::new(1).unwrap(),
            QuotaDelta::ZERO,
            QuotaDelta::new(1).unwrap(),
            0,
        )
        .unwrap();
        assert_eq!(
            BillingBatchWrite::new(
                [1; 16],
                1,
                1,
                vec![event(1)],
                vec![duplicate, duplicate],
                Vec::new(),
                Vec::new(),
            ),
            Err(BillingBatchWriteError::DuplicateSubject)
        );
    }

    #[test]
    fn debug_output_never_exposes_writer_fingerprint_identifiers_or_deltas() {
        let delta = UserBillingWrite::new(
            UserId::new(717_171).unwrap(),
            QuotaDelta::new(-987_654).unwrap(),
            QuotaDelta::new(123_456).unwrap(),
            1,
        )
        .unwrap();
        let batch = BillingBatchWrite::new(
            [0xab; 16],
            1,
            1,
            vec![event(71)],
            vec![delta],
            Vec::new(),
            Vec::new(),
        )
        .unwrap();
        let rendered = format!("{delta:?} {batch:?}");

        for secret in ["717171", "987654", "123456", &"ab".repeat(16)] {
            assert!(!rendered.contains(secret));
        }
    }
}
