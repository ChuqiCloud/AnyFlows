use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;

const DAY: i64 = 86_400;
const WEEK: i64 = 7 * DAY;

/// 每个分组允许的周计划规则数。
pub const MAX_GROUP_PEAK_WINDOWS: usize = 32;
/// 单条规则允许指定的公开计费模型数。
pub const MAX_GROUP_PEAK_MODELS: usize = 64;

/// 同一时刻捕获的 UTC 星期与日内秒数；周一为一周起点。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PricingWeekTime(u32);

impl PricingWeekTime {
    /// ISO 星期范围为 1..=7，日内秒范围为 0..86400。
    pub fn new(weekday: u8, second: u32) -> Result<Self, GroupPeakScheduleError> {
        if !(1..=7).contains(&weekday) || second >= 86_400 {
            return Err(GroupPeakScheduleError::InvalidWindow);
        }
        Ok(Self(u32::from(weekday - 1) * 86_400 + second))
    }

    /// 从一次读取的 Unix 时间计算，避免午夜时星期和时刻来自不同采样。
    #[must_use]
    pub fn from_unix_seconds(seconds: u64) -> Self {
        let days = seconds / 86_400;
        let week_second = ((days % 7 + 3) % 7) * 86_400 + seconds % 86_400;
        Self(u32::try_from(week_second).expect("weekly second fits u32"))
    }

    /// 返回 UTC 日内秒数，供旧版每日窗口使用。
    #[must_use]
    pub const fn day_second(self) -> u32 {
        self.0 % 86_400
    }
}

/// 周期性分时倍率；跨午夜时星期指起始日，模型为空表示全部模型。
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GroupPeakWindow {
    weekdays: Vec<u8>,
    start_second: u32,
    end_second: u32,
    ratio_micros: i64,
    #[serde(default)]
    models: Vec<String>,
    #[serde(default)]
    utc_offset_minutes: i16,
}

impl GroupPeakWindow {
    /// 校验一条周计划；终点可为 86400，表示起始日结束。
    pub fn new(
        weekdays: Vec<u8>,
        start_second: u32,
        end_second: u32,
        ratio_micros: i64,
        models: Vec<String>,
        utc_offset_minutes: i16,
    ) -> Result<Self, GroupPeakScheduleError> {
        let window = Self {
            weekdays,
            start_second,
            end_second,
            ratio_micros,
            models,
            utc_offset_minutes,
        };
        window.validate()?;
        Ok(window)
    }

    /// 返回采用 ISO 编号的起始星期。
    #[must_use]
    pub fn weekdays(&self) -> &[u8] {
        &self.weekdays
    }
    /// 返回本地日内起点秒数。
    #[must_use]
    pub const fn start_second(&self) -> u32 {
        self.start_second
    }
    /// 返回本地日内终点秒数（不包含）。
    #[must_use]
    pub const fn end_second(&self) -> u32 {
        self.end_second
    }
    /// 返回额外应用的百万分比倍率。
    #[must_use]
    pub const fn ratio_micros(&self) -> i64 {
        self.ratio_micros
    }
    /// 返回精确匹配的公开计费模型名称，空列表表示全部。
    #[must_use]
    pub fn models(&self) -> &[String] {
        &self.models
    }
    /// 返回计划使用的固定 UTC 偏移分钟数。
    #[must_use]
    pub const fn utc_offset_minutes(&self) -> i16 {
        self.utc_offset_minutes
    }

    fn validate(&self) -> Result<(), GroupPeakScheduleError> {
        if self.weekdays.is_empty()
            || self.weekdays.len() > 7
            || self.weekdays.iter().any(|day| !(1..=7).contains(day))
            || self.weekdays.iter().collect::<BTreeSet<_>>().len() != self.weekdays.len()
            || self.start_second >= 86_400
            || self.end_second > 86_400
            || self.start_second == self.end_second
            || self.ratio_micros < 0
            || !(-720..=840).contains(&self.utc_offset_minutes)
            || self.models.len() > MAX_GROUP_PEAK_MODELS
            || self.models.iter().collect::<BTreeSet<_>>().len() != self.models.len()
            || self.models.iter().any(|model| {
                model.is_empty()
                    || model.len() > 256
                    || model.trim() != model
                    || model.chars().any(char::is_control)
            })
        {
            return Err(GroupPeakScheduleError::InvalidWindow);
        }
        Ok(())
    }

