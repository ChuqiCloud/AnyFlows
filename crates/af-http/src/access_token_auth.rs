//! Personal access tokens authenticate system API requests only.

use std::sync::Arc;

use af_admin::{
    PersonalAccessTokenAccessRecord, PersonalAccessTokenAuthentication, PersonalAccessTokenLookup,
    PersonalAccessTokenService, PresentedPersonalAccessToken, SessionAuthentication, SessionRole,
    SessionToken,
};
use af_domain::TrustedClientIp;
use af_telemetry::RequestId;
use axum::{
    extract::{MatchedPath, Request, State},
    middleware::Next,
    response::{IntoResponse, Response},
};
use http::{HeaderValue, StatusCode, header::CACHE_CONTROL};

use crate::{
    access_token_error::{AccessTokenHttpError, insufficient_scope},
    access_token_routes,
};

pub(crate) async fn resolve_management_client_ip(
    State(resolver): State<crate::client_ip::ClientIpResolver>,
    mut request: Request,
    next: Next,
) -> Response {
    if request.extensions().get::<TrustedClientIp>().is_none()
        && let Ok(client_ip) = resolver.resolve_context(&request)
    {
        request.extensions_mut().insert(client_ip);
    }
    next.run(request).await
}

pub(crate) async fn attach_access_token_route_overlay(
    State(overlay): State<crate::access_token_routes::AccessTokenRouteOverlay>,
    mut request: Request,
    next: Next,
) -> Response {
    request.extensions_mut().insert(overlay);
    next.run(request).await
}

/// The original JWT is never retained in request extensions.
#[derive(Clone, Debug)]
pub(crate) struct InteractiveSessionEvidence {
    pub user_id: af_domain::UserId,
    pub issued_at: u64,
    pub session_version: i64,
    pub session_binding: String,
}

