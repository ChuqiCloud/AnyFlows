use std::fmt;

use af_adapter::{Bytes, Method, OpenAiAdaptor, Operation, StatusCode};
use af_domain::{AfError, UpstreamError};
use af_protocol::{
    AudioTranscriptionUsage, CanonicalAudioTranscriptionRequest, openai_audio,
    openai_audio::{OpenAiTranscriptionForm, OpenAiTranscriptionFormPart},
};

use crate::error::{classify_upstream_status, map_adaptor_error, parse_retry_after};
use crate::{RelayAttemptReport, RelayError, RelayRequest, RelayStateMachine};

/// Audio 转录出站 multipart 正文允许的最大字节数。
pub const MAX_AUDIO_MULTIPART_BODY_BYTES: usize = 32 * 1_024 * 1_024;

/// 已完成 Canonical 校验并可直接返回下游的 Audio 转录响应。
pub struct AudioTranscriptionResponse {
    body: Bytes,
    usage: Option<AudioTranscriptionUsage>,
}

impl AudioTranscriptionResponse {
    /// 组合公开响应正文与上游明确返回的 token 或时长用量。
    #[must_use]
    pub const fn new(body: Bytes, usage: Option<AudioTranscriptionUsage>) -> Self {
        Self { body, usage }
    }

    /// 消费响应并返回 HTTP 正文与可选的最终计费用量事实。
    #[must_use]
    pub fn into_parts(self) -> (Bytes, Option<AudioTranscriptionUsage>) {
        (self.body, self.usage)
    }
}

impl fmt::Debug for AudioTranscriptionResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AudioTranscriptionResponse")
            .field("body_bytes", &self.body.len())
            .field("usage", &self.usage)
            .finish()
    }
}

/// 使用固定候选状态机执行原生 OpenAI Audio 非流式文件转录。
pub async fn relay_openai_audio(
    machine: &RelayStateMachine,
    request: CanonicalAudioTranscriptionRequest,
) -> Result<AudioTranscriptionResponse, AfError> {
    relay_openai_audio_with_report(machine, request)
        .await
        .into_result()
}

/// 执行 Audio 转录并保留脱敏候选尝试报告。
pub async fn relay_openai_audio_with_report(
    machine: &RelayStateMachine,
    request: CanonicalAudioTranscriptionRequest,
) -> RelayOpenAiAudioOutcome {
    let outbound_body = match encode_openai_audio_request(&request) {
        Ok(body) => body,
        Err(error) => return RelayOpenAiAudioOutcome::failed(error),
    };
    let relay_request = match RelayRequest::new(
        request.model().to_owned(),
        Operation::Audio,
        Method::POST,
        Some(outbound_body),
    ) {
        Ok(request) => {
            request.with_response_body_limit(openai_audio::MAX_TRANSCRIPTION_RESPONSE_BODY_BYTES)
        }
        Err(error) => return RelayOpenAiAudioOutcome::failed(map_relay_error(error)),
    };
    let (response, mut report, _permit) = match machine.execute_with_report(relay_request).await {
        Ok(response) => response.into_parts_with_permit(),
        Err(error) => {
            let (error, report) = error.into_parts();
            return RelayOpenAiAudioOutcome::new(Err(map_relay_error(error)), report);
        }
    };
    let status = response.status();
    if status != StatusCode::OK {
        let retry_after = parse_retry_after(response.headers());
        let body = match response.into_body().into_bytes().await {
            Ok(body) => body,
            Err(error) => {
                let error = map_adaptor_error(error);
                report.reject_success(error);
                return RelayOpenAiAudioOutcome::new(Err(error.into()), report);
            }
        };
        let error = classify_upstream_status(status, retry_after, &body);
        report.reject_success(error);
        return RelayOpenAiAudioOutcome::new(Err(error.into()), report);
    }

    let body = match response
        .into_body()
        .into_bytes_with_limit(openai_audio::MAX_TRANSCRIPTION_RESPONSE_BODY_BYTES)
        .await
    {
        Ok(body) => body,
        Err(error) => {
            let error = map_adaptor_error(error);
            report.reject_success(error);
            return RelayOpenAiAudioOutcome::new(Err(error.into()), report);
        }
    };
    let upstream = match openai_audio::parse_transcription_response(&body) {
        Ok(response) => response,
        Err(_) => return protocol_failure(report),
    };
    let usage = upstream.usage();
    let body = match encode_response(&upstream) {
        Ok(body) => body,
        Err(()) => return protocol_failure(report),
    };
    RelayOpenAiAudioOutcome::new(Ok(AudioTranscriptionResponse::new(body, usage)), report)
}

