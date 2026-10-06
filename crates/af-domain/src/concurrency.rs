use std::num::NonZeroU32;

use thiserror::Error;

/// 账号或用户可同时占用的正整数请求槽位上限。
///
/// 持久化层的空值和零值都表示不限并发，只有正数才构造本值对象。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ConcurrencyLimit(NonZeroU32);

impl ConcurrencyLimit {
    /// 构造正整数并发上限；零值必须由调用方归一为 `None`。
    pub const fn new(value: u32) -> Result<Self, ConcurrencyLimitError> {
        match NonZeroU32::new(value) {
            Some(value) => Ok(Self(value)),
            None => Err(ConcurrencyLimitError),
        }
    }

    /// 返回 Redis 原子比较使用的正整数上限。
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0.get()
    }
}

/// 并发上限不是正整数。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("并发上限必须大于零")]
pub struct ConcurrencyLimitError;
