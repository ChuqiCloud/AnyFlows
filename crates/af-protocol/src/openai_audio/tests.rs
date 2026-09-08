use bytes::Bytes;

use super::*;

fn text(name: &str, value: &str) -> OpenAiTranscriptionFormPart {
    OpenAiTranscriptionFormPart::text(name.to_owned(), value.to_owned()).unwrap()
}

fn file(
    name: &str,
    file_name: &str,
    content_type: Option<&str>,
    bytes: Bytes,
) -> OpenAiTranscriptionFormPart {
    OpenAiTranscriptionFormPart::file(
        name.to_owned(),
        file_name.to_owned(),
        content_type.map(str::to_owned),
        bytes,
    )
    .unwrap()
}

fn form(parts: Vec<OpenAiTranscriptionFormPart>) -> OpenAiTranscriptionForm {
    OpenAiTranscriptionForm::new(parts).unwrap()
}

fn wav_bytes(canary: &[u8]) -> Bytes {
    let mut bytes = b"RIFF\x04\x00\x00\x00WAVE".to_vec();
    bytes.extend_from_slice(canary);
    Bytes::from(bytes)
}

fn minimal_form() -> OpenAiTranscriptionForm {
    form(vec![
        text("model", "gpt-transcribe-private"),
        file(
            "file",
            "private-recording.wav",
            Some("audio/wav"),
            wav_bytes(b"private-audio-canary"),
        ),
    ])
}

#[test]
fn parses_minimal_form_and_rebuilds_sanitized_file_metadata() {
    let request = parse_transcription_request(minimal_form()).unwrap();

    assert_eq!(request.operation(), af_domain::Operation::Audio);
    assert_eq!(request.model(), "gpt-transcribe-private");
    assert_eq!(request.file().format(), AudioFileFormat::Wav);
    assert_eq!(request.options().prompt(), None);
    assert_eq!(request.options().language_hints(), None);
    assert!(request.options().keywords().is_empty());
    assert_eq!(request.options().temperature(), None);

    let debug = format!("{request:?}");
    assert!(!debug.contains("gpt-transcribe-private"));
    assert!(!debug.contains("private-audio-canary"));
    assert!(!debug.contains("private-recording.wav"));

    let rebuilt = build_transcription_request(&request).unwrap();
    let rebuilt_file = rebuilt
        .parts()
        .iter()
        .find_map(OpenAiTranscriptionFormPart::as_file)
        .unwrap();
    assert_eq!(rebuilt_file.file_name(), "audio.wav");
    assert_eq!(rebuilt_file.content_type(), Some("audio/wav"));
    assert_eq!(
        parse_transcription_request(rebuilt.clone()).unwrap(),
        request
    );

    let form_debug = format!("{rebuilt:?}");
    assert!(!form_debug.contains("private-audio-canary"));
}

#[test]
fn parses_full_context_and_rebuilds_only_canonical_fields() {
    let request = parse_transcription_request(form(vec![
        text("model", "gpt-transcribe"),
        text("prompt", "A private support call about plan AC-42."),
        text("languages[]", "en"),
        text("languages[]", "fr"),
        text("keywords[]", "premium plan"),
        text("keywords[]", "AC-42"),
        text("temperature", "0.250000"),
        text("response_format", "json"),
        text("stream", "false"),
        file(
            "file",
            "meeting.wav",
            Some("application/octet-stream"),
            wav_bytes(b"audio"),
        ),
    ]))
    .unwrap();

    assert_eq!(
        request.options().prompt(),
        Some("A private support call about plan AC-42.")
    );
    let languages = request
        .options()
        .language_hints()
        .unwrap()
        .as_multiple()
        .unwrap();
    assert_eq!(
        languages
            .iter()
            .map(AudioLanguageCode::as_str)
            .collect::<Vec<_>>(),
        ["en", "fr"]
    );
    assert_eq!(request.options().keywords().len(), 2);
    assert_eq!(request.options().temperature().unwrap().to_string(), "0.25");

    let rebuilt = build_transcription_request(&request).unwrap();
    assert!(
        rebuilt
            .parts()
            .iter()
            .all(|part| { !matches!(part.name(), "response_format" | "stream") })
    );
    assert_eq!(parse_transcription_request(rebuilt).unwrap(), request);
    assert!(!format!("{:?}", request.options()).contains("AC-42"));
}

#[test]
fn request_rejects_duplicates_conflicts_unknown_and_unsupported_fields() {
    let duplicate = form(vec![
        text("model", "first"),
        text("model", "second"),
        file("file", "audio.wav", Some("audio/wav"), wav_bytes(b"x")),
    ]);
    assert_eq!(
        parse_transcription_request(duplicate),
        Err(ParseAudioTranscriptionRequestError::DuplicateField)
    );

    for (part, expected) in [
        (
            text("stream", "true"),
            ParseAudioTranscriptionRequestError::UnsupportedFeature,
        ),
        (
            text("response_format", "verbose_json"),
            ParseAudioTranscriptionRequestError::UnsupportedFeature,
        ),
        (
            text("timestamp_granularities[]", "word"),
            ParseAudioTranscriptionRequestError::UnsupportedFeature,
        ),
        (
            text("chunking_strategy", "auto"),
            ParseAudioTranscriptionRequestError::UnsupportedFeature,
        ),
        (
            text("unknown_private_field", "secret"),
            ParseAudioTranscriptionRequestError::UnknownField,
        ),
    ] {
        let request = form(vec![
            text("model", "m"),
            part,
            file("file", "audio.wav", Some("audio/wav"), wav_bytes(b"x")),
        ]);
        assert_eq!(parse_transcription_request(request), Err(expected));
    }

    let language_conflict = form(vec![
        text("model", "m"),
        text("language", "en"),
        text("languages[]", "fr"),
        file("file", "audio.wav", Some("audio/wav"), wav_bytes(b"x")),
    ]);
    assert_eq!(
        parse_transcription_request(language_conflict),
        Err(ParseAudioTranscriptionRequestError::InvalidValue)
    );
}

