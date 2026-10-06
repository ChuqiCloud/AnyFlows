use std::fmt;

use af_adapter::{Bytes, Method, Operation, StatusCode};
use af_domain::{AfError, UpstreamError};
use af_protocol::{AudioSpeechOutputFormat, CanonicalAudioSpeechRequest, openai_audio_speech};

use crate::error::{classify_upstream_status, map_adaptor_error, parse_retry_after};
use crate::{RelayAttemptReport, RelayError, RelayRequest, RelayStateMachine};

/// 已完成格式、签名与大小校验的 Speech 二进制响应。
pub struct SpeechResponse {
    body: Bytes,
    output_format: AudioSpeechOutputFormat,
}

impl SpeechResponse {
    /// 组合可直接返回下游的完整音频正文与确定格式。
    #[must_use]
    pub const fn new(body: Bytes, output_format: AudioSpeechOutputFormat) -> Self {
        Self {
            body,
            output_format,
        }
    }

    /// 消费响应并返回完整音频正文与下游 MIME 所需格式。
    #[must_use]
    pub fn into_parts(self) -> (Bytes, AudioSpeechOutputFormat) {
        (self.body, self.output_format)
    }
}

impl fmt::Debug for SpeechResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SpeechResponse")
            .field("body_bytes", &self.body.len())
            .field("output_format", &self.output_format)
            .finish()
    }
}

/// 使用固定候选状态机执行原生 OpenAI Audio Speech 非 SSE 请求。
pub async fn relay_openai_speech(
    machine: &RelayStateMachine,
    request: CanonicalAudioSpeechRequest,
) -> Result<SpeechResponse, AfError> {
    relay_openai_speech_with_report(machine, request)
        .await
        .into_result()
}

/// 执行 Speech 转发并保留脱敏候选尝试报告。
pub async fn relay_openai_speech_with_report(
    machine: &RelayStateMachine,
    request: CanonicalAudioSpeechRequest,
) -> RelayOpenAiSpeechOutcome {
    let outbound_body = match encode_openai_speech_request(&request) {
        Ok(body) => body,
        Err(error) => return RelayOpenAiSpeechOutcome::failed(error),
    };
    let relay_request = match RelayRequest::new(
        request.model().to_owned(),
        Operation::Audio,
        Method::POST,
        Some(outbound_body),
    ) {
        Ok(request) => {
            request.with_response_body_limit(openai_audio_speech::MAX_RESPONSE_BODY_BYTES)
        }
        Err(error) => return RelayOpenAiSpeechOutcome::failed(map_relay_error(error)),
    };
    let (response, mut report, _permit) = match machine.execute_with_report(relay_request).await {
        Ok(response) => response.into_parts_with_permit(),
        Err(error) => {
            let (error, report) = error.into_parts();
            return RelayOpenAiSpeechOutcome::new(Err(map_relay_error(error)), report);
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
                return RelayOpenAiSpeechOutcome::new(Err(error.into()), report);
            }
        };
        let error = classify_upstream_status(status, retry_after, &body);
        report.reject_success(error);
        return RelayOpenAiSpeechOutcome::new(Err(error.into()), report);
    }

    let body = match response
        .into_body()
        .into_bytes_with_limit(openai_audio_speech::MAX_RESPONSE_BODY_BYTES)
        .await
    {
        Ok(body) => body,
        Err(error) => {
            let error = map_adaptor_error(error);
            report.reject_success(error);
            return RelayOpenAiSpeechOutcome::new(Err(error.into()), report);
        }
    };
    let output_format = request.options().effective_output_format();
    let canonical = match openai_audio_speech::parse_response(output_format, body) {
        Ok(response) => response,
        Err(_) => return protocol_failure(report),
    };
    if canonical.validate_for_request(&request).is_err() {
        return protocol_failure(report);
    }
    let body = match openai_audio_speech::build_response(&canonical) {
        Ok(body) => body,
        Err(_) => return protocol_failure(report),
    };
    RelayOpenAiSpeechOutcome::new(Ok(SpeechResponse::new(body, output_format)), report)
}

/// 将 Canonical Speech 请求编码为受限 OpenAI JSON 正文。
pub fn encode_openai_speech_request(
    request: &CanonicalAudioSpeechRequest,
) -> Result<Bytes, AfError> {
    openai_audio_speech::build_request(request)
        .map_err(|_| AfError::Internal)
        .and_then(|value| {
            serde_json::to_vec(&value)
                .map(Bytes::from)
                .map_err(|_| AfError::Internal)
        })
}

/// OpenAI Speech 业务结果及其脱敏候选报告。
pub struct RelayOpenAiSpeechOutcome {
    result: Result<SpeechResponse, AfError>,
    report: RelayAttemptReport,
}

impl RelayOpenAiSpeechOutcome {
    fn new(result: Result<SpeechResponse, AfError>, report: RelayAttemptReport) -> Self {
        Self { result, report }
    }

    fn failed(error: AfError) -> Self {
        Self::new(Err(error), RelayAttemptReport::default())
    }

    /// 消费包装并返回业务结果。
    pub fn into_result(self) -> Result<SpeechResponse, AfError> {
        self.result
    }

    /// 消费包装并拆分业务结果与候选报告。
    pub fn into_parts(self) -> (Result<SpeechResponse, AfError>, RelayAttemptReport) {
        (self.result, self.report)
    }
}

impl fmt::Debug for RelayOpenAiSpeechOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RelayOpenAiSpeechOutcome")
            .field("result", &self.result)
            .field("report", &self.report)
            .finish()
    }
}

fn protocol_failure(mut report: RelayAttemptReport) -> RelayOpenAiSpeechOutcome {
    report.reject_success(UpstreamError::ProtocolError);
    RelayOpenAiSpeechOutcome::new(Err(UpstreamError::ProtocolError.into()), report)
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
    use af_protocol::{AudioSpeechOptions, AudioSpeechVoice, AudioSpeechVoiceName};

    use super::*;

    #[test]
    fn request_encoder_preserves_speech_fields_without_leaking_debug_content() {
        let request = CanonicalAudioSpeechRequest::new(
            "private-speech-model".to_owned(),
            "private speech input".to_owned(),
            AudioSpeechOptions::new(
                AudioSpeechVoice::Named(AudioSpeechVoiceName::new("coral".to_owned()).unwrap()),
                None,
                Some(AudioSpeechOutputFormat::Wav),
                None,
                None,
            )
            .unwrap(),
        )
        .unwrap();
        let body = encode_openai_speech_request(&request).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["model"], "private-speech-model");
        assert_eq!(value["response_format"], "wav");
        let rendered = format!("{request:?}");
        assert!(!rendered.contains("private speech input"));
        assert!(!rendered.contains("private-speech-model"));
    }
}
