use bytes::Bytes;

use super::*;
use crate::CanonicalAudioSpeechResponseError;

fn request_for_format(format: Option<&str>) -> CanonicalAudioSpeechRequest {
    let mut value = serde_json::json!({
        "model":"gpt-4o-mini-tts",
        "input":"hello",
        "voice":"alloy"
    });
    if let Some(format) = format {
        value["response_format"] = Value::String(format.to_owned());
    }
    parse_request(&serde_json::to_vec(&value).unwrap()).unwrap()
}

#[test]
fn parses_minimal_request_and_redacts_external_values() {
    let request = parse_request(
        br#"{"model":"gpt-4o-mini-tts-private","input":"private speech canary","voice":"coral-private"}"#,
    )
    .unwrap();

    assert_eq!(request.operation(), af_domain::Operation::Audio);
    assert_eq!(request.model(), "gpt-4o-mini-tts-private");
    assert_eq!(request.input(), "private speech canary");
    assert_eq!(
        request.options().effective_output_format(),
        AudioSpeechOutputFormat::Mp3
    );
    assert_eq!(
        request.options().effective_speed(),
        AudioSpeechSpeed::DEFAULT
    );
    let AudioSpeechVoice::Named(name) = request.options().voice() else {
        panic!("expected named voice");
    };
    assert_eq!(name.as_str(), "coral-private");

    let debug = format!("{request:?}");
    assert!(!debug.contains("gpt-4o-mini-tts-private"));
    assert!(!debug.contains("private speech canary"));
    assert!(!debug.contains("coral-private"));
}

#[test]
fn parses_custom_voice_and_rebuilds_only_explicit_fields() {
    let request = parse_request(
        br#"{
            "model":"gpt-4o-mini-tts",
            "input":"Speak this private sentence.",
            "voice":{"id":"voice_private_123"},
            "instructions":"Speak with a calm tone.",
            "response_format":"flac",
            "speed":0.333333,
            "stream_format":"audio"
        }"#,
    )
    .unwrap();

    let AudioSpeechVoice::Custom(id) = request.options().voice() else {
        panic!("expected custom voice");
    };
    assert_eq!(id.as_str(), "voice_private_123");
    assert_eq!(
        request.options().instructions(),
        Some("Speak with a calm tone.")
    );
    assert_eq!(
        request.options().output_format(),
        Some(AudioSpeechOutputFormat::Flac)
    );
    assert_eq!(request.options().speed().unwrap().to_string(), "0.333333");
    assert_eq!(
        request.options().stream_format(),
        Some(AudioSpeechStreamFormat::Audio)
    );

    let rebuilt = build_request(&request).unwrap();
    assert_eq!(rebuilt["voice"]["id"], "voice_private_123");
    assert_eq!(rebuilt["response_format"], "flac");
    assert_eq!(rebuilt["speed"].to_string(), "0.333333");
    assert_eq!(rebuilt["stream_format"], "audio");
    assert_eq!(
        parse_request(&serde_json::to_vec(&rebuilt).unwrap()).unwrap(),
        request
    );

    let debug = format!("{:?}", request.options());
    assert!(!debug.contains("voice_private_123"));
    assert!(!debug.contains("calm tone"));
}

#[test]
fn request_accepts_current_named_voices_and_provider_compatible_names() {
    for voice in [
        "alloy",
        "ash",
        "ballad",
        "coral",
        "echo",
        "fable",
        "nova",
        "onyx",
        "sage",
        "shimmer",
        "verse",
        "marin",
        "cedar",
        "provider_voice_42",
    ] {
        let body = serde_json::json!({"model":"m","input":"x","voice":voice});
        assert!(
            parse_request(&serde_json::to_vec(&body).unwrap()).is_ok(),
            "voice={voice}"
        );
    }
}

