//! 上游凭据与订阅账号生命周期管理。

mod credential_decryption;
mod credential_encryption;
mod oauth;
mod system_secret;

pub use credential_decryption::{
    CredentialDecryptionError, CredentialDecryptor, DecryptedCredential, DecryptedOAuthCredential,
    MAX_DECRYPTED_CREDENTIAL_SECRET_BYTES, credential_plaintext_aad,
};
pub use credential_encryption::{
    CredentialEncryptionError, CredentialEncryptor, PlainCredentialSecret, PlainOAuthCredential,
};
pub use oauth::{
    DEFAULT_MAX_OAUTH_REFRESH_FLIGHTS, DEFAULT_MAX_PENDING_OAUTH_AUTHORIZATIONS,
    DEFAULT_OAUTH_AUTHORIZATION_SESSION_TTL, DEFAULT_OAUTH_REFRESH_RESULT_RETENTION,
    MAX_OAUTH_AUTHORIZATION_SESSION_TTL, MAX_OAUTH_TOKEN_RESPONSE_BYTES,
    MAX_PENDING_OAUTH_AUTHORIZATIONS, MIN_OAUTH_AUTHORIZATION_SESSION_TTL,
    OAUTH_REFRESH_LEASE_NAMESPACE, OAUTH_REFRESH_LEASE_WRITE_BUFFER, OAuthAuthorizationCallback,
    OAuthAuthorizationContext, OAuthAuthorizationError, OAuthAuthorizationGrant,
    OAuthAuthorizationRequest, OAuthAuthorizationSessionStore, OAuthAuthorizationStart,
    OAuthClientAuthenticationMethod, OAuthConnectionCoordinator, OAuthConnectionError,
    OAuthConnectionOutcome, OAuthEndpointErrorCode, OAuthExpirationProjectionBackfillBatch,
    OAuthLoopbackRedirect, OAuthProviderProfile, OAuthProviderProfileError, OAuthRefreshCandidate,
    OAuthRefreshCoordinator, OAuthRefreshCoordinatorError, OAuthRefreshCoordinatorOutcome,
    OAuthRefreshFailureKind, OAuthRefreshFailurePersistenceOutcome, OAuthRefreshPersistenceError,
    OAuthRefreshPersistenceOutcome, OAuthRefreshPersistenceService, OAuthRefreshWriteGuard,
    OAuthRefreshedTokenSet, OAuthTokenExchange, OAuthTokenExchangeError,
    OAuthTokenPersistenceError, OAuthTokenPersistenceOutcome, OAuthTokenPersistenceService,
    OAuthTokenRefreshRequest, OAuthTokenRequestEncoding, OAuthTokenSet, UpstreamOAuthProvider,
};
pub use system_secret::{
    DecryptedSystemSecret, PlainSystemSecret, SystemSecretCipher, SystemSecretError,
    SystemSecretKind,
};