pub(crate) async fn authenticate_personal_access_token(
    service: Arc<dyn PersonalAccessTokenService>,
    token: SessionToken,
    mut request: Request,
    next: Next,
) -> Response {
    if has_conflicting_carrier(&request) {
        return AccessTokenHttpError::InvalidRequest.into_response();
    }
    let Some(client_ip) = request.extensions().get::<TrustedClientIp>().copied() else {
        return AccessTokenHttpError::Unavailable.into_response();
    };
    if let Err(error) = service.admit_request_ip(client_ip).await {
        return AccessTokenHttpError::from(error).into_response();
    }
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map(MatchedPath::as_str)
        .unwrap_or("")
        .to_owned();
    let method = request.method().as_str().to_owned();
    let request_id = request
        .extensions()
        .get::<RequestId>()
        .map(|id| id.as_str().to_owned())
        .unwrap_or_default();
    let presented = match PresentedPersonalAccessToken::parse(token.as_str()) {
        Ok(presented) => presented,
        Err(_) => return AccessTokenHttpError::InvalidToken.into_response(),
    };
    drop(token);
    let digest = presented.digest();
    drop(presented);
    for header in [
        "x-forwarded-for",
        "forwarded",
        "x-real-ip",
        "cf-connecting-ip",
    ] {
        request.headers_mut().remove(header);
    }
    let authentication = match service.authenticate(&digest, client_ip).await {
        Ok(PersonalAccessTokenLookup::Authenticated(authentication)) => authentication,
        Ok(PersonalAccessTokenLookup::Rejected { audit, .. }) => {
            let error = if audit.as_ref().is_some_and(|identity| {
                identity.reason == af_admin::PersonalAccessTokenRejectionReason::PolicyDisabled
            }) {
                AccessTokenHttpError::Disabled
            } else {
                AccessTokenHttpError::InvalidToken
            };
            let response = error.into_response();
            if let Some(audit) = audit
                && service
                    .record_access(PersonalAccessTokenAccessRecord {
                        user_id: Some(audit.user_id),
                        token_id: Some(audit.token_id),
                        request_id,
                        method,
                        route,
                        required_scope: None,
                        resource_id: None,
                        status_code: response.status().as_u16(),
                        allowed: false,
                        client_ip,
                    })
                    .await
                    .is_err()
            {
                tracing::error!(
                    error_kind = "personal_access_token_denial_audit_unavailable",
                    "PAT denial audit could not be recorded"
                );
            }
            return response;
        }
        Err(error) => return AccessTokenHttpError::from(error).into_response(),
    };
    if let Err(error) = service.admit_authenticated_request(&authentication).await {
        return AccessTokenHttpError::from(error).into_response();
    }
    let overlay = request
        .extensions()
        .get::<access_token_routes::AccessTokenRouteOverlay>();
    let rule = access_token_routes::policy_with_overlay(&method, &route, overlay);
    let denial = match rule {
        Some(rule) if rule.session_only => {
            Some(AccessTokenHttpError::InteractiveSessionRequired.into_response())
        }
        Some(rule)
            if rule.admin_only && authentication.principal().role() != SessionRole::Admin =>
        {
            Some(AccessTokenHttpError::Forbidden.into_response())
        }
        Some(rule)
            if rule
                .required_scopes
                .iter()
                .all(|scope| authentication.scopes().contains(scope)) =>
        {
            None
        }
        Some(rule) => Some(insufficient_scope(&rule.required_scopes)),
        None => Some(AccessTokenHttpError::Forbidden.into_response()),
    };
    let required_scope = rule.map(|rule| rule.required_scopes.join(" "));
    if let Some(response) = denial {
        if record_access(
            &*service,
            &authentication,
            request_id,
            method,
            route,
            required_scope,
            response.status().as_u16(),
            false,
            client_ip,
        )
        .await
        .is_err()
        {
            tracing::error!(
                error_kind = "personal_access_token_denial_audit_unavailable",
                "PAT denial audit could not be recorded"
            );
        }
        return response;
    }
    // Persist authorization before executing any side effect. A second event records
    // the result; a failed final audit cannot safely turn a committed write into a retry.
    if record_access(
        &*service,
        &authentication,
        request_id.clone(),
        method.clone(),
        route.clone(),
        required_scope.clone(),
        StatusCode::PROCESSING.as_u16(),
        true,
        client_ip,
    )
    .await
    .is_err()
    {
        return AccessTokenHttpError::Unavailable.into_response();
    }
    let principal = authentication.principal();
    let expires_at = authentication
        .expires_at()
        .and_then(|expires| u64::try_from(expires).ok())
        .unwrap_or(u64::MAX);
    request.extensions_mut().insert(SessionAuthentication::new(
        principal,
        authentication.group_id(),
        expires_at,
    ));
    request.extensions_mut().insert(authentication.clone());
    let credential =
        match af_admin::PlatformAuditCredential::personal_access_token(authentication.token_id()) {
            Ok(credential) => credential,
            Err(_) => return AccessTokenHttpError::Unavailable.into_response(),
        };
    let mut response =
        af_admin::with_platform_audit_credential(credential, next.run(request)).await;
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    if record_access(
        &*service,
        &authentication,
        request_id,
        method,
        route,
        required_scope,
        response.status().as_u16(),
        true,
        client_ip,
    )
    .await
    .is_err()
    {
        tracing::error!(
            error_kind = "personal_access_token_result_audit_unavailable",
            "PAT result audit could not be recorded; authorization event remains durable"
        );
    }
    response
}

#[allow(
    clippy::too_many_arguments,
    reason = "Audit fields follow the stable access record contract"
)]
async fn record_access(
    service: &dyn PersonalAccessTokenService,
    authentication: &PersonalAccessTokenAuthentication,
    request_id: String,
    method: String,
    route: String,
    required_scope: Option<String>,
    status_code: u16,
    allowed: bool,
    client_ip: TrustedClientIp,
) -> Result<(), af_admin::PersonalAccessTokenError> {
    service
        .record_access(PersonalAccessTokenAccessRecord {
            user_id: Some(authentication.principal().user_id()),
            token_id: Some(authentication.token_id()),
            request_id,
            method,
            route,
            required_scope,
            resource_id: None,
            status_code,
            allowed,
            client_ip,
        })
        .await
}

fn has_conflicting_carrier(request: &Request) -> bool {
    if ["x-api-key", "x-goog-api-key", "new-api-user"]
        .iter()
        .any(|header| request.headers().contains_key(*header))
    {
        return true;
    }
    request.uri().query().is_some_and(|query| {
        url::form_urlencoded::parse(query.as_bytes()).any(|(key, value)| {
            matches!(
                key.to_ascii_lowercase().as_str(),
                "key" | "api_key" | "access_token" | "authorization"
            ) || value.starts_with("af_pat_")
        })
    })
}
