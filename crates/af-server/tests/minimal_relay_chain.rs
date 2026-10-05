pub mod support;

use std::{
    net::{SocketAddr, TcpListener},
    sync::Arc,
    time::Duration,
};

use af_admin::{
    AdminBalanceAlertSettingsCommand, AdminBalanceAlertSettingsError,
    AdminBalanceAlertSettingsReadFuture, AdminBalanceAlertSettingsService,
    AdminBalanceAlertSettingsUpdateFuture, AdminChannelCreateCommand, AdminChannelCreateFuture,
    AdminChannelDeleteFuture, AdminChannelGetFuture, AdminChannelListFuture, AdminChannelListQuery,
    AdminChannelReadError, AdminChannelReader, AdminChannelUpdateCommand, AdminChannelUpdateFuture,
    AdminChannelWriteError, AdminChannelWriter, AdminCredentialCreateCommand,
    AdminCredentialCreateFuture, AdminCredentialDeleteFuture, AdminCredentialGetFuture,
    AdminCredentialListFuture, AdminCredentialListQuery, AdminCredentialProxyCreateCommand,
    AdminCredentialProxyDeleteFuture, AdminCredentialProxyError, AdminCredentialProxyFuture,
    AdminCredentialProxyListQuery, AdminCredentialProxyPageFuture, AdminCredentialProxyService,
    AdminCredentialProxyUpdateCommand, AdminCredentialUpdateCommand, AdminCredentialUpdateFuture,
    AdminDebugTraceError, AdminDebugTraceGetFuture, AdminDebugTraceListFuture,
    AdminDebugTraceListQuery, AdminDebugTraceService, AdminDebugTraceSettingsCommand,
    AdminDebugTraceSettingsFuture, AdminDebugTraceSnapshotScope, AdminDebugTraceSnapshotsFuture,
    AdminDebugTraceUpdateFuture, AdminEmailSettingsCommand, AdminEmailSettingsError,
    AdminEmailSettingsReadFuture, AdminEmailSettingsService, AdminEmailSettingsUpdateFuture,
    AdminEmailTestCommand, AdminEmailTestFuture, AdminGroupCreateCommand, AdminGroupCreateFuture,
    AdminGroupDeleteFuture, AdminGroupGetFuture, AdminGroupListFuture, AdminGroupListQuery,
    AdminGroupReadError, AdminGroupReader, AdminGroupUpdateCommand, AdminGroupUpdateFuture,
    AdminGroupWriteError, AdminGroupWriter, AdminNetworkSettingsCommand, AdminNetworkSettingsError,
    AdminNetworkSettingsReadFuture, AdminNetworkSettingsService, AdminNetworkSettingsUpdateFuture,
    AdminPaymentSettingsCommand, AdminPaymentSettingsError, AdminPaymentSettingsReadFuture,
    AdminPaymentSettingsService, AdminPaymentSettingsUpdateFuture, AdminSiteSettingsReadFuture,
    AdminSiteSettingsUpdateFuture, AdminTokenCreateCommand, AdminTokenCreateFuture,
    AdminTokenDeleteFuture, AdminTokenGetFuture, AdminTokenListFuture, AdminTokenListQuery,
    AdminTokenReadError, AdminTokenReader, AdminTokenUpdateCommand, AdminTokenUpdateFuture,
    AdminTokenWriteError, AdminTokenWriter, AdminUsageLogListFuture, AdminUsageLogListQuery,
    AdminUsageLogReadError, AdminUsageLogReader, AdminUserCreateCommand, AdminUserCreateFuture,
    AdminUserDeleteFuture, AdminUserGetFuture, AdminUserListFuture, AdminUserListQuery,
    AdminUserReadError, AdminUserReader, AdminUserUpdateCommand, AdminUserUpdateFuture,
    AdminUserWriteError, AdminUserWriter, AnnouncementFuture, AnnouncementListFuture,
    AnnouncementService, AnnouncementServiceError, AnnouncementWriteCommand, ApiKeyDigest,
    InitialSetup, InitialSetupCommand, InitialSetupError, InitialSetupFuture,
    InitialSetupStatusFuture, LoginCredentials, ModelCatalogQuery, ModelCatalogReadError,
    ModelCatalogReader, PasswordResetConfirmCommand, PasswordResetConfirmFuture,
    PasswordResetError, PasswordResetRequestCommand, PasswordResetRequestFuture,
    PasswordResetService, PlaygroundConversationDeleteFuture, PlaygroundConversationError,
    PlaygroundConversationListFuture, PlaygroundConversationReadFuture,
    PlaygroundConversationSaveCommand, PlaygroundConversationSaveFuture,
    PlaygroundConversationService, PlaygroundShareCreateCommand, PlaygroundShareCreateFuture,
    PlaygroundShareError, PlaygroundShareReadFuture, PlaygroundShareRevokeFuture,
    PlaygroundShareService, PublicSiteSettingsFuture, RedemptionCreateFuture,
    RedemptionDisableFuture, RedemptionListFuture, RedemptionRedeemFuture, RedemptionService,
    RedemptionServiceError, RegistrationCommand, RegistrationEmailVerificationCommand,
    RegistrationEmailVerificationFuture, RegistrationError, RegistrationFuture,
    RegistrationPolicyCommand, RegistrationPolicyFuture, RegistrationPolicyUpdateFuture,
    RegistrationService, RegistrationStatusFuture, SessionAuthenticationError,
    SessionAuthenticationFuture, SessionAuthenticator, SessionLoginFuture, SessionPrincipal,
    SiteSettingsCommand, SiteSettingsError, SiteSettingsService, SubscriptionService,
    SubscriptionServiceError, TokenAuthentication, TokenAuthenticationFuture, TokenAuthenticator,
    UserInvitationError, UserInvitationReadFuture, UserInvitationService, UserNotificationError,
    UserNotificationListFuture, UserNotificationPreferencesCommand, UserNotificationService,
    UserNotificationUpdateFuture, UserPasswordChangeCommand, UserPasswordChangeFuture,
    UserProfileError, UserProfileReadFuture, UserProfileService, UserProfileUpdateCommand,
    UserProfileUpdateFuture, UserTokenCreateFuture, UserTokenDeleteFuture, UserTokenError,
    UserTokenGetFuture, UserTokenListFuture, UserTokenListQuery, UserTokenService,
    UserTokenUpdateFuture, UserTokenWriteCommand, UserWalletError, UserWalletListFuture,
    UserWalletListQuery, UserWalletService, UserWalletSummaryFuture,
};
use af_analytics::{
    AdminDashboardAccess, AdminDashboardReadError, AdminDashboardReadFuture, AdminDashboardReader,
};
use af_config::ServerConfig;
use af_domain::{
    AfError, ChannelId, ConcurrencyLimit, CredentialId, GatewayPrincipal, GroupId, ProxyId, Role,
    TokenId, TokenModelPolicy, TrustedClientIp, UserId,
};
use af_http::{
    AudioService, AudioServiceFuture, EmbeddingService, EmbeddingServiceFuture, HttpListener,
    ImageService, ImageServiceFuture, QueryApiKeyPolicy, ReadinessFuture, ReadinessHandle,
    ReadinessProbe, RerankService, RerankServiceFuture, ResponsesCompactService,
    ResponsesCompactServiceFuture, ServeOutcome, SpeechService, SpeechServiceFuture, build_router,
    serve_with_graceful_shutdown,
};
use af_httpclient::{HttpClientConfig, HttpClientPool, HttpTimeouts, ProxyConfig, RemoteDnsPolicy};
use af_protocol::{
    CanonicalAudioSpeechRequest, CanonicalAudioTranscriptionRequest, CanonicalEmbeddingRequest,
    CanonicalImageGenerationRequest, CanonicalRerankRequest, CanonicalResponsesCompactionRequest,
    CanonicalStreamEvent, ContentDelta, FinishReason, TokenCount, Usage, UsageDetails,
    UsageSemantics, UsageSource, openai_responses::OpenAiResponsesStreamEncoder,
};
use af_relay::RelayService;
use serde_json::{Value, json};
use support::{
    CLIENT_KEY, GROUP_ID, IO_TIMEOUT, TOKEN_ID, UPSTREAM_BASE_URL, UPSTREAM_KEY, UPSTREAM_MODEL,
    USER_ID, send_request, spawn_abort_observing_proxy, spawn_proxy, spawn_streaming_proxy,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::oneshot,
    time::timeout,
};

struct AlwaysReadyProbe;

struct RejectAdminNetworkSettings;
struct RejectAdminPaymentSettings;

struct RejectAdminCredentialProxies;

impl AdminCredentialProxyService for RejectAdminCredentialProxies {
    fn list<'a>(
        &'a self,
        _principal: af_admin::SessionPrincipal,
        _query: AdminCredentialProxyListQuery,
    ) -> AdminCredentialProxyPageFuture<'a> {
        Box::pin(async { Err(AdminCredentialProxyError::Internal) })
    }

    fn get<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _proxy_id: ProxyId,
    ) -> AdminCredentialProxyFuture<'a> {
        Box::pin(async { Err(AdminCredentialProxyError::Internal) })
    }

    fn create<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _command: AdminCredentialProxyCreateCommand,
    ) -> AdminCredentialProxyFuture<'a> {
        Box::pin(async { Err(AdminCredentialProxyError::Internal) })
    }

    fn update<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _proxy_id: ProxyId,
        _command: AdminCredentialProxyUpdateCommand,
    ) -> AdminCredentialProxyFuture<'a> {
        Box::pin(async { Err(AdminCredentialProxyError::Internal) })
    }

    fn delete<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _proxy_id: ProxyId,
    ) -> AdminCredentialProxyDeleteFuture<'a> {
        Box::pin(async { Err(AdminCredentialProxyError::Internal) })
    }
}