#[test]
fn request_rejects_invalid_file_type_mime_and_signature() {
    for part in [
        file(
            "file",
            "audio.exe",
            Some("application/octet-stream"),
            wav_bytes(b"x"),
        ),
        file("file", "audio.wav", Some("audio/mpeg"), wav_bytes(b"x")),
        file(
            "file",
            "audio.wav",
            Some("audio/wav"),
            Bytes::from_static(b"ID3not-wav"),
        ),
    ] {
        let request = form(vec![text("model", "m"), part]);
        assert_eq!(
            parse_transcription_request(request),
            Err(ParseAudioTranscriptionRequestError::InvalidValue)
        );
    }

    let text_file = form(vec![text("model", "m"), text("file", "not-a-file")]);
    assert_eq!(
        parse_transcription_request(text_file),
        Err(ParseAudioTranscriptionRequestError::InvalidValue)
    );
}

#[test]
fn parses_token_usage_languages_and_round_trips() {
    let body = serde_json::json!({
        "text": "Bonjour, private transcript canary.",
        "languages": [{"code":"fr"}],
        "usage": {
            "type": "tokens",
            "input_tokens": 14,
            "input_token_details": {"audio_tokens": 14, "text_tokens": 0},
            "output_tokens": 45,
            "total_tokens": 59
        }
    });
    let response = parse_transcription_response(&serde_json::to_vec(&body).unwrap()).unwrap();

    assert_eq!(response.operation(), af_domain::Operation::Audio);
    assert_eq!(response.languages().unwrap()[0].as_str(), "fr");
    let AudioTranscriptionUsage::Tokens(usage) = response.usage().unwrap() else {
        panic!("expected token usage");
    };
    assert_eq!(usage.input_tokens().get(), 14);
    assert_eq!(usage.output_tokens().get(), 45);
    assert_eq!(
        usage.input_details().unwrap().audio_tokens().unwrap().get(),
        14
    );
    assert!(!format!("{response:?}").contains("private transcript canary"));

    let rebuilt = build_transcription_response(&response).unwrap();
    assert_eq!(
        parse_transcription_response(&serde_json::to_vec(&rebuilt).unwrap()).unwrap(),
        response
    );
}

#[test]
fn preserves_missing_usage_and_present_empty_languages() {
    let response = parse_transcription_response(br#"{"text":"","languages":[]}"#).unwrap();
    assert_eq!(response.usage(), None);
    assert_eq!(response.languages(), Some([].as_slice()));

    let rebuilt = build_transcription_response(&response).unwrap();
    assert_eq!(rebuilt["languages"], serde_json::json!([]));
    assert!(rebuilt.get("usage").is_none());

    let missing_languages = parse_transcription_response(br#"{"text":"silence"}"#).unwrap();
    assert_eq!(missing_languages.languages(), None);
}

#[test]
fn duration_usage_preserves_fixed_precision() {
    let response = parse_transcription_response(
        br#"{"text":"hello","usage":{"type":"duration","seconds":8.470000267}}"#,
    )
    .unwrap();
    let AudioTranscriptionUsage::Duration(duration) = response.usage().unwrap() else {
        panic!("expected duration usage");
    };
    assert_eq!(duration.to_string(), "8.470000267");
    assert_eq!(duration.ceil_seconds(), 9);

    let rebuilt = build_transcription_response(&response).unwrap();
    assert_eq!(rebuilt["usage"]["seconds"].to_string(), "8.470000267");
    assert_eq!(
        parse_transcription_response(&serde_json::to_vec(&rebuilt).unwrap()).unwrap(),
        response
    );
}

#[test]
fn response_rejects_invalid_usage_relations() {
    for usage in [
        serde_json::json!({
            "type":"tokens","input_tokens":-1,"output_tokens":1,"total_tokens":0
        }),
        serde_json::json!({
            "type":"tokens","input_tokens":1,"output_tokens":1,"total_tokens":3
        }),
        serde_json::json!({
            "type":"tokens","input_tokens":2,"output_tokens":1,"total_tokens":3,
            "input_token_details":{"audio_tokens":1,"text_tokens":0}
        }),
        serde_json::json!({
            "type":"tokens","input_tokens":2,"output_tokens":1,"total_tokens":3,
            "input_token_details":{}
        }),
        serde_json::json!({"type":"duration","seconds":-1}),
        serde_json::json!({"type":"duration","seconds":86400.000000001}),
    ] {
        let body = serde_json::json!({"text":"hello","usage":usage});
        assert_eq!(
            parse_transcription_response(&serde_json::to_vec(&body).unwrap()),
            Err(ParseAudioTranscriptionResponseError::InvalidValue)
        );
    }
}

#[test]
fn response_rejects_duplicate_unknown_and_unmodeled_fields() {
    assert_eq!(
        parse_transcription_response(br#"{"text":"first","text":"second"}"#),
        Err(ParseAudioTranscriptionResponseError::DuplicateField)
    );
    assert_eq!(
        parse_transcription_response(br#"{"text":"hello","task":"transcribe"}"#),
        Err(ParseAudioTranscriptionResponseError::InvalidValue)
    );
    assert_eq!(
        parse_transcription_response(br#"{"text":"hello","logprobs":[]}"#),
        Err(ParseAudioTranscriptionResponseError::UnsupportedFeature)
    );
}
