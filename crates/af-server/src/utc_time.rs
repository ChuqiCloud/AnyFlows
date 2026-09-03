use std::time::{SystemTime, UNIX_EPOCH};

use af_protocol::UtcDate;

const SECONDS_PER_DAY: u64 = 24 * 60 * 60;

/// 返回当前 UTC 日内秒数；系统时钟早于 Unix 纪元时失败关闭。
pub(crate) fn current_utc_day_second() -> Option<u32> {
    let elapsed = SystemTime::now().duration_since(UNIX_EPOCH).ok()?;
    u32::try_from(elapsed.as_secs() % SECONDS_PER_DAY).ok()
}

/// 返回当前 UTC 公历日期；系统时钟异常或日期超出受控范围时失败关闭。
pub(crate) fn current_utc_date() -> Option<UtcDate> {
    let elapsed = SystemTime::now().duration_since(UNIX_EPOCH).ok()?;
    utc_date_from_days(elapsed.as_secs() / SECONDS_PER_DAY)
}

/// 将 Unix 纪元后的 UTC 日数转换为公历日期，不依赖本地时区或可配置时间源。
fn utc_date_from_days(days_since_epoch: u64) -> Option<UtcDate> {
    let days = i64::try_from(days_since_epoch).ok()?;
    // Howard Hinnant 的纯整数公历转换；常量将 Unix 纪元平移至民用历原点。
    let shifted = days.checked_add(719_468)?;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.checked_sub(era.checked_mul(146_097)?)?;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = month_index + if month_index < 10 { 3 } else { -9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    UtcDate::new(
        u16::try_from(year).ok()?,
        u8::try_from(month).ok()?,
        u8::try_from(day).ok()?,
    )
    .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_date_conversion_uses_gregorian_utc_calendar() {
        assert_eq!(utc_date_from_days(0), UtcDate::new(1970, 1, 1).ok());
        assert_eq!(utc_date_from_days(18_321), UtcDate::new(2020, 2, 29).ok());
    }
}
