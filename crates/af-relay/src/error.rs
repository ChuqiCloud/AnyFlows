use af_adapter::AdaptorError;
use thiserror::Error;

/// M0 单上游转发服务的启动构造错误。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RelayBuildError {
    /// 静态模型配置不符合 OpenAI Chat 协议边界。
    #[error("M0 单上游模型配置无效")]
    InvalidModel,
    /// 基础地址或凭据无法构造安全适配器上下文。
    #[error("M0 单上游适配器配置无效")]
    Adaptor(#[source] AdaptorError),
}

impl From<AdaptorError> for RelayBuildError {
    fn from(error: AdaptorError) -> Self {
        Self::Adaptor(error)
    }
}
