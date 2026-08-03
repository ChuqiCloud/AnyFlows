mod authorization;
mod coordinator;
mod exchange;
mod persistence;
mod profile;
mod refresh_persistence;
mod store;

pub(crate) use authorization::validate_scope;

pub use authorization::{
    OAuthAuthorizationCallback, OAuthAuthorizationContext, OAuthAuthorizationError,
    OAuthAuthorizationGrant, OAuthAuthorizationRequest, OAuthAuthorizationStart,
    UpstreamOAuthProvider,
};
pub use coordinator::{OAuthConnectionCoordinator, OAuthConnectionError, OAuthConnectionOutcome};
pub use exchange::{
    MAX_OAUTH_TOKEN_RESPONSE_BYTES, OAuthClientAuthenticationMethod, OAuthEndpointErrorCode,
    OAuthRefreshedTokenSet, OAuthTokenExchange, OAuthTokenExchangeError, OAuthTokenRefreshRequest,
    OAuthTokenRequestEncoding, OAuthTokenSet,
};
pub use persistence::{
    OAuthTokenPersistenceError, OAuthTokenPersistenceOutcome, OAuthTokenPersistenceService,
};
pub use profile::{OAuthLoopbackRedirect, OAuthProviderProfile, OAuthProviderProfileError};
pub use refresh_persistence::{
    OAuthExpirationProjectionBackfillBatch, OAuthRefreshCandidate, OAuthRefreshPersistenceError,
    OAuthRefreshPersistenceOutcome, OAuthRefreshPersistenceService, OAuthRefreshWriteGuard,
};
pub use store::{
    DEFAULT_MAX_PENDING_OAUTH_AUTHORIZATIONS, DEFAULT_OAUTH_AUTHORIZATION_SESSION_TTL,
    MAX_OAUTH_AUTHORIZATION_SESSION_TTL, MAX_PENDING_OAUTH_AUTHORIZATIONS,
    MIN_OAUTH_AUTHORIZATION_SESSION_TTL, OAuthAuthorizationSessionStore,
};

#[cfg(test)]
mod tests;
