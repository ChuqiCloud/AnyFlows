use std::{fmt, sync::Arc};

use thiserror::Error;

/// 单个 Canonical 模型名允许的最大 UTF-8 字节数。
pub const MAX_MODEL_NAME_BYTES: usize = 256;
/// 单个令牌模型白名单允许的最大条目数。
pub const MAX_TOKEN_MODEL_ALLOWLIST_COUNT: usize = 512;
/// 单个令牌模型白名单允许的模型名总字节数。
pub const MAX_TOKEN_MODEL_ALLOWLIST_TEXT_BYTES: usize = 32 * 1024;

/// 令牌模型白名单违反规范化或容量约束。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum TokenModelPolicyError {
    /// 白名单为空、条目无效或超过容量上限。
    #[error("令牌模型白名单格式无效")]
    InvalidAllowlist,
}

/// 已完整校验的不可变令牌模型策略快照。
///
/// `None` 表示不限制模型；受限策略只对 Canonical 模型名执行区分大小写的精确匹配。
/// 本类型不实现序列化或 `Display`，避免策略内容进入协议响应或日志。
#[derive(Clone, Eq, PartialEq)]
pub struct TokenModelPolicy {
    allowed_models: Option<Arc<[String]>>,
}

impl TokenModelPolicy {
    /// 构造不限制模型的策略快照。
    #[must_use]
    pub const fn unrestricted() -> Self {
        Self {
            allowed_models: None,
        }
    }

    /// 从非空模型数组构造受限策略，并校验数量、名称与总字节预算。
    pub fn try_from_allowlist(mut models: Vec<String>) -> Result<Self, TokenModelPolicyError> {
        if models.is_empty() || models.len() > MAX_TOKEN_MODEL_ALLOWLIST_COUNT {
            return Err(TokenModelPolicyError::InvalidAllowlist);
        }

        let mut total_text_bytes = 0_usize;
        for model in &models {
            if model.is_empty()
                || model.len() > MAX_MODEL_NAME_BYTES
                || model.trim() != model
                || model.chars().any(char::is_control)
            {
                return Err(TokenModelPolicyError::InvalidAllowlist);
            }
            total_text_bytes = total_text_bytes
                .checked_add(model.len())
                .filter(|total| *total <= MAX_TOKEN_MODEL_ALLOWLIST_TEXT_BYTES)
                .ok_or(TokenModelPolicyError::InvalidAllowlist)?;
        }

        // 重复项仍在上方计入持久化预算，运行时只保留唯一值以稳定匹配成本。
        models.sort_unstable();
        models.dedup();
        Ok(Self {
            allowed_models: Some(models.into()),
        })
    }

    /// 判断 Canonical 模型名是否满足当前令牌策略。
    #[must_use]
    pub fn allows(&self, requested_model: &str) -> bool {
        self.allowed_models.as_ref().is_none_or(|allowed_models| {
            allowed_models
                .binary_search_by(|model| model.as_str().cmp(requested_model))
                .is_ok()
        })
    }
}

impl fmt::Debug for TokenModelPolicy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("TokenModelPolicy(<redacted>)")
    }
}