    fn intervals(&self) -> Vec<(i64, i64)> {
        let duration = if self.end_second > self.start_second {
            i64::from(self.end_second - self.start_second)
        } else {
            DAY - i64::from(self.start_second) + i64::from(self.end_second)
        };
        let mut intervals = Vec::with_capacity(self.weekdays.len() * 2);
        for day in &self.weekdays {
            let start = (i64::from(day - 1) * DAY + i64::from(self.start_second)
                - i64::from(self.utc_offset_minutes) * 60)
                .rem_euclid(WEEK);
            let end = start + duration;
            intervals.push((start, end.min(WEEK)));
            if end > WEEK {
                intervals.push((0, end - WEEK));
            }
        }
        intervals
    }

    fn intersects_models(&self, other: &Self) -> bool {
        self.models.is_empty()
            || other.models.is_empty()
            || self.models.iter().any(|model| other.models.contains(model))
    }

    fn contains_time(&self, time: PricingWeekTime) -> bool {
        let local = (i64::from(time.0) + i64::from(self.utc_offset_minutes) * 60).rem_euclid(WEEK);
        let duration =
            (i64::from(self.end_second) - i64::from(self.start_second) - 1).rem_euclid(DAY) + 1;
        self.weekdays.iter().any(|day| {
            let start = i64::from(day - 1) * DAY + i64::from(self.start_second);
            (local - start).rem_euclid(WEEK) < duration
        })
    }
}

/// 已校验、互不冲突的分组周计划；匹配规则只应用一次。
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct GroupPeakSchedule(Vec<GroupPeakWindow>);

impl GroupPeakSchedule {
    /// 拒绝超限规则，以及在任意共同模型上时间重叠的规则。
    pub fn new(windows: Vec<GroupPeakWindow>) -> Result<Self, GroupPeakScheduleError> {
        if windows.len() > MAX_GROUP_PEAK_WINDOWS {
            return Err(GroupPeakScheduleError::TooManyWindows);
        }
        for (index, window) in windows.iter().enumerate() {
            window.validate()?;
            let intervals = window.intervals();
            for other in &windows[..index] {
                if window.intersects_models(other)
                    && intervals.iter().any(|(start, end)| {
                        other
                            .intervals()
                            .iter()
                            .any(|(other_start, other_end)| start < other_end && other_start < end)
                    })
                {
                    return Err(GroupPeakScheduleError::OverlappingWindows);
                }
            }
        }
        Ok(Self(windows))
    }

    /// 返回持久化和管理接口使用的规则。
    #[must_use]
    pub fn windows(&self) -> &[GroupPeakWindow] {
        &self.0
    }

    /// 按请求固定的时刻及公开计费模型匹配额外倍率。
    #[must_use]
    pub fn ratio_for(&self, model: &str, time: PricingWeekTime) -> Option<i64> {
        self.0
            .iter()
            .find(|window| {
                (window.models.is_empty() || window.models.iter().any(|name| name == model))
                    && window.contains_time(time)
            })
            .map(GroupPeakWindow::ratio_micros)
    }
}

/// 已校验的模型级周计划；模型级规则按模型价格记录归属，不允许再声明模型列表。
///
/// 命中时由请求定价层覆盖分组当前高峰倍率；未命中时仍采用分组周计划。
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ModelPeakSchedule(Vec<GroupPeakWindow>);

impl ModelPeakSchedule {
    /// 校验模型级计划。每条规则只能作用于所属模型，且相同时间不得重叠。
    pub fn new(windows: Vec<GroupPeakWindow>) -> Result<Self, GroupPeakScheduleError> {
        if windows.iter().any(|window| !window.models().is_empty()) {
            return Err(GroupPeakScheduleError::InvalidWindow);
        }
        GroupPeakSchedule::new(windows.clone())?;
        Ok(Self(windows))
    }

    /// 返回可持久化和配置展示的周计划规则。
    #[must_use]
    pub fn windows(&self) -> &[GroupPeakWindow] {
        &self.0
    }

