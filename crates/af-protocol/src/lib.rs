//! 协议纯净的规范请求、响应与流式转换。

mod canonical;
mod request;
mod stream;
mod usage;

pub use canonical::{
    CacheHint, CanonicalRequest, ContentBlock, MediaSource, Message, RawPassthrough,
    RawPassthroughError,
};
pub use request::{
    Attachment, ReasoningConfig, ReasoningConfigError, ReasoningEffort, RequestMetadata, Sampling,
    SamplingError, ToolChoice, ToolDef,
};
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
