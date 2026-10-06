mod authorization;
mod coordinator;
mod custom;
mod exchange;
mod identity;
mod persistence;
mod profile;
mod refresh_coordinator;
mod refresh_failure;
mod refresh_persistence;
mod refresh_supervisor;
mod store;

pub(crate) use authorization::validate_scope;

pub use authorization::{
    OAuthAuthorizationCallback, OAuthAuthorizationContext, OAuthAuthorizationError,
    OAuthAuthorizationGrant, OAuthAuthorizationRequest, OAuthAuthorizationStart,
    UpstreamOAuthProvider,
};
pub use coordinator::{OAuthConnectionCoordinator, OAuthConnectionError, OAuthConnectionOutcome};
pub use custom::{
    CustomOAuth2EndpointBundle, CustomOAuth2EndpointError, CustomOAuth2ProviderConfig,
    CustomOAuth2ProviderConfigError, CustomOAuth2ProviderKey, CustomOAuth2ProviderKeyError,
};
pub use exchange::{
    MAX_OAUTH_TOKEN_RESPONSE_BYTES, OAuthClientAuthenticationMethod, OAuthEndpointErrorCode,
    OAuthRefreshedTokenSet, OAuthTokenExchange, OAuthTokenExchangeError, OAuthTokenRefreshRequest,
    OAuthTokenRequestEncoding, OAuthTokenSet,
};
pub use persistence::{
    OAuthTokenPersistenceError, OAuthTokenPersistenceOutcome, OAuthTokenPersistenceService,
};
pub use profile::{
    CODEX_CLIENT_ID, OAuthLoopbackRedirect, OAuthProviderProfile, OAuthProviderProfileError,
};
pub use refresh_coordinator::{
    DEFAULT_MAX_OAUTH_REFRESH_FLIGHTS, DEFAULT_OAUTH_REFRESH_RESULT_RETENTION,
    OAUTH_REFRESH_LEASE_NAMESPACE, OAUTH_REFRESH_LEASE_WRITE_BUFFER, OAuthRefreshCoordinator,
    OAuthRefreshCoordinatorError, OAuthRefreshCoordinatorOutcome,
};
pub use refresh_failure::OAuthRefreshFailureKind;
pub use refresh_persistence::{
    OAuthExpirationProjectionBackfillBatch, OAuthRefreshCandidate,
    OAuthRefreshFailurePersistenceOutcome, OAuthRefreshPersistenceError,
    OAuthRefreshPersistenceOutcome, OAuthRefreshPersistenceService, OAuthRefreshWriteGuard,
};
pub use refresh_supervisor::{
    OAuthRefreshRunReport, OAuthRefreshSupervisor, OAuthRefreshSupervisorConfig,
    OAuthRefreshSupervisorConfigError, OAuthRefreshSupervisorError,
};
pub use store::{
    DEFAULT_MAX_PENDING_OAUTH_AUTHORIZATIONS, DEFAULT_OAUTH_AUTHORIZATION_SESSION_TTL,
    MAX_OAUTH_AUTHORIZATION_SESSION_TTL, MAX_PENDING_OAUTH_AUTHORIZATIONS,
    MIN_OAUTH_AUTHORIZATION_SESSION_TTL, OAuthAuthorizationSessionStore,
};

#[cfg(test)]
mod tests;
