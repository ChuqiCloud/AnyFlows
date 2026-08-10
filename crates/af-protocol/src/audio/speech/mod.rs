mod media;
mod options;
mod request;
mod response;

pub use media::{GeneratedSpeechAudio, GeneratedSpeechAudioError, MAX_GENERATED_SPEECH_BYTES};
pub use options::{
    AudioSpeechOptions, AudioSpeechOptionsError, AudioSpeechOutputFormat, AudioSpeechSpeed,
    AudioSpeechSpeedError, AudioSpeechStreamFormat, AudioSpeechVoice, AudioSpeechVoiceError,
    AudioSpeechVoiceId, AudioSpeechVoiceName, MAX_AUDIO_SPEECH_INSTRUCTIONS_BYTES,
    MAX_AUDIO_SPEECH_VOICE_ID_BYTES, MAX_AUDIO_SPEECH_VOICE_NAME_BYTES,
};
pub use request::{
    CanonicalAudioSpeechRequest, CanonicalAudioSpeechRequestError, MAX_AUDIO_SPEECH_INPUT_BYTES,
    MAX_AUDIO_SPEECH_INPUT_CHARS,
};
pub use response::{CanonicalAudioSpeechResponse, CanonicalAudioSpeechResponseError};