    /// 返回该模型在捕获时刻的倍率覆盖；未命中时不覆盖分组计划。
    #[must_use]
    pub fn ratio_at(&self, time: PricingWeekTime) -> Option<i64> {
        self.0
            .iter()
            .find(|window| window.contains_time(time))
            .map(GroupPeakWindow::ratio_micros)
    }
}

/// 周计划公开校验错误。
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum GroupPeakScheduleError {
    /// 星期、时间、模型或倍率无效。
    #[error("分时计费规则参数无效")]
    InvalidWindow,
    /// 规则数超过分组容量。
    #[error("分时计费规则超过数量上限")]
    TooManyWindows,
    /// 相同模型在同一时刻只能命中一条规则。
    #[error("分时计费规则的时间和模型范围重叠")]
    OverlappingWindows,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(
        days: Vec<u8>,
        start: u32,
        end: u32,
        ratio: i64,
        models: &[&str],
        offset: i16,
    ) -> GroupPeakWindow {
        GroupPeakWindow::new(
            days,
            start,
            end,
            ratio,
            models.iter().map(|name| (*name).to_owned()).collect(),
            offset,
        )
        .unwrap()
    }

    fn at(day: u8, hour: u32) -> PricingWeekTime {
        PricingWeekTime::new(day, hour * 3_600).unwrap()
    }

    #[test]
    fn weekly_model_discount_obeys_half_open_boundaries() {
        let schedule = GroupPeakSchedule::new(vec![window(
            vec![1, 3, 5],
            8 * 3_600,
            20 * 3_600,
            500_000,
            &["deepseek-chat"],
            0,
        )])
        .unwrap();
        assert_eq!(schedule.ratio_for("deepseek-chat", at(1, 8)), Some(500_000));
        assert_eq!(
            schedule.ratio_for("deepseek-chat", at(1, 19)),
            Some(500_000)
        );
        assert_eq!(schedule.ratio_for("deepseek-chat", at(1, 20)), None);
        assert_eq!(schedule.ratio_for("deepseek-chat", at(2, 8)), None);
        assert_eq!(schedule.ratio_for("other-model", at(1, 8)), None);
    }

    #[test]
    fn model_schedule_is_optional_non_overlapping_and_scoped_by_its_own_model() {
        assert_eq!(ModelPeakSchedule::default().ratio_at(at(1, 8)), None);
        let schedule = ModelPeakSchedule::new(vec![window(
            vec![1],
            8 * 3_600,
            20 * 3_600,
            500_000,
            &[],
            0,
        )])
        .unwrap();
        assert_eq!(schedule.ratio_at(at(1, 8)), Some(500_000));
        assert_eq!(schedule.ratio_at(at(1, 20)), None);
        assert_eq!(
            ModelPeakSchedule::new(vec![window(vec![1], 0, 3_600, 1, &["other-model"], 0)]),
            Err(GroupPeakScheduleError::InvalidWindow),
        );
        assert_eq!(
            ModelPeakSchedule::new(vec![
                window(vec![7], 23 * 3_600, 3_600, 1, &[], 0),
                window(vec![1], 8 * 3_600, 10 * 3_600, 1, &[], 480),
            ]),
            Err(GroupPeakScheduleError::OverlappingWindows),
        );
    }

    #[test]
    fn beijing_overnight_uses_start_day_across_week_boundary() {
        let schedule = GroupPeakSchedule::new(vec![window(
            vec![7],
            23 * 3_600,
            9 * 3_600,
            2_000_000,
            &[],
            480,
        )])
        .unwrap();
        assert_eq!(schedule.ratio_for("any", at(7, 15)), Some(2_000_000));
        assert_eq!(schedule.ratio_for("any", at(1, 0)), Some(2_000_000));
        assert_eq!(schedule.ratio_for("any", at(1, 1)), None);
        assert_eq!(schedule.ratio_for("any", at(6, 23)), None);
    }

    #[test]
    fn timezone_can_shift_start_into_previous_utc_week() {
        let schedule =
            GroupPeakSchedule::new(vec![window(vec![1], 0, 3_600, 0, &[], 480)]).unwrap();
        assert_eq!(schedule.ratio_for("any", at(7, 16)), Some(0));
        assert_eq!(schedule.ratio_for("any", at(7, 17)), None);
        assert_eq!(schedule.ratio_for("any", at(1, 0)), None);
    }

    #[test]
    fn end_of_day_supports_full_day_and_zero_multiplier() {
        let schedule = GroupPeakSchedule::new(vec![window(vec![6], 0, 86_400, 0, &[], 0)]).unwrap();
        assert_eq!(schedule.ratio_for("any", at(6, 0)), Some(0));
        assert_eq!(schedule.ratio_for("any", at(6, 23)), Some(0));
        assert_eq!(schedule.ratio_for("any", at(7, 0)), None);
        assert_eq!(
            GroupPeakSchedule::default().ratio_for("any", at(6, 0)),
            None
        );
    }

    #[test]
    fn overlaps_compare_model_scopes_timezones_and_week_wrap() {
        let sunday = window(vec![7], 23 * 3_600, 3_600, 2_000_000, &["ds"], 0);
        let monday = window(vec![1], 8 * 3_600, 10 * 3_600, 500_000, &["ds"], 480);
        assert_eq!(
            GroupPeakSchedule::new(vec![sunday.clone(), monday.clone()]),
            Err(GroupPeakScheduleError::OverlappingWindows)
        );
        let other_model = window(vec![1], 8 * 3_600, 10 * 3_600, 500_000, &["glm"], 480);
        assert!(GroupPeakSchedule::new(vec![sunday.clone(), other_model]).is_ok());
        let all_models = window(vec![1], 0, 2 * 3_600, 500_000, &[], 0);
        assert_eq!(
            GroupPeakSchedule::new(vec![sunday.clone(), all_models]),
            Err(GroupPeakScheduleError::OverlappingWindows)
        );
        let adjacent = window(vec![1], 3_600, 2 * 3_600, 500_000, &["ds"], 0);
        assert!(GroupPeakSchedule::new(vec![sunday, adjacent]).is_ok());
    }

    #[test]
    fn rejects_invalid_days_times_models_ratios_and_rule_count() {
        for days in [vec![], vec![0], vec![8], vec![1, 1]] {
            assert!(GroupPeakWindow::new(days, 0, 3_600, 1, vec![], 0).is_err());
        }
        for (start, end) in [(0, 0), (86_400, 0), (0, 86_401)] {
            assert!(GroupPeakWindow::new(vec![1], start, end, 1, vec![], 0).is_err());
        }
        assert!(GroupPeakWindow::new(vec![1], 0, 1, -1, vec![], 0).is_err());
        assert!(GroupPeakWindow::new(vec![1], 0, 1, 1, vec![], 841).is_err());
        for names in [
            vec!["".to_owned()],
            vec![" ds".to_owned()],
            vec!["ds".to_owned(), "ds".to_owned()],
            vec!["x".repeat(257)],
        ] {
            assert!(GroupPeakWindow::new(vec![1], 0, 1, 1, names, 0).is_err());
        }
        let one = window(vec![1], 0, 1, 1, &[], 0);
        assert_eq!(
            GroupPeakSchedule::new(vec![one; 33]),
            Err(GroupPeakScheduleError::TooManyWindows)
        );
        let unchecked: GroupPeakWindow = serde_json::from_value(serde_json::json!({
            "weekdays": [0], "start_second": 0, "end_second": 1, "ratio_micros": 1
        }))
        .unwrap();
        assert_eq!(
            GroupPeakSchedule::new(vec![unchecked]),
            Err(GroupPeakScheduleError::InvalidWindow)
        );
    }

    #[test]
    fn unix_clock_uses_iso_week_and_single_timestamp() {
        assert_eq!(PricingWeekTime::from_unix_seconds(0), at(4, 0));
        assert_eq!(PricingWeekTime::from_unix_seconds(4 * 86_400), at(1, 0));
        assert_eq!(
            PricingWeekTime::from_unix_seconds(4 * 86_400 - 1),
            PricingWeekTime::new(7, 86_399).unwrap()
        );
        assert!(PricingWeekTime::new(0, 0).is_err());
        assert!(PricingWeekTime::new(1, 86_400).is_err());
        assert!(PricingWeekTime::from_unix_seconds(u64::MAX).day_second() < 86_400);
    }
}
