use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use af_admin::{
    AdminSiteSettings, AdminSiteSettingsReadFuture, AdminSiteSettingsUpdateFuture,
    PublicSiteSettings, PublicSiteSettingsFuture, RegistrationCommand,
    RegistrationEmailVerificationCommand, RegistrationEmailVerificationFuture, RegistrationError,
    RegistrationFuture, RegistrationPolicyCommand, RegistrationPolicyFuture,
    RegistrationPolicyUpdateFuture, RegistrationService, RegistrationStatus,
    RegistrationStatusFuture, SessionAuthentication, SessionAuthenticationError,
    SessionAuthenticationFuture, SessionAuthenticator, SessionLoginFuture, SessionPrincipal,
    SessionRole, SiteNavigationRecord, SiteSettingsCommand, SiteSettingsService,
};
use af_domain::{GroupId, TrustedClientIp, UserId};
use axum::body::{Body, to_bytes};
use http::{
    Request, StatusCode,
    header::{AUTHORIZATION, CACHE_CONTROL, CONTENT_TYPE},
};
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::site_settings::build_site_settings_router;

const ADMIN_TOKEN: &str = "admin-token";
const USER_TOKEN: &str = "user-token";

struct FakeSiteSettingsService {
    admin_calls: AtomicUsize,
    update_calls: AtomicUsize,
}

impl FakeSiteSettingsService {
    fn public() -> PublicSiteSettings {
        PublicSiteSettings::new(
            "AnyFlows Cloud".to_owned(),
            Some("https://example.com".to_owned()),
            Some("/brand.svg".to_owned()),
            Some("统一访问模型".to_owned()),
            Some("面向团队的模型 API 工作台。".to_owned()),
        )
        .unwrap()
    }

    fn admin() -> AdminSiteSettings {
        AdminSiteSettings::new(Self::public(), 3).unwrap()
    }
}

impl SiteSettingsService for FakeSiteSettingsService {
    fn public_settings(&self) -> PublicSiteSettingsFuture<'_> {
        Box::pin(async { Ok(Self::public()) })
    }

    fn admin_settings(&self, _principal: SessionPrincipal) -> AdminSiteSettingsReadFuture<'_> {
        self.admin_calls.fetch_add(1, Ordering::Relaxed);
        Box::pin(async { Ok(Self::admin()) })
    }

    fn update(
        &self,
        _principal: SessionPrincipal,
        _command: SiteSettingsCommand,
    ) -> AdminSiteSettingsUpdateFuture<'_> {
        self.update_calls.fetch_add(1, Ordering::Relaxed);
        Box::pin(async { Ok(Self::admin()) })
    }

    fn update_navigation(
        &self,
        _principal: SessionPrincipal,
        _navigation: SiteNavigationRecord,
        _expected_version: i64,
    ) -> AdminSiteSettingsUpdateFuture<'_> {
        self.update_calls.fetch_add(1, Ordering::Relaxed);
        Box::pin(async { Ok(Self::admin()) })
    }
}

struct FakeRegistrationService;

impl RegistrationService for FakeRegistrationService {
    fn status(&self) -> RegistrationStatusFuture<'_> {
        Box::pin(async { Ok(RegistrationStatus::from_parts(true, true, false)) })
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

struct FakeSessionAuthenticator;

impl SessionAuthenticator for FakeSessionAuthenticator {
    fn login<'a>(&'a self, _credentials: &'a af_admin::LoginCredentials) -> SessionLoginFuture<'a> {
        Box::pin(async { Err(SessionAuthenticationError::InvalidCredentials) })
    }

