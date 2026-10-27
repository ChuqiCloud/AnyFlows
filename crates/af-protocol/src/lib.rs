//! 协议纯净的规范请求、响应与流式转换。

pub mod anthropic;
mod anthropic_beta;
mod anthropic_compaction;
mod audio;
mod bounded_json;
mod canonical;
mod capability;
mod client_simulation_body_patch;
pub mod cohere_rerank_v2;
mod conversation_content;
mod conversation_features;
mod conversation_history;
mod conversation_plan;
mod embedding;
mod error_evidence;
pub mod gemini;
mod image;
pub mod openai_audio;
pub mod openai_audio_speech;
pub mod openai_chat;
pub mod openai_embeddings;
pub mod openai_images;
pub mod openai_responses;
pub mod openai_responses_compact;
mod reasoning_suffix;
mod request;
mod request_compat;
mod request_envelope;
mod rerank;
pub mod rerank_v1;
mod response;
mod sse;
mod stream;
mod tool_compatibility;
mod usage;
mod video_task;
pub mod xai_video;

pub use anthropic_beta::{
    AnthropicBetaError, MAX_ANTHROPIC_BETA_BYTES, normalize_anthropic_beta,
    normalize_anthropic_beta_values,
};
pub use anthropic_compaction::{AnthropicCompactionError, AnthropicCompactionItem};
pub use audio::{
    AudioDuration, AudioDurationError, AudioFile, AudioFileError, AudioFileFormat,
    AudioInputTokenDetails, AudioLanguageCode, AudioLanguageCodeError, AudioLanguageHints,
    AudioLanguageHintsError, AudioSpeechOptions, AudioSpeechOptionsError, AudioSpeechOutputFormat,
    AudioSpeechSpeed, AudioSpeechSpeedError, AudioSpeechStreamFormat, AudioSpeechVoice,
    AudioSpeechVoiceError, AudioSpeechVoiceId, AudioSpeechVoiceName, AudioTranscriptionOptions,
    AudioTranscriptionOptionsError, AudioTranscriptionTokenUsage, AudioTranscriptionUsage,
    AudioTranscriptionUsageError, CanonicalAudioSpeechRequest, CanonicalAudioSpeechRequestError,
    CanonicalAudioSpeechResponse, CanonicalAudioSpeechResponseError,
    CanonicalAudioTranscriptionRequest, CanonicalAudioTranscriptionRequestError,
    CanonicalAudioTranscriptionResponse, CanonicalAudioTranscriptionResponseError,
    GeneratedSpeechAudio, GeneratedSpeechAudioError, MAX_AUDIO_DETECTED_LANGUAGES,
    MAX_AUDIO_DURATION_SECONDS, MAX_AUDIO_FILE_BYTES, MAX_AUDIO_KEYWORD_BYTES, MAX_AUDIO_KEYWORDS,
    MAX_AUDIO_LANGUAGE_HINTS, MAX_AUDIO_SPEECH_INPUT_BYTES, MAX_AUDIO_SPEECH_INPUT_CHARS,
    MAX_AUDIO_SPEECH_INSTRUCTIONS_BYTES, MAX_AUDIO_SPEECH_VOICE_ID_BYTES,
    MAX_AUDIO_SPEECH_VOICE_NAME_BYTES, MAX_AUDIO_TRANSCRIPT_TEXT_BYTES,
    MAX_AUDIO_TRANSCRIPTION_PROMPT_BYTES, MAX_GENERATED_SPEECH_BYTES,
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
pub use client_simulation_body_patch::{
    ClientSimulationBodyPatch, ClientSimulationBodyPatchError, UtcDate,
};
pub use conversation_plan::{ConversationConversionError, ConversationConversionPlan};
pub use embedding::{
    CanonicalEmbeddingRequest, CanonicalEmbeddingRequestError, CanonicalEmbeddingResponse,
    CanonicalEmbeddingResponseError, EmbeddingDimensions, EmbeddingDimensionsError, EmbeddingInput,
    EmbeddingVector, MAX_EMBEDDING_DIMENSIONS, MAX_EMBEDDING_INPUTS, MAX_EMBEDDING_TEXT_BYTES,
    MAX_TOTAL_EMBEDDING_TEXT_BYTES, MAX_TOTAL_EMBEDDING_VALUES,
};
pub use error_evidence::{StructuredErrorEvidence, extract_structured_error_evidence};
pub use image::{
    CanonicalImageEditRequest, CanonicalImageEditRequestError, CanonicalImageGenerationRequest,
    CanonicalImageGenerationRequestError, CanonicalImageGenerationResponse,
    CanonicalImageGenerationResponseError, GeneratedImage, GeneratedImageError, ImageBackground,
    ImageCompression, ImageCompressionError, ImageCount, ImageCountError, ImageDimensions,
    ImageDimensionsError, ImageEditFile, ImageEditFileError, ImageGenerationOptions,
    ImageGenerationUsage, ImageGenerationUsageError, ImageInputFidelity, ImageModeration,
    ImageOutputFormat, ImageQuality, ImageResponseFormat, ImageSize, ImageStyle,
    ImageTokenBreakdown, MAX_GENERATED_IMAGE_BYTES, MAX_GENERATED_IMAGE_URL_BYTES, MAX_IMAGE_EDGE,
    MAX_IMAGE_EDIT_EDGE, MAX_IMAGE_EDIT_FILE_BYTES, MAX_IMAGE_EDIT_FILES,
    MAX_IMAGE_EDIT_MASK_BYTES, MAX_IMAGE_EDIT_PIXELS, MAX_IMAGE_EDIT_TOTAL_BYTES,
    MAX_IMAGE_GENERATION_COUNT, MAX_IMAGE_PIXELS, MAX_IMAGE_PROMPT_BYTES, MAX_IMAGE_PROMPT_CHARS,
    MAX_TOTAL_GENERATED_IMAGE_BYTES, MIN_IMAGE_PIXELS,
};
pub use openai_responses_compact::{
    CanonicalResponsesCompactionRequest, CanonicalResponsesCompactionRequestError,
    CanonicalResponsesCompactionResponse, ResponsesCompactionInput, ResponsesCompactionItem,
    ResponsesCompactionUsage, ResponsesCompactionUsageError,
};
pub use reasoning_suffix::{ReasoningModelSuffixError, apply_reasoning_model_suffix};
pub use request::{
    Attachment, ReasoningConfig, ReasoningConfigError, ReasoningEffort, ReasoningSummary,
    RequestContinuation, RequestMetadata, Sampling, SamplingError, StreamOptions, ToolChoice,
    ToolDef,
};
pub use request_envelope::{CanonicalRequestEnvelope, SameProtocolDecision, SameProtocolRequest};
pub use rerank::{
    CanonicalRerankRequest, CanonicalRerankRequestError, CanonicalRerankResponse,
    CanonicalRerankResponseError, MAX_RERANK_DOCUMENT_BYTES, MAX_RERANK_DOCUMENTS,
    MAX_RERANK_QUERY_BYTES, MAX_RERANK_RESPONSE_ID_BYTES, MAX_TOTAL_RERANK_TEXT_BYTES,
    RerankDocument, RerankRelevanceScore, RerankRelevanceScoreError, RerankResult,
    RerankSearchUnits, RerankSearchUnitsError, RerankTopN, RerankTopNError, RerankUsage,
    RerankUsageError,
};
pub use response::{CanonicalResponse, ResponseChoice};
pub use stream::{CanonicalStreamEvent, ContentDelta, FinishReason};
pub use tool_compatibility::{
    ToolCompatibilityError, ToolCompatibilitySession, ToolCompatibilityStep,
};
pub use usage::{TokenCount, Usage, UsageDetails, UsageError, UsageSemantics, UsageSource};
pub use video_task::{
    CanonicalTaskOutput, CanonicalTaskPoll, CanonicalVideoGenerationRequest, CanonicalVideoOutput,
    MAX_VIDEO_DURATION_SECONDS, MAX_VIDEO_MODEL_BYTES, MAX_VIDEO_OUTPUT_URL_BYTES,
    MAX_VIDEO_PROMPT_BYTES, MIN_VIDEO_DURATION_SECONDS, VideoAspectRatio, VideoDuration,
    VideoModel, VideoOutputUrl, VideoPrompt, VideoResolution, VideoTaskError,
};

#[cfg(test)]
mod canonical_tests;
#[cfg(test)]
mod request_tests;
#[cfg(test)]
mod stream_tests;
#[cfg(test)]
mod usage_tests;
