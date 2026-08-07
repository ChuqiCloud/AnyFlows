use thiserror::Error;
use time::{Date, Month, OffsetDateTime, UtcOffset};

use crate::SubscriptionCycle;

/// 周期窗口推进时允许一次任务跨越的最大日历周期数。
pub const MAX_SUBSCRIPTION_WINDOW_ADVANCES: u32 = 8_192;

/// 用户订阅当前额度窗口的 UTC 秒边界。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SubscriptionWindow {
    started_at: u64,
    ends_at: u64,
}

impl SubscriptionWindow {
    /// 校验并构造一个非空的订阅窗口。
    pub const fn new(started_at: u64, ends_at: u64) -> Result<Self, SubscriptionWindowError> {
        if started_at >= ends_at {
            return Err(SubscriptionWindowError::InvalidBounds);
        }
        if ends_at > i64::MAX as u64 {
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
        let timestamp = utc_timestamp(bound_at)?;
        let start_date = period_start_date(cycle, timestamp.date());
        window_from_start(cycle, start_date)
    }

    /// 若窗口已经到期，推进到包含 `now` 的最新窗口；未到期时返回空值。
    pub fn advance_until(
        self,
        cycle: SubscriptionCycle,
        now: u64,
    ) -> Result<Option<SubscriptionWindowAdvance>, SubscriptionWindowError> {
        let _ = utc_timestamp(now)?;
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

fn utc_timestamp(value: u64) -> Result<OffsetDateTime, SubscriptionWindowError> {
    let value = i64::try_from(value).map_err(|_| SubscriptionWindowError::InvalidTimestamp)?;
    OffsetDateTime::from_unix_timestamp(value)
        .map(|timestamp| timestamp.to_offset(UtcOffset::UTC))
        .map_err(|_| SubscriptionWindowError::InvalidTimestamp)
}

fn period_start_date(cycle: SubscriptionCycle, date: Date) -> Date {
    match cycle {
        SubscriptionCycle::Daily => date,
        SubscriptionCycle::Weekly => {
            date - time::Duration::days(i64::from(date.weekday().number_days_from_monday()))
        }
        SubscriptionCycle::Monthly => Date::from_calendar_date(date.year(), date.month(), 1)
            .expect("有效日期的月份第一天必须有效"),
        SubscriptionCycle::Yearly => Date::from_calendar_date(date.year(), Month::January, 1)
            .expect("有效日期的年份第一天必须有效"),
    }
}

fn window_from_start(
    cycle: SubscriptionCycle,
    start_date: Date,
) -> Result<SubscriptionWindow, SubscriptionWindowError> {
    let end_date = match cycle {
        SubscriptionCycle::Daily => start_date + time::Duration::days(1),
        SubscriptionCycle::Weekly => start_date + time::Duration::days(7),
        SubscriptionCycle::Monthly => next_month(start_date)?,
        SubscriptionCycle::Yearly => Date::from_calendar_date(
            start_date
                .year()
                .checked_add(1)
                .ok_or(SubscriptionWindowError::InvalidTimestamp)?,
            Month::January,
            1,
        )
        .map_err(|_| SubscriptionWindowError::InvalidTimestamp)?,
    };
    let started_at = start_date
        .midnight()
        .assume_utc()
        .unix_timestamp()
        .try_into()
        .map_err(|_| SubscriptionWindowError::InvalidTimestamp)?;
    let ends_at = end_date
        .midnight()
        .assume_utc()
        .unix_timestamp()
        .try_into()
        .map_err(|_| SubscriptionWindowError::InvalidTimestamp)?;
    SubscriptionWindow::new(started_at, ends_at)
}

fn next_month(date: Date) -> Result<Date, SubscriptionWindowError> {
    let (year, month) = if date.month() == Month::December {
        (
            date.year()
                .checked_add(1)
                .ok_or(SubscriptionWindowError::InvalidTimestamp)?,
            Month::January,
        )
    } else {
        (date.year(), month_after(date.month()))
    };
    Date::from_calendar_date(year, month, 1).map_err(|_| SubscriptionWindowError::InvalidTimestamp)
}

const fn month_after(month: Month) -> Month {
    match month {
        Month::January => Month::February,
        Month::February => Month::March,
        Month::March => Month::April,
        Month::April => Month::May,
        Month::May => Month::June,
        Month::June => Month::July,
        Month::July => Month::August,
        Month::August => Month::September,
        Month::September => Month::October,
        Month::October => Month::November,
        Month::November => Month::December,
        Month::December => Month::January,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const JANUARY_31_2026: u64 = 1_769_817_600;

    #[test]
    fn initial_windows_follow_utc_calendar_boundaries() {
        let daily =
            SubscriptionWindow::initial(SubscriptionCycle::Daily, JANUARY_31_2026 + 12 * 3600)
                .unwrap();
        assert_eq!(daily.started_at(), JANUARY_31_2026);
        assert_eq!(daily.ends_at() - daily.started_at(), 86_400);

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
}
