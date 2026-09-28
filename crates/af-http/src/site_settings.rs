use std::sync::Arc;

use af_admin::{
    AdminSiteSettings, BalanceDisplayMode, BalanceDisplayPolicy, BalanceSymbolPosition,
    OAuthLoginError, OAuthLoginService, PublicOAuthLoginProvider, PublicSiteSettings,
    RegistrationError, RegistrationService, SessionAuthenticator, SiteNavigationGroupRecord,
    SiteNavigationLinkRecord, SiteNavigationRecord, SiteSettingsCommand, SiteSettingsError,
    SiteSettingsService, SiteSidebarLinkRecord,
};
use axum::{
    Extension, Json, Router,
    extract::{State, rejection::JsonRejection},
    middleware,
    response::Response,
    routing::get,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_authorization::authorize_management_admin,
    management_error::ManagementError,
    management_session::no_store_json,
};

#[derive(Clone)]
struct SiteSettingsHttpState {
    site_service: Arc<dyn SiteSettingsService>,
    registration_service: Arc<dyn RegistrationService>,
    oauth_login_service: Option<Arc<dyn OAuthLoginService>>,
    turnstile_site_key: Option<String>,
}

#[derive(Clone, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = PublicBrandSettings)]
pub(crate) struct PublicBrandSettingsResponse {
    #[schema(max_length = 2048, format = "uri")]
    logo_url: Option<String>,
    #[schema(max_length = 160)]
    tagline: Option<String>,
    #[schema(max_length = 500)]
    description: Option<String>,
}

#[derive(Clone, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = PublicAuthenticationCapabilities)]
pub(crate) struct PublicAuthenticationCapabilitiesResponse {
    password_login_enabled: bool,
    registration_enabled: bool,
    registration_email_required: bool,
    oauth_providers: Vec<PublicOAuthLoginProviderResponse>,
    #[schema(max_length = 256)]
    turnstile_site_key: Option<String>,
}

#[derive(Clone, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = PublicOAuthLoginProvider)]
pub(crate) struct PublicOAuthLoginProviderResponse {
    #[schema(max_length = 32)]
    id: String,
    #[schema(max_length = 128)]
    display_name: String,
}

impl From<PublicOAuthLoginProvider> for PublicOAuthLoginProviderResponse {
    fn from(provider: PublicOAuthLoginProvider) -> Self {
        Self {
            id: provider.id().to_owned(),
            display_name: provider.display_name().to_owned(),
        }
    }
}

#[derive(Clone, Copy, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = BalanceDisplayMode)]
pub(crate) enum BalanceDisplayModeDto {
    Quota,
    CustomUnit,
}

#[derive(Clone, Copy, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = BalanceDisplaySymbolPosition)]
pub(crate) enum BalanceDisplaySymbolPositionDto {
    Prefix,
    Suffix,
}

#[derive(Clone, Deserialize, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = BalanceDisplaySettings)]
pub(crate) struct BalanceDisplaySettingsDto {
    mode: BalanceDisplayModeDto,
    #[schema(min_length = 1, max_length = 64)]
    unit_name: String,
    #[schema(min_length = 1, max_length = 24)]
    unit_symbol: String,
    #[schema(pattern = r"^[1-9][0-9]{0,18}$", example = "10000")]
    quota_units_per_display_unit: String,
    symbol_position: BalanceDisplaySymbolPositionDto,
    #[schema(minimum = 0, maximum = 4)]
    fraction_digits: u8,
}

