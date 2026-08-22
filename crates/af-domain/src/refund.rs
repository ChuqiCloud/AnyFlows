use std::fmt;

use crate::UserId;
use thiserror::Error;

const REFUND_ID_BYTES: usize = 16;
const REFUND_KEY_BYTES: usize = REFUND_ID_BYTES * 2;
const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";
pub const MAX_REFUND_ORDER_KEY_BYTES: usize = REFUND_KEY_BYTES;
pub const MAX_REFUND_PROVIDER_BYTES: usize = 64;

/// 退款业务标识构造错误；不保留外部输入。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RefundIdentifierError {
    /// 全零标识不能形成有效幂等边界。
    #[error("退款业务标识不能全为零")]
    AllZero,
    /// 持久化键不是固定长度的小写十六进制编码。
    #[error("退款持久化键格式无效")]
    InvalidEncoding,
}

macro_rules! refund_id {
    ($(#[$meta:meta])* $name:ident, $debug_name:literal) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name([u8; REFUND_ID_BYTES]);

        impl $name {
            /// 校验并构造非零的 128 位标识。
            pub const fn new(bytes: [u8; REFUND_ID_BYTES]) -> Result<Self, RefundIdentifierError> {
                let mut index = 0;
                while index < bytes.len() {
                    if bytes[index] != 0 {
                        return Ok(Self(bytes));
                    }
                    index += 1;
                }
                Err(RefundIdentifierError::AllZero)
            }

            /// 从数据库使用的 32 位小写十六进制键恢复标识。
            pub fn from_persistence_key(value: &str) -> Result<Self, RefundIdentifierError> {
                if value.len() != REFUND_KEY_BYTES {
                    return Err(RefundIdentifierError::InvalidEncoding);
                }
                let mut bytes = [0_u8; REFUND_ID_BYTES];
                for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
                    let high = decode_hex(pair[0]).ok_or(RefundIdentifierError::InvalidEncoding)?;
                    let low = decode_hex(pair[1]).ok_or(RefundIdentifierError::InvalidEncoding)?;
                    bytes[index] = (high << 4) | low;
                }
                Self::new(bytes)
            }

            /// 返回持久化边界使用的固定长度小写十六进制键。
            #[must_use]
            pub fn persistence_key(self) -> String {
                let mut encoded = String::with_capacity(REFUND_KEY_BYTES);
                for byte in self.0 {
                    encoded.push(char::from(HEX_DIGITS[usize::from(byte >> 4)]));
                    encoded.push(char::from(HEX_DIGITS[usize::from(byte & 0x0f)]));
                }
                encoded
            }
        }

        impl TryFrom<[u8; REFUND_ID_BYTES]> for $name {
            type Error = RefundIdentifierError;

            fn try_from(bytes: [u8; REFUND_ID_BYTES]) -> Result<Self, Self::Error> {
                Self::new(bytes)
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(concat!($debug_name, "(<redacted>)"))
            }
        }
    };
}

refund_id!(
    /// 退款请求的稳定公开关联标识。
    RefundRequestId,
    "RefundRequestId"
);
refund_id!(
    /// 创建退款请求时由调用方复用的用户范围幂等标识。
    RefundRequestKey,
    "RefundRequestKey"
);

/// 退款请求关联的订单类型。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i16)]
pub enum RefundOrderKind {
    /// 充值订单；首切片只记录原路退款请求，不回写钱包。
    Topup = 1,
    /// 订阅购买订单；首切片只记录原路退款请求，不撤销订阅权益。
    Subscription = 2,
}

impl RefundOrderKind {
    #[must_use]
    pub const fn code(self) -> i16 {
        self as i16
    }
}

impl TryFrom<i16> for RefundOrderKind {
    type Error = RefundStateCodeError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Topup),
            2 => Ok(Self::Subscription),
            _ => Err(RefundStateCodeError::InvalidOrderKind),
        }
    }
}

/// 退款请求的闭合状态机。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i16)]
pub enum RefundRequestStatus {
    /// 已核对订单事实，等待后续 Provider 提交。
    Requested = 1,
    /// 已向 Provider 提交或提交结果未知，等待原路回执。
    Submitted = 2,
    /// Provider 已确认退款成功。
    Succeeded = 3,
    /// Provider 明确拒绝或退款失败，可由后续策略重新提交。
    Failed = 4,
    /// 在尚未提交 Provider 前取消。
    Canceled = 5,
}

impl RefundRequestStatus {
    #[must_use]
    pub const fn code(self) -> i16 {
        self as i16
    }

    /// 只允许单向推进；Provider 回执和重试由后续切片使用。
    #[must_use]
    pub const fn can_transition_to(self, target: Self) -> bool {
        matches!(
            (self, target),
            (Self::Requested, Self::Submitted | Self::Canceled)
                | (Self::Submitted, Self::Succeeded | Self::Failed)
                | (Self::Failed, Self::Submitted)
        )
    }
}

