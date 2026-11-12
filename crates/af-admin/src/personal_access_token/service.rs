use std::fmt;

use af_db::{
    PersonalAccessTokenCreateRecord, PersonalAccessTokenLookup as RepositoryLookup,
    PersonalAccessTokenPolicyUpdateRecord, PersonalAccessTokenProofRecord,
    PersonalAccessTokenRepository, PersonalAccessTokenUpdateRecord,
};
use af_domain::{TrustedClientIp, UserId};

use crate::SessionPrincipal;

use super::{
    CreatePersonalAccessTokenCommand, IssuedPersonalAccessToken, IssuedStepUpProof,
    PersonalAccessToken, PersonalAccessTokenAccessRecord, PersonalAccessTokenAuthentication,
    PersonalAccessTokenDigest, PersonalAccessTokenError, PersonalAccessTokenLookup,
    PersonalAccessTokenMutationContext, PersonalAccessTokenPage, PersonalAccessTokenPolicy,
    PersonalAccessTokenProofSecret, PersonalAccessTokenSecret, PersonalAccessTokenStepUpEvidence,
    UpdatePersonalAccessTokenCommand, digest_personal_access_token_proof, map_error,
    validate_proof_evidence,
};

pub type PersonalAccessTokenListFuture<'a> =
    super::PersonalAccessTokenFuture<'a, PersonalAccessTokenPage>;
pub type PersonalAccessTokenGetPolicyFuture<'a> =
    super::PersonalAccessTokenFuture<'a, PersonalAccessTokenPolicy>;
pub type PersonalAccessTokenMutationFuture<'a> = super::PersonalAccessTokenFuture<'a, ()>;
pub type PersonalAccessTokenCreateFuture<'a> =
    super::PersonalAccessTokenFuture<'a, IssuedPersonalAccessToken>;
pub type PersonalAccessTokenUpdateFuture<'a> =
    super::PersonalAccessTokenFuture<'a, PersonalAccessToken>;
pub type PersonalAccessTokenAuthFuture<'a> =
    super::PersonalAccessTokenFuture<'a, PersonalAccessTokenLookup>;
pub type PersonalAccessTokenProofFuture<'a> =
    super::PersonalAccessTokenFuture<'a, IssuedStepUpProof>;

pub trait PersonalAccessTokenService: Send + Sync {
    fn list(
        &self,
        principal: SessionPrincipal,
        after: Option<i64>,
        limit: usize,
    ) -> PersonalAccessTokenListFuture<'_>;
    fn create(
        &self,
        principal: SessionPrincipal,
        command: CreatePersonalAccessTokenCommand,
        context: PersonalAccessTokenMutationContext,
    ) -> PersonalAccessTokenCreateFuture<'_>;
    fn update(
        &self,
        principal: SessionPrincipal,
        id: i64,
        command: UpdatePersonalAccessTokenCommand,
        context: PersonalAccessTokenMutationContext,
    ) -> PersonalAccessTokenUpdateFuture<'_>;
    fn revoke(
        &self,
        principal: SessionPrincipal,
        id: i64,
        context: PersonalAccessTokenMutationContext,
    ) -> PersonalAccessTokenMutationFuture<'_>;
    fn revoke_all(
        &self,
        principal: SessionPrincipal,
        target_user_id: UserId,
        context: PersonalAccessTokenMutationContext,
    ) -> super::PersonalAccessTokenFuture<'_, u64>;
    fn policy(&self, principal: SessionPrincipal) -> PersonalAccessTokenGetPolicyFuture<'_>;
    fn update_policy(
        &self,
        principal: SessionPrincipal,
        policy: PersonalAccessTokenPolicy,
        context: PersonalAccessTokenMutationContext,
    ) -> PersonalAccessTokenGetPolicyFuture<'_>;
    #[allow(
        clippy::too_many_arguments,
        reason = "A proof is bound to the interactive identity, target, purpose and exact request"
    )]
    fn issue_step_up_proof(
        &self,
        principal: SessionPrincipal,
        evidence: PersonalAccessTokenStepUpEvidence,
        purpose: String,
        target_id: Option<i64>,
        payload_digest: String,
        context: PersonalAccessTokenMutationContext,
    ) -> PersonalAccessTokenProofFuture<'_>;
    fn authenticate<'a>(
        &'a self,
        digest: &'a PersonalAccessTokenDigest,
        client_ip: TrustedClientIp,
    ) -> PersonalAccessTokenAuthFuture<'a>;
    fn admit_request_ip(&self, client_ip: TrustedClientIp)
    -> PersonalAccessTokenMutationFuture<'_>;
    fn admit_authenticated_request<'a>(
        &'a self,
        authentication: &'a PersonalAccessTokenAuthentication,
    ) -> PersonalAccessTokenMutationFuture<'a>;
    fn record_access(
        &self,
        record: PersonalAccessTokenAccessRecord,
    ) -> PersonalAccessTokenMutationFuture<'_>;
}

pub struct DatabasePersonalAccessTokenService {
    repository: PersonalAccessTokenRepository,
}

