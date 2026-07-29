use std::time::{SystemTime, UNIX_EPOCH};

const SECONDS_PER_DAY: u64 = 24 * 60 * 60;

/// 返回当前 UTC 日内秒数；系统时钟早于 Unix 纪元时失败关闭。
pub(crate) fn current_utc_day_second() -> Option<u32> {
    let elapsed = SystemTime::now().duration_since(UNIX_EPOCH).ok()?;
    u32::try_from(elapsed.as_secs() % SECONDS_PER_DAY).ok()
}