impl AdminNetworkSettingsService for RejectAdminNetworkSettings {
    fn settings<'a>(&'a self, _principal: SessionPrincipal) -> AdminNetworkSettingsReadFuture<'a> {
        Box::pin(async { Err(AdminNetworkSettingsError::Internal) })
    }

    fn update<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _command: AdminNetworkSettingsCommand,
    ) -> AdminNetworkSettingsUpdateFuture<'a> {
        Box::pin(async { Err(AdminNetworkSettingsError::Internal) })
    }
}

impl AdminPaymentSettingsService for RejectAdminPaymentSettings {
    fn settings<'a>(&'a self, _principal: SessionPrincipal) -> AdminPaymentSettingsReadFuture<'a> {
        Box::pin(async { Err(AdminPaymentSettingsError::Internal) })
    }

    fn update<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _command: AdminPaymentSettingsCommand,
    ) -> AdminPaymentSettingsUpdateFuture<'a> {
        Box::pin(async { Err(AdminPaymentSettingsError::Internal) })
    }
}

impl ReadinessProbe for AlwaysReadyProbe {
    fn check(&self) -> ReadinessFuture<'_> {
        Box::pin(async { true })
    }
}

struct AlwaysAuthenticates;

struct RejectSessions;
struct RejectModelCatalog;
struct RejectPlaygroundShares;
struct RejectPlaygroundConversations;
struct RejectUserTokens;
struct RejectInitialSetup;
struct RejectRegistration;
struct RejectPasswordReset;
struct RejectUserProfile;
struct RejectUserWallet;
struct RejectUserNotifications;
struct RejectRedemptions;
struct RejectSubscriptions;
struct RejectUserInvitations;
struct RejectSiteSettings;
struct RejectAnnouncements;
struct RejectAdminEmailSettings;
struct RejectAdminBalanceAlertSettings;
struct RejectAdminDebugTraces;
struct RejectAdminChannels;
struct RejectAdminGroups;
struct RejectAdminTokens;
struct RejectAdminDashboard;
struct RejectAdminUsageLogs;
struct RejectAdminUsers;
struct RejectEmbeddings;
struct RejectImages;
struct RejectAudio;
struct RejectRerank;
struct RejectSpeech;
struct RejectResponsesCompact;

impl ResponsesCompactService for RejectResponsesCompact {
    fn compact<'a>(
        &'a self,
        _principal: &'a GatewayPrincipal,
        _user_concurrency: Option<ConcurrencyLimit>,
        _request: CanonicalResponsesCompactionRequest,
        _request_id: &'a str,
    ) -> ResponsesCompactServiceFuture<'a> {
        Box::pin(async { Err(AfError::Internal) })
    }
}

impl AudioService for RejectAudio {
    fn transcribe<'a>(
        &'a self,
        _principal: &'a GatewayPrincipal,
        _user_concurrency: Option<ConcurrencyLimit>,
        _request: CanonicalAudioTranscriptionRequest,
        _request_id: &'a str,
    ) -> AudioServiceFuture<'a> {
        Box::pin(async { Err(AfError::Internal) })
    }
}

impl RerankService for RejectRerank {
    fn rerank<'a>(
        &'a self,
        _principal: &'a GatewayPrincipal,
        _user_concurrency: Option<ConcurrencyLimit>,
        _request: CanonicalRerankRequest,
        _request_id: &'a str,
    ) -> RerankServiceFuture<'a> {
        Box::pin(async { Err(AfError::Internal) })
    }
}

impl SpeechService for RejectSpeech {
    fn synthesize<'a>(
        &'a self,
        _principal: &'a GatewayPrincipal,
        _user_concurrency: Option<ConcurrencyLimit>,
        _request: CanonicalAudioSpeechRequest,
        _request_id: &'a str,
    ) -> SpeechServiceFuture<'a> {
        Box::pin(async { Err(AfError::Internal) })
    }
}

impl EmbeddingService for RejectEmbeddings {
    fn embeddings<'a>(
        &'a self,
        _principal: &'a GatewayPrincipal,
        _user_concurrency: Option<ConcurrencyLimit>,
        _request: CanonicalEmbeddingRequest,
        _request_id: &'a str,
    ) -> EmbeddingServiceFuture<'a> {
        Box::pin(async { Err(AfError::Internal) })
    }
}

impl ImageService for RejectImages {
    fn generate<'a>(
        &'a self,
        _principal: &'a GatewayPrincipal,
        _user_concurrency: Option<ConcurrencyLimit>,
        _request: CanonicalImageGenerationRequest,
        _request_id: &'a str,
    ) -> ImageServiceFuture<'a> {
        Box::pin(async { Err(AfError::Internal) })
    }
}

impl AdminDebugTraceService for RejectAdminDebugTraces {
    fn settings<'a>(&'a self, _principal: SessionPrincipal) -> AdminDebugTraceSettingsFuture<'a> {
        Box::pin(async { Err(AdminDebugTraceError::Internal) })
    }

    fn update<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _command: AdminDebugTraceSettingsCommand,
    ) -> AdminDebugTraceUpdateFuture<'a> {
        Box::pin(async { Err(AdminDebugTraceError::Internal) })
    }

    fn list<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _query: AdminDebugTraceListQuery,
    ) -> AdminDebugTraceListFuture<'a> {
        Box::pin(async { Err(AdminDebugTraceError::Internal) })
    }

    fn detail<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _id: i64,
    ) -> AdminDebugTraceGetFuture<'a> {
        Box::pin(async { Err(AdminDebugTraceError::Internal) })
    }

    fn snapshots<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _id: i64,
        _scope: AdminDebugTraceSnapshotScope,
    ) -> AdminDebugTraceSnapshotsFuture<'a> {
        Box::pin(async { Err(AdminDebugTraceError::Internal) })
    }
}

impl SessionAuthenticator for RejectSessions {
    fn login<'a>(&'a self, _credentials: &'a LoginCredentials) -> SessionLoginFuture<'a> {
        Box::pin(async { Err(SessionAuthenticationError::InvalidCredentials) })
    }

    fn authenticate<'a>(&'a self, _token: &'a str) -> SessionAuthenticationFuture<'a> {
        Box::pin(async { Err(SessionAuthenticationError::InvalidSession) })
    }
}

impl PasswordResetService for RejectPasswordReset {
    fn request<'a>(
        &'a self,
        _client_ip: TrustedClientIp,
        _command: &'a PasswordResetRequestCommand,
    ) -> PasswordResetRequestFuture<'a> {
        Box::pin(async { Err(PasswordResetError::Internal) })
    }

    fn confirm(&self, _command: PasswordResetConfirmCommand) -> PasswordResetConfirmFuture<'_> {
        Box::pin(async { Err(PasswordResetError::Internal) })
    }
}

impl UserProfileService for RejectUserProfile {
    fn get<'a>(&'a self, _principal: SessionPrincipal) -> UserProfileReadFuture<'a> {
        Box::pin(async { Err(UserProfileError::Internal) })
    }

    fn update_profile<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _command: UserProfileUpdateCommand,
    ) -> UserProfileUpdateFuture<'a> {
        Box::pin(async { Err(UserProfileError::Internal) })
    }

    fn change_password<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _command: UserPasswordChangeCommand,
    ) -> UserPasswordChangeFuture<'a> {
        Box::pin(async { Err(UserProfileError::Internal) })
    }

    fn update_notifications<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _command: UserNotificationPreferencesCommand,
    ) -> UserNotificationUpdateFuture<'a> {
        Box::pin(async { Err(UserProfileError::Internal) })
    }
}

impl UserWalletService for RejectUserWallet {
    fn summary<'a>(&'a self, _principal: SessionPrincipal) -> UserWalletSummaryFuture<'a> {
        Box::pin(async { Err(UserWalletError::Internal) })
    }

    fn list<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _query: UserWalletListQuery,
    ) -> UserWalletListFuture<'a> {
        Box::pin(async { Err(UserWalletError::Internal) })
    }
}

impl UserNotificationService for RejectUserNotifications {
    fn list<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _query: af_admin::UserNotificationListQuery,
    ) -> UserNotificationListFuture<'a> {
        Box::pin(async { Err(UserNotificationError::Internal) })
    }

    fn mark_read<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _command: af_admin::UserNotificationMarkReadCommand,
    ) -> af_admin::UserNotificationMarkReadFuture<'a> {
        Box::pin(async { Err(UserNotificationError::Internal) })
    }
}

impl RedemptionService for RejectRedemptions {
    fn audit<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _query: af_admin::AdminRedemptionAuditQuery,
    ) -> af_admin::RedemptionAuditFuture<'a> {
        Box::pin(async { Err(RedemptionServiceError::Internal) })
    }

    fn list<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _query: af_admin::AdminRedemptionBatchListQuery,
    ) -> RedemptionListFuture<'a> {
        Box::pin(async { Err(RedemptionServiceError::Internal) })
    }

    fn create<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _command: af_admin::AdminRedemptionBatchCreateCommand,
    ) -> RedemptionCreateFuture<'a> {
        Box::pin(async { Err(RedemptionServiceError::Internal) })
    }

    fn disable<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _batch_id: af_domain::RedemptionBatchId,
        _command: af_admin::AdminRedemptionBatchDisableCommand,
    ) -> RedemptionDisableFuture<'a> {
        Box::pin(async { Err(RedemptionServiceError::Internal) })
    }

    fn redeem<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _command: af_admin::UserRedemptionCommand,
    ) -> RedemptionRedeemFuture<'a> {
        Box::pin(async { Err(RedemptionServiceError::Internal) })
    }
}

