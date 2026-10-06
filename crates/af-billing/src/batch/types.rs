use std::{fmt, future::Future, pin::Pin};

use af_domain::{BillingReservationId, ChannelId, QuotaDelta, TokenId, UserId};
use thiserror::Error;
use uuid::Uuid;

const WRITER_ID_BYTES: usize = 16;
const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";

/// 单个 WAL writer 的稳定标识；同一目录重启后必须保持不变。
#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BillingWriterId([u8; WRITER_ID_BYTES]);

impl BillingWriterId {
    /// 从非零 128 位值恢复 writer 标识。
    pub const fn from_bytes(bytes: [u8; WRITER_ID_BYTES]) -> Result<Self, BillingBatchError> {
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] != 0 {
                return Ok(Self(bytes));
            }
            index += 1;
        }
        Err(BillingBatchError::InvalidWriterId)
    }

    /// 生成新的随机 writer 标识；仅在空 WAL 目录首次初始化时调用。
    #[must_use]
    pub fn generate() -> Self {
        Self(Uuid::new_v4().into_bytes())
    }

    /// 返回 WAL 头使用的原始字节。
    #[must_use]
    pub const fn bytes(self) -> [u8; WRITER_ID_BYTES] {
        self.0
    }

    /// 返回数据库 checkpoint 使用的固定长度小写十六进制键。
    #[must_use]
    pub fn persistence_key(self) -> String {
        let mut encoded = String::with_capacity(WRITER_ID_BYTES * 2);
        for byte in self.0 {
            encoded.push(char::from(HEX_DIGITS[usize::from(byte >> 4)]));
            encoded.push(char::from(HEX_DIGITS[usize::from(byte & 0x0f)]));
        }
        encoded
    }
}

impl fmt::Debug for BillingWriterId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("BillingWriterId(<redacted>)")
    }
}

/// 单个用户维度的计费增量。
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct UserBillingDelta {
    user_id: UserId,
    quota_delta: QuotaDelta,
    used_quota_delta: QuotaDelta,
    request_count_delta: i64,
}

impl UserBillingDelta {
    /// 构造用户增量；请求数必须可持久化为非负 `i64`，且至少一个字段非零。
    pub fn new(
        user_id: UserId,
        quota_delta: QuotaDelta,
        used_quota_delta: QuotaDelta,
        request_count_delta: u64,
    ) -> Result<Self, BillingBatchError> {
        let request_count_delta = i64::try_from(request_count_delta)
            .map_err(|_| BillingBatchError::InvalidRequestCount)?;
        let delta = Self {
            user_id,
            quota_delta,
            used_quota_delta,
            request_count_delta,
        };
        if delta.is_empty() {
            return Err(BillingBatchError::EmptyDelta);
        }
        Ok(delta)
    }

    /// 返回用户标识。
    #[must_use]
    pub const fn user_id(self) -> UserId {
        self.user_id
    }

    /// 返回可用额度调整量。
    #[must_use]
    pub const fn quota_delta(self) -> QuotaDelta {
        self.quota_delta
    }

    /// 返回累计消耗调整量。
    #[must_use]
    pub const fn used_quota_delta(self) -> QuotaDelta {
        self.used_quota_delta
    }

    /// 返回请求数增量。
    #[must_use]
    pub const fn request_count_delta(self) -> i64 {
        self.request_count_delta
    }

    const fn is_empty(self) -> bool {
        self.quota_delta.is_zero()
            && self.used_quota_delta.is_zero()
            && self.request_count_delta == 0
    }
}

impl fmt::Debug for UserBillingDelta {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("UserBillingDelta(<redacted>)")
    }
}

/// 单个令牌维度的计费增量。
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct TokenBillingDelta {
    token_id: TokenId,
    remain_quota_delta: QuotaDelta,
    used_quota_delta: QuotaDelta,
}

impl TokenBillingDelta {
    /// 构造令牌增量；至少一个字段必须非零。
    pub const fn new(
        token_id: TokenId,
        remain_quota_delta: QuotaDelta,
        used_quota_delta: QuotaDelta,
    ) -> Result<Self, BillingBatchError> {
        let delta = Self {
            token_id,
            remain_quota_delta,
            used_quota_delta,
        };
        if delta.is_empty() {
            return Err(BillingBatchError::EmptyDelta);
        }
        Ok(delta)
    }

    /// 返回令牌标识。
    #[must_use]
    pub const fn token_id(self) -> TokenId {
        self.token_id
    }

    /// 返回令牌剩余额度调整量。
    #[must_use]
    pub const fn remain_quota_delta(self) -> QuotaDelta {
        self.remain_quota_delta
    }

    /// 返回令牌累计消耗调整量。
    #[must_use]
    pub const fn used_quota_delta(self) -> QuotaDelta {
        self.used_quota_delta
    }

