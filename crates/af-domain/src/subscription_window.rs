use thiserror::Error;

use crate::SubscriptionCycle;

const SECONDS_PER_DAY: u64 = 86_400;
const CIVIL_EPOCH_OFFSET_DAYS: i64 = 719_468;
const DAYS_PER_400_YEARS: i64 = 146_097;
const MAX_SUPPORTED_UNIX_SECONDS: u64 = 253_402_300_799;

/// 周期窗口推进时允许一次任务跨越的最大日历周期数。
pub const MAX_SUBSCRIPTION_WINDOW_ADVANCES: u32 = 8_192;

/// 用户订阅当前额度窗口的 UTC 秒边界。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SubscriptionWindow {
    started_at: u64,
    ends_at: u64,
}

impl SubscriptionWindow {
    /// 校验并构造一个非空且处于受支持 UTC 日历范围内的订阅窗口。
    pub const fn new(started_at: u64, ends_at: u64) -> Result<Self, SubscriptionWindowError> {
        if started_at >= ends_at {
            return Err(SubscriptionWindowError::InvalidBounds);
        }
        if ends_at > MAX_SUPPORTED_UNIX_SECONDS {
            return Err(SubscriptionWindowError::InvalidTimestamp);
        }
        Ok(Self {
            started_at,
            ends_at,
        })
    }

    /// 按 UTC 日历边界计算绑定时应使用的首个窗口。
    pub fn initial(
        cycle: SubscriptionCycle,
        bound_at: u64,
    ) -> Result<Self, SubscriptionWindowError> {
        let bound_day = utc_day(bound_at)?;
        let start_day = period_start_day(cycle, bound_day)?;
        window_from_start(cycle, start_day)
    }

    /// 若窗口已经到期，推进到包含 `now` 的最新窗口；未到期时返回空值。
    pub fn advance_until(
        self,
        cycle: SubscriptionCycle,
        now: u64,
    ) -> Result<Option<SubscriptionWindowAdvance>, SubscriptionWindowError> {
        let _ = utc_day(now)?;
        if now < self.ends_at {
            return Ok(None);
        }

        let mut started_at = self.ends_at;
        let mut periods_elapsed = 0_u32;
        while periods_elapsed < MAX_SUBSCRIPTION_WINDOW_ADVANCES {
            let next = Self::initial(cycle, started_at)?;
            periods_elapsed += 1;
            if now < next.ends_at {
                return Ok(Some(SubscriptionWindowAdvance {
                    window: next,
                    periods_elapsed,
                }));
            }
            started_at = next.ends_at;
        }
        Err(SubscriptionWindowError::AdvanceLimitExceeded)
    }

    /// 返回窗口起点 Unix 秒数。
    #[must_use]
    pub const fn started_at(self) -> u64 {
        self.started_at
    }

    /// 返回窗口终点 Unix 秒数（不包含）。
    #[must_use]
    pub const fn ends_at(self) -> u64 {
        self.ends_at
    }
}

/// 一次周期推进的结果，包含跳过的完整周期数量。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SubscriptionWindowAdvance {
    window: SubscriptionWindow,
    periods_elapsed: u32,
}

impl SubscriptionWindowAdvance {
    /// 返回推进后的窗口。
    #[must_use]
    pub const fn window(self) -> SubscriptionWindow {
        self.window
    }

    /// 返回从旧窗口到新窗口之间经过的完整周期数。
    #[must_use]
    pub const fn periods_elapsed(self) -> u32 {
        self.periods_elapsed
    }
}

/// 周期窗口计算错误；不携带外部时间或标识。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum SubscriptionWindowError {
    /// Unix 秒无法转换为受支持的 UTC 时间。
    #[error("订阅窗口时间戳无效")]
    InvalidTimestamp,
    /// 起止边界没有形成非空窗口。
    #[error("订阅窗口边界无效")]
    InvalidBounds,
    /// 单次补偿跨越过多周期，必须由有界任务分批推进。
    #[error("订阅窗口推进超过单次上限")]
    AdvanceLimitExceeded,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CivilDate {
    year: i32,
    month: u8,
    day: u8,
}

