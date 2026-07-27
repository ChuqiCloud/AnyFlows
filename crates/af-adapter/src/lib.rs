//! 上游供应商适配器契约与传输边界。
//!
//! 协议转换属于 `af-protocol`，适配器只负责上游 URL、鉴权和请求传输。

mod adaptor;
mod context;
mod credential;
mod error;
mod factory;
mod openai;
mod request;

pub use adaptor::{Adaptor, AdaptorSendExt};
pub use context::{MAX_REQUEST_ID_BYTES, MAX_UPSTREAM_BASE_URL_BYTES, RelayContext};
pub use credential::{Credential, MAX_CREDENTIAL_SECRET_BYTES};
pub use error::{AdaptorError, AdaptorResult, AdaptorTransportError};
pub use factory::{AdaptorSettings, OpenAiAdaptorSettings, get_adaptor};
pub use openai::OpenAiAdaptor;
pub use request::{
    MAX_UPSTREAM_REQUEST_BODY_BYTES, MAX_UPSTREAM_REQUEST_HEADER_COUNT,
    MAX_UPSTREAM_REQUEST_HEADER_VALUE_BYTES, MAX_UPSTREAM_REQUEST_HEADERS_BYTES,
    MAX_UPSTREAM_REQUEST_TARGET_BYTES, MAX_UPSTREAM_RESPONSE_BODY_BYTES,
    MAX_UPSTREAM_RESPONSE_CHUNK_BYTES, MAX_UPSTREAM_RESPONSE_HEADER_COUNT,
    MAX_UPSTREAM_RESPONSE_HEADER_VALUE_BYTES, MAX_UPSTREAM_RESPONSE_HEADERS_BYTES, ResponseMode,
    UpstreamBody, UpstreamBodyStream, UpstreamRequest, UpstreamResponse,
};

pub use af_domain::{ChannelType, CredentialKind, Operation, Protocol};
pub use af_httpclient::{
    Bytes, HeaderMap, HeaderName, HeaderValue, Method, PooledClient, StatusCode,
};
