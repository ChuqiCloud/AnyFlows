use std::{fmt, time::Duration};

use af_domain::{TrustedClientIp, UserId};
use tokio::time::timeout;

use crate::DatabasePool;

mod audit;
mod mutation;
mod proof;
mod rate_limit;
mod read;
#[cfg(test)]
mod tests;
mod types;
mod validation;

pub use types::{
    MAX_PERSONAL_ACCESS_TOKEN_IPS, MAX_PERSONAL_ACCESS_TOKEN_NAME_BYTES,
    MAX_PERSONAL_ACCESS_TOKEN_PAGE_SIZE, MAX_PERSONAL_ACCESS_TOKEN_SCOPES,
    MAX_PERSONAL_ACCESS_TOKENS_PER_USER, PersonalAccessTokenAccessAuditRecord,
    PersonalAccessTokenAuditCleanupResult, PersonalAccessTokenAuditIdentity,
    PersonalAccessTokenCreateRecord, PersonalAccessTokenLookup, PersonalAccessTokenMutationContext,
    PersonalAccessTokenPageRecord, PersonalAccessTokenPolicyRecord,
    PersonalAccessTokenPolicyUpdateRecord, PersonalAccessTokenProofClaim,
    PersonalAccessTokenProofRecord, PersonalAccessTokenRecord, PersonalAccessTokenRejectionReason,
    PersonalAccessTokenRepositoryConfigError, PersonalAccessTokenRepositoryError,
    PersonalAccessTokenUpdateRecord,
};

#[derive(Clone)]
pub struct PersonalAccessTokenRepository {
    pub(super) pool: DatabasePool,
    pub(super) lookup_timeout: Duration,
}

impl PersonalAccessTokenRepository {
    pub fn new(
        pool: DatabasePool,
        lookup_timeout: Duration,
    ) -> Result<Self, PersonalAccessTokenRepositoryConfigError> {
        if lookup_timeout.is_zero() {
            return Err(PersonalAccessTokenRepositoryConfigError::ZeroLookupTimeout);
        }
        Ok(Self {
            pool,
            lookup_timeout,
        })
    }

    pub async fn list(
        &self,
        owner: UserId,
        after: Option<i64>,
        limit: usize,
    ) -> Result<PersonalAccessTokenPageRecord, PersonalAccessTokenRepositoryError> {
        if !(1..=MAX_PERSONAL_ACCESS_TOKEN_PAGE_SIZE).contains(&limit)
            || after.is_some_and(|id| id <= 0)
        {
            return Err(PersonalAccessTokenRepositoryError::InvalidInput);
        }
        self.run(read::list(self, owner, after, limit)).await
    }

    pub async fn create(
        &self,
        record: PersonalAccessTokenCreateRecord,
        proof: PersonalAccessTokenProofClaim,
        context: PersonalAccessTokenMutationContext,
    ) -> Result<PersonalAccessTokenRecord, PersonalAccessTokenRepositoryError> {
        self.run(mutation::create(self, record, proof, context))
            .await
    }

    pub async fn update(
        &self,
        owner: UserId,
        id: i64,
        record: PersonalAccessTokenUpdateRecord,
        proof: Option<PersonalAccessTokenProofClaim>,
        context: PersonalAccessTokenMutationContext,
    ) -> Result<PersonalAccessTokenRecord, PersonalAccessTokenRepositoryError> {
        self.run(mutation::update(self, owner, id, record, proof, context))
            .await
    }

    pub async fn revoke(
        &self,
        owner: UserId,
        id: i64,
        context: PersonalAccessTokenMutationContext,
    ) -> Result<(), PersonalAccessTokenRepositoryError> {
        self.run(mutation::revoke(self, owner, id, context)).await
    }

    /// An administrator can revoke every access token owned by an account.
    pub async fn revoke_all(
        &self,
        actor: UserId,
        target: UserId,
        context: PersonalAccessTokenMutationContext,
    ) -> Result<u64, PersonalAccessTokenRepositoryError> {
        self.run(mutation::revoke_all(self, actor, target, context))
            .await
    }

    pub async fn lookup_policy(
        &self,
    ) -> Result<PersonalAccessTokenPolicyRecord, PersonalAccessTokenRepositoryError> {
        self.run(read::policy(self)).await
    }