impl SubscriptionService for RejectSubscriptions {
    fn list_catalog<'a>(
        &'a self,
        _principal: SessionPrincipal,
    ) -> af_admin::SubscriptionCatalogFuture<'a> {
        Box::pin(async { Err(SubscriptionServiceError::Internal) })
    }

    fn create_order<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _command: af_admin::SubscriptionOrderCreateCommand,
    ) -> af_admin::SubscriptionCreateOrderFuture<'a> {
        Box::pin(async { Err(SubscriptionServiceError::Internal) })
    }

    fn get_order<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _order_id: af_domain::SubscriptionOrderId,
    ) -> af_admin::SubscriptionGetOrderFuture<'a> {
        Box::pin(async { Err(SubscriptionServiceError::Internal) })
    }

    fn submit_order<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _order_id: af_domain::SubscriptionOrderId,
        _command: af_admin::SubscriptionOrderPaymentCommand,
        _provider: Arc<dyn af_billing::PaymentOrderProvider>,
    ) -> af_admin::SubscriptionSubmitOrderFuture<'a> {
        Box::pin(async { Err(SubscriptionServiceError::Internal) })
    }

    fn list_plans<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _query: af_admin::AdminSubscriptionPageQuery,
    ) -> af_admin::SubscriptionListPlansFuture<'a> {
        Box::pin(async { Err(SubscriptionServiceError::Internal) })
    }

    fn create_plan<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _command: af_admin::AdminSubscriptionPlanCreateCommand,
    ) -> af_admin::SubscriptionCreatePlanFuture<'a> {
        Box::pin(async { Err(SubscriptionServiceError::Internal) })
    }

    fn disable_plan<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _plan_id: af_domain::SubscriptionPlanId,
        _command: af_admin::AdminSubscriptionPlanDisableCommand,
    ) -> af_admin::SubscriptionDisablePlanFuture<'a> {
        Box::pin(async { Err(SubscriptionServiceError::Internal) })
    }

    fn list_user_subscriptions<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _user_id: UserId,
        _query: af_admin::AdminSubscriptionPageQuery,
    ) -> af_admin::SubscriptionListUserFuture<'a> {
        Box::pin(async { Err(SubscriptionServiceError::Internal) })
    }

    fn list_current_subscriptions<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _query: af_admin::AdminSubscriptionPageQuery,
    ) -> af_admin::SubscriptionListUserFuture<'a> {
        Box::pin(async { Err(SubscriptionServiceError::Internal) })
    }

    fn bind_user<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _user_id: UserId,
        _command: af_admin::AdminUserSubscriptionBindCommand,
    ) -> af_admin::SubscriptionBindFuture<'a> {
        Box::pin(async { Err(SubscriptionServiceError::Internal) })
    }

    fn transition_user_lifecycle<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _user_id: UserId,
        _subscription_id: af_domain::UserSubscriptionId,
        _command: af_admin::AdminUserSubscriptionLifecycleCommand,
    ) -> af_admin::SubscriptionLifecycleFuture<'a> {
        Box::pin(async { Err(SubscriptionServiceError::Internal) })
    }
}

impl UserInvitationService for RejectUserInvitations {
    fn get<'a>(&'a self, _principal: SessionPrincipal) -> UserInvitationReadFuture<'a> {
        Box::pin(async { Err(UserInvitationError::Internal) })
    }
}

impl ModelCatalogReader for RejectModelCatalog {
    fn list_for_token(
        &self,
        _authentication: af_admin::TokenAuthentication,
    ) -> af_admin::GatewayModelListFuture<'_> {
        Box::pin(async { Err(ModelCatalogReadError::Internal) })
    }

    fn list<'a>(
        &'a self,
        _authentication: Option<af_admin::SessionAuthentication>,
        _query: &'a ModelCatalogQuery,
    ) -> af_admin::ModelCatalogListFuture<'a> {
        Box::pin(async { Err(ModelCatalogReadError::Internal) })
    }

    fn providers(
        &self,
        _authentication: Option<af_admin::SessionAuthentication>,
    ) -> af_admin::ModelCatalogProvidersFuture<'_> {
        Box::pin(async { Err(ModelCatalogReadError::Internal) })
    }
}

impl PlaygroundShareService for RejectPlaygroundShares {
    fn create(
        &self,
        _principal: af_admin::SessionPrincipal,
        _command: PlaygroundShareCreateCommand,
    ) -> PlaygroundShareCreateFuture<'_> {
        Box::pin(async { Err(PlaygroundShareError::Internal) })
    }

    fn read(&self, _token: String) -> PlaygroundShareReadFuture<'_> {
        Box::pin(async { Err(PlaygroundShareError::Internal) })
    }

    fn revoke(
        &self,
        _principal: af_admin::SessionPrincipal,
        _token: String,
    ) -> PlaygroundShareRevokeFuture<'_> {
        Box::pin(async { Err(PlaygroundShareError::Internal) })
    }
}

impl PlaygroundConversationService for RejectPlaygroundConversations {
    fn save(
        &self,
        _principal: af_admin::SessionPrincipal,
        _command: PlaygroundConversationSaveCommand,
    ) -> PlaygroundConversationSaveFuture<'_> {
        Box::pin(async { Err(PlaygroundConversationError::Internal) })
    }

    fn list(&self, _principal: af_admin::SessionPrincipal) -> PlaygroundConversationListFuture<'_> {
        Box::pin(async { Err(PlaygroundConversationError::Internal) })
    }

    fn read(
        &self,
        _principal: af_admin::SessionPrincipal,
        _conversation_id: String,
    ) -> PlaygroundConversationReadFuture<'_> {
        Box::pin(async { Err(PlaygroundConversationError::Internal) })
    }

    fn delete(
        &self,
        _principal: af_admin::SessionPrincipal,
        _conversation_id: String,
    ) -> PlaygroundConversationDeleteFuture<'_> {
        Box::pin(async { Err(PlaygroundConversationError::Internal) })
    }
}

impl UserTokenService for RejectUserTokens {
    fn list(
        &self,
        _principal: af_admin::SessionPrincipal,
        _query: UserTokenListQuery,
    ) -> UserTokenListFuture<'_> {
        Box::pin(async { Err(UserTokenError::Internal) })
    }

    fn get(
        &self,
        _principal: af_admin::SessionPrincipal,
        _token_id: TokenId,
    ) -> UserTokenGetFuture<'_> {
        Box::pin(async { Err(UserTokenError::Internal) })
    }

    fn create(
        &self,
        _principal: af_admin::SessionPrincipal,
        _command: UserTokenWriteCommand,
    ) -> UserTokenCreateFuture<'_> {
        Box::pin(async { Err(UserTokenError::Internal) })
    }

    fn update(
        &self,
        _principal: af_admin::SessionPrincipal,
        _token_id: TokenId,
        _command: UserTokenWriteCommand,
    ) -> UserTokenUpdateFuture<'_> {
        Box::pin(async { Err(UserTokenError::Internal) })
    }

    fn delete(
        &self,
        _principal: af_admin::SessionPrincipal,
        _token_id: TokenId,
    ) -> UserTokenDeleteFuture<'_> {
        Box::pin(async { Err(UserTokenError::Internal) })
    }
}

impl InitialSetup for RejectInitialSetup {
    fn status(&self) -> InitialSetupStatusFuture<'_> {
        Box::pin(async { Err(InitialSetupError::Internal) })
    }

    fn initialize(&self, _command: InitialSetupCommand) -> InitialSetupFuture<'_> {
        Box::pin(async { Err(InitialSetupError::Internal) })
    }
}

impl RegistrationService for RejectRegistration {
    fn status(&self) -> RegistrationStatusFuture<'_> {
        Box::pin(async { Err(RegistrationError::Internal) })
    }

    fn send_email_verification<'a>(
        &'a self,
        _client_ip: TrustedClientIp,
        _command: &'a RegistrationEmailVerificationCommand,
    ) -> RegistrationEmailVerificationFuture<'a> {
        Box::pin(async { Err(RegistrationError::Internal) })
    }

    fn register<'a>(
        &'a self,
        _client_ip: TrustedClientIp,
        _command: &'a RegistrationCommand,
    ) -> RegistrationFuture<'a> {
        Box::pin(async { Err(RegistrationError::Internal) })
    }

    fn policy(&self, _principal: SessionPrincipal) -> RegistrationPolicyFuture<'_> {
        Box::pin(async { Err(RegistrationError::Internal) })
    }

    fn update_policy(
        &self,
        _principal: SessionPrincipal,
        _command: RegistrationPolicyCommand,
    ) -> RegistrationPolicyUpdateFuture<'_> {
        Box::pin(async { Err(RegistrationError::Internal) })
    }
}

impl SiteSettingsService for RejectSiteSettings {
    fn public_settings(&self) -> PublicSiteSettingsFuture<'_> {
        Box::pin(async { Err(SiteSettingsError::Internal) })
    }

    fn admin_settings(&self, _principal: SessionPrincipal) -> AdminSiteSettingsReadFuture<'_> {
        Box::pin(async { Err(SiteSettingsError::Internal) })
    }

    fn update(
        &self,
        _principal: SessionPrincipal,
        _command: SiteSettingsCommand,
    ) -> AdminSiteSettingsUpdateFuture<'_> {
        Box::pin(async { Err(SiteSettingsError::Internal) })
    }

    fn update_navigation(
        &self,
        _principal: SessionPrincipal,
        _navigation: af_admin::SiteNavigationRecord,
        _expected_version: i64,
    ) -> AdminSiteSettingsUpdateFuture<'_> {
        Box::pin(async { Err(SiteSettingsError::Internal) })
    }
}

