use std::fmt;

use af_domain::Quota;
use af_protocol::{ImageCount, ImageDimensions};
use thiserror::Error;

use crate::quota_math::{self, QuotaMathError};

/// 单次多模态计费最多组合的独立维度数量。
pub const MAX_MULTIMODAL_DIMENSIONS: usize = 3;
/// 图片张数的统一业务上限。
pub const MAX_BILLING_IMAGE_COUNT: i64 = 10;
/// 媒体时长的统一业务上限，单位为秒，当前固定为二十四小时。
pub const MAX_BILLING_DURATION_SECONDS: i64 = 86_400;
/// 分辨率单边的统一业务上限，单位为像素，当前固定为 16K。
pub const MAX_BILLING_RESOLUTION_EDGE: i64 = 16_384;
/// 分辨率总像素的统一业务上限，当前固定为 16K 正方形。
pub const MAX_BILLING_RESOLUTION_PIXELS: i64 = 268_435_456;

/// 多模态维度输入违反非负性、零值或固定上限时的闭合错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum MultimodalDimensionError {
    /// 外部输入为负数，不能作为计费维度。
    #[error("多模态计费维度不能为负数")]
    Negative,
    /// 该值必须为正数，例如分辨率的宽和高。
    #[error("多模态计费维度不能为零")]
    Zero,
    /// 输入超过固定业务上限。
    #[error("多模态计费维度超出允许范围")]
    OutOfRange,
    /// 宽高相乘或像素换算发生溢出。
    #[error("多模态计费维度计算溢出")]
    Overflow,
}

/// 多模态计费乘数组合或应用额度时的闭合错误。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum MultimodalBillingError {
    /// 计费维度值对象未通过边界校验。
    #[error(transparent)]
    Dimension(#[from] MultimodalDimensionError),
    /// 组合后的乘数超过可表示的非负 `i64` 范围。
    #[error("多模态计费乘数组合溢出")]
    FactorOverflow,
    /// 组合的维度数量超过固定上限。
    #[error("多模态计费维度数量超出上限")]
    TooManyDimensions,
    /// 乘数应用到额度时发生 checked 溢出。
    #[error(transparent)]
    Quota(#[from] QuotaMathError),
}

/// 已校验的图片张数。
///
/// 零值保留为合法的中性结果，便于上游明确报告“没有生成结果”；协议层若要求至少一张，
/// 应在协议边界使用自己的 `ImageCount` 校验，而不是在计费层猜测供应商语义。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BillingImageCount(i64);

impl BillingImageCount {
    /// 从有符号外部值构造图片张数。
    pub const fn new(value: i64) -> Result<Self, MultimodalDimensionError> {
        if value < 0 {
            return Err(MultimodalDimensionError::Negative);
        }
        if value > MAX_BILLING_IMAGE_COUNT {
            return Err(MultimodalDimensionError::OutOfRange);
        }
        Ok(Self(value))
    }

    /// 从无符号外部值构造图片张数，并在转换前检查范围。
    pub fn from_u64(value: u64) -> Result<Self, MultimodalDimensionError> {
        let value = i64::try_from(value).map_err(|_| MultimodalDimensionError::OutOfRange)?;
        Self::new(value)
    }

    /// 从已经通过协议边界校验的图片张数创建计费值对象。
    pub fn from_protocol(value: ImageCount) -> Result<Self, MultimodalDimensionError> {
        Self::new(i64::from(value.get()))
    }

    /// 返回图片张数。
    #[must_use]
    pub const fn count(self) -> i64 {
        self.0
    }

    /// 返回可参与 checked 组合的乘数。
    #[must_use]
    pub const fn factor(self) -> BillingFactor {
        BillingFactor(self.0)
    }
}

/// 已校验的媒体时长，单位为整秒。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BillingDurationSeconds(i64);

impl BillingDurationSeconds {
    /// 从有符号外部值构造媒体时长。
    pub const fn new(value: i64) -> Result<Self, MultimodalDimensionError> {
        if value < 0 {
            return Err(MultimodalDimensionError::Negative);
        }
        if value > MAX_BILLING_DURATION_SECONDS {
            return Err(MultimodalDimensionError::OutOfRange);
        }
        Ok(Self(value))
    }

    /// 从无符号外部值构造媒体时长，并在转换前检查范围。
    pub fn from_u64(value: u64) -> Result<Self, MultimodalDimensionError> {
        let value = i64::try_from(value).map_err(|_| MultimodalDimensionError::OutOfRange)?;
        Self::new(value)
    }

    /// 返回整秒时长。
    #[must_use]
    pub const fn seconds(self) -> i64 {
        self.0
    }

    /// 返回可参与 checked 组合的乘数。
    #[must_use]
    pub const fn factor(self) -> BillingFactor {
        BillingFactor(self.0)
    }
}

/// 已校验的总像素数。
///
/// 该值可以来自已校验的宽高，也可以来自上游只返回总像素的用量事实；两种来源共用同一
/// 固定上限，避免调用方绕过边界直接参与计费。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BillingPixelCount(i64);

impl BillingPixelCount {
    /// 从有符号外部值构造总像素数。
    pub const fn new(value: i64) -> Result<Self, MultimodalDimensionError> {
        if value < 0 {
            return Err(MultimodalDimensionError::Negative);
        }
        if value > MAX_BILLING_RESOLUTION_PIXELS {
            return Err(MultimodalDimensionError::OutOfRange);
        }
        Ok(Self(value))
    }

    /// 从无符号外部值构造总像素数，并在转换前检查范围。
    pub fn from_u64(value: u64) -> Result<Self, MultimodalDimensionError> {
        let value = i64::try_from(value).map_err(|_| MultimodalDimensionError::OutOfRange)?;
        Self::new(value)
    }

    /// 返回总像素数。
    #[must_use]
    pub const fn pixels(self) -> i64 {
        self.0
    }

    /// 返回可参与 checked 组合的乘数。
    #[must_use]
    pub const fn factor(self) -> BillingFactor {
        BillingFactor(self.0)
    }
}

/// 已校验的宽高分辨率及其总像素数。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BillingResolution {
    width: i64,
    height: i64,
    pixels: BillingPixelCount,
}

