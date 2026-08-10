mod file;
mod options;
mod request;
mod response;
mod speech;
mod usage;

pub use file::{AudioFile, AudioFileError, AudioFileFormat, MAX_AUDIO_FILE_BYTES};
pub use options::{
    AudioLanguageCode, AudioLanguageCodeError, AudioLanguageHints, AudioLanguageHintsError,
    AudioTranscriptionOptions, AudioTranscriptionOptionsError, MAX_AUDIO_KEYWORD_BYTES,
    MAX_AUDIO_KEYWORDS, MAX_AUDIO_LANGUAGE_HINTS, MAX_AUDIO_TRANSCRIPTION_PROMPT_BYTES,
    MAX_TOTAL_AUDIO_KEYWORD_BYTES, TranscriptionKeyword, TranscriptionKeywordError,
    TranscriptionTemperature, TranscriptionTemperatureError,
};
pub use request::{CanonicalAudioTranscriptionRequest, CanonicalAudioTranscriptionRequestError};
pub use response::{
    CanonicalAudioTranscriptionResponse, CanonicalAudioTranscriptionResponseError,
    MAX_AUDIO_DETECTED_LANGUAGES, MAX_AUDIO_TRANSCRIPT_TEXT_BYTES,
};
pub use speech::{
    AudioSpeechOptions, AudioSpeechOptionsError, AudioSpeechOutputFormat, AudioSpeechSpeed,
    AudioSpeechSpeedError, AudioSpeechStreamFormat, AudioSpeechVoice, AudioSpeechVoiceError,
    AudioSpeechVoiceId, AudioSpeechVoiceName, CanonicalAudioSpeechRequest,
    CanonicalAudioSpeechRequestError, CanonicalAudioSpeechResponse,
    CanonicalAudioSpeechResponseError, GeneratedSpeechAudio, GeneratedSpeechAudioError,
    MAX_AUDIO_SPEECH_INPUT_BYTES, MAX_AUDIO_SPEECH_INPUT_CHARS,
    MAX_AUDIO_SPEECH_INSTRUCTIONS_BYTES, MAX_AUDIO_SPEECH_VOICE_ID_BYTES,
    MAX_AUDIO_SPEECH_VOICE_NAME_BYTES, MAX_GENERATED_SPEECH_BYTES,
};
pub use usage::{
    AudioDuration, AudioDurationError, AudioInputTokenDetails, AudioTranscriptionTokenUsage,
    AudioTranscriptionUsage, AudioTranscriptionUsageError, MAX_AUDIO_DURATION_SECONDS,
};