impl AnnouncementService for RejectAnnouncements {
    fn list_public(&self) -> AnnouncementListFuture<'_> {
        Box::pin(async { Err(AnnouncementServiceError::Internal) })
    }

    fn list_admin(&self, _principal: SessionPrincipal) -> AnnouncementListFuture<'_> {
        Box::pin(async { Err(AnnouncementServiceError::Internal) })
    }

    fn create(
        &self,
        _principal: SessionPrincipal,
        _command: AnnouncementWriteCommand,
    ) -> AnnouncementFuture<'_> {
        Box::pin(async { Err(AnnouncementServiceError::Internal) })
    }

    fn update_draft(
        &self,
        _principal: SessionPrincipal,
        _id: i64,
        _expected_version: i64,
        _command: AnnouncementWriteCommand,
    ) -> AnnouncementFuture<'_> {
        Box::pin(async { Err(AnnouncementServiceError::Internal) })
    }

    fn publish(
        &self,
        _principal: SessionPrincipal,
        _id: i64,
        _expected_version: i64,
    ) -> AnnouncementFuture<'_> {
        Box::pin(async { Err(AnnouncementServiceError::Internal) })
    }

    fn revoke(
        &self,
        _principal: SessionPrincipal,
        _id: i64,
        _expected_version: i64,
    ) -> AnnouncementFuture<'_> {
        Box::pin(async { Err(AnnouncementServiceError::Internal) })
    }
}

impl AdminEmailSettingsService for RejectAdminEmailSettings {
    fn settings(&self, _principal: SessionPrincipal) -> AdminEmailSettingsReadFuture<'_> {
        Box::pin(async { Err(AdminEmailSettingsError::Internal) })
    }

    fn update(
        &self,
        _principal: SessionPrincipal,
        _command: AdminEmailSettingsCommand,
    ) -> AdminEmailSettingsUpdateFuture<'_> {
        Box::pin(async { Err(AdminEmailSettingsError::Internal) })
    }

    fn send_test(
        &self,
        _principal: SessionPrincipal,
        _command: AdminEmailTestCommand,
    ) -> AdminEmailTestFuture<'_> {
        Box::pin(async { Err(AdminEmailSettingsError::Internal) })
    }
}

impl AdminBalanceAlertSettingsService for RejectAdminBalanceAlertSettings {
    fn settings(&self, _principal: SessionPrincipal) -> AdminBalanceAlertSettingsReadFuture<'_> {
        Box::pin(async { Err(AdminBalanceAlertSettingsError::Internal) })
    }

    fn update(
        &self,
        _principal: SessionPrincipal,
        _command: AdminBalanceAlertSettingsCommand,
    ) -> AdminBalanceAlertSettingsUpdateFuture<'_> {
        Box::pin(async { Err(AdminBalanceAlertSettingsError::Internal) })
    }
}

impl AdminChannelReader for RejectAdminChannels {
    fn list_channels<'a>(
        &'a self,
        _principal: af_admin::SessionPrincipal,
        _query: AdminChannelListQuery,
    ) -> AdminChannelListFuture<'a> {
        Box::pin(async { Err(AdminChannelReadError::Internal) })
    }

    fn get_channel<'a>(
        &'a self,
        _principal: af_admin::SessionPrincipal,
        _channel_id: ChannelId,
    ) -> AdminChannelGetFuture<'a> {
        Box::pin(async { Err(AdminChannelReadError::Internal) })
    }

    fn list_credentials<'a>(
        &'a self,
        _principal: af_admin::SessionPrincipal,
        _channel_id: ChannelId,
        _query: AdminCredentialListQuery,
    ) -> AdminCredentialListFuture<'a> {
        Box::pin(async { Err(AdminChannelReadError::Internal) })
    }

    fn get_credential<'a>(
        &'a self,
        _principal: af_admin::SessionPrincipal,
        _channel_id: ChannelId,
        _credential_id: CredentialId,
    ) -> AdminCredentialGetFuture<'a> {
        Box::pin(async { Err(AdminChannelReadError::Internal) })
    }
}

impl AdminChannelWriter for RejectAdminChannels {
    fn create_channel<'a>(
        &'a self,
        _principal: af_admin::SessionPrincipal,
        _command: AdminChannelCreateCommand,
    ) -> AdminChannelCreateFuture<'a> {
        Box::pin(async { Err(AdminChannelWriteError::Internal) })
    }

    fn update_channel<'a>(
        &'a self,
        _principal: af_admin::SessionPrincipal,
        _channel_id: ChannelId,
        _command: AdminChannelUpdateCommand,
    ) -> AdminChannelUpdateFuture<'a> {
        Box::pin(async { Err(AdminChannelWriteError::Internal) })
    }

    fn delete_channel<'a>(
        &'a self,
        _principal: af_admin::SessionPrincipal,
        _channel_id: ChannelId,
    ) -> AdminChannelDeleteFuture<'a> {
        Box::pin(async { Err(AdminChannelWriteError::Internal) })
    }

    fn create_credential<'a>(
        &'a self,
        _principal: af_admin::SessionPrincipal,
        _channel_id: ChannelId,
        _command: AdminCredentialCreateCommand,
    ) -> AdminCredentialCreateFuture<'a> {
        Box::pin(async { Err(AdminChannelWriteError::Internal) })
    }

    fn update_credential<'a>(
        &'a self,
        _principal: af_admin::SessionPrincipal,
        _channel_id: ChannelId,
        _credential_id: CredentialId,
        _command: AdminCredentialUpdateCommand,
    ) -> AdminCredentialUpdateFuture<'a> {
        Box::pin(async { Err(AdminChannelWriteError::Internal) })
    }

    fn delete_credential<'a>(
        &'a self,
        _principal: af_admin::SessionPrincipal,
        _channel_id: ChannelId,
        _credential_id: CredentialId,
    ) -> AdminCredentialDeleteFuture<'a> {
        Box::pin(async { Err(AdminChannelWriteError::Internal) })
    }
}

impl AdminGroupReader for RejectAdminGroups {
    fn list<'a>(
        &'a self,
        _principal: af_admin::SessionPrincipal,
        _query: AdminGroupListQuery,
    ) -> AdminGroupListFuture<'a> {
        Box::pin(async { Err(AdminGroupReadError::Internal) })
    }

    fn get<'a>(
        &'a self,
        _principal: af_admin::SessionPrincipal,
        _group_id: GroupId,
    ) -> AdminGroupGetFuture<'a> {
        Box::pin(async { Err(AdminGroupReadError::Internal) })
    }
}

impl AdminGroupWriter for RejectAdminGroups {
    fn create<'a>(
        &'a self,
        _principal: af_admin::SessionPrincipal,
        _command: AdminGroupCreateCommand,
    ) -> AdminGroupCreateFuture<'a> {
        Box::pin(async { Err(AdminGroupWriteError::Internal) })
    }

    fn update<'a>(
        &'a self,
        _principal: af_admin::SessionPrincipal,
        _group_id: af_domain::GroupId,
        _command: AdminGroupUpdateCommand,
    ) -> AdminGroupUpdateFuture<'a> {
        Box::pin(async { Err(AdminGroupWriteError::Internal) })
    }

    fn delete<'a>(
        &'a self,
        _principal: af_admin::SessionPrincipal,
        _group_id: af_domain::GroupId,
    ) -> AdminGroupDeleteFuture<'a> {
        Box::pin(async { Err(AdminGroupWriteError::Internal) })
    }
}

impl AdminTokenReader for RejectAdminTokens {
    fn list<'a>(
        &'a self,
        _principal: af_admin::SessionPrincipal,
        _query: AdminTokenListQuery,
    ) -> AdminTokenListFuture<'a> {
        Box::pin(async { Err(AdminTokenReadError::Internal) })
    }

    fn get<'a>(
        &'a self,
        _principal: af_admin::SessionPrincipal,
        _token_id: TokenId,
    ) -> AdminTokenGetFuture<'a> {
        Box::pin(async { Err(AdminTokenReadError::Internal) })
    }
}

impl AdminTokenWriter for RejectAdminTokens {
    fn create<'a>(
        &'a self,
        _principal: af_admin::SessionPrincipal,
        _command: AdminTokenCreateCommand,
    ) -> AdminTokenCreateFuture<'a> {
        Box::pin(async { Err(AdminTokenWriteError::Internal) })
    }

    fn update<'a>(
        &'a self,
        _principal: af_admin::SessionPrincipal,
        _token_id: TokenId,
        _command: AdminTokenUpdateCommand,
    ) -> AdminTokenUpdateFuture<'a> {
        Box::pin(async { Err(AdminTokenWriteError::Internal) })
    }

    fn delete<'a>(
        &'a self,
        _principal: af_admin::SessionPrincipal,
        _token_id: TokenId,
    ) -> AdminTokenDeleteFuture<'a> {
        Box::pin(async { Err(AdminTokenWriteError::Internal) })
    }
}

impl AdminUsageLogReader for RejectAdminUsageLogs {
    fn list<'a>(
        &'a self,
        _principal: af_admin::SessionPrincipal,
        _query: AdminUsageLogListQuery,
    ) -> AdminUsageLogListFuture<'a> {
        Box::pin(async { Err(AdminUsageLogReadError::Internal) })
    }

    fn list_own<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _query: AdminUsageLogListQuery,
    ) -> AdminUsageLogListFuture<'a> {
        Box::pin(async { Err(AdminUsageLogReadError::Internal) })
    }
}

impl AdminDashboardReader for RejectAdminDashboard {
    fn read(&self, _access: AdminDashboardAccess) -> AdminDashboardReadFuture<'_> {
        Box::pin(async { Err(AdminDashboardReadError::Internal) })
    }
}

impl AdminUserReader for RejectAdminUsers {
    fn list<'a>(
        &'a self,
        _principal: af_admin::SessionPrincipal,
        _query: AdminUserListQuery,
    ) -> AdminUserListFuture<'a> {
        Box::pin(async { Err(AdminUserReadError::Internal) })
    }

    fn get<'a>(
        &'a self,
        _principal: af_admin::SessionPrincipal,
        _user_id: UserId,
    ) -> AdminUserGetFuture<'a> {
        Box::pin(async { Err(AdminUserReadError::Internal) })
    }
}