impl TryFrom<i16> for RefundRequestStatus {
    type Error = RefundStateCodeError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::Requested),
            2 => Ok(Self::Submitted),
            3 => Ok(Self::Succeeded),
            4 => Ok(Self::Failed),
            5 => Ok(Self::Canceled),
            _ => Err(RefundStateCodeError::InvalidStatus),
        }
    }
}

/// 退款状态持久化代码错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RefundStateCodeError {
    /// 订单类型不属于闭合集合。
    #[error("退款订单类型无效")]
    InvalidOrderKind,
    /// 退款状态不属于闭合集合。
    #[error("退款状态无效")]
    InvalidStatus,
}

/// 创建退款请求时固化的订单与金额事实。
pub struct RefundRequestCreate {
    pub(super) request_id: RefundRequestId,
    pub(super) idempotency_key: RefundRequestKey,
    pub(super) user_id: UserId,
    pub(super) order_kind: RefundOrderKind,
    pub(super) order_key: String,
    pub(super) provider: String,
    pub(super) currency: String,
    pub(super) original_amount_minor: i64,
    pub(super) refund_amount_minor: i64,
    pub(super) created_at: u64,
}

impl RefundRequestCreate {
    /// 校验退款金额、订单范围和时间边界，构造尚未调用 Provider 的请求事实。
    #[allow(clippy::too_many_arguments, reason = "退款事实字段逐项对应持久化边界")]
    pub fn new(
        request_id: RefundRequestId,
        idempotency_key: RefundRequestKey,
        user_id: UserId,
        order_kind: RefundOrderKind,
        order_key: String,
        provider: String,
        currency: String,
        original_amount_minor: u64,
        refund_amount_minor: u64,
        created_at: u64,
    ) -> Result<Self, RefundRequestInputError> {
        if !valid_order_key(&order_key) || !valid_provider(&provider) || !valid_currency(&currency)
        {
            return Err(RefundRequestInputError::InvalidOrderFact);
        }
        let original_amount_minor = i64::try_from(original_amount_minor)
            .map_err(|_| RefundRequestInputError::InvalidAmount)?;
        let refund_amount_minor = i64::try_from(refund_amount_minor)
            .map_err(|_| RefundRequestInputError::InvalidAmount)?;
        if original_amount_minor <= 0
            || refund_amount_minor <= 0
            || refund_amount_minor > original_amount_minor
            || created_at > i64::MAX as u64
        {
            return Err(RefundRequestInputError::InvalidAmount);
        }
        Ok(Self {
            request_id,
            idempotency_key,
            user_id,
            order_kind,
            order_key,
            provider,
            currency,
            original_amount_minor,
            refund_amount_minor,
            created_at,
        })
    }

    #[must_use]
    pub const fn request_id(&self) -> RefundRequestId {
        self.request_id
    }
    #[must_use]
    pub const fn idempotency_key(&self) -> RefundRequestKey {
        self.idempotency_key
    }
    #[must_use]
    pub const fn user_id(&self) -> UserId {
        self.user_id
    }
    #[must_use]
    pub const fn order_kind(&self) -> RefundOrderKind {
        self.order_kind
    }
    #[must_use]
    pub fn order_key(&self) -> &str {
        &self.order_key
    }
    #[must_use]
    pub fn provider(&self) -> &str {
        &self.provider
    }
    #[must_use]
    pub fn currency(&self) -> &str {
        &self.currency
    }
    #[must_use]
    pub const fn original_amount_minor(&self) -> i64 {
        self.original_amount_minor
    }
    #[must_use]
    pub const fn refund_amount_minor(&self) -> i64 {
        self.refund_amount_minor
    }
    #[must_use]
    pub const fn created_at(&self) -> u64 {
        self.created_at
    }
}

impl fmt::Debug for RefundRequestCreate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RefundRequestCreate(<redacted>)")
    }
}

/// 已持久化的退款请求快照。
pub struct RefundRequestRecord {
    pub(super) database_id: i64,
    pub(super) request_id: RefundRequestId,
    pub(super) idempotency_key: RefundRequestKey,
    pub(super) user_id: UserId,
    pub(super) order_kind: RefundOrderKind,
    pub(super) order_key: String,
    pub(super) provider: String,
    pub(super) currency: String,
    pub(super) original_amount_minor: i64,
    pub(super) refund_amount_minor: i64,
    pub(super) status: RefundRequestStatus,
    pub(super) version: u64,
    pub(super) created_at: u64,
    pub(super) updated_at: u64,
}