    const fn is_empty(self) -> bool {
        self.remain_quota_delta.is_zero() && self.used_quota_delta.is_zero()
    }
}

impl fmt::Debug for TokenBillingDelta {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("TokenBillingDelta(<redacted>)")
    }
}

/// 单个渠道维度的累计消耗增量。
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct ChannelBillingDelta {
    channel_id: ChannelId,
    used_quota_delta: QuotaDelta,
}

impl ChannelBillingDelta {
    /// 构造非零渠道累计消耗增量。
    pub const fn new(
        channel_id: ChannelId,
        used_quota_delta: QuotaDelta,
    ) -> Result<Self, BillingBatchError> {
        if used_quota_delta.is_zero() {
            return Err(BillingBatchError::EmptyDelta);
        }
        Ok(Self {
            channel_id,
            used_quota_delta,
        })
    }

    /// 返回渠道标识。
    #[must_use]
    pub const fn channel_id(self) -> ChannelId {
        self.channel_id
    }

    /// 返回渠道累计消耗调整量。
    #[must_use]
    pub const fn used_quota_delta(self) -> QuotaDelta {
        self.used_quota_delta
    }
}

impl fmt::Debug for ChannelBillingDelta {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ChannelBillingDelta(<redacted>)")
    }
}

/// 一次请求写入 WAL 的完整增量事件。
///
/// `event_id` 应复用请求的计费预留标识；同一未确认事件重放时，完全相同的内容会返回
/// `Existing`，内容不一致则闭合为冲突。
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct BillingBatchEvent {
    event_id: BillingReservationId,
    user: Option<UserBillingDelta>,
    token: Option<TokenBillingDelta>,
    channel: Option<ChannelBillingDelta>,
}

impl BillingBatchEvent {
    /// 构造至少包含一个维度的计费增量事件。
    pub const fn new(
        event_id: BillingReservationId,
        user: Option<UserBillingDelta>,
        token: Option<TokenBillingDelta>,
        channel: Option<ChannelBillingDelta>,
    ) -> Result<Self, BillingBatchError> {
        if user.is_none() && token.is_none() && channel.is_none() {
            return Err(BillingBatchError::EmptyEvent);
        }
        Ok(Self {
            event_id,
            user,
            token,
            channel,
        })
    }

    /// 返回事件幂等标识。
    #[must_use]
    pub const fn event_id(self) -> BillingReservationId {
        self.event_id
    }

    /// 返回用户维度增量。
    #[must_use]
    pub const fn user(self) -> Option<UserBillingDelta> {
        self.user
    }

    /// 返回令牌维度增量。
    #[must_use]
    pub const fn token(self) -> Option<TokenBillingDelta> {
        self.token
    }

    /// 返回渠道维度增量。
    #[must_use]
    pub const fn channel(self) -> Option<ChannelBillingDelta> {
        self.channel
    }
}

impl fmt::Debug for BillingBatchEvent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("BillingBatchEvent(<redacted>)")
    }
}

/// 已封存并准备交给持久化 sink 的连续序列批次。
#[derive(Clone, Eq, PartialEq)]
pub struct BillingBatch {
    writer_id: BillingWriterId,
    start_sequence: u64,
    end_sequence: u64,
    event_ids: Vec<BillingReservationId>,
    users: Vec<UserBillingDelta>,
    tokens: Vec<TokenBillingDelta>,
    channels: Vec<ChannelBillingDelta>,
}

impl BillingBatch {
    pub(super) fn from_parts(
        writer_id: BillingWriterId,
        start_sequence: u64,
        end_sequence: u64,
        event_ids: Vec<BillingReservationId>,
        users: Vec<UserBillingDelta>,
        tokens: Vec<TokenBillingDelta>,
        channels: Vec<ChannelBillingDelta>,
    ) -> Self {
        Self {
            writer_id,
            start_sequence,
            end_sequence,
            event_ids,
            users,
            tokens,
            channels,
        }
    }

    /// 返回 writer 标识。
    #[must_use]
    pub const fn writer_id(&self) -> BillingWriterId {
        self.writer_id
    }

    /// 返回本批次首个连续序号。
    #[must_use]
    pub const fn start_sequence(&self) -> u64 {
        self.start_sequence
    }

    /// 返回本批次最后一个连续序号。
    #[must_use]
    pub const fn end_sequence(&self) -> u64 {
        self.end_sequence
    }

    /// 返回本批次原始事件数量。
    #[must_use]
    pub fn event_count(&self) -> usize {
        self.event_ids.len()
    }

    /// 返回按序记录的事件幂等标识。
    #[must_use]
    pub fn event_ids(&self) -> &[BillingReservationId] {
        &self.event_ids
    }

