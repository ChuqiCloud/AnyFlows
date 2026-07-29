use af_domain::{ChannelType, CredentialKind, Operation};
use af_httpclient::HttpTransportError;
use thiserror::Error;

/// 与具体传输实现无关的安全传输错误分类。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum AdaptorTransportError {
    /// 目标地址无效。
    #[error("上游请求目标无效")]
    InvalidRequestTarget,
    /// 请求方法不受支持。
    #[error("上游请求方法不受支持")]
    UnsupportedRequestMethod,
    /// 目标解析到被安全策略阻断的地址。
    #[error("上游目标地址被安全策略阻断")]
    TargetAddressBlocked,
    /// 上游域名解析失败或返回无效地址集合。
    #[error("上游目标解析失败")]
    TargetResolution,
    /// 代理端 DNS 未获得显式信任授权。
    #[error("代理端解析上游域名未获授权")]
    RemoteDnsDenied,
    /// 建连阶段超时。
    #[error("上游建连超时")]
    ConnectTimeout,
    /// 读取阶段超时。
    #[error("上游读取超时")]
    ReadTimeout,
    /// 请求总时限耗尽。
    #[error("上游请求超过总时限")]
    RequestTimeout,
    /// 建连失败。
    #[error("上游建连失败")]
    Connect,
    /// 请求发送失败。
    #[error("上游请求发送失败")]
    Request,
    /// 响应体读取失败。
    #[error("上游响应体读取失败")]
    ResponseBody,
}

impl From<HttpTransportError> for AdaptorTransportError {
    fn from(error: HttpTransportError) -> Self {
        match error {
            HttpTransportError::InvalidRequestTarget => Self::InvalidRequestTarget,
            HttpTransportError::UnsupportedRequestMethod => Self::UnsupportedRequestMethod,
            HttpTransportError::TargetAddressBlocked => Self::TargetAddressBlocked,
            HttpTransportError::TargetResolution => Self::TargetResolution,
            HttpTransportError::RemoteDnsDenied => Self::RemoteDnsDenied,
            HttpTransportError::ConnectTimeout => Self::ConnectTimeout,
            HttpTransportError::ReadTimeout => Self::ReadTimeout,
            HttpTransportError::RequestTimeout => Self::RequestTimeout,
            HttpTransportError::Connect => Self::Connect,
            HttpTransportError::Request => Self::Request,
            HttpTransportError::ResponseBody => Self::ResponseBody,
            _ => Self::Request,
        }
    }
}

/// 适配层对外暴露的稳定错误；不携带 URL、请求头、请求体或凭据内容。
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum AdaptorError {
    /// 当前版本尚未实现该渠道适配器。
    #[error("适配器不支持渠道 {channel_type}")]
    UnsupportedChannel { channel_type: ChannelType },
    /// 强类型适配器配置与目标渠道不匹配。
    #[error("适配器配置与渠道 {channel_type} 不匹配")]
    InvalidChannelSettings { channel_type: ChannelType },
    /// 上游基础地址不是受支持的 HTTP(S) 地址。
    #[error("上游基础地址无效")]
    InvalidBaseUrl,
    /// 请求标识不符合边界约束。
    #[error("请求标识无效")]
    InvalidRequestId,
    /// AWS 区域无法安全映射到受支持的 Bedrock Runtime 端点。
    #[error("AWS 区域无效或暂不支持")]
    InvalidAwsRegion,
    /// 凭据为空或超出内存边界。
    #[error("凭据内容无效")]
    InvalidCredential,
    /// 当前适配器不能消费该凭据种类。
    #[error("适配器不支持凭据类型 {kind}")]
    UnsupportedCredential { kind: CredentialKind },
    /// 当前适配器不能处理该操作。
    #[error("适配器不支持操作 {operation}")]
    UnsupportedOperation { operation: Operation },
    /// 上游请求目标不符合 HTTP 客户端边界。
    #[error("上游请求目标无效")]
    InvalidRequestTarget,
    /// 请求方法不在适配层允许的标准方法白名单内。
    #[error("上游请求方法不受支持")]
    UnsupportedRequestMethod,
    /// 请求头值无法安全写入。
    #[error("上游请求头无效")]
    InvalidHeader,
    /// 完整请求无法生成安全的供应商签名。
    #[error("上游请求签名失败")]
    RequestSigning,
    /// 响应头超过适配层容量边界。
    #[error("上游响应头无效")]
    InvalidResponseHeader,
    /// 请求体超过适配层上限。
    #[error("上游请求体超过大小限制")]
    RequestBodyTooLarge,
    /// 响应体超过适配层上限。
    #[error("上游响应体超过大小限制")]
    ResponseBodyTooLarge,
    /// 底层传输失败；具体错误本身已完成脱敏。
    #[error("上游传输失败")]
    Transport(#[source] AdaptorTransportError),
}

impl From<AdaptorTransportError> for AdaptorError {
    fn from(error: AdaptorTransportError) -> Self {
        Self::Transport(error)
    }
}

impl From<HttpTransportError> for AdaptorError {
    fn from(error: HttpTransportError) -> Self {
        Self::Transport(error.into())
    }
}

/// 适配层统一返回类型。
pub type AdaptorResult<T> = Result<T, AdaptorError>;