fn utc_day(value: u64) -> Result<i64, SubscriptionWindowError> {
    if value > MAX_SUPPORTED_UNIX_SECONDS {
        return Err(SubscriptionWindowError::InvalidTimestamp);
    }
    i64::try_from(value / SECONDS_PER_DAY).map_err(|_| SubscriptionWindowError::InvalidTimestamp)
}

fn period_start_day(
    cycle: SubscriptionCycle,
    bound_day: i64,
) -> Result<i64, SubscriptionWindowError> {
    match cycle {
        SubscriptionCycle::Daily => Ok(bound_day),
        SubscriptionCycle::Weekly => {
            // Unix 纪元是周四，因此加三后对七取模可得到周一偏移。
            let days_since_monday = (bound_day + 3).rem_euclid(7);
            bound_day
                .checked_sub(days_since_monday)
                .ok_or(SubscriptionWindowError::InvalidTimestamp)
        }
        SubscriptionCycle::Monthly => {
            let date = civil_from_days(bound_day)?;
            days_from_civil(date.year, date.month, 1)
        }
        SubscriptionCycle::Yearly => {
            let date = civil_from_days(bound_day)?;
            days_from_civil(date.year, 1, 1)
        }
    }
}

fn window_from_start(
    cycle: SubscriptionCycle,
    start_day: i64,
) -> Result<SubscriptionWindow, SubscriptionWindowError> {
    let end_day = match cycle {
        SubscriptionCycle::Daily => start_day.checked_add(1),
        SubscriptionCycle::Weekly => start_day.checked_add(7),
        SubscriptionCycle::Monthly => Some(next_month_start(start_day)?),
        SubscriptionCycle::Yearly => Some(next_year_start(start_day)?),
    }
    .ok_or(SubscriptionWindowError::InvalidTimestamp)?;
    SubscriptionWindow::new(
        unix_seconds_at_day(start_day)?,
        unix_seconds_at_day(end_day)?,
    )
}

fn next_month_start(start_day: i64) -> Result<i64, SubscriptionWindowError> {
    let date = civil_from_days(start_day)?;
    let (year, month) = if date.month == 12 {
        (
            date.year
                .checked_add(1)
                .ok_or(SubscriptionWindowError::InvalidTimestamp)?,
            1,
        )
    } else {
        (date.year, date.month + 1)
    };
    days_from_civil(year, month, 1)
}

fn next_year_start(start_day: i64) -> Result<i64, SubscriptionWindowError> {
    let year = civil_from_days(start_day)?
        .year
        .checked_add(1)
        .ok_or(SubscriptionWindowError::InvalidTimestamp)?;
    days_from_civil(year, 1, 1)
}

fn unix_seconds_at_day(day: i64) -> Result<u64, SubscriptionWindowError> {
    let day = u64::try_from(day).map_err(|_| SubscriptionWindowError::InvalidTimestamp)?;
    let timestamp = day
        .checked_mul(SECONDS_PER_DAY)
        .ok_or(SubscriptionWindowError::InvalidTimestamp)?;
    if timestamp > MAX_SUPPORTED_UNIX_SECONDS {
        return Err(SubscriptionWindowError::InvalidTimestamp);
    }
    Ok(timestamp)
}

// 采用公历 400 年循环换算，避免领域 crate 为简单 UTC 日历引入外部时间依赖。
fn civil_from_days(days_since_epoch: i64) -> Result<CivilDate, SubscriptionWindowError> {
    let shifted = days_since_epoch
        .checked_add(CIVIL_EPOCH_OFFSET_DAYS)
        .ok_or(SubscriptionWindowError::InvalidTimestamp)?;
    let era = shifted.div_euclid(DAYS_PER_400_YEARS);
    let day_of_era = shifted.rem_euclid(DAYS_PER_400_YEARS);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    if month <= 2 {
        year += 1;
    }
    Ok(CivilDate {
        year: i32::try_from(year).map_err(|_| SubscriptionWindowError::InvalidTimestamp)?,
        month: u8::try_from(month).map_err(|_| SubscriptionWindowError::InvalidTimestamp)?,
        day: u8::try_from(day).map_err(|_| SubscriptionWindowError::InvalidTimestamp)?,
    })
}

