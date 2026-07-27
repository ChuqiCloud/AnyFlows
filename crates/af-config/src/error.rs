use std::io;

use thiserror::Error;

/// CORS 来源未满足严格 Origin 契约。
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
#[error("CORS 来源无效")]
pub struct CorsOriginError;

/// 配置加载与校验错误；错误内容不会携带配置值或底层解析器原文。
#[non_exhaustive]
#[derive(Debug, Error, Eq, PartialEq)]
pub enum ConfigError {
    /// 显式指定的配置文件不存在。
    #[error("显式指定的配置文件不存在")]
    FileNotFound,

    /// 配置文件无法读取。
    #[error("读取配置文件失败 ({kind:?})")]
    FileRead { kind: io::ErrorKind },

    /// 配置解析失败，但不暴露可能包含密钥的解析器详情。
    #[error("配置内容无效")]
    Invalid,

    /// 配置解析失败且解析器能定位到具体字段。
    #[error("配置字段无效: {field}")]
    InvalidField { field: &'static str },

    /// 配置项为空。
    #[error("配置项不能为空: {field}")]
    EmptyValue { field: &'static str },

    /// 配置项必须为正数。
    #[error("配置项必须大于零: {field}")]
    NonPositive { field: &'static str },

    /// 配置项超出服务允许的固定边界。
    #[error("配置项超出允许范围: {field}")]
    OutOfRange { field: &'static str },
}