/// 将 Canonical Audio 请求编码为适配器所声明 boundary 的受限 multipart 正文。
pub fn encode_openai_audio_request(
    request: &CanonicalAudioTranscriptionRequest,
) -> Result<Bytes, AfError> {
    let form = openai_audio::build_transcription_request(request).map_err(|_| AfError::Internal)?;
    encode_multipart(&form, OpenAiAdaptor::AUDIO_MULTIPART_BOUNDARY).map_err(|error| match error {
        MultipartEncodingError::BoundaryCollision => AfError::InvalidRequest,
        MultipartEncodingError::InvalidBoundary | MultipartEncodingError::BodyTooLarge => {
            AfError::Internal
        }
    })
}

/// OpenAI Audio 业务结果及其脱敏候选报告。
pub struct RelayOpenAiAudioOutcome {
    result: Result<AudioTranscriptionResponse, AfError>,
    report: RelayAttemptReport,
}

impl RelayOpenAiAudioOutcome {
    fn new(
        result: Result<AudioTranscriptionResponse, AfError>,
        report: RelayAttemptReport,
    ) -> Self {
        Self { result, report }
    }

    fn failed(error: AfError) -> Self {
        Self::new(Err(error), RelayAttemptReport::default())
    }

    /// 消费包装并返回业务结果。
    pub fn into_result(self) -> Result<AudioTranscriptionResponse, AfError> {
        self.result
    }

    /// 消费包装并拆分业务结果与候选报告。
    pub fn into_parts(
        self,
    ) -> (
        Result<AudioTranscriptionResponse, AfError>,
        RelayAttemptReport,
    ) {
        (self.result, self.report)
    }
}

impl fmt::Debug for RelayOpenAiAudioOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RelayOpenAiAudioOutcome")
            .field("result", &self.result)
            .field("report", &self.report)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MultipartEncodingError {
    InvalidBoundary,
    BoundaryCollision,
    BodyTooLarge,
}

fn encode_multipart(
    form: &OpenAiTranscriptionForm,
    boundary: &str,
) -> Result<Bytes, MultipartEncodingError> {
    if boundary.is_empty()
        || boundary.len() > 70
        || !boundary.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'\''
                        | b'('
                        | b')'
                        | b'+'
                        | b'_'
                        | b','
                        | b'-'
                        | b'.'
                        | b'/'
                        | b':'
                        | b'='
                        | b'?'
                )
        })
    {
        return Err(MultipartEncodingError::InvalidBoundary);
    }
    let boundary_bytes = boundary.as_bytes();
    for part in form.parts() {
        let value = match part {
            OpenAiTranscriptionFormPart::Text(part) => part.value().as_bytes(),
            OpenAiTranscriptionFormPart::File(part) => part.bytes().as_ref(),
        };
        if contains_subslice(value, boundary_bytes) {
            return Err(MultipartEncodingError::BoundaryCollision);
        }
    }

    let mut body = Vec::new();
    for part in form.parts() {
        append(&mut body, b"--")?;
        append(&mut body, boundary_bytes)?;
        append(&mut body, b"\r\n")?;
        match part {
            OpenAiTranscriptionFormPart::Text(part) => {
                append(&mut body, b"Content-Disposition: form-data; name=\"")?;
                append(&mut body, part.name().as_bytes())?;
                append(&mut body, b"\"\r\n\r\n")?;
                append(&mut body, part.value().as_bytes())?;
            }
            OpenAiTranscriptionFormPart::File(part) => {
                append(&mut body, b"Content-Disposition: form-data; name=\"")?;
                append(&mut body, part.name().as_bytes())?;
                append(&mut body, b"\"; filename=\"")?;
                append(&mut body, part.file_name().as_bytes())?;
                append(&mut body, b"\"\r\n")?;
                if let Some(content_type) = part.content_type() {
                    append(&mut body, b"Content-Type: ")?;
                    append(&mut body, content_type.as_bytes())?;
                    append(&mut body, b"\r\n")?;
                }
                append(&mut body, b"\r\n")?;
                append(&mut body, part.bytes())?;
            }
        }
        append(&mut body, b"\r\n")?;
    }
    append(&mut body, b"--")?;
    append(&mut body, boundary_bytes)?;
    append(&mut body, b"--\r\n")?;
    Ok(Bytes::from(body))
}

