//! Single-use, payload-bound authorization for changes to personal access tokens.

use std::time::{SystemTime, UNIX_EPOCH};

use af_admin::{
    PersonalAccessTokenProofSecret, PersonalAccessTokenStepUpEvidence, SessionAuthentication,
};
use af_domain::TrustedClientIp;
use af_telemetry::RequestId;
use axum::{
    Json,
    extract::{Extension, State, rejection::JsonRejection},
    response::Response,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    access_token_auth::InteractiveSessionEvidence,
    access_token_error::AccessTokenHttpError,
    access_tokens::{
        PersonalAccessTokenHttpState, PersonalAccessTokenWriteRequest, find_owned_token,
        mutation_context, update_proof_purpose, validate_write_with_overlay,
    },
    management_session::no_store_json,
};

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
enum VerificationAction {
    Create,
    Update,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct VerifyPersonalAccessTokenRequest {
    action: VerificationAction,
    token_id: Option<i64>,
    payload: PersonalAccessTokenWriteRequest,
}

#[derive(Serialize, ToSchema)]
pub(crate) struct PersonalAccessTokenProofResponse {
    #[schema(value_type = String)]
    proof: PersonalAccessTokenProofSecret,
    expires_at: i64,
}

pub(crate) async fn verify(
    State(state): State<PersonalAccessTokenHttpState>,
    Extension(authentication): Extension<SessionAuthentication>,
    Extension(evidence): Extension<InteractiveSessionEvidence>,
    Extension(ip): Extension<TrustedClientIp>,
    Extension(request_id): Extension<RequestId>,
    overlay: Option<Extension<crate::access_token_routes::AccessTokenRouteOverlay>>,
    body: Result<Json<VerifyPersonalAccessTokenRequest>, JsonRejection>,
) -> Result<Response, AccessTokenHttpError> {
    let request = body.map_err(|_| AccessTokenHttpError::InvalidRequest)?.0;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| AccessTokenHttpError::Unavailable)?
        .as_secs();
    require_recent_authentication(authentication, &evidence, now)?;
    if request.payload.proof.is_some() {
        return Err(AccessTokenHttpError::InvalidRequest);
    }
    validate_write_with_overlay(
        authentication,
        &request.payload,
        overlay.as_ref().map(|Extension(value)| value),
    )?;
    let service = state.service()?;
    let purpose = match request.action {
        VerificationAction::Create => {
            if request.token_id.is_some()
                || request.payload.version.is_some()
                || request.payload.enabled == Some(false)
            {
                return Err(AccessTokenHttpError::InvalidRequest);
            }
            "create"
        }
        VerificationAction::Update => {
            let id = request
                .token_id
                .filter(|id| *id > 0)
                .ok_or(AccessTokenHttpError::InvalidRequest)?;
            if request.payload.version.is_none_or(|version| version <= 0)
                || request.payload.enabled.is_none()
            {
                return Err(AccessTokenHttpError::InvalidRequest);
            }
            let old = find_owned_token(&*service, authentication.principal(), id).await?;
            if request.payload.version != Some(old.version) {
                return Err(AccessTokenHttpError::Conflict);
            }
            update_proof_purpose(&old, &request.payload).unwrap_or("enable")
        }
    };
    service.admit_request_ip(ip).await?;
    let issued = service
        .issue_step_up_proof(
            authentication.principal(),
            PersonalAccessTokenStepUpEvidence {
                session_version: evidence.session_version,
                session_binding: evidence.session_binding,
                issued_at: i64::try_from(evidence.issued_at)
                    .map_err(|_| AccessTokenHttpError::RecentAuthenticationRequired)?,
            },
            purpose.to_owned(),
            request.token_id,
            request.payload.payload_digest()?,
            mutation_context(
                &request_id,
                ip,
                "POST",
                "/api/account/access-tokens/verify",
                request.token_id,
            ),
        )
        .await?;
    Ok(no_store_json(PersonalAccessTokenProofResponse {
        proof: issued.secret,
        expires_at: issued.expires_at,
    }))
}

fn require_recent_authentication(
    authentication: SessionAuthentication,
    evidence: &InteractiveSessionEvidence,
    now: u64,
) -> Result<(), AccessTokenHttpError> {
    if evidence.user_id != authentication.principal().user_id()
        || evidence.session_version <= 0
        || evidence.issued_at == 0
        || evidence.issued_at > now
        || now - evidence.issued_at > 300
    {
        return Err(AccessTokenHttpError::RecentAuthenticationRequired);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use af_admin::{SessionPrincipal, SessionRole};
    use af_domain::{GroupId, UserId};

    #[test]
    fn access_token_proof_rejects_stale_future_and_unbound_sessions() {
        let owner = UserId::new(7).unwrap();
        let authentication = SessionAuthentication::new(
            SessionPrincipal::new(owner, SessionRole::User),
            GroupId::new(1).unwrap(),
            2_000,
        );
        let mut evidence = InteractiveSessionEvidence {
            user_id: owner,
            issued_at: 1_000,
            session_version: 1,
            session_binding: "a".repeat(64),
        };
        assert!(require_recent_authentication(authentication, &evidence, 1_300).is_ok());
        assert!(require_recent_authentication(authentication, &evidence, 1_301).is_err());
        assert!(require_recent_authentication(authentication, &evidence, 999).is_err());
        evidence.issued_at = 0;
        assert!(require_recent_authentication(authentication, &evidence, 100).is_err());
        evidence.issued_at = 1_000;
        evidence.user_id = UserId::new(8).unwrap();
        assert!(require_recent_authentication(authentication, &evidence, 1_001).is_err());
        evidence.user_id = owner;
        evidence.session_version = 0;
        assert!(require_recent_authentication(authentication, &evidence, 1_001).is_err());
    }
}
