//! 上游凭据与订阅账号生命周期管理。

mod credential_decryption;
mod credential_encryption;
mod credential_secret;
mod diagnostic_snapshot;
mod login_oauth;
mod oauth;
mod system_secret;

pub use credential_decryption::{
    CredentialDecryptionError, CredentialDecryptor, DecryptedCredential, DecryptedOAuthCredential,
    MAX_DECRYPTED_CREDENTIAL_SECRET_BYTES, credential_plaintext_aad,
};
pub use credential_encryption::{
    CredentialEncryptionError, CredentialEncryptor, PlainOAuthCredential,
};
pub use credential_secret::{
    MAX_AWS_ACCESS_KEY_ID_BYTES, MAX_AWS_SECRET_ACCESS_KEY_BYTES, MAX_AWS_SESSION_TOKEN_BYTES,
    MAX_GOOGLE_PRIVATE_KEY_BYTES, MAX_GOOGLE_PRIVATE_KEY_ID_BYTES,
    MAX_GOOGLE_SERVICE_ACCOUNT_EMAIL_BYTES, PlainCredentialSecret,
};
pub use diagnostic_snapshot::DiagnosticSnapshotCipher;
pub use login_oauth::{
    LoginOAuthProvider, OAuthLoginClient, OAuthLoginClientError, OAuthLoginIdentity,
};
pub use oauth::{
    CODEX_CLIENT_ID, CustomOAuth2EndpointBundle, CustomOAuth2EndpointError,
    CustomOAuth2ProviderConfig, CustomOAuth2ProviderConfigError, CustomOAuth2ProviderKey,
    CustomOAuth2ProviderKeyError, DEFAULT_MAX_OAUTH_REFRESH_FLIGHTS,
    DEFAULT_MAX_PENDING_OAUTH_AUTHORIZATIONS, DEFAULT_OAUTH_AUTHORIZATION_SESSION_TTL,
    DEFAULT_OAUTH_REFRESH_RESULT_RETENTION, MAX_OAUTH_AUTHORIZATION_SESSION_TTL,
    MAX_OAUTH_TOKEN_RESPONSE_BYTES, MAX_PENDING_OAUTH_AUTHORIZATIONS,
    MIN_OAUTH_AUTHORIZATION_SESSION_TTL, OAUTH_REFRESH_LEASE_NAMESPACE,
    OAUTH_REFRESH_LEASE_WRITE_BUFFER, OAuthAuthorizationCallback, OAuthAuthorizationContext,
    OAuthAuthorizationError, OAuthAuthorizationGrant, OAuthAuthorizationRequest,
    OAuthAuthorizationSessionStore, OAuthAuthorizationStart, OAuthClientAuthenticationMethod,
    OAuthConnectionCoordinator, OAuthConnectionError, OAuthConnectionOutcome,
    OAuthEndpointErrorCode, OAuthExpirationProjectionBackfillBatch, OAuthLoopbackRedirect,
    OAuthProviderProfile, OAuthProviderProfileError, OAuthRefreshCandidate,
    OAuthRefreshCoordinator, OAuthRefreshCoordinatorError, OAuthRefreshCoordinatorOutcome,
    OAuthRefreshFailureKind, OAuthRefreshFailurePersistenceOutcome, OAuthRefreshPersistenceError,
    OAuthRefreshPersistenceOutcome, OAuthRefreshPersistenceService, OAuthRefreshRunReport,
    OAuthRefreshSupervisor, OAuthRefreshSupervisorConfig, OAuthRefreshSupervisorConfigError,
    OAuthRefreshSupervisorError, OAuthRefreshWriteGuard, OAuthRefreshedTokenSet,
    OAuthTokenExchange, OAuthTokenExchangeError, OAuthTokenPersistenceError,
    OAuthTokenPersistenceOutcome, OAuthTokenPersistenceService, OAuthTokenRefreshRequest,
    OAuthTokenRequestEncoding, OAuthTokenSet, UpstreamOAuthProvider,
};
pub use system_secret::{
    DecryptedSystemSecret, PlainSystemSecret, SystemSecretCipher, SystemSecretError,
    SystemSecretKind,
};
