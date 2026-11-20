//! Model API projections for tasks explicitly bound to a standard protocol.

use std::{sync::Arc, time::Duration};

use af_admin::{
    AdminPluginLifecycleError, DatabaseAdminPluginLifecycleService, GatewayModelAuthentication,
    PluginNativeTaskProtocol, TokenAuthentication,
};
use af_domain::{AfError, UpstreamError};
use af_telemetry::RequestId;
use axum::{
    Extension, Json,
    body::Bytes,
    extract::Path,
    response::{IntoResponse, Response},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use http::{HeaderMap, StatusCode, header::CACHE_CONTROL};
use serde_json::{Value, json};

use crate::{OpenAiHttpError, error_response::OpenAiResponsesHttpError};

pub(crate) type NativeTaskService = Arc<DatabaseAdminPluginLifecycleService>;
const VIDEO_PREFIX: &str = "video_plugin_";
const RESPONSE_PREFIX: &str = "resp_plugin_";
const SYNC_WAIT: Duration = Duration::from_secs(30);

fn project_error(error: AdminPluginLifecycleError) -> AfError {
    match error {
        AdminPluginLifecycleError::Forbidden => AfError::ModelNotAllowed,
        AdminPluginLifecycleError::InvalidInput | AdminPluginLifecycleError::PayloadTooLarge => {
            AfError::InvalidRequest
        }
        AdminPluginLifecycleError::NotFound => AfError::TaskNotFound,
        AdminPluginLifecycleError::Conflict => AfError::IdempotencyConflict,
        _ => AfError::Internal,
    }
}

fn no_store(body: Value) -> Response {
    ([(CACHE_CONTROL, "no-store")], Json(body)).into_response()
}

pub(crate) async fn submit(
    service: &NativeTaskService,
    auth: &TokenAuthentication,
    protocol: PluginNativeTaskProtocol,
    headers: &HeaderMap,
    body: &[u8],
    request_id: &RequestId,
) -> Result<Option<Response>, AfError> {
    let Ok(request) = serde_json::from_slice::<Value>(body) else {
        return Ok(None);
    };
    let Some(model) = request["model"].as_str().map(str::to_owned) else {
        return Ok(None);
    };
    let mut keys = headers.get_all("idempotency-key").iter();
    let key = match keys.next() {
        Some(value) if keys.next().is_none() => value.to_str().ok(),
        Some(_) => None,
        None => Some(request_id.as_str()),
    };
    let background = request["background"].as_bool() == Some(true);
    let b64 = request["response_format"].as_str() == Some("b64_json");
    let Some(task) = service
        .submit_native_plugin_task(
            auth,
            protocol,
            &model,
            || validate_request(protocol, body, request),
            key.unwrap_or_default(),
            request_id.as_str(),
        )
        .await
        .map_err(project_error)?
    else {
        return Ok(None);
    };
    let id = task["id"].as_str().ok_or(AfError::Internal)?;
    let response = match protocol {
        PluginNativeTaskProtocol::Video => no_store(video_view(&task, id)),
        PluginNativeTaskProtocol::Responses if background => no_store(responses_view(&task, id)?),
        PluginNativeTaskProtocol::Responses | PluginNativeTaskProtocol::Image => {
            wait_for_result(service, auth, protocol, id, task.clone(), b64).await?
        }
    };
    Ok(Some(response))
}

fn validate_request(
    protocol: PluginNativeTaskProtocol,
    body: &[u8],
    request: Value,
) -> Result<Value, AdminPluginLifecycleError> {
    let invalid = || AdminPluginLifecycleError::InvalidInput;
    if body.len() > 256 * 1024 {
        return Err(AdminPluginLifecycleError::PayloadTooLarge);
    }
    match protocol {
        PluginNativeTaskProtocol::Responses => {
            af_protocol::openai_responses::parse_plugin_task_request(body).map_err(|_| invalid())
        }
        PluginNativeTaskProtocol::Image => {
            let image = af_protocol::openai_images::parse_request(body).map_err(|_| invalid())?;
            af_protocol::openai_images::build_request(&image).map_err(|_| invalid())
        }
        PluginNativeTaskProtocol::Video => {
            let prompt = request["prompt"].as_str().ok_or_else(invalid)?;
            if prompt.trim().is_empty() || prompt.len() > 32_768 {
                return Err(invalid());
            }
            Ok(request)
        }
    }
}

async fn wait_for_result(
    service: &NativeTaskService,
    auth: &TokenAuthentication,
    protocol: PluginNativeTaskProtocol,
    id: &str,
    mut task: Value,
    b64: bool,
) -> Result<Response, AfError> {
    let deadline = tokio::time::Instant::now() + SYNC_WAIT;
    loop {
        match task["state"].as_str() {
            Some("succeeded") if protocol == PluginNativeTaskProtocol::Image => {
                return image_view(service, auth, &task, id, b64).await;
            }
            Some("failed" | "cancelled" | "succeeded" | "unknown")
                if protocol == PluginNativeTaskProtocol::Responses =>
            {
                return Ok(no_store(responses_view(&task, id)?));
            }
            Some("failed" | "cancelled") => return Err(UpstreamError::ProtocolError.into()),
            Some("unknown") => return Err(AfError::RequestOutcomeUnknown),
            _ => {}
        }
        let next = async {
            tokio::time::sleep(Duration::from_secs(1)).await;
            service.get_native_plugin_task(auth, protocol, id).await
        };
        match tokio::time::timeout_at(deadline, next).await {
            Ok(result) => task = result.map_err(project_error)?,
            Err(_) if protocol == PluginNativeTaskProtocol::Responses => {
                return Ok(no_store(responses_view(&task, id)?));
            }
            Err(_) => {
                let mut response =
                    OpenAiHttpError::from(AfError::RequestOutcomeUnknown).into_response();
                if let Ok(value) = http::HeaderValue::from_str(id) {
                    response
                        .headers_mut()
                        .insert("x-anyflows-plugin-task-id", value);
                }
                return Ok(response);
            }
        }
    }
}

fn responses_view(task: &Value, id: &str) -> Result<Value, AfError> {
    let status = match task["state"].as_str() {
        Some("succeeded") => "completed",
        Some("failed" | "unknown") => "failed",
        Some("cancelled") => "cancelled",
        Some("running" | "polling" | "settling") => "in_progress",
        _ => "queued",
    };
    let output = if status == "completed" {
        task["output"]["output"]
            .as_array()
            .ok_or(UpstreamError::ProtocolError)?
            .clone()
    } else {
        Vec::new()
    };
    Ok(json!({
        "id": format!("{RESPONSE_PREFIX}{id}"), "object": "response",
        "model": task["model"], "status": status, "created_at": task["createdAt"],
        "output": output,
        "error": if status == "failed" {
            json!({"code": "plugin_task_failed", "message": "The upstream task did not complete."})
        } else { Value::Null }
    }))
}

fn video_view(task: &Value, id: &str) -> Value {
    let status = match task["state"].as_str() {
        Some("succeeded") => "completed",
        Some("failed" | "unknown" | "cancelled") => "failed",
        Some("running" | "polling" | "settling") => "in_progress",
        _ => "queued",
    };
    json!({
        "id": format!("{VIDEO_PREFIX}{id}"), "object": "video",
        "model": task["model"], "status": status, "created_at": task["createdAt"],
        "progress": if status == "completed" { 100 } else {
            task["progressBasisPoints"].as_u64().unwrap_or(0) / 100
        },
        "error": if status == "failed" {
            json!({"code": "plugin_task_failed", "message": "The upstream task did not complete."})
        } else { Value::Null }
    })
}

fn published_https_url(value: &Value) -> Result<&str, AfError> {
    let raw = value.as_str().ok_or(UpstreamError::ProtocolError)?;
    let valid = raw.len() <= 8192
        && url::Url::parse(raw).is_ok_and(|url| {
            url.scheme() == "https"
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
        });
    if valid {
        Ok(raw)
    } else {
        Err(UpstreamError::ProtocolError.into())
    }
}

async fn image_view(
    service: &NativeTaskService,
    auth: &TokenAuthentication,
    task: &Value,
    id: &str,
    b64: bool,
) -> Result<Response, AfError> {
    let mut data = Vec::new();
    if b64 {
        let files = task["files"]
            .as_array()
            .ok_or(UpstreamError::ProtocolError)?;
        for (index, file) in files.iter().enumerate() {
            if matches!(
                file["mediaType"].as_str(),
                Some("image/png" | "image/jpeg" | "image/webp")
            ) {
                let bytes = service
                    .get_model_plugin_task_file(auth, id, index)
                    .await
                    .map_err(project_error)?;
                data.push(json!({"b64_json": STANDARD.encode(bytes)}));
            }
        }
    } else {
        let items = task["output"]["data"]
            .as_array()
            .ok_or(UpstreamError::ProtocolError)?;
        for item in items {
            data.push(json!({"url": published_https_url(&item["url"])?}));
        }
    }
    if data.is_empty() {
        return Err(UpstreamError::ProtocolError.into());
    }
    Ok(no_store(
        json!({"created": task["createdAt"], "data": data}),
    ))
}

pub(crate) fn plugin_video_id(value: &str) -> Option<&str> {
    value.strip_prefix(VIDEO_PREFIX)
}

pub(crate) async fn poll_video(
    service: Option<Extension<NativeTaskService>>,
    auth: &TokenAuthentication,
    id: &str,
) -> Result<Response, OpenAiHttpError> {
    let service = service.ok_or(AfError::TaskNotFound)?.0;
    let task = service
        .get_native_plugin_task(auth, PluginNativeTaskProtocol::Video, id)
        .await
        .map_err(project_error)?;
    Ok(no_store(video_view(&task, id)))
}

pub(crate) async fn post_video(
    service: Option<Extension<NativeTaskService>>,
    Extension(auth): Extension<TokenAuthentication>,
    Extension(request_id): Extension<RequestId>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, OpenAiHttpError> {
    let service = service.ok_or(AfError::ModelUnavailable)?.0;
    submit(
        &service,
        &auth,
        PluginNativeTaskProtocol::Video,
        &headers,
        &body,
        &request_id,
    )
    .await?
    .ok_or_else(|| AfError::ModelUnavailable.into())
}

pub(crate) async fn video_content(
    service: Option<Extension<NativeTaskService>>,
    Extension(auth): Extension<TokenAuthentication>,
    Path(video_id): Path<String>,
) -> Result<Response, OpenAiHttpError> {
    let id = plugin_video_id(&video_id).ok_or(AfError::TaskNotFound)?;
    let service = service.ok_or(AfError::TaskNotFound)?.0;
    let task = service
        .get_native_plugin_task(&auth, PluginNativeTaskProtocol::Video, id)
        .await
        .map_err(project_error)?;
    if task["state"].as_str() != Some("succeeded") {
        return Err(AfError::TaskNotFound.into());
    }
    let index = task["files"].as_array().and_then(|files| {
        files
            .iter()
            .position(|file| file["mediaType"].as_str() == Some("video/mp4"))
    });
    let Some(index) = index else {
        if task["output"]["url"].is_null() {
            return Err(AfError::TaskNotFound.into());
        }
        let url = published_https_url(&task["output"]["url"])?;
        let location = http::HeaderValue::from_str(url).map_err(|_| AfError::Internal)?;
        return Ok((
            StatusCode::SEE_OTHER,
            [(http::header::LOCATION, location)],
            [(CACHE_CONTROL, "no-store")],
        )
            .into_response());
    };
    let bytes = service
        .get_model_plugin_task_file(&auth, id, index)
        .await
        .map_err(project_error)?;
    Ok((
        StatusCode::OK,
        [
            (CACHE_CONTROL, "no-store"),
            (http::header::CONTENT_TYPE, "video/mp4"),
            (http::header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        bytes,
    )
        .into_response())
}

pub(crate) async fn get_responses(
    service: Option<Extension<NativeTaskService>>,
    Extension(auth): Extension<GatewayModelAuthentication>,
    Path(response_id): Path<String>,
) -> Result<Response, OpenAiResponsesHttpError> {
    let GatewayModelAuthentication::User(auth) = auth else {
        return Err(AfError::TaskNotFound.into());
    };
    let id = response_id
        .strip_prefix(RESPONSE_PREFIX)
        .ok_or(AfError::TaskNotFound)?;
    let service = service.ok_or(AfError::TaskNotFound)?.0;
    let task = service
        .get_native_plugin_task(&auth, PluginNativeTaskProtocol::Responses, id)
        .await
        .map_err(project_error)?;
    Ok(no_store(responses_view(&task, id)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_native_tasks_use_host_ids_and_statuses() {
        let task = json!({
            "state":"succeeded","model":"vendor.test","createdAt":42,
            "output":{"id":"external-id","status":"failed","output":[]}
        });
        let response = responses_view(&task, "host").unwrap();
        assert_eq!(response["id"], "resp_plugin_host");
        assert_eq!(response["status"], "completed");
        assert_eq!(video_view(&task, "host")["progress"], 100);
        assert_eq!(plugin_video_id("video_plugin_host"), Some("host"));
        assert_eq!(plugin_video_id("9e0beda1210f47eba37e909560551fcb"), None);
    }

    #[test]
    fn plugin_native_tasks_validate_responses_background_and_image_input() {
        let valid = br#"{"model":"example","input":"hello","background":true}"#;
        let invalid =
            br#"{"model":"example","input":"hello","background":true,"background":false}"#;
        assert!(validate_request(PluginNativeTaskProtocol::Responses, valid, json!(null)).is_ok());
        assert!(
            validate_request(PluginNativeTaskProtocol::Responses, invalid, json!(null)).is_err()
        );
        assert!(
            validate_request(
                PluginNativeTaskProtocol::Image,
                br#"{"model":"example"}"#,
                json!(null)
            )
            .is_err()
        );
        assert!(published_https_url(&json!("https://cdn.example/result")).is_ok());
        assert!(published_https_url(&json!("https://user:password@cdn.example/result")).is_err());
    }
}
