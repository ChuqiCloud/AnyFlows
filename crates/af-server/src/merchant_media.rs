//! Platform-observed media usage replaces every supplier-owned billing claim.
use af_adapter::Bytes;
use af_domain::AfError;
use af_protocol::Usage;

pub(crate) fn normalized_json_usage(body: Bytes, usage: Usage) -> Result<Bytes, AfError> {
    // Transcription supports plain text/SRT/VTT as well as JSON.
    if body.iter().copied().find(|b| !b.is_ascii_whitespace()) != Some(b'{') {
        return Ok(body);
    }
    let mut value: serde_json::Value =
        serde_json::from_slice(&body).map_err(|_| AfError::Internal)?;
    let object = value.as_object_mut().ok_or(AfError::Internal)?;
    object.insert("usage".to_owned(), serde_json::json!({
        "input_tokens":usage.input_tokens().get(),"output_tokens":usage.output_tokens().get(),
        "total_tokens":usage.input_tokens().get().checked_add(usage.output_tokens().get()).ok_or(AfError::Internal)?,
    }));
    serde_json::to_vec(&value)
        .map(Bytes::from)
        .map_err(|_| AfError::Internal)
}

/// Audio's tagged usage schema differs from image generation's token totals.
pub(crate) fn normalized_audio_usage(body: Bytes, usage: Usage) -> Result<Bytes, AfError> {
    if body
        .iter()
        .copied()
        .find(|byte| !byte.is_ascii_whitespace())
        != Some(b'{')
    {
        return Ok(body);
    }
    let audio_tokens = usage.details().audio_input().get();
    let text_tokens = usage
        .input_tokens()
        .get()
        .checked_sub(audio_tokens)
        .ok_or(AfError::Internal)?;
    let mut value: serde_json::Value =
        serde_json::from_slice(&body).map_err(|_| AfError::Internal)?;
    let object = value.as_object_mut().ok_or(AfError::Internal)?;
    object.insert("usage".to_owned(),serde_json::json!({
        "type":"tokens",
        "input_tokens":usage.input_tokens().get(),
        "output_tokens":usage.output_tokens().get(),
        "total_tokens":usage.input_tokens().get().checked_add(usage.output_tokens().get()).ok_or(AfError::Internal)?,
        "input_token_details":{"audio_tokens":audio_tokens,"text_tokens":text_tokens},
    }));
    let result = serde_json::to_vec(&value).map_err(|_| AfError::Internal)?;
    af_protocol::openai_audio::parse_transcription_response(&result)
        .map_err(|_| AfError::Internal)?;
    Ok(Bytes::from(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use af_protocol::{TokenCount, UsageDetails, UsageSemantics, UsageSource};
    #[test]
    fn merchant_media_replaces_supplier_usage_but_preserves_the_result() {
        let zero = TokenCount::ZERO;
        let usage = Usage::new(
            TokenCount::new(7).unwrap(),
            TokenCount::new(3).unwrap(),
            UsageDetails::new(zero, zero, zero, zero, zero, zero),
            UsageSource::Estimated,
            UsageSemantics::Inclusive,
        )
        .unwrap();
        let body = normalized_json_usage(
            Bytes::from_static(br#"{"text":"hello","usage":{"input_tokens":90000000}}"#),
            usage,
        )
        .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["text"], "hello");
        assert_eq!(value["usage"]["input_tokens"], 7);
        assert_eq!(value["usage"]["total_tokens"], 10);
    }

    fn local_audio_usage() -> Usage {
        let zero = TokenCount::ZERO;
        Usage::new(
            TokenCount::new(17).unwrap(),
            TokenCount::new(3).unwrap(),
            UsageDetails::new(zero, zero, zero, zero, TokenCount::new(14).unwrap(), zero),
            UsageSource::Estimated,
            UsageSemantics::Inclusive,
        )
        .unwrap()
    }
    #[test]
    fn merchant_audio_wire_round_trips_the_tagged_token_schema_and_local_details() {
        for supplier in [
            br#"{"text":"hello","languages":[{"code":"en"}],"usage":{"type":"duration","seconds":80000}}"#.as_slice(),
            br#"{"text":"hello","languages":[{"code":"en"}],"usage":{"type":"tokens","input_tokens":900000,"output_tokens":800000,"total_tokens":1700000}}"#.as_slice(),
        ]{
            let normalized=normalized_audio_usage(Bytes::copy_from_slice(supplier),local_audio_usage()).unwrap();
            let decoded=af_protocol::openai_audio::parse_transcription_response(&normalized).unwrap();
            assert_eq!(decoded.text(),"hello");assert_eq!(decoded.languages().unwrap()[0].as_str(),"en");
            let Some(af_protocol::AudioTranscriptionUsage::Tokens(usage))=decoded.usage()else{panic!("merchant audio must expose platform token totals")};
            assert_eq!(usage.input_tokens().get(),17);assert_eq!(usage.output_tokens().get(),3);
            let details=usage.input_details().unwrap();assert_eq!(details.audio_tokens().unwrap().get(),14);assert_eq!(details.text_tokens().unwrap().get(),3);
            assert_eq!(local_audio_usage().source(),UsageSource::Estimated);
        }
    }
    #[test]
    fn merchant_audio_preserves_text_srt_and_vtt_responses() {
        for body in [
            "hello",
            "1\n00:00:00,000 --> 00:00:01,000\nhello",
            "WEBVTT\n\n00:00.000 --> 00:01.000\nhello",
        ] {
            let body = Bytes::copy_from_slice(body.as_bytes());
            assert_eq!(
                normalized_audio_usage(body.clone(), local_audio_usage()).unwrap(),
                body
            );
        }
    }
    #[test]
    fn merchant_audio_rejects_malformed_json_and_image_usage_keeps_its_own_shape() {
        assert!(
            normalized_audio_usage(Bytes::from_static(b"{broken"), local_audio_usage()).is_err()
        );
        assert!(normalized_audio_usage(Bytes::from_static(b"{}"), local_audio_usage()).is_err());
        let image=normalized_json_usage(Bytes::from_static(br#"{"data":[{"url":"https://media.example/image.png"}],"usage":{"total_tokens":900000}}"#),local_audio_usage()).unwrap();
        let image: serde_json::Value = serde_json::from_slice(&image).unwrap();
        assert!(image["usage"].get("type").is_none());
        assert_eq!(image["usage"]["total_tokens"], 20);
        assert_eq!(image["data"][0]["url"], "https://media.example/image.png");
    }
}