impl AdminUserWriter for RejectAdminUsers {
    fn create<'a>(
        &'a self,
        _principal: af_admin::SessionPrincipal,
        _command: AdminUserCreateCommand,
    ) -> AdminUserCreateFuture<'a> {
        Box::pin(async { Err(AdminUserWriteError::Internal) })
    }

    fn update<'a>(
        &'a self,
        _principal: af_admin::SessionPrincipal,
        _user_id: UserId,
        _command: AdminUserUpdateCommand,
    ) -> AdminUserUpdateFuture<'a> {
        Box::pin(async { Err(AdminUserWriteError::Internal) })
    }

    fn delete<'a>(
        &'a self,
        _principal: af_admin::SessionPrincipal,
        _user_id: UserId,
    ) -> AdminUserDeleteFuture<'a> {
        Box::pin(async { Err(AdminUserWriteError::Internal) })
    }
}

impl TokenAuthenticator for AlwaysAuthenticates {
    fn authenticate<'a>(
        &'a self,
        _digest: &'a ApiKeyDigest,
        _client_ip: TrustedClientIp,
    ) -> TokenAuthenticationFuture<'a> {
        Box::pin(async {
            Ok(TokenAuthentication::new(
                GatewayPrincipal::new(
                    TokenId::new(TOKEN_ID).unwrap(),
                    UserId::new(USER_ID).unwrap(),
                    GroupId::new(GROUP_ID).unwrap(),
                ),
                TokenModelPolicy::unrestricted(),
            ))
        })
    }
}

#[tokio::test]
async fn invalid_requests_and_wrong_model_never_reach_upstream() {
    let proxy = TcpListener::bind("127.0.0.1:0").unwrap();
    proxy.set_nonblocking(true).unwrap();
    let (address, shutdown, server) =
        start_server(proxy.local_addr().unwrap(), ServerConfig::default()).await;

    let authorization = format!("Bearer {CLIENT_KEY}");
    for (method, body, expected_status, expected_code) in [
        (
            "POST",
            br#"{"model":"test-model","model":"private-duplicate","messages":[]}"#.as_slice(),
            400,
            "invalid_request",
        ),
        (
            "POST",
            br#"{"model":"test-model","messages":[{"role":"user","content":"hello"}],"stream":true,"stream_options":{"include_obfuscation":false}}"#.as_slice(),
            400,
            "invalid_request",
        ),
        (
            "POST",
            br#"{"model":"private-wrong-model","messages":[{"role":"user","content":"hello"}]}"#.as_slice(),
            404,
            "model_not_found",
        ),
        ("GET", b"".as_slice(), 405, ""),
    ] {
        let response = send_request(
            address,
            method,
            "/v1/chat/completions",
            body,
            &[
                ("Content-Type", "application/json"),
                ("Authorization", &authorization),
            ],
        )
        .await;
        assert_eq!(response.status, expected_status);
        if !expected_code.is_empty() {
            let value: Value = serde_json::from_slice(&response.body).unwrap();
            assert_eq!(value["error"]["code"], expected_code);
            let text = String::from_utf8_lossy(&response.body);
            assert!(!text.contains("private-duplicate"));
            assert!(!text.contains("private-wrong-model"));
        }
    }
    stop_server(shutdown, server).await;
    assert!(matches!(
        proxy.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    ));
}

#[tokio::test]
async fn responses_stateful_requests_and_wrong_model_never_reach_upstream() {
    let proxy = TcpListener::bind("127.0.0.1:0").unwrap();
    proxy.set_nonblocking(true).unwrap();
    let (address, shutdown, server) =
        start_server(proxy.local_addr().unwrap(), ServerConfig::default()).await;
    let authorization = format!("Bearer {CLIENT_KEY}");

    for (method, body, expected_status, expected_code) in [
        (
            "POST",
            br#"{"model":"test-model","model":"private-duplicate","input":"hello"}"#.as_slice(),
            400,
            "invalid_request",
        ),
        (
            "POST",
            br#"{"model":"test-model","input":"hello","store":true}"#.as_slice(),
            400,
            "invalid_request",
        ),
        (
            "POST",
            br#"{"model":"test-model","previous_response_id":"resp_private"}"#.as_slice(),
            400,
            "invalid_request",
        ),
        (
            "POST",
            br#"{"model":"private-wrong-model","input":"hello","store":false}"#.as_slice(),
            404,
            "model_not_found",
        ),
        ("GET", b"".as_slice(), 405, ""),
    ] {
        let response = send_request(
            address,
            method,
            "/v1/responses",
            body,
            &[
                ("Content-Type", "application/json"),
                ("Authorization", &authorization),
            ],
        )
        .await;
        assert_eq!(response.status, expected_status);
        if !expected_code.is_empty() {
            let value: Value = serde_json::from_slice(&response.body).unwrap();
            assert_eq!(value["error"]["code"], expected_code);
            let text = String::from_utf8_lossy(&response.body);
            for private in ["private-duplicate", "resp_private", "private-wrong-model"] {
                assert!(!text.contains(private));
            }
        }
    }

    stop_server(shutdown, server).await;
    assert!(matches!(
        proxy.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    ));
}

#[tokio::test]
async fn anthropic_invalid_requests_use_messages_errors_and_never_reach_upstream() {
    let proxy = TcpListener::bind("127.0.0.1:0").unwrap();
    proxy.set_nonblocking(true).unwrap();
    let (address, shutdown, server) =
        start_server(proxy.local_addr().unwrap(), ServerConfig::default()).await;

    for (body, expected_status, expected_type) in [
        (
            br#"{"model":"test-model","model":"private-duplicate","max_tokens":8,"messages":[{"role":"user","content":"hello"}]}"#.as_slice(),
            400,
            "invalid_request_error",
        ),
        (
            br#"{"model":"private-wrong-model","max_tokens":8,"messages":[{"role":"user","content":"hello"}]}"#.as_slice(),
            404,
            "not_found_error",
        ),
    ] {
        let response = send_request(
            address,
            "POST",
            "/v1/messages",
            body,
            &[
                ("Content-Type", "application/json"),
                ("x-api-key", CLIENT_KEY),
            ],
        )
        .await;
        assert_eq!(response.status, expected_status);
        let value: Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(value["type"], "error");
        assert_eq!(value["error"]["type"], expected_type);
        let text = String::from_utf8_lossy(&response.body);
        assert!(!text.contains("private-duplicate"));
        assert!(!text.contains("private-wrong-model"));
    }

    stop_server(shutdown, server).await;
    assert!(matches!(
        proxy.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    ));
}

#[tokio::test]
async fn gemini_invalid_requests_use_google_errors_and_never_reach_upstream() {
    let proxy = TcpListener::bind("127.0.0.1:0").unwrap();
    proxy.set_nonblocking(true).unwrap();
    let (address, shutdown, server) =
        start_server(proxy.local_addr().unwrap(), ServerConfig::default()).await;

    for (path, body, expected_status, expected_google_status) in [
        (
            "/v1beta/models/test-model:generateContent",
            br#"{"contents":[{"role":"user","parts":[{"text":"hello"}]}],"contents":[]}"#
                .as_slice(),
            400,
            "INVALID_ARGUMENT",
        ),
        (
            "/v1beta/models/private-wrong-model:generateContent",
            br#"{"contents":[{"role":"user","parts":[{"text":"hello"}]}]}"#.as_slice(),
            404,
            "NOT_FOUND",
        ),
    ] {
        let response = send_request(
            address,
            "POST",
            path,
            body,
            &[
                ("Content-Type", "application/json"),
                ("x-goog-api-key", CLIENT_KEY),
            ],
        )
        .await;
        assert_eq!(response.status, expected_status);
        let value: Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(value["error"]["code"], expected_status);
        assert_eq!(value["error"]["status"], expected_google_status);
        let text = String::from_utf8_lossy(&response.body);
        assert!(!text.contains("private-wrong-model"));
    }

    stop_server(shutdown, server).await;
    assert!(matches!(
        proxy.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    ));
}

#[tokio::test]
async fn verified_same_protocol_full_request_and_response_preserve_exact_bytes() {
    let upstream_body = br#"{
  "id": "chatcmpl-byte-exact",
  "object": "chat.completion",
  "created": 1700000000,
  "model": "test-model",
  "choices": [{"index":0,"message":{"role":"assistant","content":"exact"},"finish_reason":"stop"}],
  "usage": {"prompt_tokens":2,"completion_tokens":1,"total_tokens":3}
}"#;
    let request_body = br#"{
  "model": "test-model",
  "messages": [{"role":"user","content":"hello"}]
}"#;
    let (proxy_address, captured, proxy) = spawn_proxy(
        "200 OK",
        upstream_body,
        &[("Content-Type", "application/json")],
    );
    let (address, shutdown, server) = start_server(proxy_address, ServerConfig::default()).await;
    let authorization = format!("Bearer {CLIENT_KEY}");

    let response = send_request(
        address,
        "POST",
        "/v1/chat/completions",
        request_body,
        &[
            ("Content-Type", "application/json"),
            ("Authorization", &authorization),
        ],
    )
    .await;
    stop_server(shutdown, server).await;
    let upstream = captured.recv_timeout(IO_TIMEOUT).unwrap();
    proxy.join().unwrap();

    assert_eq!(upstream.body, request_body);
    assert_eq!(response.status, 200);
    assert_eq!(response.body, upstream_body);
}