impl From<&BalanceDisplayPolicy> for BalanceDisplaySettingsDto {
    fn from(policy: &BalanceDisplayPolicy) -> Self {
        Self {
            mode: match policy.mode() {
                BalanceDisplayMode::Quota => BalanceDisplayModeDto::Quota,
                BalanceDisplayMode::CustomUnit => BalanceDisplayModeDto::CustomUnit,
            },
            unit_name: policy.unit_name().to_owned(),
            unit_symbol: policy.unit_symbol().to_owned(),
            quota_units_per_display_unit: policy.quota_units_per_display_unit().to_string(),
            symbol_position: match policy.symbol_position() {
                BalanceSymbolPosition::Prefix => BalanceDisplaySymbolPositionDto::Prefix,
                BalanceSymbolPosition::Suffix => BalanceDisplaySymbolPositionDto::Suffix,
            },
            fraction_digits: policy.fraction_digits(),
        }
    }
}

impl BalanceDisplaySettingsDto {
    fn into_policy(self) -> Result<BalanceDisplayPolicy, SiteSettingsError> {
        let quota_units_per_display_unit = self
            .quota_units_per_display_unit
            .parse::<i64>()
            .map_err(|_| SiteSettingsError::InvalidInput)?;
        BalanceDisplayPolicy::new(
            match self.mode {
                BalanceDisplayModeDto::Quota => BalanceDisplayMode::Quota,
                BalanceDisplayModeDto::CustomUnit => BalanceDisplayMode::CustomUnit,
            },
            self.unit_name,
            self.unit_symbol,
            quota_units_per_display_unit,
            match self.symbol_position {
                BalanceDisplaySymbolPositionDto::Prefix => BalanceSymbolPosition::Prefix,
                BalanceDisplaySymbolPositionDto::Suffix => BalanceSymbolPosition::Suffix,
            },
            self.fraction_digits,
        )
    }
}

#[derive(Clone, Deserialize, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = SiteNavigationLink)]
pub(crate) struct SiteNavigationLinkDto {
    #[schema(min_length = 1, max_length = 64)]
    label: String,
    #[schema(max_length = 64)]
    label_en: Option<String>,
    #[schema(min_length = 1, max_length = 2048)]
    url: String,
}

#[derive(Clone, Deserialize, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = SiteNavigationGroup)]
pub(crate) struct SiteNavigationGroupDto {
    #[schema(min_length = 1, max_length = 64)]
    title: String,
    #[schema(max_length = 64)]
    title_en: Option<String>,
    #[schema(min_items = 1, max_items = 6)]
    links: Vec<SiteNavigationLinkDto>,
}

#[derive(Clone, Deserialize, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = SiteSidebarLink)]
pub(crate) struct SiteSidebarLinkDto {
    #[schema(min_length = 1, max_length = 64)]
    label: String,
    #[schema(max_length = 64)]
    label_en: Option<String>,
    #[schema(min_length = 0, max_length = 2048)]
    url: String,
    #[schema(
        pattern = "^(link|globe|book|sparkles|building|message|headphones|shield|chart|home|dashboard|settings|users|user|briefcase|calendar|card|key|lock|file|folder|help|info|bell|mail|phone|map|database|server|code|terminal|bot|cpu|workflow|gauge|rocket|megaphone|shopping|monitor|cloud|bookmark|graduation|newspaper|clipboard|list|wrench|search|star|heart|zap|command|panels)$"
    )]
    icon: String,
    #[serde(default = "default_sidebar_kind")]
    #[schema(pattern = "^(link|group)$")]
    kind: String,
    #[serde(default = "default_sidebar_level")]
    #[schema(minimum = 1, maximum = 3)]
    level: u8,
    #[serde(default = "default_sidebar_style")]
    #[schema(pattern = "^(default|accent)$")]
    style: String,
    #[schema(pattern = "^(all|admin)$")]
    audience: String,
}

fn default_sidebar_level() -> u8 {
    1
}

fn default_sidebar_kind() -> String {
    "link".to_owned()
}

fn default_sidebar_style() -> String {
    "default".to_owned()
}

#[derive(Clone, Deserialize, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = SiteNavigation)]
pub(crate) struct SiteNavigationDto {
    #[schema(max_items = 8)]
    header_links: Vec<SiteNavigationLinkDto>,
    #[schema(max_items = 4)]
    footer_groups: Vec<SiteNavigationGroupDto>,
    #[schema(max_items = 24)]
    sidebar_links: Vec<SiteSidebarLinkDto>,
}