impl BillingResolution {
    /// 从有符号宽高构造分辨率，并使用 checked 乘法计算像素数。
    pub fn new(width: i64, height: i64) -> Result<Self, MultimodalDimensionError> {
        if width < 0 || height < 0 {
            return Err(MultimodalDimensionError::Negative);
        }
        if width == 0 || height == 0 {
            return Err(MultimodalDimensionError::Zero);
        }
        if width > MAX_BILLING_RESOLUTION_EDGE || height > MAX_BILLING_RESOLUTION_EDGE {
            return Err(MultimodalDimensionError::OutOfRange);
        }

        let pixels = width
            .checked_mul(height)
            .ok_or(MultimodalDimensionError::Overflow)?;
        let pixels = BillingPixelCount::new(pixels)?;
        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    /// 从已通过协议边界校验的图片尺寸创建计费分辨率。
    pub fn from_protocol(value: ImageDimensions) -> Result<Self, MultimodalDimensionError> {
        Self::new(i64::from(value.width()), i64::from(value.height()))
    }

    /// 返回宽度像素数。
    #[must_use]
    pub const fn width(self) -> i64 {
        self.width
    }

    /// 返回高度像素数。
    #[must_use]
    pub const fn height(self) -> i64 {
        self.height
    }

    /// 返回已 checked 的总像素数。
    #[must_use]
    pub const fn pixels(self) -> BillingPixelCount {
        self.pixels
    }

    /// 返回可参与 checked 组合的像素乘数。
    #[must_use]
    pub const fn factor(self) -> BillingFactor {
        self.pixels.factor()
    }
}

/// 可参与多模态计费组合的非负乘数。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BillingFactor(i64);

impl BillingFactor {
    /// 组合中不增加倍率的中性元素。
    pub const ONE: Self = Self(1);

    /// 从已经通过具体业务上限校验的有符号值构造乘数。
    ///
    /// 用户可控的张数、时长和像素必须先经过本模块对应值对象，不能直接使用本入口。
    pub const fn new(value: i64) -> Result<Self, MultimodalDimensionError> {
        if value < 0 {
            return Err(MultimodalDimensionError::Negative);
        }
        Ok(Self(value))
    }

    /// 从已经通过具体业务上限校验的无符号值构造乘数。
    ///
    /// 本入口只负责拒绝无法表示为 `i64` 的值，不替代具体维度的固定上限校验。
    pub fn from_u64(value: u64) -> Result<Self, MultimodalDimensionError> {
        let value = i64::try_from(value).map_err(|_| MultimodalDimensionError::OutOfRange)?;
        Self::new(value)
    }