#[tokio::test]
async fn responses_full_request_is_stateless_and_preserves_validated_source() {
    let upstream_body = br#"{
  "id": "resp_private_upstream",
  "object": "response",
  "created_at": 1700000000,
  "status": "completed",
  "error": null,
  "incomplete_details": null,
  "model": "private-upstream-model",
  "output": [{"id":"msg_private","type":"message","status":"completed","role":"assistant","content":[{"type":"output_text","text":"responses-answer","annotations":[],"logprobs":[]}]}],
  "usage": {"input_tokens":2,"input_tokens_details":{"cache_write_tokens":0,"cached_tokens":0},"output_tokens":1,"output_tokens_details":{"reasoning_tokens":0},"total_tokens":3},
  "service_tier": "priority"
}"#;
    let request_body = br#"{
  "model": "test-model",
  "input": "hello",
  "include": ["reasoning.encrypted_content"],
  "max_output_tokens": 32
}"#;
    let (proxy_address, captured, proxy) = spawn_proxy(
        "200 OK",
        upstream_body,
        &[("Content-Type", "application/json")],
    );
    let (address, shutdown, server) = start_server(proxy_address, ServerConfig::default()).await;
    let authorization = format!("Bearer {CLIENT_KEY}");

    let response = send_request(
        address,
        "POST",
        "/v1/responses",
        request_body,
        &[
            ("Content-Type", "application/json"),
            ("Authorization", &authorization),
        ],
    )
    .await;
    stop_server(shutdown, server).await;
    let upstream = captured.recv_timeout(IO_TIMEOUT).unwrap();
    proxy.join().unwrap();

    let upstream_head = upstream.head.to_ascii_lowercase();
    assert!(upstream_head.starts_with("post http://upstream.example/proxy/v1/responses http/1.1"));
    assert!(!upstream.head.contains(CLIENT_KEY));
    assert!(upstream_head.contains(&format!("authorization: bearer {UPSTREAM_KEY}")));
    let upstream_json: Value = serde_json::from_slice(&upstream.body).unwrap();
    assert_eq!(upstream_json["model"], "test-model");
    assert!(upstream_json.get("stream").is_none());
    assert_eq!(upstream_json["store"], false);
    assert_eq!(upstream_json["max_output_tokens"], 32);
    assert_eq!(upstream_json["input"], "hello");
    assert_eq!(
        upstream_json["include"],
        json!(["reasoning.encrypted_content"])
    );

    assert_eq!(response.status, 200);
    let body: Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(body["object"], "response");
    assert_eq!(body["model"], "test-model");
    assert_eq!(body["output"][0]["content"][0]["text"], "responses-answer");
    assert_eq!(body["usage"]["input_tokens"], 2);
    assert_eq!(body["usage"]["output_tokens"], 1);
    assert!(body["id"].as_str().unwrap().starts_with("resp_"));
    assert!(body.get("service_tier").is_none());
    let text = String::from_utf8_lossy(&response.body);
    assert!(!text.contains("resp_private_upstream"));
    assert!(!text.contains("private-upstream-model"));
    assert!(!text.contains("msg_private"));
}

#[tokio::test]
async fn anthropic_full_request_and_response_cross_the_canonical_boundary() {
    let upstream_body = br#"{
  "id": "upstream-private-id",
  "object": "chat.completion",
  "created": 1700000000,
  "model": "upstream-private-model",
  "choices": [{"index":0,"message":{"role":"assistant","content":"cross-protocol-answer"},"finish_reason":"stop"}],
  "usage": {"prompt_tokens":2,"completion_tokens":1,"total_tokens":3}
}"#;
    let request_body = br#"{
  "model": "test-model",
  "max_tokens": 32,
  "messages": [{"role":"user","content":"hello"}]
}"#;
    let (proxy_address, captured, proxy) = spawn_proxy(
        "200 OK",
        upstream_body,
        &[("Content-Type", "application/json")],
    );
    let (address, shutdown, server) = start_server(proxy_address, ServerConfig::default()).await;

    let response = send_request(
        address,
        "POST",
        "/v1/messages",
        request_body,
        &[
            ("Content-Type", "application/json"),
            ("x-api-key", CLIENT_KEY),
        ],
    )
    .await;
    stop_server(shutdown, server).await;
    let upstream = captured.recv_timeout(IO_TIMEOUT).unwrap();
    proxy.join().unwrap();

    assert_ne!(upstream.body, request_body);
    let upstream_json: Value = serde_json::from_slice(&upstream.body).unwrap();
    assert_eq!(upstream_json["model"], "test-model");
    assert_eq!(upstream_json["max_completion_tokens"], 32);
    assert_eq!(upstream_json["messages"][0]["role"], "user");

    assert_eq!(response.status, 200);
    let body: Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(body["type"], "message");
    assert_eq!(body["role"], "assistant");
    assert_eq!(body["model"], "test-model");
    assert_eq!(body["content"][0]["type"], "text");
    assert_eq!(body["content"][0]["text"], "cross-protocol-answer");
    assert_eq!(body["usage"]["input_tokens"], 2);
    assert_eq!(body["usage"]["output_tokens"], 1);
    let text = String::from_utf8_lossy(&response.body);
    assert!(!text.contains("upstream-private-id"));
    assert!(!text.contains("upstream-private-model"));
}

#[tokio::test]
async fn gemini_full_request_and_response_cross_the_canonical_boundary() {
    let upstream_body = br#"{
  "id": "upstream-private-id",
  "object": "chat.completion",
  "created": 1700000000,
  "model": "upstream-private-model",
  "choices": [{"index":0,"message":{"role":"assistant","content":"gemini-answer"},"finish_reason":"stop"}],
  "usage": {"prompt_tokens":2,"completion_tokens":1,"total_tokens":3}
}"#;
    let request_body = br#"{
  "contents": [{"role":"user","parts":[{"text":"hello"}]}],
  "generationConfig": {"maxOutputTokens":32}
}"#;
    let (proxy_address, captured, proxy) = spawn_proxy(
        "200 OK",
        upstream_body,
        &[("Content-Type", "application/json")],
    );
    let (address, shutdown, server) = start_server(proxy_address, ServerConfig::default()).await;

    let response = send_request(
        address,
        "POST",
        "/v1beta/models/test-model:generateContent",
        request_body,
        &[
            ("Content-Type", "application/json"),
            ("x-goog-api-key", CLIENT_KEY),
        ],
    )
    .await;
    stop_server(shutdown, server).await;
    let upstream = captured.recv_timeout(IO_TIMEOUT).unwrap();
    proxy.join().unwrap();

    assert_ne!(upstream.body, request_body);
    let upstream_head = upstream.head.to_ascii_lowercase();
    assert!(!upstream_head.contains("x-goog-api-key"));
    assert!(!upstream.head.contains(CLIENT_KEY));
    assert!(upstream_head.contains(&format!("authorization: bearer {UPSTREAM_KEY}")));
    let upstream_json: Value = serde_json::from_slice(&upstream.body).unwrap();
    assert_eq!(upstream_json["model"], "test-model");
    assert_eq!(upstream_json["max_completion_tokens"], 32);
    assert_eq!(upstream_json["messages"][0]["role"], "user");
    assert_eq!(upstream_json["messages"][0]["content"], "hello");

    assert_eq!(response.status, 200);
    let body: Value = serde_json::from_slice(&response.body).unwrap();
    assert_eq!(body["modelVersion"], "test-model");
    assert_eq!(body["candidates"][0]["content"]["role"], "model");
    assert_eq!(
        body["candidates"][0]["content"]["parts"][0]["text"],
        "gemini-answer"
    );
    assert_eq!(body["usageMetadata"]["promptTokenCount"], 2);
    assert_eq!(body["usageMetadata"]["candidatesTokenCount"], 1);
    let text = String::from_utf8_lossy(&response.body);
    assert!(!text.contains("upstream-private-id"));
    assert!(!text.contains("upstream-private-model"));
}

