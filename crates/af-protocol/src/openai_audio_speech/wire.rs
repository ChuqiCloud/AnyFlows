use serde::{Deserialize, Deserializer, de::Error as _};
use serde_json::Number;

/// 区分字段缺失与显式提供；显式 `null` 在协议边界拒绝。
#[derive(Default)]
pub(super) enum Field<T> {
    /// 客户端未提供字段。
    #[default]
    Missing,
    /// 客户端提供了非空字段。
    Value(T),
}

/// 反序列化可选非空字段，并保留“缺失”和“显式提供”的差异。
pub(super) fn deserialize_field<'de, D, T>(deserializer: D) -> Result<Field<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)?
        .map(Field::Value)
        .ok_or_else(|| D::Error::custom("字段不得为 null"))
}

/// OpenAI `POST /v1/audio/speech` 请求。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AudioSpeechRequestWire {
    /// 客户端请求的模型名。
    pub(super) model: String,
    /// 待合成语音的文本。
    pub(super) input: String,
    /// 命名声音或 custom voice ID 对象。
    pub(super) voice: AudioSpeechVoiceWire,
    /// 可选的发声控制指令。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) instructions: Field<String>,
    /// 可选音频输出格式。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) response_format: Field<AudioSpeechOutputFormatWire>,
    /// 可选语速。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) speed: Field<Number>,
    /// 首切片只接受缺失或原始音频字节流。
    #[serde(default, deserialize_with = "deserialize_field")]
    pub(super) stream_format: Field<AudioSpeechStreamFormatWire>,
}

/// Speech API 的声音联合类型。
#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum AudioSpeechVoiceWire {
    /// 内建或兼容供应商提供的命名声音。
    Named(String),
    /// OpenAI custom voice ID 对象。
    Custom(AudioSpeechVoiceIdWire),
}

/// OpenAI custom voice ID 对象。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AudioSpeechVoiceIdWire {
    /// 已创建的 custom voice 标识。
    pub(super) id: String,
}

/// Speech API 支持的六种音频格式。
#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum AudioSpeechOutputFormatWire {
    /// MP3 压缩音频。
    Mp3,
    /// Ogg Opus 压缩音频。
    Opus,
    /// AAC 压缩音频。
    Aac,
    /// FLAC 无损压缩音频。
    Flac,
    /// WAV 容器音频。
    Wav,
    /// 24kHz 16-bit little-endian 单声道原始 PCM 音频。
    Pcm,
}

/// Speech API 支持的输出流格式。
#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum AudioSpeechStreamFormatWire {
    /// 原始音频字节流。
    Audio,
    /// Server-Sent Events 音频事件流。
    Sse,
}