#[test]
fn request_rejects_sse_aliases_invalid_ranges_null_and_unknown_fields() {
    let oversized_input = "语".repeat(crate::MAX_AUDIO_SPEECH_INPUT_CHARS + 1);
    let oversized_body = serde_json::json!({
        "model":"m",
        "input":oversized_input,
        "voice":"alloy"
    });
    assert!(parse_request(&serde_json::to_vec(&oversized_body).unwrap()).is_err());

    for body in [
        br#"{"model":"m","input":"","voice":"alloy"}"#.as_slice(),
        br#"{"model":"m","input":"x","voice":""}"#.as_slice(),
        br#"{"model":"m","input":"x","voice":{"id":""}}"#.as_slice(),
        br#"{"model":"m","input":"x","voice":{"id":"voice_1","extra":true}}"#.as_slice(),
        br#"{"model":"m","input":"x","voice":"alloy","speed":0.249999}"#.as_slice(),
        br#"{"model":"m","input":"x","voice":"alloy","speed":4.000001}"#.as_slice(),
        br#"{"model":"m","input":"x","voice":"alloy","stream_format":"sse"}"#.as_slice(),
        br#"{"model":"m","input":"x","voice":"alloy","language":"fr"}"#.as_slice(),
        br#"{"model":"m","input":"x","voice":"alloy","format":"wav"}"#.as_slice(),
        br#"{"model":"m","input":"x","voice":"alloy","instructions":null}"#.as_slice(),
        br#"{"model":"m","input":"x","voice":"alloy","unknown":true}"#.as_slice(),
    ] {
        assert!(
            parse_request(body).is_err(),
            "body={}",
            String::from_utf8_lossy(body)
        );
    }

    assert_eq!(
        parse_request(br#"{"model":"m","input":"x","voice":"alloy","stream_format":"sse"}"#),
        Err(ParseAudioSpeechRequestError::UnsupportedFeature)
    );
}

#[test]
fn request_rejects_duplicate_keys_before_deserialization() {
    assert_eq!(
        parse_request(br#"{"model":"first","model":"second","input":"x","voice":"alloy"}"#),
        Err(ParseAudioSpeechRequestError::DuplicateKey)
    );
}

#[test]
fn parses_and_rebuilds_all_supported_binary_formats() {
    let cases = [
        (
            AudioSpeechOutputFormat::Mp3,
            Bytes::from_static(b"ID3private-mp3"),
        ),
        (
            AudioSpeechOutputFormat::Opus,
            Bytes::from_static(b"OggS\x00\x00OpusHeadprivate-opus"),
        ),
        (
            AudioSpeechOutputFormat::Aac,
            Bytes::from_static(b"\xff\xf1private-aac"),
        ),
        (
            AudioSpeechOutputFormat::Flac,
            Bytes::from_static(b"fLaCprivate-flac"),
        ),
        (
            AudioSpeechOutputFormat::Wav,
            Bytes::from_static(b"RIFF\x04\x00\x00\x00WAVEprivate-wav"),
        ),
        (
            AudioSpeechOutputFormat::Pcm,
            Bytes::from_static(b"\x00\x01\x02\x03"),
        ),
    ];

    for (format, body) in cases {
        let response = parse_response(format, body.clone()).unwrap();
        assert_eq!(response.operation(), af_domain::Operation::Audio);
        assert_eq!(response.output_format(), format);
        assert_eq!(build_response(&response).unwrap(), body);
        assert!(!format!("{response:?}").contains("private"));
    }
}

#[test]
fn response_rejects_invalid_signatures_pcm_frames_and_request_mismatch() {
    assert_eq!(
        parse_response(AudioSpeechOutputFormat::Mp3, Bytes::from_static(b"not-mp3")),
        Err(ParseAudioSpeechResponseError::InvalidAudio)
    );
    assert_eq!(
        parse_response(AudioSpeechOutputFormat::Pcm, Bytes::from_static(b"\x00")),
        Err(ParseAudioSpeechResponseError::InvalidAudio)
    );
    assert_eq!(
        parse_response(AudioSpeechOutputFormat::Wav, Bytes::new()),
        Err(ParseAudioSpeechResponseError::InvalidAudio)
    );

    let response = parse_response(
        AudioSpeechOutputFormat::Wav,
        Bytes::from_static(b"RIFF\x04\x00\x00\x00WAVEaudio"),
    )
    .unwrap();
    assert_eq!(
        response.validate_for_request(&request_for_format(None)),
        Err(CanonicalAudioSpeechResponseError::OutputFormatMismatch)
    );
    response
        .validate_for_request(&request_for_format(Some("wav")))
        .unwrap();
}

#[test]
fn request_body_budget_is_enforced_before_json_conversion() {
    let oversized_instructions = "x".repeat(MAX_REQUEST_BODY_BYTES);
    let body = serde_json::json!({
        "model":"m",
        "input":"x",
        "voice":"alloy",
        "instructions":oversized_instructions
    })
    .to_string();
    assert_eq!(
        parse_request(body.as_bytes()),
        Err(ParseAudioSpeechRequestError::BodyTooLarge)
    );
}

#[test]
fn request_accepts_combined_valid_field_boundaries() {
    let body = serde_json::json!({
        "model": "m".repeat(af_domain::MAX_MODEL_NAME_BYTES),
        "input": "𐀀".repeat(crate::MAX_AUDIO_SPEECH_INPUT_CHARS),
        "voice": {"id": "v".repeat(crate::MAX_AUDIO_SPEECH_VOICE_ID_BYTES)},
        "instructions": "i".repeat(crate::MAX_AUDIO_SPEECH_INSTRUCTIONS_BYTES),
        "response_format": "flac",
        "speed": 4.0,
        "stream_format": "audio"
    });
    let encoded = serde_json::to_vec(&body).unwrap();

    assert!(encoded.len() < MAX_REQUEST_BODY_BYTES);
    let request = parse_request(&encoded).unwrap();
    assert_eq!(
        request.input().chars().count(),
        crate::MAX_AUDIO_SPEECH_INPUT_CHARS
    );
    assert_eq!(
        request.options().instructions().unwrap().len(),
        crate::MAX_AUDIO_SPEECH_INSTRUCTIONS_BYTES
    );
}