impl From<&SiteNavigationRecord> for SiteNavigationDto {
    fn from(navigation: &SiteNavigationRecord) -> Self {
        let link = |item: &SiteNavigationLinkRecord| SiteNavigationLinkDto {
            label: item.label.clone(),
            label_en: item.label_en.clone(),
            url: item.url.clone(),
        };
        Self {
            header_links: navigation.header_links.iter().map(link).collect(),
            footer_groups: navigation
                .footer_groups
                .iter()
                .map(|group| SiteNavigationGroupDto {
                    title: group.title.clone(),
                    title_en: group.title_en.clone(),
                    links: group.links.iter().map(link).collect(),
                })
                .collect(),
            sidebar_links: navigation
                .sidebar_links
                .iter()
                .map(|item| SiteSidebarLinkDto {
                    label: item.label.clone(),
                    label_en: item.label_en.clone(),
                    url: item.url.clone(),
                    icon: item.icon.clone(),
                    kind: item.kind.clone(),
                    level: item.level,
                    style: item.style.clone(),
                    audience: item.audience.clone(),
                })
                .collect(),
        }
    }
}

impl From<SiteNavigationDto> for SiteNavigationRecord {
    fn from(navigation: SiteNavigationDto) -> Self {
        let link = |item: SiteNavigationLinkDto| SiteNavigationLinkRecord {
            label: item.label,
            label_en: item.label_en,
            url: item.url,
        };
        Self {
            header_links: navigation.header_links.into_iter().map(link).collect(),
            footer_groups: navigation
                .footer_groups
                .into_iter()
                .map(|group| SiteNavigationGroupRecord {
                    title: group.title,
                    title_en: group.title_en,
                    links: group.links.into_iter().map(link).collect(),
                })
                .collect(),
            sidebar_links: navigation
                .sidebar_links
                .into_iter()
                .map(|item| SiteSidebarLinkRecord {
                    label: item.label,
                    label_en: item.label_en,
                    url: item.url,
                    icon: item.icon,
                    kind: item.kind,
                    level: item.level,
                    style: item.style,
                    audience: item.audience,
                })
                .collect(),
        }
    }
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminSiteNavigationRequest)]
pub(crate) struct AdminSiteNavigationRequest {
    navigation: SiteNavigationDto,
    #[schema(minimum = 1)]
    expected_version: i64,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = PublicSiteSettings)]
pub(crate) struct PublicSiteSettingsResponse {
    #[schema(min_length = 1, max_length = 80)]
    site_name: String,
    #[schema(max_length = 2048, format = "uri")]
    public_base_url: Option<String>,
    brand: PublicBrandSettingsResponse,
    navigation: SiteNavigationDto,
    balance_display: BalanceDisplaySettingsDto,
    authentication: PublicAuthenticationCapabilitiesResponse,
}

#[derive(Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminSiteSettings)]
pub(crate) struct AdminSiteSettingsResponse {
    #[schema(min_length = 1, max_length = 80)]
    site_name: String,
    #[schema(max_length = 2048, format = "uri")]
    public_base_url: Option<String>,
    brand: PublicBrandSettingsResponse,
    navigation: SiteNavigationDto,
    balance_display: BalanceDisplaySettingsDto,
    #[schema(minimum = 1)]
    version: i64,
}

