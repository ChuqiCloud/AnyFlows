use thiserror::Error;

/// tracing 初始化与运行期调级错误；错误内容不携带底层实现细节。
#[non_exhaustive]
#[derive(Debug, Error, Eq, PartialEq)]
pub enum TelemetryError {
    /// 当前进程已经安装了全局 tracing subscriber。
    #[error("全局 tracing 已初始化")]
    InitializationConflict,

    /// subscriber 已退出或过滤器锁不可用，无法完成调级。
    #[error("tracing 日志级别更新失败")]
    ReloadFailed,

    /// 当前进程已经安装了全局 metrics recorder。
    #[error("Prometheus 指标已初始化")]
    MetricsInitializationConflict,
}

/// 服务端请求 ID 校验错误；错误内容不会回显原始值。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum RequestIdError {
    /// 请求 ID 不符合 canonical 字符集或长度限制。
    #[error("请求 ID 无效")]
    Invalid,
}

/// 动态指标标签注册错误；错误内容不会回显原始标签值。
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum MetricLabelError {
    /// 标签为空、过长或包含非规范字符。
    #[error("指标标签无效")]
    Invalid,

    /// 当前标签维度已达到受控容量上限。
    #[error("指标标签容量已满")]
    CapacityExceeded,
}
