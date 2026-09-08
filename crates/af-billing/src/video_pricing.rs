use af_domain::Quota;
use af_protocol::{MAX_VIDEO_DURATION_SECONDS, VideoDuration, VideoResolution};
use rust_decimal::Decimal;
use thiserror::Error;

use crate::{PricingRatios, quota_math};

/// xAI 视频官方价目快照版本；升级价目时必须保留旧版本恢复逻辑。
pub const XAI_VIDEO_PRICE_CARD_VERSION: u16 = 1;
/// xAI 请求省略时长时使用的官方默认秒数。
pub const DEFAULT_XAI_VIDEO_DURATION_SECONDS: u8 = 8;

const MICRO_USD_SCALE: u32 = 6;
const STANDARD_480P_MICRO_USD: i64 = 50_000;
const STANDARD_720P_MICRO_USD: i64 = 70_000;
const V15_480P_MICRO_USD: i64 = 80_000;
const V15_720P_MICRO_USD: i64 = 140_000;
const V15_1080P_MICRO_USD: i64 = 250_000;

/// xAI 视频每秒价目或额度换算错误；不回显模型、价格或倍率。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum XaiVideoPricingError {
    /// 当前上游模型不在已审计官方价目表中。
    #[error("xAI 视频模型没有可用价目")]
    UnsupportedModel,
    /// 当前模型不支持请求的计费分辨率。
    #[error("xAI 视频模型不支持该计费分辨率")]
    UnsupportedResolution,
    /// 持久化价目快照与当前版本的闭合集合不一致。
    #[error("xAI 视频价目快照无效")]
    InvalidSnapshot,
    /// 美元金额、倍率或额度换算越界。
    #[error("xAI 视频计费换算失败")]
    Math,
}

/// 一次视频任务提交固定的官方价目和三层分组倍率。
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct XaiVideoPricingSnapshot {
    resolution: VideoResolution,
    ratios: PricingRatios,
}

impl XaiVideoPricingSnapshot {
    /// 固定请求有效计费分辨率；省略时按 xAI 默认 480p 计费，但审计层仍须保留缺失。
    #[must_use]
    pub const fn new(resolution: Option<VideoResolution>, ratios: PricingRatios) -> Self {
        Self {
            resolution: match resolution {
                Some(value) => value,
                None => VideoResolution::P480,
            },
            ratios,
        }
    }

    /// 从持久化倍率和有效分辨率恢复同一任务定价快照。
    #[must_use]
    pub const fn restore(resolution: VideoResolution, ratios: PricingRatios) -> Self {
        Self { resolution, ratios }
    }

    /// 返回持久化边界必须保存的有效计费分辨率。
    #[must_use]
    pub const fn resolution(self) -> VideoResolution {
        self.resolution
    }

    /// 返回持久化边界必须保存的请求级倍率。
    #[must_use]
    pub const fn ratios(self) -> PricingRatios {
        self.ratios
    }