impl DatabasePersonalAccessTokenService {
    #[must_use]
    pub const fn new(repository: PersonalAccessTokenRepository) -> Self {
        Self { repository }
    }
}

impl PersonalAccessTokenService for DatabasePersonalAccessTokenService {
    fn list(
        &self,
        principal: SessionPrincipal,
        after: Option<i64>,
        limit: usize,
    ) -> PersonalAccessTokenListFuture<'_> {
        Box::pin(async move {
            let page = self
                .repository
                .list(principal.user_id(), after, limit)
                .await
                .map_err(map_error)?;
            Ok(PersonalAccessTokenPage {
                tokens: page
                    .tokens
                    .into_iter()
                    .map(PersonalAccessToken::from_record)
                    .collect(),
                next_cursor: page.next_cursor,
                capacity: page.capacity,
            })
        })
    }

    fn create(
        &self,
        principal: SessionPrincipal,
        command: CreatePersonalAccessTokenCommand,
        context: PersonalAccessTokenMutationContext,
    ) -> PersonalAccessTokenCreateFuture<'_> {
        Box::pin(async move {
            if command.scopes.is_empty()
                || command.scopes.len() > af_db::MAX_PERSONAL_ACCESS_TOKEN_SCOPES
                || command
                    .scopes
                    .iter()
                    .any(|scope| !scope_allowed(principal, scope))
            {
                return Err(PersonalAccessTokenError::Forbidden);
            }
            let issued = PersonalAccessTokenSecret::generate()?;
            let digest = issued.presented().digest();
            let record = PersonalAccessTokenCreateRecord {
                user_id: principal.user_id(),
                secret_hash: digest.as_str().to_owned(),
                token_hint: issued.presented().display_hint(),
                name: command.name,
                scopes: command.scopes,
                expires_at: command.expires_at,
                allow_ips: command.allow_ips,
            };
            let token = self
                .repository
                .create(record, command.proof, context)
                .await
                .map_err(map_error)?;
            Ok(IssuedPersonalAccessToken {
                token: PersonalAccessToken::from_record(token),
                secret: issued,
            })
        })
    }

    fn update(
        &self,
        principal: SessionPrincipal,
        id: i64,
        command: UpdatePersonalAccessTokenCommand,
        context: PersonalAccessTokenMutationContext,
    ) -> PersonalAccessTokenUpdateFuture<'_> {
        Box::pin(async move {
            if command.scopes.is_empty()
                || command.scopes.len() > af_db::MAX_PERSONAL_ACCESS_TOKEN_SCOPES
                || command
                    .scopes
                    .iter()
                    .any(|scope| !scope_allowed(principal, scope))
            {
                return Err(PersonalAccessTokenError::Forbidden);
            }
            let record = PersonalAccessTokenUpdateRecord {
                name: command.name,
                scopes: command.scopes,
                expires_at: command.expires_at,
                allow_ips: command.allow_ips,
                enabled: command.enabled,
                expected_version: command.version,
            };
            self.repository
                .update(principal.user_id(), id, record, command.proof, context)
                .await
                .map(PersonalAccessToken::from_record)
                .map_err(map_error)
        })
    }

    fn revoke(
        &self,
        principal: SessionPrincipal,
        id: i64,
        context: PersonalAccessTokenMutationContext,
    ) -> PersonalAccessTokenMutationFuture<'_> {
        Box::pin(async move {
            self.repository
                .revoke(principal.user_id(), id, context)
                .await
                .map_err(map_error)
        })
    }

    fn revoke_all(
        &self,
        principal: SessionPrincipal,
        target_user_id: UserId,
        context: PersonalAccessTokenMutationContext,
    ) -> super::PersonalAccessTokenFuture<'_, u64> {
        Box::pin(async move {
            self.repository
                .revoke_all(principal.user_id(), target_user_id, context)
                .await
                .map_err(map_error)
        })
    }

    fn policy(&self, _principal: SessionPrincipal) -> PersonalAccessTokenGetPolicyFuture<'_> {
        Box::pin(async move {
            self.repository
                .lookup_policy()
                .await
                .map(PersonalAccessTokenPolicy::from_record)
                .map_err(map_error)
        })
    }

    fn update_policy(
        &self,
        principal: SessionPrincipal,
        policy: PersonalAccessTokenPolicy,
        context: PersonalAccessTokenMutationContext,
    ) -> PersonalAccessTokenGetPolicyFuture<'_> {
        Box::pin(async move {
            let record = PersonalAccessTokenPolicyUpdateRecord {
                enabled: policy.enabled,
                issuance_enabled: policy.issuance_enabled,
                max_tokens_per_user: policy.max_tokens_per_user,
                expected_version: policy.version,
            };
            self.repository
                .update_policy(principal.user_id(), record, context)
                .await
                .map(PersonalAccessTokenPolicy::from_record)
                .map_err(map_error)
        })
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "A proof is bound to the interactive identity, target, purpose and exact request"
    )]
    fn issue_step_up_proof(
        &self,
        principal: SessionPrincipal,
        evidence: PersonalAccessTokenStepUpEvidence,
        purpose: String,
        target_id: Option<i64>,
        payload_digest: String,
        context: PersonalAccessTokenMutationContext,
    ) -> PersonalAccessTokenProofFuture<'_> {
        Box::pin(async move {
            let now = validate_proof_evidence(&evidence)?;
            if !matches!(
                purpose.as_str(),
                "create" | "scope_expand" | "extend" | "enable"
            ) || target_id.is_some_and(|id| id <= 0)
                || !is_digest(&payload_digest)
                || (purpose == "create") != target_id.is_none()
            {
                return Err(PersonalAccessTokenError::InvalidInput);
            }
            let secret = PersonalAccessTokenSecret::generate()?;
            let proof = PersonalAccessTokenProofRecord {
                proof_hash: digest_personal_access_token_proof(secret.expose_secret()),
                user_id: principal.user_id(),
                session_version: evidence.session_version,
                session_binding: evidence.session_binding,
                purpose: purpose.clone(),
                target_id,
                payload_digest,
                issued_at: now,
                expires_at: now + 300,
            };
            self.repository
                .issue_proof(proof, context)
                .await
                .map_err(map_error)?;
            Ok(IssuedStepUpProof {
                secret: PersonalAccessTokenProofSecret(zeroize::Zeroizing::new(
                    secret.expose_secret().to_owned(),
                )),
                expires_at: now + 300,
            })
        })
    }

    fn authenticate<'a>(
        &'a self,
        digest: &'a PersonalAccessTokenDigest,
        client_ip: TrustedClientIp,
    ) -> PersonalAccessTokenAuthFuture<'a> {
        Box::pin(async move {
            let lookup = self
                .repository
                .authenticate(digest.as_str(), client_ip)
                .await
                .map_err(map_error)?;
            Ok(match lookup {
                RepositoryLookup::Rejected(audit) => PersonalAccessTokenLookup::Rejected { audit },
                RepositoryLookup::Authenticated {
                    user_id,
                    role,
                    group_id,
                    token,
                } => {
                    let principal = SessionPrincipal::new(
                        user_id,
                        match role {
                            0 => crate::SessionRole::User,
                            1 => crate::SessionRole::Admin,
                            _ => return Err(PersonalAccessTokenError::Internal),
                        },
                    );
                    PersonalAccessTokenLookup::Authenticated(PersonalAccessTokenAuthentication {
                        principal,
                        group_id,
                        token_id: token.id,
                        scopes: token.scopes,
                        expires_at: token.expires_at,
                    })
                }
            })
        })
    }

    fn record_access(
        &self,
        record: PersonalAccessTokenAccessRecord,
    ) -> PersonalAccessTokenMutationFuture<'_> {
        Box::pin(async move {
            self.repository
                .record_access(record)
                .await
                .map_err(map_error)
        })
    }

    fn admit_request_ip(
        &self,
        client_ip: TrustedClientIp,
    ) -> PersonalAccessTokenMutationFuture<'_> {
        Box::pin(async move {
            self.repository
                .admit_request_ip(client_ip)
                .await
                .map_err(map_error)
        })
    }

    fn admit_authenticated_request<'a>(
        &'a self,
        authentication: &'a PersonalAccessTokenAuthentication,
    ) -> PersonalAccessTokenMutationFuture<'a> {
        Box::pin(async move {
            self.repository
                .admit_authenticated_request(
                    authentication.principal.user_id(),
                    authentication.token_id,
                )
                .await
                .map_err(map_error)
        })
    }
}

