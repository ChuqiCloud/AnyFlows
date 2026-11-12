//! User-owned PAT control plane. These endpoints always require an interactive session.

use std::sync::Arc;

use af_admin::{
    CreatePersonalAccessTokenCommand, PersonalAccessToken, PersonalAccessTokenMutationContext,
    PersonalAccessTokenPolicy, PersonalAccessTokenProofClaim, PersonalAccessTokenService,
    SessionAuthentication, SessionAuthenticator, SessionRole, UpdatePersonalAccessTokenCommand,
};
use af_domain::{TrustedClientIp, UserId};
use af_telemetry::RequestId;
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Extension, Path, Query, State, rejection::JsonRejection},
    middleware,
    response::{IntoResponse, Response},
    routing::get,
};
use http::{HeaderValue, StatusCode, header::CACHE_CONTROL};
use serde::{Deserialize, Serialize};
use tower_http::limit::RequestBodyLimitLayer;
use utoipa::ToSchema;

use crate::{
    access_token_auth::InteractiveSessionEvidence,
    access_token_error::AccessTokenHttpError,
    access_token_routes::{self, AccessTokenPresetResponse, AccessTokenScopeResponse},
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_session::no_store_json,
};

#[derive(Clone)]
pub(crate) struct PersonalAccessTokenHttpState {
    pub authenticator: Arc<dyn SessionAuthenticator>,
}

impl PersonalAccessTokenHttpState {
    pub(crate) fn service(
        &self,
    ) -> Result<Arc<dyn PersonalAccessTokenService>, AccessTokenHttpError> {
        self.authenticator
            .personal_access_tokens()
            .ok_or(AccessTokenHttpError::Unavailable)
    }
}

#[derive(Deserialize, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PersonalAccessTokenWriteRequest {
    #[schema(min_length = 1, max_length = 128)]
    pub name: String,
    #[schema(min_items = 1, max_items = 1024)]
    pub scopes: Vec<String>,
    #[schema(required = true)]
    pub expires_at: Option<i64>,
    #[schema(required = true)]
    pub allow_ips: Option<Vec<String>>,
    pub enabled: Option<bool>,
    pub version: Option<i64>,
    #[schema(write_only)]
    pub proof: Option<String>,
}

impl PersonalAccessTokenWriteRequest {
    pub(crate) fn payload_digest(&self) -> Result<String, AccessTokenHttpError> {
        af_admin::personal_access_token_payload_digest(
            &self.name,
            &self.scopes,
            self.expires_at,
            self.allow_ips.as_deref(),
            self.enabled.unwrap_or(true),
            self.version,
        )
        .map_err(Into::into)
    }

    pub(crate) fn proof_claim(
        &self,
        evidence: &InteractiveSessionEvidence,
        purpose: &str,
        target_id: Option<i64>,
    ) -> Result<Option<PersonalAccessTokenProofClaim>, AccessTokenHttpError> {
        let Some(proof) = self.proof.as_deref() else {
            return Ok(None);
        };
        if proof.is_empty() || proof.len() > 128 {
            return Err(AccessTokenHttpError::InvalidRequest);
        }
        Ok(Some(PersonalAccessTokenProofClaim {
            proof_hash: af_admin::digest_personal_access_token_proof(proof),
            session_version: evidence.session_version,
            session_binding: evidence.session_binding.clone(),
            purpose: purpose.to_owned(),
            target_id,
            payload_digest: self.payload_digest()?,
        }))
    }
}

#[derive(Serialize, ToSchema)]
pub(crate) struct PersonalAccessTokenResponse {
    id: i64,
    name: String,
    token_hint: String,
    scopes: Vec<String>,
    enabled: bool,
    expires_at: Option<i64>,
    allow_ips: Option<Vec<String>>,
    created_at: i64,
    updated_at: i64,
    last_used_at: Option<i64>,
    last_used_ip: Option<String>,
    version: i64,
}

impl From<PersonalAccessToken> for PersonalAccessTokenResponse {
    fn from(token: PersonalAccessToken) -> Self {
        Self {
            id: token.id,
            name: token.name,
            token_hint: token.token_hint,
            scopes: token.scopes,
            enabled: token.enabled,
            expires_at: token.expires_at,
            allow_ips: token.allow_ips,
            created_at: token.created_at,
            updated_at: token.updated_at,
            last_used_at: token.last_used_at,
            last_used_ip: token.last_used_ip,
            version: token.version,
        }
    }
}

#[derive(Serialize, ToSchema)]
pub(crate) struct PersonalAccessTokenListResponse {
    tokens: Vec<PersonalAccessTokenResponse>,
    next_cursor: Option<i64>,
    capacity: usize,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct IssuedPersonalAccessTokenResponse<'a> {
    token: PersonalAccessTokenResponse,
    /// Returned only by the successful issuance response.
    secret: &'a str,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct PersonalAccessTokenScopeCatalogResponse {
    scopes: Vec<AccessTokenScopeResponse>,
    presets: Vec<AccessTokenPresetResponse>,
    default_expiry_days: u32,
    max_expiry_days: u32,
    allow_permanent: bool,
}