fn append(body: &mut Vec<u8>, value: &[u8]) -> Result<(), MultipartEncodingError> {
    let next_len = body
        .len()
        .checked_add(value.len())
        .ok_or(MultipartEncodingError::BodyTooLarge)?;
    if next_len > MAX_AUDIO_MULTIPART_BODY_BYTES {
        return Err(MultipartEncodingError::BodyTooLarge);
    }
    body.extend_from_slice(value);
    Ok(())
}

fn contains_subslice(value: &[u8], needle: &[u8]) -> bool {
    value
        .windows(needle.len())
        .any(|candidate| candidate == needle)
}

fn encode_response(
    response: &af_protocol::CanonicalAudioTranscriptionResponse,
) -> Result<Bytes, ()> {
    let value = openai_audio::build_transcription_response(response).map_err(|_| ())?;
    serde_json::to_vec(&value).map(Bytes::from).map_err(|_| ())
}

fn protocol_failure(mut report: RelayAttemptReport) -> RelayOpenAiAudioOutcome {
    report.reject_success(UpstreamError::ProtocolError);
    RelayOpenAiAudioOutcome::new(Err(UpstreamError::ProtocolError.into()), report)
}

fn map_relay_error(error: RelayError) -> AfError {
    match error {
        RelayError::InvalidModel => AfError::InvalidRequest,
        RelayError::Request(_) | RelayError::Adaptor(_) | RelayError::AttemptGateFailed => {
            AfError::Internal
        }
        RelayError::ConcurrencyUnavailable => AfError::ConcurrencyLimited,
        RelayError::Upstream(error) => error.into(),
    }
}

#[cfg(test)]
mod tests {
    use af_protocol::openai_audio::{
        OpenAiTranscriptionForm, OpenAiTranscriptionFormPart, parse_transcription_request,
    };

    use super::*;

    #[test]
    fn multipart_uses_normalized_file_metadata_and_fixed_boundary() {
        let request = request_with_audio(Bytes::from_static(b"RIFF\x04\x00\x00\x00WAVEdata"));
        let body = encode_openai_audio_request(&request).unwrap();
        let text = String::from_utf8_lossy(&body);

        assert!(text.starts_with(&format!(
            "--{}\r\n",
            OpenAiAdaptor::AUDIO_MULTIPART_BOUNDARY
        )));
        assert!(text.contains("name=\"model\"\r\n\r\ngpt-audio-private"));
        assert!(text.contains("name=\"file\"; filename=\"audio.wav\""));
        assert!(text.contains("Content-Type: audio/wav\r\n"));
        assert!(!text.contains("client-secret-name.wav"));
        assert!(text.ends_with(&format!(
            "\r\n--{}--\r\n",
            OpenAiAdaptor::AUDIO_MULTIPART_BOUNDARY
        )));
    }

    #[test]
    fn multipart_rejects_boundary_collision_in_file_bytes() {
        let mut bytes = b"RIFF\x04\x00\x00\x00WAVEdata".to_vec();
        bytes.extend_from_slice(OpenAiAdaptor::AUDIO_MULTIPART_BOUNDARY.as_bytes());
        let request = request_with_audio(Bytes::from(bytes));

        assert!(matches!(
            encode_openai_audio_request(&request),
            Err(AfError::InvalidRequest)
        ));
    }

    fn request_with_audio(bytes: Bytes) -> CanonicalAudioTranscriptionRequest {
        let form = OpenAiTranscriptionForm::new(vec![
            OpenAiTranscriptionFormPart::text("model".to_owned(), "gpt-audio-private".to_owned())
                .unwrap(),
            OpenAiTranscriptionFormPart::file(
                "file".to_owned(),
                "client-secret-name.wav".to_owned(),
                Some("audio/wav".to_owned()),
                bytes,
            )
            .unwrap(),
        ])
        .unwrap();
        parse_transcription_request(form).unwrap()
    }
}
