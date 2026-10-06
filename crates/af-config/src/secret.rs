use std::fmt;

use serde::Deserialize;
use zeroize::Zeroize as _;

/// 保存数据库、缓存等敏感文本，并在调试输出中统一脱敏。
#[derive(Clone, Deserialize, Eq, PartialEq)]
#[serde(transparent)]
pub struct SecretString(String);

impl SecretString {
    /// 从明文构造敏感值；调用方应避免将返回值写入日志。
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// 在确有必要交给下层客户端时短暂读取明文。
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// 判断敏感值是否为空。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<String> for SecretString {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<&str> for SecretString {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<redacted>")
    }
}

impl Drop for SecretString {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}