#[derive(Deserialize, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct PersonalAccessTokenPolicyRequest {
    enabled: bool,
    issuance_enabled: bool,
    max_tokens_per_user: usize,
    version: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PageQuery {
    after: Option<i64>,
    limit: Option<usize>,
}

pub(crate) fn build_access_token_router(authenticator: Arc<dyn SessionAuthenticator>) -> Router {
    let authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(Arc::clone(&authenticator)),
        authenticate_management_session,
    );
    Router::new()
        .route(
            "/api/account/access-tokens",
            get(list_tokens).post(create_token),
        )
        .route("/api/account/access-tokens/scopes", get(scope_catalog))
        .route(
            "/api/account/access-tokens/verify",
            axum::routing::post(crate::access_token_proof::verify),
        )
        .route(
            "/api/account/access-tokens/{id}",
            axum::routing::patch(update_token).delete(revoke_token),
        )
        .route(
            "/api/account/access-tokens/revoke-all",
            axum::routing::post(revoke_own_tokens),
        )
        .route("/api/access-token/self", get(get_token_self))
        .route(
            "/api/admin/access-tokens/policy",
            get(get_policy).put(update_policy),
        )
        .route(
            "/api/admin/users/{user_id}/access-tokens/revoke-all",
            axum::routing::post(revoke_user_tokens),
        )
        .layer(DefaultBodyLimit::max(192 * 1024))
        .layer(RequestBodyLimitLayer::new(192 * 1024))
        .layer(authentication)
        .layer(middleware::from_fn(crate::middleware::ensure_request_id))
        .with_state(PersonalAccessTokenHttpState { authenticator })
}

async fn list_tokens(
    State(state): State<PersonalAccessTokenHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Query(query): Query<PageQuery>,
) -> Result<Response, AccessTokenHttpError> {
    let page = state
        .service()?
        .list(
            authentication.principal(),
            query.after,
            query.limit.unwrap_or(50),
        )
        .await?;
    Ok(no_store_json(PersonalAccessTokenListResponse {
        tokens: page.tokens.into_iter().map(Into::into).collect(),
        next_cursor: page.next_cursor,
        capacity: page.capacity,
    }))
}

async fn scope_catalog(
    Extension(authentication): Extension<SessionAuthentication>,
    overlay: Option<Extension<crate::access_token_routes::AccessTokenRouteOverlay>>,
) -> Response {
    let overlay = overlay.as_ref().map(|Extension(overlay)| overlay);
    no_store_json(PersonalAccessTokenScopeCatalogResponse {
        scopes: access_token_routes::scope_catalog_with_overlay(
            authentication.principal().role(),
            overlay,
        ),
        presets: access_token_routes::scope_presets_with_overlay(
            authentication.principal().role(),
            overlay,
        ),
        default_expiry_days: 30,
        max_expiry_days: 365,
        allow_permanent: true,
    })
}

async fn create_token(
    State(state): State<PersonalAccessTokenHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(evidence): Extension<InteractiveSessionEvidence>,
    overlay: Option<Extension<crate::access_token_routes::AccessTokenRouteOverlay>>,
    Extension(ip): Extension<TrustedClientIp>,
    Extension(id): Extension<RequestId>,
    body: Result<Json<PersonalAccessTokenWriteRequest>, JsonRejection>,
) -> Result<Response, AccessTokenHttpError> {
    let request = body.map_err(|_| AccessTokenHttpError::InvalidRequest)?.0;
    validate_write_with_overlay(
        authentication,
        &request,
        overlay.as_ref().map(|Extension(value)| value),
    )?;
    if request.version.is_some() || request.enabled.is_some_and(|enabled| !enabled) {
        return Err(AccessTokenHttpError::InvalidRequest);
    }
    let proof = request
        .proof_claim(&evidence, "create", None)?
        .ok_or(AccessTokenHttpError::ProofRequired)?;
    let issued = state
        .service()?
        .create(
            authentication.principal(),
            CreatePersonalAccessTokenCommand {
                name: request.name,
                scopes: request.scopes,
                expires_at: request.expires_at,
                allow_ips: request.allow_ips,
                proof,
            },
            mutation_context(&id, ip, "POST", "/api/account/access-tokens", None),
        )
        .await?;
    let mut response = no_store_json(IssuedPersonalAccessTokenResponse {
        secret: issued.secret.expose_secret(),
        token: issued.token.into(),
    });
    *response.status_mut() = StatusCode::CREATED;
    Ok(response)
}