    pub async fn update_policy(
        &self,
        actor: UserId,
        update: PersonalAccessTokenPolicyUpdateRecord,
        context: PersonalAccessTokenMutationContext,
    ) -> Result<PersonalAccessTokenPolicyRecord, PersonalAccessTokenRepositoryError> {
        self.run(mutation::update_policy(self, actor, update, context))
            .await
    }

    pub async fn authenticate(
        &self,
        digest: &str,
        client_ip: TrustedClientIp,
    ) -> Result<PersonalAccessTokenLookup, PersonalAccessTokenRepositoryError> {
        if !valid_digest(digest) {
            return Ok(PersonalAccessTokenLookup::Rejected(None));
        }
        self.run(read::authenticate(self, digest, client_ip)).await
    }

    pub async fn admit_request_ip(
        &self,
        client_ip: TrustedClientIp,
    ) -> Result<(), PersonalAccessTokenRepositoryError> {
        self.run(rate_limit::ip(self, client_ip)).await
    }

    pub async fn admit_authenticated_request(
        &self,
        user: UserId,
        token_id: i64,
    ) -> Result<(), PersonalAccessTokenRepositoryError> {
        self.run(rate_limit::identity(self, user, token_id)).await
    }

    pub async fn issue_proof(
        &self,
        record: PersonalAccessTokenProofRecord,
        context: PersonalAccessTokenMutationContext,
    ) -> Result<(), PersonalAccessTokenRepositoryError> {
        self.run(proof::issue(self, record, context)).await
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "The proof must bind identity, operation, target and payload atomically"
    )]
    pub(crate) async fn consume_proof(
        &self,
        transaction: &sea_orm::DatabaseTransaction,
        owner: UserId,
        claim: &PersonalAccessTokenProofClaim,
        expected_action: &str,
        expected_target: Option<i64>,
        expected_payload: &str,
        now: i64,
    ) -> Result<(), PersonalAccessTokenRepositoryError> {
        proof::consume(
            self,
            transaction,
            owner,
            claim,
            expected_action,
            expected_target,
            expected_payload,
            now,
        )
        .await
    }

    pub async fn record_access(
        &self,
        record: PersonalAccessTokenAccessAuditRecord,
    ) -> Result<(), PersonalAccessTokenRepositoryError> {
        self.run(audit::record(self, record)).await
    }

    pub async fn prune_access_audit(
        &self,
        before: i64,
        limit: usize,
    ) -> Result<PersonalAccessTokenAuditCleanupResult, PersonalAccessTokenRepositoryError> {
        if before <= 0 || !(1..=10_000).contains(&limit) {
            return Err(PersonalAccessTokenRepositoryError::InvalidInput);
        }
        self.run(audit::prune(self, before, limit)).await
    }

    pub async fn prune_expired_proofs(
        &self,
        now: i64,
        limit: usize,
    ) -> Result<u64, PersonalAccessTokenRepositoryError> {
        if now <= 0 || !(1..=10_000).contains(&limit) {
            return Err(PersonalAccessTokenRepositoryError::InvalidInput);
        }
        self.run(proof::prune(self, now, limit)).await
    }

    async fn run<T>(
        &self,
        future: impl std::future::Future<Output = Result<T, PersonalAccessTokenRepositoryError>>,
    ) -> Result<T, PersonalAccessTokenRepositoryError> {
        use tracing::{instrument::WithSubscriber as _, subscriber::NoSubscriber};
        match timeout(
            self.lookup_timeout,
            future.with_subscriber(NoSubscriber::default()),
        )
        .await
        {
            Ok(result) => result,
            Err(_) => {
                tracing::error!(target: "af_db::personal_access_token", error_kind = "timeout", "访问令牌仓储操作超时");
                Err(PersonalAccessTokenRepositoryError::Timeout)
            }
        }
    }
}

impl fmt::Debug for PersonalAccessTokenRepository {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PersonalAccessTokenRepository")
            .field("lookup_timeout", &self.lookup_timeout)
            .finish_non_exhaustive()
    }
}

pub(crate) fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

/// Canonical operation binding shared by HTTP proof issuance and transactional consumption.
pub fn personal_access_token_payload_digest(
    name: &str,
    scopes: &[String],
    expiry: Option<i64>,
    ips: Option<&[String]>,
    enabled: bool,
    version: Option<i64>,
) -> Result<String, PersonalAccessTokenRepositoryError> {
    validation::payload_digest(name, scopes, expiry, ips, enabled, version)
}
