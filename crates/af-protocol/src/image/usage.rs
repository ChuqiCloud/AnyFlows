use std::{error::Error, fmt};

use crate::TokenCount;

/// 图片 token 用量中的文本与图片明细。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImageTokenBreakdown {
    text_tokens: TokenCount,
    image_tokens: TokenCount,
}

impl ImageTokenBreakdown {
    /// 构造不含隐式默认值的图片 token 明细。
    #[must_use]
    pub const fn new(text_tokens: TokenCount, image_tokens: TokenCount) -> Self {
        Self {
            text_tokens,
            image_tokens,
        }
    }

    /// 返回文本 token 数。
    #[must_use]
    pub const fn text_tokens(self) -> TokenCount {
        self.text_tokens
    }

    /// 返回图片 token 数。
    #[must_use]
    pub const fn image_tokens(self) -> TokenCount {
        self.image_tokens
    }

    fn checked_total(self) -> Result<TokenCount, ImageGenerationUsageError> {
        checked_add_tokens(self.text_tokens, self.image_tokens)
    }
}

/// 图片生成专用 token 用量。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImageGenerationUsage {
    input_tokens: TokenCount,
    input_details: ImageTokenBreakdown,
    output_tokens: TokenCount,
    output_details: Option<ImageTokenBreakdown>,
}

impl ImageGenerationUsage {
    /// 构造并校验输入、输出总量与明细闭合关系。
    pub fn new(
        input_tokens: TokenCount,
        input_details: ImageTokenBreakdown,
        output_tokens: TokenCount,
        output_details: Option<ImageTokenBreakdown>,
    ) -> Result<Self, ImageGenerationUsageError> {
        if input_details.checked_total()? != input_tokens {
            return Err(ImageGenerationUsageError::InputDetailsMismatch);
        }
        if output_details
            .map(ImageTokenBreakdown::checked_total)
            .transpose()?
            .is_some_and(|total| total != output_tokens)
        {
            return Err(ImageGenerationUsageError::OutputDetailsMismatch);
        }
        checked_add_tokens(input_tokens, output_tokens)?;
        Ok(Self {
            input_tokens,
            input_details,
            output_tokens,
            output_details,
        })
    }

    /// 返回输入 token 总量。
    #[must_use]
    pub const fn input_tokens(self) -> TokenCount {
        self.input_tokens
    }

    /// 返回输入 token 明细。
    #[must_use]
    pub const fn input_details(self) -> ImageTokenBreakdown {
        self.input_details
    }

    /// 返回输出 token 总量。
    #[must_use]
    pub const fn output_tokens(self) -> TokenCount {
        self.output_tokens
    }

    /// 返回可选的输出 token 明细。
    #[must_use]
    pub const fn output_details(self) -> Option<ImageTokenBreakdown> {
        self.output_details
    }

    /// checked 计算输入与输出 token 总量。
    pub fn checked_total_tokens(self) -> Result<TokenCount, ImageGenerationUsageError> {
        checked_add_tokens(self.input_tokens, self.output_tokens)
    }
}

/// 图片生成用量错误，不保留外部 token 数值。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImageGenerationUsageError {
    /// 输入 token 明细之和与输入总量不一致。
    InputDetailsMismatch,
    /// 输出 token 明细之和与输出总量不一致。
    OutputDetailsMismatch,
    /// token 汇总超过 `i64` 上界。
    Overflow,
}

impl fmt::Display for ImageGenerationUsageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InputDetailsMismatch => "图片输入 token 明细与总量不一致",
            Self::OutputDetailsMismatch => "图片输出 token 明细与总量不一致",
            Self::Overflow => "图片 token 汇总溢出",
        };
        formatter.write_str(message)
    }
}

impl Error for ImageGenerationUsageError {}

fn checked_add_tokens(
    left: TokenCount,
    right: TokenCount,
) -> Result<TokenCount, ImageGenerationUsageError> {
    let total = left
        .get()
        .checked_add(right.get())
        .ok_or(ImageGenerationUsageError::Overflow)?;
    TokenCount::new(total).map_err(|_| ImageGenerationUsageError::Overflow)
}
