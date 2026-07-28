//! 协议纯净的规范请求、响应与流式转换。

pub mod anthropic;
mod bounded_json;
mod canonical;
mod capability;
pub mod gemini;
pub mod openai_chat;
pub mod openai_responses;
mod request;
mod request_envelope;
mod response;
mod sse;
mod stream;
mod usage;

pub use canonical::{
    CacheHint, CanonicalRequest, ContentBlock, MediaSource, Message, RawPassthrough,
    RawPassthroughError,
};
pub use capability::{
    ProtocolCapabilities, ProtocolCapability, RequestCapability, ResponseCapability,
    StreamCapability, UnsupportedCapability, protocol_capabilities, supports_request_capability,
    supports_response_capability, supports_stream_capability, validate_request_capabilities,
    validate_response_capabilities, validate_stream_event_capabilities,
};
pub use request::{
    Attachment, ReasoningConfig, ReasoningConfigError, ReasoningEffort, RequestContinuation,
    RequestMetadata, Sampling, SamplingError, StreamOptions, ToolChoice, ToolDef,
};
pub use request_envelope::{CanonicalRequestEnvelope, SameProtocolDecision, SameProtocolRequest};
pub use response::{CanonicalResponse, ResponseChoice};
pub use stream::{CanonicalStreamEvent, ContentDelta, FinishReason};
pub use usage::{TokenCount, Usage, UsageDetails, UsageError, UsageSemantics, UsageSource};

#[cfg(test)]
mod canonical_tests;
#[cfg(test)]
mod request_tests;
#[cfg(test)]
mod stream_tests;
#[cfg(test)]
mod usage_tests;