    /// 按本次计划全部候选的最高官方单价计算 15 秒严格冻结上界。
    pub fn upper_bound<'a>(
        self,
        models: impl IntoIterator<Item = &'a str>,
    ) -> Result<Quota, XaiVideoPricingError> {
        let mut highest_rate = None;
        for model in models {
            let rate = self.rate_microusd(model)?;
            highest_rate = Some(highest_rate.map_or(rate, |current: i64| current.max(rate)));
        }
        let rate = highest_rate.ok_or(XaiVideoPricingError::UnsupportedModel)?;
        let quota = self.quota_for_rate(
            rate,
            VideoDuration::new(MAX_VIDEO_DURATION_SECONDS).expect("协议最大视频时长必须始终有效"),
        )?;
        if quota.is_zero() {
            Quota::new(1).map_err(|_| XaiVideoPricingError::Math)
        } else {
            Ok(quota)
        }
    }

    /// 按当前分辨率在整张官方价目表中的最高每秒价格计算 15 秒上界。
    pub fn maximum_upper_bound(self) -> Result<Quota, XaiVideoPricingError> {
        let rate = match self.resolution {
            VideoResolution::P480 => V15_480P_MICRO_USD,
            VideoResolution::P720 => V15_720P_MICRO_USD,
            VideoResolution::P1080 => V15_1080P_MICRO_USD,
        };
        let quota = self.quota_for_rate(
            rate,
            VideoDuration::new(MAX_VIDEO_DURATION_SECONDS).expect("协议最大视频时长必须始终有效"),
        )?;
        if quota.is_zero() {
            Quota::new(1).map_err(|_| XaiVideoPricingError::Math)
        } else {
            Ok(quota)
        }
    }

    /// 返回请求模型和有效分辨率对应的官方每秒微美元单价。
    pub fn rate_microusd(self, model: &str) -> Result<i64, XaiVideoPricingError> {
        rate_microusd(model, self.resolution)
    }

    /// 使用已接受模型的官方单价计算提交阶段 fallback。
    pub fn fallback_quota(
        self,
        rate_microusd: i64,
        requested_duration: Option<VideoDuration>,
    ) -> Result<Quota, XaiVideoPricingError> {
        let duration = match requested_duration {
            Some(value) => value,
            None => VideoDuration::new(DEFAULT_XAI_VIDEO_DURATION_SECONDS)
                .expect("xAI 默认视频时长必须位于协议边界内"),
        };
        self.quota_for_rate(rate_microusd, duration)
    }

    /// 按上游成功终态返回的真实时长计算最终额度。
    pub fn actual_quota(
        self,
        rate_microusd: i64,
        duration: VideoDuration,
    ) -> Result<Quota, XaiVideoPricingError> {
        self.quota_for_rate(rate_microusd, duration)
    }

    fn quota_for_rate(
        self,
        rate_microusd: i64,
        duration: VideoDuration,
    ) -> Result<Quota, XaiVideoPricingError> {
        if !rate_matches_resolution(rate_microusd, self.resolution) {
            return Err(XaiVideoPricingError::InvalidSnapshot);
        }
        let base_usd = Decimal::new(rate_microusd, MICRO_USD_SCALE)
            .checked_mul(Decimal::from(duration.seconds()))
            .ok_or(XaiVideoPricingError::Math)?;
        quota_math::quota_from_usd_with_ratios(base_usd, self.ratios.micros())
            .map_err(|_| XaiVideoPricingError::Math)
    }
}

impl std::fmt::Debug for XaiVideoPricingSnapshot {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("XaiVideoPricingSnapshot")
            .field("resolution", &self.resolution)
            .finish_non_exhaustive()
    }
}

fn rate_microusd(model: &str, resolution: VideoResolution) -> Result<i64, XaiVideoPricingError> {
    if model == "grok-imagine-video-1.5" || model.starts_with("grok-imagine-video-1.5-") {
        return Ok(match resolution {
            VideoResolution::P480 => V15_480P_MICRO_USD,
            VideoResolution::P720 => V15_720P_MICRO_USD,
            VideoResolution::P1080 => V15_1080P_MICRO_USD,
        });
    }
    if model == "grok-imagine-video" {
        return match resolution {
            VideoResolution::P480 => Ok(STANDARD_480P_MICRO_USD),
            VideoResolution::P720 => Ok(STANDARD_720P_MICRO_USD),
            VideoResolution::P1080 => Err(XaiVideoPricingError::UnsupportedResolution),
        };
    }
    Err(XaiVideoPricingError::UnsupportedModel)
}

const fn rate_matches_resolution(rate_microusd: i64, resolution: VideoResolution) -> bool {
    match resolution {
        VideoResolution::P480 => {
            matches!(rate_microusd, STANDARD_480P_MICRO_USD | V15_480P_MICRO_USD)
        }
        VideoResolution::P720 => {
            matches!(rate_microusd, STANDARD_720P_MICRO_USD | V15_720P_MICRO_USD)
        }
        VideoResolution::P1080 => rate_microusd == V15_1080P_MICRO_USD,
    }
}
