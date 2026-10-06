use std::time::Duration;

use thiserror::Error;

/// 渠道级上游超时允许的最小秒数。
pub const MIN_CHANNEL_TIMEOUT_SECS: u64 = 1;
/// 渠道级上游超时允许的最大秒数，与当前单流硬期限保持一致。
pub const MAX_CHANNEL_TIMEOUT_SECS: u64 = 900;

/// 渠道级上游读取停顿与完整请求共用的超时覆盖。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ChannelTimeout {
    seconds: u64,
}

impl ChannelTimeout {
    /// 校验并创建渠道超时；零值和超过单流硬期限的值都会被拒绝。
    pub const fn new(seconds: u64) -> Result<Self, ChannelTimeoutError> {
        if seconds < MIN_CHANNEL_TIMEOUT_SECS || seconds > MAX_CHANNEL_TIMEOUT_SECS {
            return Err(ChannelTimeoutError);
        }
        Ok(Self { seconds })
    }

    /// 返回管理 API 和持久化使用的秒数。
    #[must_use]
    pub const fn seconds(self) -> u64 {
        self.seconds
    }

    /// 返回 HTTP Client 使用的精确时长。
    #[must_use]
    pub const fn duration(self) -> Duration {
        Duration::from_secs(self.seconds)
    }
}

/// 渠道级上游超时不在受支持范围内。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[error("渠道上游超时必须位于允许范围内")]
pub struct ChannelTimeoutError;