#[tokio::test]
async fn streaming_response_is_incremental_canonical_and_unbuffered() {
    let first = br#"data: {"id":"upstream-private-id","object":"chat.completion.chunk","created":1,"model":"test-model","choices":[{"index":0,"delta":{"role":"assistant","content":"first-part"},"finish_reason":null}]}

"#;
    let rest = br#"data: {"id":"upstream-private-id","object":"chat.completion.chunk","created":1,"model":"test-model","choices":[{"index":0,"delta":{"content":"second-part"},"finish_reason":null}]}

data: {"id":"upstream-private-id","object":"chat.completion.chunk","created":1,"model":"test-model","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}

data: [DONE]

"#;
    let (proxy_address, captured, release, proxy) = spawn_streaming_proxy(first, rest);
    let (address, shutdown, server) = start_server(proxy_address, ServerConfig::default()).await;
    let authorization = format!("Bearer {CLIENT_KEY}");
    let request_body = br#"{"model":"test-model","messages":[{"role":"user","content":"hello"}],"stream":true,"stream_options":{"include_usage":true}}"#;
    let mut client = connect_and_send(address, request_body, &authorization).await;

    let first_response = read_until(&mut client, b"first-part").await;
    let first_text = String::from_utf8_lossy(&first_response);
    let first_lower = first_text.to_ascii_lowercase();
    assert!(first_lower.starts_with("http/1.1 200"));
    assert!(first_lower.contains("content-type: text/event-stream; charset=utf-8"));
    assert!(first_lower.contains("cache-control: no-cache, no-transform"));
    assert!(first_lower.contains("x-accel-buffering: no"));
    assert!(!first_lower.contains("content-encoding:"));
    assert!(!first_text.contains("second-part"));
    assert!(!first_text.contains("[DONE]"));
    assert!(!first_text.contains("upstream-private-id"));

    let upstream = captured.recv_timeout(IO_TIMEOUT).unwrap();
    assert_eq!(upstream.body, request_body);
    let upstream_json: Value = serde_json::from_slice(&upstream.body).unwrap();
    assert_eq!(upstream_json["stream"], true);
    assert_eq!(upstream_json["stream_options"]["include_usage"], true);
    release.send(()).unwrap();

    let mut complete = first_response;
    timeout(IO_TIMEOUT, client.read_to_end(&mut complete))
        .await
        .unwrap()
        .unwrap();
    let complete = String::from_utf8_lossy(&complete);
    assert!(complete.contains("second-part"));
    assert!(complete.contains("\"usage\":{"));
    assert!(complete.contains("\"prompt_tokens\""));
    assert!(complete.contains("data: [DONE]"));
    assert!(complete.contains("chat.completion.chunk"));
    assert!(!complete.contains("upstream-private-id"));

    stop_server(shutdown, server).await;
    proxy.join().unwrap();
}

#[tokio::test]
async fn responses_streaming_is_incremental_stateless_and_rebuilds_identity() {
    let (first, rest) = responses_stream_chunks();
    let (proxy_address, captured, release, proxy) = spawn_streaming_proxy(&first, &rest);
    let (address, shutdown, server) = start_server(proxy_address, ServerConfig::default()).await;
    let authorization = format!("Bearer {CLIENT_KEY}");
    let request_body = br#"{"model":"test-model","input":"hello","stream":true,"store":false}"#;
    let mut client = connect_and_send_responses(address, request_body, &authorization).await;

    let first_response = read_until(&mut client, b"first-part").await;
    let first_text = String::from_utf8_lossy(&first_response);
    let first_lower = first_text.to_ascii_lowercase();
    assert!(first_lower.starts_with("http/1.1 200"));
    assert!(first_lower.contains("content-type: text/event-stream; charset=utf-8"));
    assert!(first_lower.contains("cache-control: no-cache, no-transform"));
    assert!(first_lower.contains("x-accel-buffering: no"));
    assert!(first_text.contains("event: response.created"));
    assert!(first_text.contains("event: response.output_text.delta"));
    assert!(!first_text.contains("second-part"));
    assert!(!first_text.contains("resp_private_upstream"));
    assert!(!first_text.contains("private-upstream-model"));

    let upstream = captured.recv_timeout(IO_TIMEOUT).unwrap();
    let upstream_head = upstream.head.to_ascii_lowercase();
    assert!(upstream_head.starts_with("post http://upstream.example/proxy/v1/responses http/1.1"));
    let upstream_json: Value = serde_json::from_slice(&upstream.body).unwrap();
    assert_eq!(upstream_json["stream"], true);
    assert_eq!(upstream_json["store"], false);
    assert_eq!(upstream_json["model"], "test-model");
    release.send(()).unwrap();

    let mut complete = first_response;
    timeout(IO_TIMEOUT, client.read_to_end(&mut complete))
        .await
        .unwrap()
        .unwrap();
    let complete = String::from_utf8_lossy(&complete);
    assert!(complete.contains("second-part"));
    assert!(complete.contains("event: response.completed"));
    assert!(complete.contains("\"input_tokens\":3"));
    assert!(complete.contains("\"output_tokens\":2"));
    assert!(!complete.contains("data: [DONE]"));
    assert!(!complete.contains("resp_private_upstream"));
    assert!(!complete.contains("private-upstream-model"));

    stop_server(shutdown, server).await;
    proxy.join().unwrap();
}

#[tokio::test]
async fn anthropic_streaming_response_is_incremental_and_uses_messages_events() {
    let first = br#"data: {"id":"upstream-private-id","object":"chat.completion.chunk","created":1,"model":"upstream-private-model","choices":[{"index":0,"delta":{"role":"assistant","content":"first-part"},"finish_reason":null}]}

"#;
    let rest = br#"data: {"id":"upstream-private-id","object":"chat.completion.chunk","created":1,"model":"upstream-private-model","choices":[{"index":0,"delta":{"content":"second-part"},"finish_reason":null}]}

data: {"id":"upstream-private-id","object":"chat.completion.chunk","created":1,"model":"upstream-private-model","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}

data: {"id":"upstream-private-id","object":"chat.completion.chunk","created":1,"model":"upstream-private-model","choices":[],"usage":{"prompt_tokens":11,"completion_tokens":7,"total_tokens":18}}

data: [DONE]

"#;
    let (proxy_address, captured, release, proxy) = spawn_streaming_proxy(first, rest);
    let (address, shutdown, server) = start_server(proxy_address, ServerConfig::default()).await;
    let request_body = br#"{"model":"test-model","max_tokens":32,"messages":[{"role":"user","content":"hello"}],"stream":true}"#;
    let mut client = connect_and_send_anthropic(address, request_body).await;

    let first_response = read_until(&mut client, b"first-part").await;
    let first_text = String::from_utf8_lossy(&first_response);
    let first_lower = first_text.to_ascii_lowercase();
    assert!(first_lower.starts_with("http/1.1 200"));
    assert!(first_lower.contains("content-type: text/event-stream; charset=utf-8"));
    assert!(first_text.contains("event: message_start"));
    assert!(first_text.contains("event: content_block_delta"));
    assert!(!first_text.contains("second-part"));
    assert!(!first_text.contains("[DONE]"));
    assert!(!first_text.contains("upstream-private-id"));

    let upstream = captured.recv_timeout(IO_TIMEOUT).unwrap();
    let upstream_json: Value = serde_json::from_slice(&upstream.body).unwrap();
    assert_eq!(upstream_json["stream"], true);
    assert_eq!(upstream_json["stream_options"]["include_usage"], true);
    assert_eq!(upstream_json["max_completion_tokens"], 32);
    release.send(()).unwrap();

    let mut complete = first_response;
    timeout(IO_TIMEOUT, client.read_to_end(&mut complete))
        .await
        .unwrap()
        .unwrap();
    let complete = String::from_utf8_lossy(&complete);
    assert!(complete.contains("second-part"));
    assert!(complete.contains("event: message_delta"));
    assert!(complete.contains("event: message_stop"));
    assert!(complete.contains("\"input_tokens\""));
    assert!(complete.contains("\"output_tokens\""));
    assert!(!complete.contains("data: [DONE]"));
    assert!(!complete.contains("upstream-private-model"));

    stop_server(shutdown, server).await;
    proxy.join().unwrap();
}

#[tokio::test]
async fn gemini_streaming_response_is_incremental_and_uses_generate_content_sse() {
    let first = br#"data: {"id":"upstream-private-id","object":"chat.completion.chunk","created":1,"model":"upstream-private-model","choices":[{"index":0,"delta":{"role":"assistant","content":"first-part"},"finish_reason":null}]}

"#;
    let rest = br#"data: {"id":"upstream-private-id","object":"chat.completion.chunk","created":1,"model":"upstream-private-model","choices":[{"index":0,"delta":{"content":"second-part"},"finish_reason":null}]}

data: {"id":"upstream-private-id","object":"chat.completion.chunk","created":1,"model":"upstream-private-model","choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}

data: {"id":"upstream-private-id","object":"chat.completion.chunk","created":1,"model":"upstream-private-model","choices":[],"usage":{"prompt_tokens":11,"completion_tokens":7,"total_tokens":18}}

data: [DONE]

"#;
    let (proxy_address, captured, release, proxy) = spawn_streaming_proxy(first, rest);
    let (address, shutdown, server) = start_server(proxy_address, ServerConfig::default()).await;
    let request_body = br#"{"contents":[{"role":"user","parts":[{"text":"hello"}]}]}"#;
    let mut client = connect_and_send_gemini(
        address,
        "/v1beta/models/test-model:streamGenerateContent?alt=sse",
        request_body,
    )
    .await;

    let first_response = read_until(&mut client, b"first-part").await;
    let first_text = String::from_utf8_lossy(&first_response);
    let first_lower = first_text.to_ascii_lowercase();
    assert!(first_lower.starts_with("http/1.1 200"));
    assert!(first_lower.contains("content-type: text/event-stream; charset=utf-8"));
    assert!(first_lower.contains("cache-control: no-cache, no-transform"));
    assert!(first_lower.contains("x-accel-buffering: no"));
    assert!(first_text.contains("data:"));
    assert!(first_text.contains("modelVersion"));
    assert!(!first_text.contains("second-part"));
    assert!(!first_text.contains("[DONE]"));
    assert!(!first_text.contains("upstream-private-id"));

    let upstream = captured.recv_timeout(IO_TIMEOUT).unwrap();
    let upstream_json: Value = serde_json::from_slice(&upstream.body).unwrap();
    assert_eq!(upstream_json["stream"], true);
    assert_eq!(upstream_json["stream_options"]["include_usage"], true);
    release.send(()).unwrap();

    let mut complete = first_response;
    timeout(IO_TIMEOUT, client.read_to_end(&mut complete))
        .await
        .unwrap()
        .unwrap();
    let complete = String::from_utf8_lossy(&complete);
    assert!(complete.contains("second-part"));
    assert!(complete.contains("usageMetadata"));
    assert!(complete.contains("promptTokenCount"));
    assert!(complete.contains("candidatesTokenCount"));
    assert!(!complete.contains("data: [DONE]"));
    assert!(!complete.contains("upstream-private-model"));

    stop_server(shutdown, server).await;
    proxy.join().unwrap();
}

#[tokio::test]
async fn client_disconnect_aborts_pending_upstream_stream() {
    let first = br#"data: {"id":"abort-private-id","object":"chat.completion.chunk","created":1,"model":"test-model","choices":[{"index":0,"delta":{"role":"assistant","content":"abort-first-part"},"finish_reason":null}]}

"#;
    let (proxy_address, captured, aborted, proxy) = spawn_abort_observing_proxy(first);
    let (address, shutdown, server) = start_server(proxy_address, ServerConfig::default()).await;
    let authorization = format!("Bearer {CLIENT_KEY}");
    let request_body =
        br#"{"model":"test-model","messages":[{"role":"user","content":"hello"}],"stream":true}"#;
    let mut client = connect_and_send(address, request_body, &authorization).await;
    let response = read_until(&mut client, b"abort-first-part").await;
    let response = String::from_utf8_lossy(&response);
    assert!(!response.contains("abort-private-id"));
    assert!(!response.contains("\"usage\""));
    let upstream = captured.recv_timeout(IO_TIMEOUT).unwrap();
    assert_ne!(upstream.body, request_body);
    let upstream_json: Value = serde_json::from_slice(&upstream.body).unwrap();
    assert_eq!(upstream_json["stream_options"]["include_usage"], true);
    drop(client);

    timeout(IO_TIMEOUT, async {
        loop {
            if aborted.try_recv().is_ok() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();

    stop_server(shutdown, server).await;
    proxy.join().unwrap();
}

fn responses_stream_chunks() -> (Vec<u8>, Vec<u8>) {
    let mut encoder = OpenAiResponsesStreamEncoder::new(
        "resp_private_upstream",
        "private-upstream-model",
        1_700_000_000,
    )
    .unwrap();
    let mut first = Vec::new();
    for event in [
        CanonicalStreamEvent::MessageStart {
            choice_index: 0,
            role: Role::Assistant,
        },
        CanonicalStreamEvent::ContentDelta {
            choice_index: 0,
            content_index: 0,
            delta: ContentDelta::Text("first-part".to_owned()),
        },
    ] {
        first.extend(encoder.encode(event).unwrap());
    }
    let usage = Usage::new(
        TokenCount::new(3).unwrap(),
        TokenCount::new(2).unwrap(),
        UsageDetails::new(
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::ZERO,
            TokenCount::ZERO,
        ),
        UsageSource::Upstream,
        UsageSemantics::Inclusive,
    )
    .unwrap();
    let mut rest = Vec::new();
    for event in [
        CanonicalStreamEvent::ContentDelta {
            choice_index: 0,
            content_index: 0,
            delta: ContentDelta::Text("second-part".to_owned()),
        },
        CanonicalStreamEvent::Finish {
            choice_index: 0,
            reason: FinishReason::Stop,
            stop_sequence: None,
        },
        CanonicalStreamEvent::Usage(usage),
        CanonicalStreamEvent::StreamEnd,
    ] {
        rest.extend(encoder.encode(event).unwrap());
    }
    (first, rest)
}

async fn connect_and_send(address: SocketAddr, body: &[u8], authorization: &str) -> TcpStream {
    let mut client = TcpStream::connect(address).await.unwrap();
    let request = format!(
        "POST /v1/chat/completions HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nAuthorization: {authorization}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    client.write_all(request.as_bytes()).await.unwrap();
    client.write_all(body).await.unwrap();
    client
}

async fn connect_and_send_responses(
    address: SocketAddr,
    body: &[u8],
    authorization: &str,
) -> TcpStream {
    let mut client = TcpStream::connect(address).await.unwrap();
    let request = format!(
        "POST /v1/responses HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nAuthorization: {authorization}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    client.write_all(request.as_bytes()).await.unwrap();
    client.write_all(body).await.unwrap();
    client
}

async fn connect_and_send_anthropic(address: SocketAddr, body: &[u8]) -> TcpStream {
    let mut client = TcpStream::connect(address).await.unwrap();
    let request = format!(
        "POST /v1/messages HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nx-api-key: {CLIENT_KEY}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    client.write_all(request.as_bytes()).await.unwrap();
    client.write_all(body).await.unwrap();
    client
}

async fn connect_and_send_gemini(address: SocketAddr, path: &str, body: &[u8]) -> TcpStream {
    let mut client = TcpStream::connect(address).await.unwrap();
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nx-goog-api-key: {CLIENT_KEY}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    client.write_all(request.as_bytes()).await.unwrap();
    client.write_all(body).await.unwrap();
    client
}

async fn read_until(client: &mut TcpStream, needle: &[u8]) -> Vec<u8> {
    timeout(IO_TIMEOUT, async {
        let mut response = Vec::new();
        let mut buffer = [0_u8; 1_024];
        while !response
            .windows(needle.len())
            .any(|window| window == needle)
        {
            let read = client.read(&mut buffer).await.unwrap();
            assert!(read > 0, "SSE 目标内容到达前连接已关闭");
            response.extend_from_slice(&buffer[..read]);
        }
        response
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn upstream_errors_are_classified_without_forwarding_private_bodies() {
    let cases = [
        (
            "429 Too Many Requests",
            br#"{"error":{"message":"upstream-rate-secret","type":"tokens","code":"rate_limit_exceeded"}}"#.as_slice(),
            429,
            "rate_limited",
        ),
        (
            "429 Too Many Requests",
            br#"{"error":{"message":"upstream-quota-secret","type":"insufficient_quota","code":"insufficient_quota"}}"#.as_slice(),
            503,
            "upstream_unavailable",
        ),
        (
            "500 Internal Server Error",
            br#"{"error":{"message":"upstream-server-secret"}}"#.as_slice(),
            503,
            "upstream_unavailable",
        ),
        (
            "200 OK",
            br#"{"private":"invalid-success-secret"}"#.as_slice(),
            503,
            "upstream_unavailable",
        ),
    ];

    for (status_line, upstream_body, expected_status, expected_code) in cases {
        let (proxy_address, captured, proxy) = spawn_proxy(
            status_line,
            upstream_body,
            &[("Content-Type", "application/json")],
        );
        let (address, shutdown, server) =
            start_server(proxy_address, ServerConfig::default()).await;
        let authorization = format!("Bearer {CLIENT_KEY}");
        let response = send_request(
            address,
            "POST",
            "/v1/chat/completions",
            br#"{"model":"test-model","messages":[{"role":"user","content":"hello"}]}"#,
            &[
                ("Content-Type", "application/json"),
                ("Authorization", &authorization),
            ],
        )
        .await;
        stop_server(shutdown, server).await;
        captured.recv_timeout(IO_TIMEOUT).unwrap();
        proxy.join().unwrap();

        assert_eq!(response.status, expected_status);
        let value: Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(value["error"]["code"], expected_code);
        let response_text = String::from_utf8_lossy(&response.body);
        for secret in [
            "upstream-rate-secret",
            "upstream-quota-secret",
            "upstream-server-secret",
            "invalid-success-secret",
        ] {
            assert!(!response_text.contains(secret));
        }
    }
}

async fn start_server(
    proxy_address: SocketAddr,
    server_config: ServerConfig,
) -> (
    SocketAddr,
    oneshot::Sender<()>,
    tokio::task::JoinHandle<Result<ServeOutcome, af_http::ServeError>>,
) {
    let proxy = ProxyConfig::parse(format!("http://{proxy_address}")).unwrap();
    let client_config = HttpClientConfig::new(proxy, HttpTimeouts::default())
        .with_remote_dns_policy(RemoteDnsPolicy::TrustProxy);
    let client = HttpClientPool::default().get(&client_config).unwrap();
    let relay = Arc::new(
        RelayService::new(client, UPSTREAM_BASE_URL, UPSTREAM_MODEL, UPSTREAM_KEY).unwrap(),
    );
    let readiness = ReadinessHandle::new(AlwaysReadyProbe);
    readiness.mark_ready();
    let router = build_router(
        &server_config,
        relay,
        Arc::new(RejectAudio),
        Arc::new(RejectEmbeddings),
        Arc::new(RejectImages),
        Arc::new(RejectRerank),
        None,
        Arc::new(RejectSpeech),
        Arc::new(RejectResponsesCompact),
        readiness,
        Arc::new(AlwaysAuthenticates),
        Arc::new(RejectSessions),
        Arc::new(RejectModelCatalog),
        Arc::new(RejectPlaygroundShares),
        Arc::new(RejectPlaygroundConversations),
        Arc::new(RejectUserTokens),
        Arc::new(RejectInitialSetup),
        Arc::new(RejectRegistration),
        None,
        Arc::new(RejectPasswordReset),
        Arc::new(RejectUserProfile),
        Arc::new(RejectUserWallet),
        Arc::new(RejectUserNotifications),
        None,
        None,
        Arc::new(RejectRedemptions),
        Arc::new(RejectSubscriptions),
        Arc::new(RejectUserInvitations),
        Arc::new(RejectSiteSettings),
        Arc::new(RejectAnnouncements),
        None,
        None,
        None,
        None,
        None,
        Arc::new(RejectAdminEmailSettings),
        Arc::new(RejectAdminNetworkSettings),
        Arc::new(RejectAdminPaymentSettings),
        Arc::new(RejectAdminCredentialProxies),
        None,
        Arc::new(RejectAdminBalanceAlertSettings),
        Arc::new(RejectAdminDebugTraces),
        Arc::new(RejectAdminChannels),
        Arc::new(RejectAdminChannels),
        None,
        None,
        Arc::new(RejectAdminGroups),
        Arc::new(RejectAdminGroups),
        None,
        None,
        None,
        None,
        None,
        None,
        Arc::new(RejectAdminTokens),
        Arc::new(RejectAdminTokens),
        Arc::new(RejectAdminDashboard),
        None,
        Arc::new(RejectAdminUsageLogs),
        Arc::new(RejectAdminUsers),
        Arc::new(RejectAdminUsers),
        None,
        None,
        QueryApiKeyPolicy::Deny,
        None,
        None,
        None,
    );
    let listener = HttpListener::bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let address = listener.local_addr();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let server = tokio::spawn(serve_with_graceful_shutdown(
        listener,
        router,
        async move {
            let _ = shutdown_rx.await;
        },
        Duration::from_secs(1),
    ));
    (address, shutdown_tx, server)
}

async fn stop_server(
    shutdown: oneshot::Sender<()>,
    server: tokio::task::JoinHandle<Result<ServeOutcome, af_http::ServeError>>,
) {
    shutdown.send(()).unwrap();
    assert_eq!(
        timeout(IO_TIMEOUT, server).await.unwrap().unwrap().unwrap(),
        ServeOutcome::Drained
    );
}
