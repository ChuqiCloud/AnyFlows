//! 上游供应商适配器契约与传输边界。
//!
//! 协议转换属于 `af-protocol`，适配器只负责上游 URL、鉴权和请求传输。

mod adaptor;
mod anthropic;
mod bedrock;
mod cohere;
mod context;
mod credential;
mod custom;
mod error;
mod factory;
mod gemini;
mod jina;
mod mimicry;
mod openai;
mod request;
mod responses_websocket_pool;
mod responses_websocket_transport;
mod task;
mod transport_dispatcher;
mod vertex;
mod xai_video;

pub use adaptor::{Adaptor, AdaptorSendExt, AdaptorTarget};
pub use anthropic::AnthropicAdaptor;
pub use bedrock::{BedrockAdaptor, MAX_AWS_REGION_BYTES, MAX_BEDROCK_MODEL_ID_BYTES};
pub use cohere::CohereAdaptor;
pub use context::{MAX_REQUEST_ID_BYTES, MAX_UPSTREAM_BASE_URL_BYTES, RelayContext};
pub use credential::{
    Credential, MAX_AWS_ACCESS_KEY_ID_BYTES, MAX_AWS_SECRET_ACCESS_KEY_BYTES,
    MAX_AWS_SESSION_TOKEN_BYTES, MAX_CREDENTIAL_SECRET_BYTES, MAX_GOOGLE_PRIVATE_KEY_BYTES,
    MAX_GOOGLE_PRIVATE_KEY_ID_BYTES, MAX_GOOGLE_SERVICE_ACCOUNT_EMAIL_BYTES,
};
pub use custom::{
    CUSTOM_CREDENTIAL_PLACEHOLDER, CUSTOM_MODEL_PLACEHOLDER, CustomAdaptor, CustomAuthentication,
    CustomEndpointTemplate, CustomHeaderAuthentication, CustomStreamEndpoint,
    MAX_CUSTOM_AUTH_TEMPLATE_BYTES, MAX_CUSTOM_ENDPOINT_PATH_SEGMENTS,
    MAX_CUSTOM_ENDPOINT_QUERY_PAIRS, MAX_CUSTOM_ENDPOINT_TEMPLATE_BYTES,
};
pub use error::{AdaptorError, AdaptorResult, AdaptorTransportError};
pub use factory::{
    AdaptorSettings, AnthropicAdaptorSettings, BedrockAdaptorSettings, CohereAdaptorSettings,
    CustomAdaptorSettings, GeminiAdaptorSettings, JinaAdaptorSettings, OpenAiAdaptorSettings,
    VertexAdaptorSettings, get_adaptor, get_task_adaptor,
};
pub use gemini::GeminiAdaptor;
pub use jina::JinaAdaptor;
pub use mimicry::{
    BuiltInClientSimulation, ClientSimulationContext, ClientSimulationHeaders,
    ClientSimulationMiddleware, MAX_CLIENT_SIMULATION_HEADER_VALUE_BYTES, apply_client_simulation,
};
pub use openai::OpenAiAdaptor;
pub use request::{
    MAX_UPSTREAM_REQUEST_BODY_BYTES, MAX_UPSTREAM_REQUEST_HEADER_COUNT,
    MAX_UPSTREAM_REQUEST_HEADER_VALUE_BYTES, MAX_UPSTREAM_REQUEST_HEADERS_BYTES,
    MAX_UPSTREAM_REQUEST_TARGET_BYTES, MAX_UPSTREAM_RESPONSE_BODY_BYTES,
    MAX_UPSTREAM_RESPONSE_BODY_LIMIT_BYTES, MAX_UPSTREAM_RESPONSE_CHUNK_BYTES,
    MAX_UPSTREAM_RESPONSE_HEADER_COUNT, MAX_UPSTREAM_RESPONSE_HEADER_VALUE_BYTES,
    MAX_UPSTREAM_RESPONSE_HEADERS_BYTES, ResponseMode, UpstreamBody, UpstreamBodyStream,
    UpstreamRequest, UpstreamResponse,
};
pub use responses_websocket_pool::{
    MAX_RESPONSES_WEBSOCKET_CONNECTION_AGE, MAX_RESPONSES_WEBSOCKET_EVENTS_PER_TURN,
    MAX_RESPONSES_WEBSOCKET_SESSION_ID_BYTES, ResponsesWebSocketConnection,
    ResponsesWebSocketConnector, ResponsesWebSocketFrame, ResponsesWebSocketHandshake,
    ResponsesWebSocketPool, ResponsesWebSocketPoolConfig, ResponsesWebSocketPoolError,
    ResponsesWebSocketPoolKey, ResponsesWebSocketTransportError, build_responses_websocket_event,
};
pub use responses_websocket_transport::PooledResponsesWebSocketConnector;
pub use task::{TaskAdaptor, TaskAdaptorSendExt, VideoTaskAdaptor};
pub use transport_dispatcher::{
    TransportDispatchOutcome, TransportDispatcher, TransportFallbackKind,
};
pub use vertex::{
    MAX_VERTEX_LOCATION_BYTES, MAX_VERTEX_PROJECT_ID_BYTES, MAX_VERTEX_TOKEN_RESPONSE_BYTES,
    VertexAdaptor,
};
pub use xai_video::XaiVideoAdaptor;

pub use af_domain::{ChannelType, ClientSimulationProfile, CredentialKind, Operation, Protocol};
pub use af_httpclient::{
    Bytes, HeaderMap, HeaderName, HeaderValue, HttpClientConfig, HttpClientPool, HttpTimeouts,
    Method, PooledClient, PooledClientIdentity, ProxyConfig, RemoteDnsPolicy, StatusCode,
};
