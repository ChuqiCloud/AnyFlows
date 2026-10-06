use std::fmt;

use af_adapter::{Bytes, Method, Operation, StatusCode};
use af_domain::{AfError, UpstreamError};
use af_protocol::{
    CanonicalImageGenerationRequest, ImageGenerationUsage, TokenCount, Usage, UsageDetails,
    UsageSemantics, UsageSource, openai_images,
};

use crate::error::{classify_upstream_status, map_adaptor_error, parse_retry_after};
use crate::{RelayAttemptReport, RelayError, RelayRequest, RelayStateMachine};

/// 已完成 Canonical 校验并可直接返回下游的 Images 响应。
pub struct ImageResponse {
    body: Bytes,
    usage: Option<Usage>,
}

impl ImageResponse {
    /// 组合公开响应正文与可选的真实上游用量。
    #[must_use]
    pub const fn new(body: Bytes, usage: Option<Usage>) -> Self {
        Self { body, usage }
    }

    /// 消费响应并返回 HTTP 正文与可选的最终计费用量。
    #[must_use]
    pub fn into_parts(self) -> (Bytes, Option<Usage>) {
        (self.body, self.usage)
    }
}

impl fmt::Debug for ImageResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ImageResponse")
            .field("body_bytes", &self.body.len())
            .field("usage", &self.usage)
            .finish()
    }
}

/// 使用固定候选状态机执行原生 OpenAI Images 非流式生成。
pub async fn relay_openai_images(
    machine: &RelayStateMachine,
    request: CanonicalImageGenerationRequest,
) -> Result<ImageResponse, AfError> {
    relay_openai_images_with_report(machine, request)
        .await
        .into_result()
}

/// 执行 Images 转发并保留脱敏候选尝试报告。
pub async fn relay_openai_images_with_report(
    machine: &RelayStateMachine,
    request: CanonicalImageGenerationRequest,
) -> RelayOpenAiImagesOutcome {
    let outbound_body = match encode_request(&request) {
        Ok(body) => body,
        Err(error) => return RelayOpenAiImagesOutcome::failed(error),
    };
    let relay_request = match RelayRequest::new(
        request.model().to_owned(),
        Operation::Image,
        Method::POST,
        Some(outbound_body),
    ) {
        Ok(request) => request.with_response_body_limit(openai_images::MAX_RESPONSE_BODY_BYTES),
        Err(error) => return RelayOpenAiImagesOutcome::failed(map_relay_error(error)),
    };
    let (response, mut report, _permit) = match machine.execute_with_report(relay_request).await {
        Ok(response) => response.into_parts_with_permit(),
        Err(error) => {
            let (error, report) = error.into_parts();
            return RelayOpenAiImagesOutcome::new(Err(map_relay_error(error)), report);
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
                return RelayOpenAiImagesOutcome::new(Err(error.into()), report);
            }
        };
        let error = classify_upstream_status(status, retry_after, &body);
        report.reject_success(error);
        return RelayOpenAiImagesOutcome::new(Err(error.into()), report);
    }

    let body = match response
        .into_body()
        .into_bytes_with_limit(openai_images::MAX_RESPONSE_BODY_BYTES)
        .await
    {
        Ok(body) => body,
        Err(error) => {
            let error = map_adaptor_error(error);
            report.reject_success(error);
            return RelayOpenAiImagesOutcome::new(Err(error.into()), report);
        }
    };
    let upstream = match openai_images::parse_response(&body) {
        Ok(response) => response,
        Err(_) => return protocol_failure(report),
    };
    if upstream.validate_for_request(&request).is_err() {
        return protocol_failure(report);
    }
    let usage = match upstream.usage().map(normalize_usage).transpose() {
        Ok(usage) => usage,
        Err(()) => return protocol_failure(report),
    };
    let body = match encode_response(&upstream) {
        Ok(body) => body,
        Err(()) => return protocol_failure(report),
    };
    RelayOpenAiImagesOutcome::new(Ok(ImageResponse::new(body, usage)), report)
}

/// OpenAI Images 业务结果及其脱敏候选报告。
pub struct RelayOpenAiImagesOutcome {
    result: Result<ImageResponse, AfError>,
    report: RelayAttemptReport,
}

impl RelayOpenAiImagesOutcome {
    fn new(result: Result<ImageResponse, AfError>, report: RelayAttemptReport) -> Self {
        Self { result, report }
    }

    fn failed(error: AfError) -> Self {
        Self::new(Err(error), RelayAttemptReport::default())
    }

    /// 消费包装并返回业务结果。
    pub fn into_result(self) -> Result<ImageResponse, AfError> {
        self.result
    }

    /// 消费包装并拆分业务结果与候选报告。
    pub fn into_parts(self) -> (Result<ImageResponse, AfError>, RelayAttemptReport) {
        (self.result, self.report)
    }
}

impl fmt::Debug for RelayOpenAiImagesOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RelayOpenAiImagesOutcome")
            .field("result", &self.result)
            .field("report", &self.report)
            .finish()
    }
}

fn encode_request(request: &CanonicalImageGenerationRequest) -> Result<Bytes, AfError> {
    openai_images::build_request(request)
        .map_err(|_| AfError::Internal)
        .and_then(|value| {
            serde_json::to_vec(&value)
                .map(Bytes::from)
                .map_err(|_| AfError::Internal)
        })
}

fn encode_response(response: &af_protocol::CanonicalImageGenerationResponse) -> Result<Bytes, ()> {
    let value = openai_images::build_response(response).map_err(|_| ())?;
    serde_json::to_vec(&value).map(Bytes::from).map_err(|_| ())
}

fn normalize_usage(usage: ImageGenerationUsage) -> Result<Usage, ()> {
    Usage::new(
        usage.input_tokens(),
        usage.output_tokens(),
        UsageDetails::new(
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::ZERO,
        ),
        UsageSource::Upstream,
        UsageSemantics::Inclusive,
    )
    .map_err(|_| ())
}

fn protocol_failure(mut report: RelayAttemptReport) -> RelayOpenAiImagesOutcome {
    report.reject_success(UpstreamError::ProtocolError);
    RelayOpenAiImagesOutcome::new(Err(UpstreamError::ProtocolError.into()), report)
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
    use af_protocol::{ImageTokenBreakdown, TokenCount};

    use super::*;

    #[test]
    fn image_usage_maps_only_input_and_output_totals() {
        let usage = ImageGenerationUsage::new(
            TokenCount::new(7).unwrap(),
            ImageTokenBreakdown::new(TokenCount::new(7).unwrap(), TokenCount::ZERO),
            TokenCount::new(272).unwrap(),
            Some(ImageTokenBreakdown::new(
                TokenCount::ZERO,
                TokenCount::new(272).unwrap(),
            )),
        )
        .unwrap();
        let normalized = normalize_usage(usage).unwrap();

        assert_eq!(normalized.input_tokens().get(), 7);
        assert_eq!(normalized.output_tokens().get(), 272);
        assert_eq!(normalized.source(), UsageSource::Upstream);
        assert_eq!(normalized.semantics(), UsageSemantics::Inclusive);
        assert_eq!(normalized.details().reasoning(), TokenCount::ZERO);
        assert_eq!(normalized.details().audio_input(), TokenCount::ZERO);
        assert_eq!(normalized.details().audio_output(), TokenCount::ZERO);
    }
}
