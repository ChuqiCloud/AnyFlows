//! 协议纯净的规范请求、响应与流式转换。

pub mod anthropic;
mod audio;
mod bounded_json;
mod canonical;
mod capability;
mod embedding;
mod error_evidence;
pub mod gemini;
mod image;
pub mod openai_audio;
pub mod openai_chat;
pub mod openai_embeddings;
pub mod openai_images;
pub mod openai_responses;
mod reasoning_suffix;
mod request;
mod request_envelope;
mod response;
mod sse;
mod stream;
mod usage;

pub use audio::{
    AudioDuration, AudioDurationError, AudioFile, AudioFileError, AudioFileFormat,
    AudioInputTokenDetails, AudioLanguageCode, AudioLanguageCodeError, AudioLanguageHints,
    AudioLanguageHintsError, AudioTranscriptionOptions, AudioTranscriptionOptionsError,
    AudioTranscriptionTokenUsage, AudioTranscriptionUsage, AudioTranscriptionUsageError,
    CanonicalAudioTranscriptionRequest, CanonicalAudioTranscriptionRequestError,
    CanonicalAudioTranscriptionResponse, CanonicalAudioTranscriptionResponseError,
    MAX_AUDIO_DETECTED_LANGUAGES, MAX_AUDIO_DURATION_SECONDS, MAX_AUDIO_FILE_BYTES,
    MAX_AUDIO_KEYWORD_BYTES, MAX_AUDIO_KEYWORDS, MAX_AUDIO_LANGUAGE_HINTS,
    MAX_AUDIO_TRANSCRIPT_TEXT_BYTES, MAX_AUDIO_TRANSCRIPTION_PROMPT_BYTES,
    MAX_TOTAL_AUDIO_KEYWORD_BYTES, TranscriptionKeyword, TranscriptionKeywordError,
    TranscriptionTemperature, TranscriptionTemperatureError,
};
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
pub use embedding::{
    CanonicalEmbeddingRequest, CanonicalEmbeddingRequestError, CanonicalEmbeddingResponse,
    CanonicalEmbeddingResponseError, EmbeddingDimensions, EmbeddingDimensionsError, EmbeddingInput,
    EmbeddingVector, MAX_EMBEDDING_DIMENSIONS, MAX_EMBEDDING_INPUTS, MAX_EMBEDDING_TEXT_BYTES,
    MAX_TOTAL_EMBEDDING_TEXT_BYTES, MAX_TOTAL_EMBEDDING_VALUES,
};
pub use error_evidence::{StructuredErrorEvidence, extract_structured_error_evidence};
pub use image::{
    CanonicalImageGenerationRequest, CanonicalImageGenerationRequestError,
    CanonicalImageGenerationResponse, CanonicalImageGenerationResponseError, GeneratedImage,
    GeneratedImageError, ImageBackground, ImageCompression, ImageCompressionError, ImageCount,
    ImageCountError, ImageDimensions, ImageDimensionsError, ImageGenerationOptions,
    ImageGenerationUsage, ImageGenerationUsageError, ImageModeration, ImageOutputFormat,
    ImageQuality, ImageSize, ImageTokenBreakdown, MAX_GENERATED_IMAGE_BYTES, MAX_IMAGE_EDGE,
    MAX_IMAGE_GENERATION_COUNT, MAX_IMAGE_PIXELS, MAX_IMAGE_PROMPT_BYTES, MAX_IMAGE_PROMPT_CHARS,
    MAX_TOTAL_GENERATED_IMAGE_BYTES, MIN_IMAGE_PIXELS,
};
pub use reasoning_suffix::{ReasoningModelSuffixError, apply_reasoning_model_suffix};
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