impl fmt::Debug for DatabasePersonalAccessTokenService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DatabasePersonalAccessTokenService(<redacted>)")
    }
}

fn scope_allowed(principal: SessionPrincipal, scope: &str) -> bool {
    if scope.starts_with("admin:") {
        principal.role() == crate::SessionRole::Admin && valid_admin_scope(scope)
    } else {
        normal_scope(scope)
    }
}

fn normal_scope(scope: &str) -> bool {
    const SCOPES: &[&str] = &[
        "account:read",
        "account:write",
        "account_security:read",
        "account_security:write",
        "models:read",
        "usage:read",
        "billing:read",
        "billing:write",
        "api_keys:read",
        "api_keys:write",
        "routes:read",
        "routes:write",
        "merchant:read",
        "merchant:write",
        "merchant_secrets:write",
        "merchant_finance:read",
        "merchant_finance:write",
        "notifications:read",
        "notifications:write",
        "playground:read",
        "playground:write",
        "verification:read",
        "verification:write",
        "subscriptions:read",
        "subscriptions:write",
        "invitations:read",
        "api_catalog:read",
    ];
    SCOPES.contains(&scope)
}

fn valid_admin_scope(scope: &str) -> bool {
    let parts = scope.split(':').collect::<Vec<_>>();
    parts.len() == 3
        && parts[0] == "admin"
        && parts[2] == "execute"
        && !parts[1].is_empty()
        && parts[1]
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        && scope.len() <= 128
}

fn is_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}
