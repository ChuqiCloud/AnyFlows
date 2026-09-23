use crate::{AccountVerificationError, PlatformPolicy, SessionPrincipal};
use af_db::{OrganizationEntitlementWrite, OrganizationRepository, OrganizationRepositoryError};
use af_domain::{OrganizationEntitlementSnapshot, OrganizationId, PlatformPermission};
use std::sync::Arc;

pub struct PlatformEntitlementService {
    repository: Arc<OrganizationRepository>,
}
impl PlatformEntitlementService {
    #[must_use]
    pub fn new(repository: Arc<OrganizationRepository>) -> Self {
        Self { repository }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn update(
        &self,
        principal: SessionPrincipal,
        id: OrganizationId,
        snapshot: OrganizationEntitlementSnapshot,
        expected_version: u64,
        reason: String,
        request_id: String,
    ) -> Result<i64, AccountVerificationError> {
        PlatformPolicy::authorize(principal, PlatformPermission::OrganizationsManage)
            .map_err(|_| AccountVerificationError::Forbidden)?;
        let write = OrganizationEntitlementWrite::new(
            snapshot,
            "platform_override".to_owned(),
            None,
            Some(expected_version),
        )
        .map_err(map_error)?;
        let result = self
            .repository
            .replace_entitlement_as_platform(id, write, principal.user_id(), reason, request_id)
            .await
            .map_err(map_error)?;
        Ok(result.version())
    }
}
fn map_error(e: OrganizationRepositoryError) -> AccountVerificationError {
    match e {
        OrganizationRepositoryError::Conflict => AccountVerificationError::Conflict,
        OrganizationRepositoryError::NotFound => AccountVerificationError::NotFound,
        OrganizationRepositoryError::UserUnavailable => AccountVerificationError::Forbidden,
        OrganizationRepositoryError::InvalidInput
        | OrganizationRepositoryError::InvalidTransition
        | OrganizationRepositoryError::EntitlementRestricted => AccountVerificationError::Invalid,
        _ => AccountVerificationError::Unavailable,
    }
}