async fn update_token(
    State(state): State<PersonalAccessTokenHttpState>,
    Path(token_id): Path<i64>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(evidence): Extension<InteractiveSessionEvidence>,
    overlay: Option<Extension<crate::access_token_routes::AccessTokenRouteOverlay>>,
    Extension(ip): Extension<TrustedClientIp>,
    Extension(id): Extension<RequestId>,
    body: Result<Json<PersonalAccessTokenWriteRequest>, JsonRejection>,
) -> Result<Response, AccessTokenHttpError> {
    let request = body.map_err(|_| AccessTokenHttpError::InvalidRequest)?.0;
    validate_write_with_overlay(
        authentication,
        &request,
        overlay.as_ref().map(|Extension(value)| value),
    )?;
    let service = state.service()?;
    let old = find_owned_token(&*service, authentication.principal(), token_id).await?;
    let proof = match update_proof_purpose(&old, &request) {
        Some(purpose) => Some(
            request
                .proof_claim(&evidence, purpose, Some(token_id))?
                .ok_or(AccessTokenHttpError::ProofRequired)?,
        ),
        None => None,
    };
    let token = service
        .update(
            authentication.principal(),
            token_id,
            UpdatePersonalAccessTokenCommand {
                name: request.name,
                scopes: request.scopes,
                expires_at: request.expires_at,
                allow_ips: request.allow_ips,
                enabled: request
                    .enabled
                    .ok_or(AccessTokenHttpError::InvalidRequest)?,
                version: request
                    .version
                    .ok_or(AccessTokenHttpError::InvalidRequest)?,
                proof,
            },
            mutation_context(
                &id,
                ip,
                "PATCH",
                "/api/account/access-tokens/{id}",
                Some(token_id),
            ),
        )
        .await?;
    Ok(no_store_json(PersonalAccessTokenResponse::from(token)))
}

pub(crate) fn validate_write(
    authentication: SessionAuthentication,
    request: &PersonalAccessTokenWriteRequest,
) -> Result<(), AccessTokenHttpError> {
    validate_write_with_overlay(authentication, request, None)
}

pub(crate) fn validate_write_with_overlay(
    authentication: SessionAuthentication,
    request: &PersonalAccessTokenWriteRequest,
    overlay: Option<&crate::access_token_routes::AccessTokenRouteOverlay>,
) -> Result<(), AccessTokenHttpError> {
    if !access_token_routes::validate_scopes_with_overlay(
        authentication.principal().role(),
        &request.scopes,
        overlay,
    ) {
        return Err(AccessTokenHttpError::Forbidden);
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| AccessTokenHttpError::Unavailable)?
        .as_secs();
    let now = i64::try_from(now).map_err(|_| AccessTokenHttpError::Unavailable)?;
    if request.name.is_empty()
        || request.name.len() > 128
        || request.name.trim() != request.name
        || request.name.chars().any(char::is_control)
        || request.version.is_some_and(|version| version <= 0)
        || request.expires_at.is_some_and(|expiry| {
            expiry < 0
                || expiry > now.saturating_add(366 * 86_400)
                || (request.version.is_none() && expiry <= now)
        })
        || request.allow_ips.as_ref().is_some_and(|ips| {
            ips.is_empty()
                || ips.len() > 64
                || ips
                    .iter()
                    .any(|ip| ip.parse::<af_domain::IpCidr>().is_err())
        })
        || serde_json::to_vec(&request.scopes)
            .map_err(|_| AccessTokenHttpError::InvalidRequest)?
            .len()
            > 128 * 1024
    {
        return Err(AccessTokenHttpError::InvalidRequest);
    }
    request.payload_digest().map(|_| ())
}

pub(crate) async fn find_owned_token(
    service: &dyn PersonalAccessTokenService,
    principal: af_admin::SessionPrincipal,
    token_id: i64,
) -> Result<PersonalAccessToken, AccessTokenHttpError> {
    let mut after = None;
    loop {
        let page = service.list(principal, after, 100).await?;
        if let Some(token) = page.tokens.into_iter().find(|token| token.id == token_id) {
            return Ok(token);
        }
        let Some(cursor) = page.next_cursor else {
            return Err(AccessTokenHttpError::NotFound);
        };
        if after.is_some_and(|previous| previous <= cursor) {
            return Err(AccessTokenHttpError::Unavailable);
        }
        after = Some(cursor);
    }
}

pub(crate) fn update_proof_purpose(
    old: &PersonalAccessToken,
    request: &PersonalAccessTokenWriteRequest,
) -> Option<&'static str> {
    let ip_expansion = match (old.allow_ips.as_deref(), request.allow_ips.as_deref()) {
        (Some(_), None) => true,
        (Some(old), Some(new)) => new.iter().any(|ip| !old.contains(ip)),
        _ => false,
    };
    if request
        .scopes
        .iter()
        .any(|scope| !old.scopes.contains(scope))
        || ip_expansion
    {
        Some("scope_expand")
    } else if matches!((old.expires_at, request.expires_at), (Some(_), None))
        || matches!((old.expires_at, request.expires_at), (Some(before), Some(after)) if after > before)
    {
        Some("extend")
    } else if !old.enabled && request.enabled == Some(true) {
        Some("enable")
    } else {
        None
    }
}