    /// 返回非负乘数。
    #[must_use]
    pub const fn get(self) -> i64 {
        self.0
    }

    /// 使用 checked 乘法组合两个维度。
    pub const fn checked_mul(self, other: Self) -> Result<Self, MultimodalBillingError> {
        match self.0.checked_mul(other.0) {
            Some(value) => Ok(Self(value)),
            None => Err(MultimodalBillingError::FactorOverflow),
        }
    }

    /// 按固定维度数量上限组合多个维度。
    pub fn checked_product(factors: &[Self]) -> Result<Self, MultimodalBillingError> {
        if factors.len() > MAX_MULTIMODAL_DIMENSIONS {
            return Err(MultimodalBillingError::TooManyDimensions);
        }

        let mut product = Self::ONE;
        for factor in factors {
            product = product.checked_mul(*factor)?;
        }
        Ok(product)
    }

    /// 将 checked 乘数应用到额度，统一复用 `quota_math` 的审计边界。
    pub fn apply_to_quota(self, base: Quota) -> Result<Quota, MultimodalBillingError> {
        quota_math::checked_mul_quota(base, self.0).map_err(Into::into)
    }
}

/// 一次请求可用的供应商无关多模态计费维度集合。
///
/// 只有被明确提供的维度才参与组合；未提供的维度使用乘法中性值 `1`。调用方应根据
/// 具体计价规则选择字段，避免把不相关的维度误相乘。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MultimodalBillingDimensions {
    image_count: Option<BillingImageCount>,
    duration_seconds: Option<BillingDurationSeconds>,
    resolution_pixels: Option<BillingPixelCount>,
}

impl MultimodalBillingDimensions {
    /// 构造已经完成各维度边界校验的集合。
    #[must_use]
    pub const fn new(
        image_count: Option<BillingImageCount>,
        duration_seconds: Option<BillingDurationSeconds>,
        resolution_pixels: Option<BillingPixelCount>,
    ) -> Self {
        Self {
            image_count,
            duration_seconds,
            resolution_pixels,
        }
    }

    /// 返回空维度集合；其组合乘数为 `1`。
    #[must_use]
    pub const fn empty() -> Self {
        Self::new(None, None, None)
    }

    /// 返回图片张数。
    #[must_use]
    pub const fn image_count(self) -> Option<BillingImageCount> {
        self.image_count
    }

    /// 返回媒体时长。
    #[must_use]
    pub const fn duration_seconds(self) -> Option<BillingDurationSeconds> {
        self.duration_seconds
    }

    /// 返回总像素数。
    #[must_use]
    pub const fn resolution_pixels(self) -> Option<BillingPixelCount> {
        self.resolution_pixels
    }

    /// 将分辨率对象转换为只参与计费的像素维度。
    #[must_use]
    pub const fn with_resolution(
        image_count: Option<BillingImageCount>,
        duration_seconds: Option<BillingDurationSeconds>,
        resolution: Option<BillingResolution>,
    ) -> Self {
        let resolution_pixels = match resolution {
            Some(value) => Some(value.pixels()),
            None => None,
        };
        Self::new(image_count, duration_seconds, resolution_pixels)
    }

    /// 按集合中已提供的维度执行 checked 组合。
    pub fn checked_factor(self) -> Result<BillingFactor, MultimodalBillingError> {
        let mut factors = [BillingFactor::ONE; MAX_MULTIMODAL_DIMENSIONS];
        let mut length = 0;
        if let Some(value) = self.image_count {
            factors[length] = value.factor();
            length += 1;
        }
        if let Some(value) = self.duration_seconds {
            factors[length] = value.factor();
            length += 1;
        }
        if let Some(value) = self.resolution_pixels {
            factors[length] = value.factor();
            length += 1;
        }
        BillingFactor::checked_product(&factors[..length])
    }

    /// 将集合乘数 checked 应用到基础额度。
    pub fn apply_to_quota(self, base: Quota) -> Result<Quota, MultimodalBillingError> {
        self.checked_factor()?.apply_to_quota(base)
    }
}

impl fmt::Display for BillingResolution {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}x{}", self.width, self.height)
    }
}