impl From<AdminSiteSettings> for AdminSiteSettingsResponse {
    fn from(settings: AdminSiteSettings) -> Self {
        Self {
            site_name: settings.site_name().to_owned(),
            public_base_url: settings.public_base_url().map(str::to_owned),
            brand: PublicBrandSettingsResponse {
                logo_url: settings.brand_logo_url().map(str::to_owned),
                tagline: settings.brand_tagline().map(str::to_owned),
                description: settings.brand_description().map(str::to_owned),
            },
            navigation: SiteNavigationDto::from(settings.navigation()),
            balance_display: BalanceDisplaySettingsDto::from(settings.balance_display()),
            version: settings.version(),
        }
    }
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminSiteSettingsRequest)]
pub(crate) struct AdminSiteSettingsRequest {
    #[schema(min_length = 1, max_length = 80)]
    site_name: String,
    #[schema(max_length = 2048, format = "uri")]
    public_base_url: Option<String>,
    brand: AdminBrandSettingsRequest,
    balance_display: BalanceDisplaySettingsDto,
    #[schema(minimum = 1)]
    expected_version: i64,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AdminBrandSettingsRequest)]
pub(crate) struct AdminBrandSettingsRequest {
    #[schema(max_length = 2048, format = "uri")]
    logo_url: Option<String>,
    #[schema(max_length = 160)]
    tagline: Option<String>,
    #[schema(max_length = 500)]
    description: Option<String>,
}

/// 构建公开站点投影与管理员站点设置路由。
pub(crate) fn build_site_settings_router(
    site_service: Arc<dyn SiteSettingsService>,
    registration_service: Arc<dyn RegistrationService>,
    oauth_login_service: Option<Arc<dyn OAuthLoginService>>,
    turnstile_site_key: Option<&str>,
    session_authenticator: Arc<dyn SessionAuthenticator>,
) -> Router {
    let authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(session_authenticator),
        authenticate_management_session,
    );
    Router::new()
        .route("/api/site", get(get_public_site_settings))
        .route(
            "/api/admin/site-settings",
            get(get_admin_site_settings)
                .put(update_admin_site_settings)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(authentication.clone()),
        )
        .route(
            "/api/admin/site-settings/navigation",
            axum::routing::put(update_admin_site_navigation)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(authentication),
        )
        .with_state(SiteSettingsHttpState {
            site_service,
            registration_service,
            oauth_login_service,
            turnstile_site_key: turnstile_site_key.map(str::to_owned),
        })
}

/// 组合站点与认证域的最小公开投影，任一域损坏时整次失败关闭。
async fn get_public_site_settings(
    State(state): State<SiteSettingsHttpState>,
) -> Result<Response, ManagementError> {
    let oauth_providers = async {
        match state.oauth_login_service.as_ref() {
            Some(service) => service.public_providers().await,
            None => Ok(Vec::new()),
        }
    };
    let (site, authentication, oauth_providers) = tokio::join!(
        state.site_service.public_settings(),
        state.registration_service.status(),
        oauth_providers,
    );
    let site = site.map_err(map_site_settings_error)?;
    let authentication = authentication.map_err(map_registration_error)?;
    let oauth_providers = oauth_providers.map_err(map_oauth_login_error)?;
    Ok(no_store_json(public_response(
        site,
        authentication,
        oauth_providers,
        state.turnstile_site_key,
    )))
}

async fn get_admin_site_settings(
    State(state): State<SiteSettingsHttpState>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
) -> Result<Response, ManagementError> {
    let settings = state
        .site_service
        .admin_settings(authentication.principal())
        .await
        .map_err(map_site_settings_error)?;
    Ok(no_store_json(AdminSiteSettingsResponse::from(settings)))
}

async fn update_admin_site_settings(
    State(state): State<SiteSettingsHttpState>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
    request: Result<Json<AdminSiteSettingsRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let command = SiteSettingsCommand::new(
        request.site_name,
        request.public_base_url,
        request.brand.logo_url,
        request.brand.tagline,
        request.brand.description,
        request
            .balance_display
            .into_policy()
            .map_err(map_site_settings_error)?,
        request.expected_version,
    )
    .map_err(map_site_settings_error)?;
    let settings = state
        .site_service
        .update(authentication.principal(), command)
        .await
        .map_err(map_site_settings_error)?;
    Ok(no_store_json(AdminSiteSettingsResponse::from(settings)))
}