async fn get_token_self(
    authentication: Option<Extension<af_admin::PersonalAccessTokenAuthentication>>,
) -> Result<Response, AccessTokenHttpError> {
    let Some(Extension(authentication)) = authentication else {
        return Err(AccessTokenHttpError::InvalidToken);
    };
    Ok(no_store_json(
        serde_json::json!({"id":authentication.token_id(),
        "user_id":authentication.principal().user_id().get(),"scopes":authentication.scopes(),
        "expires_at":authentication.expires_at()}),
    ))
}

async fn revoke_token(
    State(state): State<PersonalAccessTokenHttpState>,
    Path(token_id): Path<i64>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(ip): Extension<TrustedClientIp>,
    Extension(id): Extension<RequestId>,
) -> Result<Response, AccessTokenHttpError> {
    state
        .service()?
        .revoke(
            authentication.principal(),
            token_id,
            mutation_context(
                &id,
                ip,
                "DELETE",
                "/api/account/access-tokens/{id}",
                Some(token_id),
            ),
        )
        .await?;
    Ok(no_store_empty())
}

async fn revoke_own_tokens(
    State(state): State<PersonalAccessTokenHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(ip): Extension<TrustedClientIp>,
    Extension(id): Extension<RequestId>,
) -> Result<Response, AccessTokenHttpError> {
    let revoked = state
        .service()?
        .revoke_all(
            authentication.principal(),
            authentication.principal().user_id(),
            mutation_context(
                &id,
                ip,
                "POST",
                "/api/account/access-tokens/revoke-all",
                None,
            ),
        )
        .await?;
    Ok(no_store_json(serde_json::json!({"revoked":revoked})))
}

async fn revoke_user_tokens(
    State(state): State<PersonalAccessTokenHttpState>,
    Path(user_id): Path<i64>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(ip): Extension<TrustedClientIp>,
    Extension(id): Extension<RequestId>,
) -> Result<Response, AccessTokenHttpError> {
    require_admin(authentication)?;
    let target = UserId::new(user_id).map_err(|_| AccessTokenHttpError::InvalidRequest)?;
    let revoked = state
        .service()?
        .revoke_all(
            authentication.principal(),
            target,
            mutation_context(
                &id,
                ip,
                "POST",
                "/api/admin/users/{user_id}/access-tokens/revoke-all",
                Some(user_id),
            ),
        )
        .await?;
    Ok(no_store_json(serde_json::json!({"revoked":revoked})))
}

async fn get_policy(
    State(state): State<PersonalAccessTokenHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
) -> Result<Response, AccessTokenHttpError> {
    require_admin(authentication)?;
    Ok(no_store_json(
        state.service()?.policy(authentication.principal()).await?,
    ))
}

async fn update_policy(
    State(state): State<PersonalAccessTokenHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(ip): Extension<TrustedClientIp>,
    Extension(id): Extension<RequestId>,
    body: Result<Json<PersonalAccessTokenPolicyRequest>, JsonRejection>,
) -> Result<Response, AccessTokenHttpError> {
    require_admin(authentication)?;
    let request = body.map_err(|_| AccessTokenHttpError::InvalidRequest)?.0;
    let policy = state
        .service()?
        .update_policy(
            authentication.principal(),
            PersonalAccessTokenPolicy {
                enabled: request.enabled,
                issuance_enabled: request.issuance_enabled,
                max_tokens_per_user: request.max_tokens_per_user,
                version: request.version,
            },
            mutation_context(&id, ip, "PUT", "/api/admin/access-tokens/policy", None),
        )
        .await?;
    Ok(no_store_json(policy))
}

fn require_admin(authentication: SessionAuthentication) -> Result<(), AccessTokenHttpError> {
    if authentication.principal().role() == SessionRole::Admin {
        Ok(())
    } else {
        Err(AccessTokenHttpError::Forbidden)
    }
}

pub(crate) fn mutation_context(
    id: &RequestId,
    client_ip: TrustedClientIp,
    method: &str,
    route: &str,
    resource_id: Option<i64>,
) -> PersonalAccessTokenMutationContext {
    PersonalAccessTokenMutationContext {
        request_id: id.as_str().to_owned(),
        method: method.to_owned(),
        route: route.to_owned(),
        resource_id: resource_id.map(|id| id.to_string()),
        client_ip,
    }
}

fn no_store_empty() -> Response {
    let mut response = StatusCode::NO_CONTENT.into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