fn days_from_civil(year: i32, month: u8, day: u8) -> Result<i64, SubscriptionWindowError> {
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return Err(SubscriptionWindowError::InvalidTimestamp);
    }
    let mut year = i64::from(year);
    if month <= 2 {
        year = year
            .checked_sub(1)
            .ok_or(SubscriptionWindowError::InvalidTimestamp)?;
    }
    let era = year.div_euclid(400);
    let year_of_era = year.rem_euclid(400);
    let month_prime = i64::from(month) + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * month_prime + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era.checked_mul(DAYS_PER_400_YEARS)
        .and_then(|value| value.checked_add(day_of_era))
        .and_then(|value| value.checked_sub(CIVIL_EPOCH_OFFSET_DAYS))
        .ok_or(SubscriptionWindowError::InvalidTimestamp)
}

#[cfg(test)]
mod tests {
    use super::*;

    const JANUARY_31_2026: u64 = 1_769_817_600;

    #[test]
    fn civil_calendar_round_trips_known_boundaries() {
        assert_eq!(
            civil_from_days(0).unwrap(),
            CivilDate {
                year: 1970,
                month: 1,
                day: 1,
            }
        );
        assert_eq!(days_from_civil(1970, 1, 1).unwrap(), 0);
        assert_eq!(days_from_civil(2024, 2, 29).unwrap(), 19_782);
        assert_eq!(
            civil_from_days(19_782).unwrap(),
            CivilDate {
                year: 2024,
                month: 2,
                day: 29,
            }
        );
    }

    #[test]
    fn initial_windows_follow_utc_calendar_boundaries() {
        let daily =
            SubscriptionWindow::initial(SubscriptionCycle::Daily, JANUARY_31_2026 + 12 * 3600)
                .unwrap();
        assert_eq!(daily.started_at(), JANUARY_31_2026);
        assert_eq!(daily.ends_at() - daily.started_at(), SECONDS_PER_DAY);

        let weekly =
            SubscriptionWindow::initial(SubscriptionCycle::Weekly, JANUARY_31_2026).unwrap();
        assert_eq!(weekly.started_at(), 1_769_385_600);
        assert_eq!(weekly.ends_at(), 1_769_990_400);

        let monthly =
            SubscriptionWindow::initial(SubscriptionCycle::Monthly, JANUARY_31_2026).unwrap();
        assert_eq!(monthly.started_at(), 1_767_225_600);
        assert_eq!(monthly.ends_at(), 1_769_904_000);
    }

    #[test]
    fn monthly_and_yearly_windows_handle_variable_lengths() {
        let monthly =
            SubscriptionWindow::initial(SubscriptionCycle::Monthly, JANUARY_31_2026).unwrap();
        let advanced = monthly
            .advance_until(SubscriptionCycle::Monthly, 1_776_211_200)
            .unwrap()
            .unwrap();
        assert_eq!(advanced.periods_elapsed(), 3);
        assert_eq!(advanced.window().started_at(), 1_775_001_600);
        assert_eq!(advanced.window().ends_at(), 1_777_593_600);

        let yearly = SubscriptionWindow::initial(SubscriptionCycle::Yearly, 1_735_689_600).unwrap();
        assert_eq!(yearly.ends_at() - yearly.started_at(), 31_536_000);
        let leap_year =
            SubscriptionWindow::initial(SubscriptionCycle::Yearly, 1_709_208_000).unwrap();
        assert_eq!(leap_year.started_at(), 1_704_067_200);
        assert_eq!(leap_year.ends_at(), 1_735_689_600);
        assert_eq!(leap_year.ends_at() - leap_year.started_at(), 31_622_400);
    }

    #[test]
    fn advance_is_idempotent_while_window_is_current() {
        let window = SubscriptionWindow::initial(SubscriptionCycle::Weekly, 1_769_817_600).unwrap();
        assert_eq!(
            window
                .advance_until(SubscriptionCycle::Weekly, window.ends_at() - 1)
                .unwrap(),
            None
        );
    }

    #[test]
    fn timestamps_outside_supported_calendar_fail_closed() {
        assert_eq!(
            SubscriptionWindow::initial(SubscriptionCycle::Daily, MAX_SUPPORTED_UNIX_SECONDS + 1),
            Err(SubscriptionWindowError::InvalidTimestamp)
        );
        assert_eq!(
            SubscriptionWindow::new(1, MAX_SUPPORTED_UNIX_SECONDS + 1),
            Err(SubscriptionWindowError::InvalidTimestamp)
        );
    }
}