async fn update_admin_site_navigation(
    State(state): State<SiteSettingsHttpState>,
    Extension(authentication): Extension<af_admin::SessionAuthentication>,
    request: Result<Json<AdminSiteNavigationRequest>, JsonRejection>,
) -> Result<Response, ManagementError> {
    let Json(request) = request.map_err(|_| ManagementError::InvalidRequest)?;
    let settings = state
        .site_service
        .update_navigation(
            authentication.principal(),
            request.navigation.into(),
            request.expected_version,
        )
        .await
        .map_err(map_site_settings_error)?;
    Ok(no_store_json(AdminSiteSettingsResponse::from(settings)))
}

fn public_response(
    site: PublicSiteSettings,
    authentication: af_admin::RegistrationStatus,
    oauth_providers: Vec<PublicOAuthLoginProvider>,
    turnstile_site_key: Option<String>,
) -> PublicSiteSettingsResponse {
    PublicSiteSettingsResponse {
        site_name: site.site_name().to_owned(),
        public_base_url: site.public_base_url().map(str::to_owned),
        brand: PublicBrandSettingsResponse {
            logo_url: site.brand_logo_url().map(str::to_owned),
            tagline: site.brand_tagline().map(str::to_owned),
            description: site.brand_description().map(str::to_owned),
        },
        navigation: SiteNavigationDto::from(site.navigation()),
        balance_display: BalanceDisplaySettingsDto::from(site.balance_display()),
        authentication: PublicAuthenticationCapabilitiesResponse {
            password_login_enabled: authentication.password_login_enabled(),
            registration_enabled: authentication.enabled(),
            registration_email_required: authentication.email_required(),
            oauth_providers: oauth_providers
                .into_iter()
                .map(PublicOAuthLoginProviderResponse::from)
                .collect(),
            turnstile_site_key,
        },
    }
}

fn map_oauth_login_error(error: OAuthLoginError) -> ManagementError {
    match error {
        OAuthLoginError::InvalidInput => ManagementError::InvalidRequest,
        OAuthLoginError::Forbidden => ManagementError::Forbidden,
        OAuthLoginError::Unavailable => ManagementError::OauthProviderNotConfigured,
        OAuthLoginError::Rejected => ManagementError::OauthAuthorizationDenied,
        OAuthLoginError::ConcurrentUpdate => ManagementError::OauthLoginSettingsConflict,
        OAuthLoginError::ProviderUnavailable => ManagementError::OauthUnavailable,
        OAuthLoginError::Internal => ManagementError::Internal,
    }
}

fn map_site_settings_error(error: SiteSettingsError) -> ManagementError {
    match error {
        SiteSettingsError::InvalidInput => ManagementError::InvalidRequest,
        SiteSettingsError::Forbidden => ManagementError::Forbidden,
        SiteSettingsError::Internal => ManagementError::Internal,
        SiteSettingsError::Conflict => ManagementError::SiteSettingsConflict,
    }
}

fn map_registration_error(error: RegistrationError) -> ManagementError {
    match error {
        RegistrationError::InvalidInput => ManagementError::InvalidRequest,
        RegistrationError::Forbidden => ManagementError::Forbidden,
        RegistrationError::LoginDisabled => ManagementError::LoginDisabled,
        RegistrationError::Disabled => ManagementError::RegistrationDisabled,
        RegistrationError::Conflict => ManagementError::UserConflict,
        RegistrationError::VerificationRejected => ManagementError::RegistrationRejected,
        RegistrationError::InvitationRejected => ManagementError::Internal,
        RegistrationError::RateLimited {
            retry_after_seconds,
        } => ManagementError::RegistrationRateLimited {
            retry_after_seconds,
        },
        RegistrationError::EmailNotConfigured => ManagementError::EmailNotConfigured,
        RegistrationError::EmailDeliveryFailed => ManagementError::EmailDeliveryFailed,
        RegistrationError::Internal => ManagementError::Internal,
    }
}
