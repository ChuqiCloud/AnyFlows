use af_admin::TokenAuthentication;
use af_domain::{AfError, Protocol};
use af_protocol::gemini::{parse_request_envelope, parse_stream_request_envelope};
use af_relay::RelayDiagnosticInput;
use af_telemetry::RequestId;
use axum::{
    body::Bytes,
    extract::{Extension, Path, RawQuery, State},
    response::Response,
};
use http::HeaderMap;
use url::form_urlencoded;

use crate::{
    chat_completions::{HttpState, prepare_canonical_request, render_chat_response},
    error_response::GeminiHttpError,
};

const GENERATE_CONTENT_ACTION: &str = ":generateContent";
const STREAM_GENERATE_CONTENT_ACTION: &str = ":streamGenerateContent";

/// 解析 Gemini 模型动作并经 Canonical Chat 执行边界转发。
pub(crate) async fn gemini_generate_content(
    State(state): State<HttpState>,
    Path(model_action): Path<String>,
    RawQuery(query): RawQuery,
    Extension(request_id): Extension<RequestId>,
    Extension(authentication): Extension<TokenAuthentication>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, GeminiHttpError> {
    let diagnostic_path = query.as_deref().map_or_else(
        || format!("/v1beta/models/{model_action}"),
        |query| format!("/v1beta/models/{model_action}?{query}"),
    );
    let diagnostic = RelayDiagnosticInput::capture("POST", &diagnostic_path, &headers, &body);
    let target = parse_target(&model_action, query.as_deref())
        .map_err(|_| GeminiHttpError::from(AfError::InvalidRequest))?;
    let model_resource = format!("models/{}", target.model);
    let request = if target.stream {
        parse_stream_request_envelope(&model_resource, body)
    } else {
        parse_request_envelope(&model_resource, body)
    }
    .map_err(|_| GeminiHttpError::from(AfError::InvalidRequest))?;
    let request = prepare_canonical_request(request, authentication.model_policy())
        .map_err(GeminiHttpError::from)?;
    let principal = authentication.principal();
    let response = state
        .chat_service()
        .chat_completions(
            &principal,
            authentication.user_concurrency(),
            request,
            Protocol::Gemini,
            request_id.as_str(),
            diagnostic,
        )
        .await
        .map_err(GeminiHttpError::from)?;
    Ok(render_chat_response(response, state.stream_delivery()))
}

struct GeminiTarget<'a> {
    model: &'a str,
    stream: bool,
}

fn parse_target<'a>(model_action: &'a str, query: Option<&str>) -> Result<GeminiTarget<'a>, ()> {
    let (model, stream_action) =
        if let Some(model) = model_action.strip_suffix(STREAM_GENERATE_CONTENT_ACTION) {
            (model, true)
        } else if let Some(model) = model_action.strip_suffix(GENERATE_CONTENT_ACTION) {
            (model, false)
        } else {
            return Err(());
        };
    if model.is_empty() {
        return Err(());
    }

    let alt = parse_alt(query)?;
    if stream_action && alt == Some(ResponseAlt::Json) {
        // 当前入口只实现官方 SSE wire，不把流悄悄聚合成 JSON 数组。
        return Err(());
    }
    Ok(GeminiTarget {
        model,
        stream: stream_action || alt == Some(ResponseAlt::Sse),
    })
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum ResponseAlt {
    Json,
    Sse,
}

fn parse_alt(query: Option<&str>) -> Result<Option<ResponseAlt>, ()> {
    let Some(query) = query else {
        return Ok(None);
    };
    let mut alt = None;
    for (name, value) in form_urlencoded::parse(query.as_bytes()) {
        if name != "alt" || alt.is_some() {
            return Err(());
        }
        alt = Some(match value.as_ref() {
            "json" => ResponseAlt::Json,
            "sse" => ResponseAlt::Sse,
            _ => return Err(()),
        });
    }
    Ok(alt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_action_and_alt_choose_a_closed_response_mode() {
        for (path, query, expected_model, expected_stream) in [
            (
                "gemini-2.5-pro:generateContent",
                None,
                "gemini-2.5-pro",
                false,
            ),
            (
                "gemini-2.5-pro:generateContent",
                Some("alt=sse"),
                "gemini-2.5-pro",
                true,
            ),
            (
                "gemini-2.5-pro:streamGenerateContent",
                None,
                "gemini-2.5-pro",
                true,
            ),
            (
                "gemini-2.5-pro:streamGenerateContent",
                Some("alt=sse"),
                "gemini-2.5-pro",
                true,
            ),
        ] {
            let target = parse_target(path, query).unwrap();
            assert_eq!(target.model, expected_model);
            assert_eq!(target.stream, expected_stream);
        }
    }

    #[test]
    fn unknown_actions_and_unimplemented_query_semantics_fail_closed() {
        for (path, query) in [
            ("gemini-2.5-pro:GenerateContent", None),
            ("gemini-2.5-pro:embedContent", None),
            ("gemini-2.5-pro:generateContent", Some("alt=proto")),
            ("gemini-2.5-pro:streamGenerateContent", Some("alt=json")),
            ("gemini-2.5-pro:generateContent", Some("alt=sse&alt=sse")),
            ("gemini-2.5-pro:generateContent", Some("fields=private")),
        ] {
            assert!(parse_target(path, query).is_err());
        }
    }
}
