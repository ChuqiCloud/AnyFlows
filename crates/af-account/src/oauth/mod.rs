mod authorization;
mod store;

pub use authorization::{
    OAuthAuthorizationCallback, OAuthAuthorizationContext, OAuthAuthorizationError,
    OAuthAuthorizationGrant, OAuthAuthorizationRequest, OAuthAuthorizationStart,
    UpstreamOAuthProvider,
};
pub use store::{
    DEFAULT_MAX_PENDING_OAUTH_AUTHORIZATIONS, DEFAULT_OAUTH_AUTHORIZATION_SESSION_TTL,
    MAX_OAUTH_AUTHORIZATION_SESSION_TTL, MAX_PENDING_OAUTH_AUTHORIZATIONS,
    MIN_OAUTH_AUTHORIZATION_SESSION_TTL, OAuthAuthorizationSessionStore,
};

#[cfg(test)]
mod tests;