impl RefundRequestRecord {
    /// 从数据库快照恢复记录，并在离开持久化边界时重新校验闭合状态。
    #[allow(
        clippy::too_many_arguments,
        reason = "持久化快照字段与退款事实一一对应"
    )]
    pub fn from_persistence(
        database_id: i64,
        request_id: RefundRequestId,
        idempotency_key: RefundRequestKey,
        user_id: UserId,
        order_kind: RefundOrderKind,
        order_key: String,
        provider: String,
        currency: String,
        original_amount_minor: i64,
        refund_amount_minor: i64,
        status: RefundRequestStatus,
        version: i64,
        created_at: u64,
        updated_at: u64,
    ) -> Result<Self, RefundRequestInputError> {
        if database_id <= 0 || version <= 0 || updated_at < created_at {
            return Err(RefundRequestInputError::InvalidPersistedState);
        }
        let create = RefundRequestCreate::new(
            request_id,
            idempotency_key,
            user_id,
            order_kind,
            order_key,
            provider,
            currency,
            u64::try_from(original_amount_minor)
                .map_err(|_| RefundRequestInputError::InvalidPersistedState)?,
            u64::try_from(refund_amount_minor)
                .map_err(|_| RefundRequestInputError::InvalidPersistedState)?,
            created_at,
        )
        .map_err(|_| RefundRequestInputError::InvalidPersistedState)?;
        Ok(Self {
            database_id,
            request_id: create.request_id,
            idempotency_key: create.idempotency_key,
            user_id: create.user_id,
            order_kind: create.order_kind,
            order_key: create.order_key,
            provider: create.provider,
            currency: create.currency,
            original_amount_minor: create.original_amount_minor,
            refund_amount_minor: create.refund_amount_minor,
            status,
            version: u64::try_from(version)
                .map_err(|_| RefundRequestInputError::InvalidPersistedState)?,
            created_at,
            updated_at,
        })
    }

    #[must_use]
    pub const fn request_id(&self) -> RefundRequestId {
        self.request_id
    }
    #[must_use]
    pub const fn idempotency_key(&self) -> RefundRequestKey {
        self.idempotency_key
    }
    #[must_use]
    pub const fn user_id(&self) -> UserId {
        self.user_id
    }
    #[must_use]
    pub const fn order_kind(&self) -> RefundOrderKind {
        self.order_kind
    }
    #[must_use]
    pub fn order_key(&self) -> &str {
        &self.order_key
    }
    #[must_use]
    pub fn provider(&self) -> &str {
        &self.provider
    }
    #[must_use]
    pub fn currency(&self) -> &str {
        &self.currency
    }
    #[must_use]
    pub const fn original_amount_minor(&self) -> i64 {
        self.original_amount_minor
    }
    #[must_use]
    pub const fn refund_amount_minor(&self) -> i64 {
        self.refund_amount_minor
    }
    #[must_use]
    pub const fn status(&self) -> RefundRequestStatus {
        self.status
    }
    #[must_use]
    pub const fn version(&self) -> u64 {
        self.version
    }
    #[must_use]
    pub const fn created_at(&self) -> u64 {
        self.created_at
    }
    #[must_use]
    pub const fn updated_at(&self) -> u64 {
        self.updated_at
    }

    pub fn matches_create(&self, write: &RefundRequestCreate) -> bool {
        self.request_id == write.request_id
            && self.user_id == write.user_id
            && self.order_kind == write.order_kind
            && self.order_key == write.order_key
            && self.provider == write.provider
            && self.currency == write.currency
            && self.original_amount_minor == write.original_amount_minor
            && self.refund_amount_minor == write.refund_amount_minor
    }
}

impl fmt::Debug for RefundRequestRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RefundRequestRecord(<redacted>)")
    }
}

/// 创建退款请求后的闭合结果。
pub enum RefundRequestCreateOutcome {
    /// 本次调用创建了新事实。
    Created(RefundRequestRecord),
    /// 相同用户范围幂等事实已经存在。
    Existing(RefundRequestRecord),
}

impl fmt::Debug for RefundRequestCreateOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Created(_) => {
                formatter.write_str("RefundRequestCreateOutcome::Created(<redacted>)")
            }
            Self::Existing(_) => {
                formatter.write_str("RefundRequestCreateOutcome::Existing(<redacted>)")
            }
        }
    }
}

/// 退款请求输入错误；不携带订单、Provider 或金额原文。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RefundRequestInputError {
    /// 订单关联事实、Provider 或币种不符合边界。
    #[error("退款订单事实无效")]
    InvalidOrderFact,
    /// 金额不是正整数最小单位或超出原订单金额。
    #[error("退款金额无效")]
    InvalidAmount,
    /// 数据库快照违反正数、时间或字段闭合约束。
    #[error("退款持久化状态无效")]
    InvalidPersistedState,
}

fn valid_order_key(value: &str) -> bool {
    value.len() == MAX_REFUND_ORDER_KEY_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        && value.bytes().any(|byte| byte != b'0')
}

fn valid_provider(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_REFUND_PROVIDER_BYTES
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
        })
}

fn valid_currency(value: &str) -> bool {
    value.len() == 3 && value.bytes().all(|byte| byte.is_ascii_uppercase())
}

const fn decode_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}