    fn authenticate<'a>(&'a self, token: &'a str) -> SessionAuthenticationFuture<'a> {
        let principal = match token {
            ADMIN_TOKEN => Some(SessionPrincipal::new(
                UserId::new(1).unwrap(),
                SessionRole::Admin,
            )),
            USER_TOKEN => Some(SessionPrincipal::new(
                UserId::new(2).unwrap(),
                SessionRole::User,
            )),
            _ => None,
        };
        Box::pin(async move {
            principal
                .map(|principal| {
                    SessionAuthentication::new(
                        principal,
                        GroupId::new(3).unwrap(),
                        current_timestamp() + 60,
                    )
                })
                .ok_or(SessionAuthenticationError::InvalidSession)
        })
    }
}

fn router(service: Arc<FakeSiteSettingsService>) -> axum::Router {
    let site_service: Arc<dyn SiteSettingsService> = service;
    let registration_service: Arc<dyn RegistrationService> = Arc::new(FakeRegistrationService);
    let authenticator: Arc<dyn SessionAuthenticator> = Arc::new(FakeSessionAuthenticator);
    build_site_settings_router(
        site_service,
        registration_service,
        None,
        None,
        authenticator,
    )
}

#[tokio::test]
async fn public_projection_exposes_only_brand_and_authentication_capabilities() {
    let response = router(Arc::new(FakeSiteSettingsService {
        admin_calls: AtomicUsize::new(0),
        update_calls: AtomicUsize::new(0),
    }))
    .oneshot(Request::get("/api/site").body(Body::empty()).unwrap())
    .await
    .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[CACHE_CONTROL], "no-store");
    let body = response_json(response).await;
    assert_eq!(body["site_name"], "AnyFlows Cloud");
    assert_eq!(body["brand"]["logo_url"], "/brand.svg");
    assert_eq!(
        body["navigation"],
        json!({"header_links": [], "footer_groups": [], "sidebar_links": []})
    );
    assert_eq!(body["balance_display"]["mode"], "quota");
    assert_eq!(
        body["balance_display"]["quota_units_per_display_unit"],
        "10000"
    );
    assert_eq!(body["authentication"]["password_login_enabled"], true);
    assert_eq!(body["authentication"]["registration_enabled"], true);
    assert_eq!(body["authentication"]["oauth_providers"], json!([]));
    assert_eq!(body["authentication"]["turnstile_site_key"], Value::Null);
    assert!(body.get("version").is_none());
    assert!(body.get("default_group_id").is_none());
}

#[tokio::test]
async fn admin_projection_rejects_normal_users_before_service_access() {
    let service = Arc::new(FakeSiteSettingsService {
        admin_calls: AtomicUsize::new(0),
        update_calls: AtomicUsize::new(0),
    });
    let app = router(Arc::clone(&service));

    let forbidden = app
        .clone()
        .oneshot(json_request(
            "GET",
            "/api/admin/site-settings",
            Value::Null,
            USER_TOKEN,
        ))
        .await
        .unwrap();
    assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);
    assert_eq!(service.admin_calls.load(Ordering::Relaxed), 0);

    let updated = app
        .oneshot(json_request(
            "PUT",
            "/api/admin/site-settings",
            json!({
                "site_name": "AnyFlows Cloud",
                "public_base_url": "https://example.com",
                "brand": {
                    "logo_url": "/brand.svg",
                    "tagline": "统一访问模型",
                    "description": "面向团队的模型 API 工作台。"
                },
                "balance_display": {
                    "mode": "custom_unit",
                    "unit_name": "算力积分",
                    "unit_symbol": "积分",
                    "quota_units_per_display_unit": "10000",
                    "symbol_position": "suffix",
                    "fraction_digits": 2
                },
                "expected_version": 3
            }),
            ADMIN_TOKEN,
        ))
        .await
        .unwrap();
    assert_eq!(updated.status(), StatusCode::OK);
    assert_eq!(response_json(updated).await["version"], 3);
    assert_eq!(service.update_calls.load(Ordering::Relaxed), 1);

    let forbidden_navigation = router(Arc::clone(&service))
        .oneshot(json_request(
            "PUT",
            "/api/admin/site-settings/navigation",
            json!({
                "navigation": {"header_links": [], "footer_groups": [], "sidebar_links": []},
                "expected_version": 3
            }),
            USER_TOKEN,
        ))
        .await
        .unwrap();
    assert_eq!(forbidden_navigation.status(), StatusCode::FORBIDDEN);
    assert_eq!(service.update_calls.load(Ordering::Relaxed), 1);

    let navigation = router(Arc::clone(&service))
        .oneshot(json_request("PUT", "/api/admin/site-settings/navigation", json!({
            "navigation": {"header_links": [{"label": "Docs", "label_en": null, "url": "/api"}], "footer_groups": [], "sidebar_links": []},
            "expected_version": 3
        }), ADMIN_TOKEN)).await.unwrap();
    assert_eq!(navigation.status(), StatusCode::OK);
    assert_eq!(service.update_calls.load(Ordering::Relaxed), 2);
}

fn json_request(method: &str, uri: &str, body: Value, token: &str) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(AUTHORIZATION, format!("Bearer {token}"));
    if method != "GET" {
        builder = builder.header(CONTENT_TYPE, "application/json");
    }
    builder
        .body(if method == "GET" {
            Body::empty()
        } else {
            Body::from(body.to_string())
        })
        .unwrap()
}

async fn response_json(response: axum::response::Response) -> Value {
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&body).unwrap()
}

fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