    /// 返回按用户标识排序的聚合增量。
    #[must_use]
    pub fn users(&self) -> &[UserBillingDelta] {
        &self.users
    }

    /// 返回按令牌标识排序的聚合增量。
    #[must_use]
    pub fn tokens(&self) -> &[TokenBillingDelta] {
        &self.tokens
    }

    /// 返回按渠道标识排序的聚合增量。
    #[must_use]
    pub fn channels(&self) -> &[ChannelBillingDelta] {
        &self.channels
    }
}

impl fmt::Debug for BillingBatch {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BillingBatch")
            .field("event_count", &self.event_ids.len())
            .field("user_bucket_count", &self.users.len())
            .field("token_bucket_count", &self.tokens.len())
            .field("channel_bucket_count", &self.channels.len())
            .finish_non_exhaustive()
    }
}

/// sink 应用批次后的幂等结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BillingBatchApplyOutcome {
    /// 本次调用首次应用了该连续序列。
    Applied,
    /// sink 已应用过该连续序列，本次仅确认重放结果。
    Existing,
}

/// sink 的闭合错误分类；不得携带 SQL、标识或额度原值。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum BillingBatchSinkError {
    /// sink 配置不合法。
    #[error("计费批次 sink 配置无效")]
    InvalidConfiguration,
    /// 批次序列与持久化 checkpoint 冲突。
    #[error("计费批次序列冲突")]
    SequenceConflict,
    /// 持久化数据违反计费不变量。
    #[error("计费批次持久化不变量损坏")]
    Invariant,
    /// 持久化查询失败。
    #[error("计费批次持久化查询失败")]
    Query,
    /// 无法确认提交是否成功，只能重放同一批次。
    #[error("计费批次持久化结果未知")]
    OutcomeUnknown,
}

/// 对象安全 sink Future。
pub type BillingBatchSinkFuture<'a> = Pin<
    Box<dyn Future<Output = Result<BillingBatchApplyOutcome, BillingBatchSinkError>> + Send + 'a>,
>;

/// 批量落盘 sink；实现方必须按 writer 与连续序列提供 exactly-once 语义。
pub trait BillingBatchSink: Send + Sync + 'static {
    /// 原子应用完整批次；结果未知时调用方只会重放同一 writer/序列范围。
    fn apply<'a>(&'a self, batch: &'a BillingBatch) -> BillingBatchSinkFuture<'a>;
}

/// WAL 内核错误；不保留路径、标识、额度或底层 IO 诊断。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum BillingBatchError {
    /// writer 标识不能全零。
    #[error("计费 WAL writer 标识无效")]
    InvalidWriterId,
    /// 请求数增量无法表示为非负 `i64`。
    #[error("计费请求数增量超出范围")]
    InvalidRequestCount,
    /// 单个维度没有任何实际增量。
    #[error("计费维度增量不能为空")]
    EmptyDelta,
    /// 事件没有任何用户、令牌或渠道维度。
    #[error("计费批次事件不能为空")]
    EmptyEvent,
    /// 同一未确认事件标识携带了不同内容。
    #[error("计费批次事件幂等冲突")]
    EventConflict,
    /// 内存分桶 checked 汇总溢出。
    #[error("计费批次分桶汇总溢出")]
    AggregateOverflow,
    /// 持久化序号已达到数据库可表示上限。
    #[error("计费 WAL 序号已耗尽")]
    SequenceExhausted,
    /// 同一 WAL 目录已被其他 writer 占用。
    #[error("计费 WAL 目录已被占用")]
    WalLocked,
    /// WAL 头、记录、序列或校验和损坏。
    #[error("计费 WAL 数据损坏")]
    CorruptWal,
    /// WAL 文件操作失败。
    #[error("计费 WAL 文件操作失败")]
    WalIo,
    /// WAL 状态锁已中毒或一次文件操作后无法安全继续。
    #[error("计费 WAL 状态不可继续使用")]
    WalPoisoned,
    /// 隔离阻塞文件 IO 的任务异常终止。
    #[error("计费 WAL 文件任务异常终止")]
    FileTaskFailed,
    /// sink 拒绝或无法确认批次。
    #[error(transparent)]
    Sink(#[from] BillingBatchSinkError),
}

/// 单个事件写入 WAL 的幂等结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BillingBatchRecordOutcome {
    /// 首次写入并完成同步。
    Applied { sequence: u64 },
    /// 相同未确认事件已存在于 WAL。
    Existing { sequence: u64 },
}

/// 单次 flush 的结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BillingBatchFlushOutcome {
    /// 当前没有待落盘事件。
    Empty,
    /// sink 首次应用并确认删除了一个 WAL 分段。
    Applied { event_count: usize },
    /// sink 已应用过该分段，本次确认重放并删除分段。
    Existing { event_count: usize },
}
