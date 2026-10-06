use std::{sync::Arc, time::Duration};

use af_account::SystemSecretCipher;
use af_admin::{
    AccountVerificationService, AdminGroupCreateCommand, AdminGroupWriter, AdminUserCreateCommand,
    AdminUserStatus, AdminUserWriter, DatabaseAdminGroupWriter, DatabaseAdminUserWriter,
    DatabasePlatformAuditService, LoginCredentials, SessionAuthentication,
    SessionAuthenticationError, SessionAuthenticationFuture, SessionAuthenticator,
    SessionLoginFuture, SessionPrincipal, SessionRole,
};
use af_db::{
    AccountVerificationRepository, AdminGroupRepository, AdminUserRepository, DatabaseOptions,
    MigrationOptions, PlatformAuditRepository,
};
use af_domain::{GroupId, UserId};
use af_http::{
    Body, Router, build_account_verification_router, build_verification_settings_router,
    http::{Method, Request, StatusCode},
    to_bytes,
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde_json::{Value, json};
use tower::ServiceExt as _;

struct Sessions {
    group: GroupId,
    users: [UserId; 3],
}

impl SessionAuthenticator for Sessions {
    fn login<'a>(&'a self, _: &'a LoginCredentials) -> SessionLoginFuture<'a> {
        Box::pin(async { Err(SessionAuthenticationError::InvalidCredentials) })
    }
    fn authenticate<'a>(&'a self, token: &'a str) -> SessionAuthenticationFuture<'a> {
        Box::pin(async move {
            let (index, role) = match token {
                "applicant" => (0, SessionRole::User),
                "other" => (1, SessionRole::User),
                "admin" => (2, SessionRole::Admin),
                _ => return Err(SessionAuthenticationError::InvalidSession),
            };
            Ok(SessionAuthentication::new(
                SessionPrincipal::new(self.users[index], role),
                self.group,
                4_102_444_800,
            ))
        })
    }
}

async fn fixture() -> Result<(Router, af_db::DatabasePool), Box<dyn std::error::Error>> {
    fixture_with_provider(None).await
}

async fn fixture_with_provider(
    provider: Option<Arc<dyn af_admin::AccountVerificationProvider>>,
) -> Result<(Router, af_db::DatabasePool), Box<dyn std::error::Error>> {
    let pool = af_db::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let setup = SessionPrincipal::new(UserId::new(900)?, SessionRole::Admin);
    let groups = DatabaseAdminGroupWriter::new(AdminGroupRepository::new(
        pool.clone(),
        Duration::from_secs(5),
    )?);
    let group = groups
        .create(
            setup,
            AdminGroupCreateCommand::new(
                "verification-http".to_owned(),
                "Verification HTTP".to_owned(),
                1_000_000,
                None,
                false,
                None,
                None,
                None,
                None,
                None,
                json!({}),
            )?,
        )
        .await?;
    let users = DatabaseAdminUserWriter::new(AdminUserRepository::new(
        pool.clone(),
        Duration::from_secs(5),
    )?);
    let mut ids = Vec::new();
    for index in 0..3 {
        let user = users
            .create(
                setup,
                AdminUserCreateCommand::new(
                    format!("verification-http-{index}"),
                    None,
                    None,
                    if index == 2 {
                        SessionRole::Admin
                    } else {
                        SessionRole::User
                    },
                    AdminUserStatus::Enabled,
                    group.group_id(),
                    0,
                    None,
                    None,
                )?,
            )
            .await?;
        ids.push(user.user_id());
    }
    let sessions = Arc::new(Sessions {
        group: group.group_id(),
        users: ids.try_into().expect("three users"),
    });
    let audit = Arc::new(DatabasePlatformAuditService::new(
        PlatformAuditRepository::new(pool.clone()),
    ));
    let encryption = serde_json::from_value(json!({
        "key_id": "verification-http-test-key",
        "key": URL_SAFE_NO_PAD.encode([0x42; 32])
    }))?;
    let settings = Arc::new(af_admin::DatabaseVerificationSettingsService::new(
        af_db::AccountVerificationSettingsRepository::new(pool.clone(), Duration::from_secs(5)),
        SystemSecretCipher::new(&encryption)?,
        af_config::AlipayVerificationSettings::default(),
        af_httpclient::HttpClientProvider::new(af_httpclient::HttpClientConfig::default(), 1)?,
    ));
    let mut service =
        AccountVerificationService::new(AccountVerificationRepository::new(pool.clone()), audit)
            .with_provider(Arc::new(
                af_admin::DatabaseManualAccountVerificationProvider::new(settings.clone()),
            ))
            .with_provider(settings.clone())
            .with_settings(settings.clone());
    if let Some(provider) = provider {
        service = service.with_provider(provider);
    }
    let service = Arc::new(service);
    Ok((
        build_account_verification_router(service, sessions.clone())
            .merge(build_verification_settings_router(settings, sessions)),
        pool,
    ))
}

struct CertDocCallbackProvider;

#[async_trait::async_trait]
impl af_admin::AccountVerificationProvider for CertDocCallbackProvider {
    fn key(&self) -> &'static str {
        "alipay"
    }
    fn configured(&self) -> bool {
        true
    }

    async fn initialize(
        &self,
        _: &af_db::AccountVerificationProviderRequest,
    ) -> Result<af_db::AccountVerificationProviderStart, af_admin::AccountVerificationProviderError>
    {
        Ok(af_db::AccountVerificationProviderStart {
            reference: "verify-test.0123456789abcdef0123456789abcdef".to_owned(),
            action_url: "https://openauth.alipay.com/oauth2/publicAppAuthorize.htm".to_owned(),
            status: "pending".to_owned(),
        })
    }

    async fn complete(
        &self,
        reference: &str,
        authorization_code: &str,
    ) -> Result<af_db::AccountVerificationProviderResult, af_admin::AccountVerificationProviderError>
    {
        assert_eq!(reference, "verify-test.0123456789abcdef0123456789abcdef");
        assert_eq!(authorization_code, "test-auth-code");
        Ok(af_db::AccountVerificationProviderResult {
            status: "approved".to_owned(),
            terminal_status: Some(4),
            reason: None,
        })
    }
}

#[tokio::test]
async fn alipay_auth_code_callback_saves_result_without_browser_session()
-> Result<(), Box<dyn std::error::Error>> {
    let (app, pool) = fixture_with_provider(Some(Arc::new(CertDocCallbackProvider))).await?;
    let metadata = json!({"kind":"individual","provider":"alipay","document_country":"CN","document_type":"national_id","document_number":"11010519491231002X","subject_name":"Test Person","summary":"Identity verification","materials":[]});
    let body = format!(
        "--verification-boundary\r\nContent-Disposition: form-data; name=\"metadata\"\r\n\r\n{metadata}\r\n--verification-boundary--\r\n"
    );
    let submitted = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/account/verifications",
            "applicant",
            "multipart/form-data; boundary=verification-boundary",
            body,
        ))
        .await?;
    assert_eq!(submitted.status(), StatusCode::OK);
    let record: Value = serde_json::from_slice(&to_bytes(submitted.into_body(), 65_536).await?)?;
    let id = record["id"].as_i64().unwrap();
    let callback = "/api/account/verifications/alipay/callback";
    for query in [
        "state=0123456789abcdef0123456789abcdef",
        "state=ffffffffffffffffffffffffffffffff&auth_code=test-auth-code",
        "state=0123456789abcdef0123456789abcdef&auth_code=test-auth-code&error=access_denied",
    ] {
        let response = app
            .clone()
            .oneshot(request(
                Method::GET,
                &format!("{callback}?{query}"),
                "",
                "text/html",
                String::new(),
            ))
            .await?;
        let html = String::from_utf8(to_bytes(response.into_body(), 65_536).await?.to_vec())?;
        assert!(html.contains("支付宝实名认证未完成"));
    }
    let response = app.clone().oneshot(request(Method::GET, &format!("{callback}?state=0123456789abcdef0123456789abcdef&auth_code=test-auth-code&app_id=test-app&source=alipay_wallet"), "", "text/html", String::new())).await?;
    assert_eq!(response.status(), StatusCode::OK);
    let html = String::from_utf8(to_bytes(response.into_body(), 65_536).await?.to_vec())?;
    assert!(html.contains("支付宝实名认证已完成"));
    assert!(!html.contains("test-auth-code"));
    let detail = app
        .oneshot(request(
            Method::GET,
            &format!("/api/account/verifications/{id}"),
            "applicant",
            "application/json",
            String::new(),
        ))
        .await?;
    let detail: Value = serde_json::from_slice(&to_bytes(detail.into_body(), 65_536).await?)?;
    assert_eq!(detail["case"]["status"], 4);
    assert_eq!(detail["case"]["provider_status"], "approved");
    pool.close().await?;
    Ok(())
}

fn request(
    method: Method,
    path: &str,
    token: &str,
    content_type: &str,
    body: String,
) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", content_type);
    if !token.is_empty() {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    builder.body(Body::from(body)).expect("request")
}

fn upload(content_type: &str, bytes: &str) -> String {
    let metadata = json!({"kind":"enterprise","subject_name":"Example Enterprise","summary":"Enterprise identity review","materials":[{"kind":"business_document","file_field":"0"}]}).to_string();
    format!(
        "--verification-boundary\r\nContent-Disposition: form-data; name=\"metadata\"\r\n\r\n{metadata}\r\n--verification-boundary\r\nContent-Disposition: form-data; name=\"file:0\"; filename=\"license.pdf\"\r\nContent-Type: {content_type}\r\n\r\n{bytes}\r\n--verification-boundary--\r\n"
    )
}

#[tokio::test]
async fn multipart_review_and_material_download_remain_accessible_only_to_authorized_users()
-> Result<(), Box<dyn std::error::Error>> {
    let (app, pool) = fixture().await?;
    let base = "/api/account/verifications";
    let response = app
        .clone()
        .oneshot(request(
            Method::POST,
            base,
            "applicant",
            "multipart/form-data; boundary=verification-boundary",
            upload("application/pdf", "%PDF-1.7\nfixture"),
        ))
        .await?;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let submitted: Value = serde_json::from_slice(&to_bytes(response.into_body(), 65_536).await?)?;
    let id = submitted["id"].as_i64().expect("case id");
    let detail_path = format!("{base}/{id}");
    let response = app
        .clone()
        .oneshot(request(
            Method::GET,
            &detail_path,
            "applicant",
            "application/json",
            String::new(),
        ))
        .await?;
    assert_eq!(response.status(), StatusCode::OK);
    let detail: Value = serde_json::from_slice(&to_bytes(response.into_body(), 65_536).await?)?;
    let material = detail["materials"][0]["id"].as_i64().expect("material id");
    assert!(detail["materials"][0].get("content_bytes").is_none());
    let download_path = format!("{detail_path}/materials/{material}");
    for (token, path, expected) in [
        ("", detail_path.as_str(), StatusCode::UNAUTHORIZED),
        ("other", detail_path.as_str(), StatusCode::NOT_FOUND),
        ("other", download_path.as_str(), StatusCode::NOT_FOUND),
        (
            "applicant",
            "/api/admin/account-verifications",
            StatusCode::FORBIDDEN,
        ),
    ] {
        let response = app
            .clone()
            .oneshot(request(
                Method::GET,
                path,
                token,
                "application/json",
                String::new(),
            ))
            .await?;
        assert_eq!(response.status(), expected);
    }
    let decision_path = format!("/api/admin/account-verifications/{id}/decision");
    let body = json!({"expected_version":1,"status":4}).to_string();
    let response = app
        .clone()
        .oneshot(request(
            Method::POST,
            &decision_path,
            "applicant",
            "application/json",
            body.clone(),
        ))
        .await?;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let response = app
        .clone()
        .oneshot(request(
            Method::POST,
            &decision_path,
            "admin",
            "application/json",
            body.clone(),
        ))
        .await?;
    assert_eq!(response.status(), StatusCode::OK);
    let response = app
        .clone()
        .oneshot(request(
            Method::POST,
            &decision_path,
            "admin",
            "application/json",
            body,
        ))
        .await?;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let response = app
        .clone()
        .oneshot(request(
            Method::GET,
            &format!("{base}/eligibility"),
            "applicant",
            "application/json",
            String::new(),
        ))
        .await?;
    let eligibility: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 65_536).await?)?;
    assert_eq!(eligibility["can_apply_for_organization"], true);
    assert_eq!(eligibility["providers"], json!(["manual"]));
    let response = app
        .clone()
        .oneshot(request(
            Method::GET,
            &download_path,
            "applicant",
            "application/json",
            String::new(),
        ))
        .await?;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert_eq!(response.headers()["x-content-type-options"], "nosniff");
    assert!(
        response.headers()["content-disposition"]
            .to_str()?
            .split(';')
            .next()
            .is_some_and(|value| value == "attachment")
    );
    assert_eq!(
        to_bytes(response.into_body(), 65_536).await?.as_ref(),
        b"%PDF-1.7\nfixture"
    );
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn enterprise_manual_review_and_optional_reason_follow_admin_policy()
-> Result<(), Box<dyn std::error::Error>> {
    let (app, pool) = fixture().await?;
    let settings = "/api/admin/account-verification-settings";
    let initial = app
        .clone()
        .oneshot(request(
            Method::GET,
            settings,
            "admin",
            "application/json",
            String::new(),
        ))
        .await?;
    let initial: Value = serde_json::from_slice(&to_bytes(initial.into_body(), 65_536).await?)?;
    let body = json!({
        "expected_version": initial["version"],
        "manual_enabled": true,
        "individual_manual_enabled": false,
        "enterprise_manual_enabled": true,
        "individual_reason_required": true,
        "enterprise_reason_required": false,
        "enabled": false,
        "app_id": null,
        "gateway_url": "https://openapi.alipay.com/gateway.do",
        "biz_code": "FACE",
        "timeout_secs": 10
    })
    .to_string();
    let saved = app
        .clone()
        .oneshot(request(
            Method::PUT,
            settings,
            "admin",
            "application/json",
            body,
        ))
        .await?;
    assert_eq!(saved.status(), StatusCode::OK);
    let eligibility = app
        .clone()
        .oneshot(request(
            Method::GET,
            "/api/account/verifications/eligibility",
            "applicant",
            "application/json",
            String::new(),
        ))
        .await?;
    let eligibility: Value =
        serde_json::from_slice(&to_bytes(eligibility.into_body(), 65_536).await?)?;
    assert_eq!(eligibility["individual_providers"], json!([]));
    assert_eq!(eligibility["enterprise_providers"], json!(["manual"]));
    assert_eq!(eligibility["enterprise_reason_required"], false);

    let multipart =
        upload("application/pdf", "%PDF-1.7\nfixture").replace("Enterprise identity review", "");
    let submitted = app
        .oneshot(request(
            Method::POST,
            "/api/account/verifications",
            "applicant",
            "multipart/form-data; boundary=verification-boundary",
            multipart,
        ))
        .await?;
    assert_eq!(submitted.status(), StatusCode::OK);
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn invalid_materials_and_ambiguous_pagination_are_rejected_at_http_boundary()
-> Result<(), Box<dyn std::error::Error>> {
    let (app, pool) = fixture().await?;
    let response = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/account/verifications",
            "applicant",
            "multipart/form-data; boundary=verification-boundary",
            upload("image/png", "%PDF-1.7\nfixture"),
        ))
        .await?;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    for query in [
        "limit=0",
        "limit=51",
        "limit=2&limit=3",
        "before=0",
        "status=2",
        "unknown=1",
    ] {
        let response = app
            .clone()
            .oneshot(request(
                Method::GET,
                &format!("/api/account/verifications?{query}"),
                "applicant",
                "application/json",
                String::new(),
            ))
            .await?;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{query}");
    }
    // OpenAPI clients send the named binary property file_0; the UI uses file:0.
    let alias_upload = upload("application/pdf", "%PDF-1.7\nfixture")
        .replace("name=\"file:0\"", "name=\"file_0\"");
    let response = app
        .clone()
        .oneshot(request(
            Method::POST,
            "/api/account/verifications",
            "applicant",
            "multipart/form-data; boundary=verification-boundary",
            alias_upload,
        ))
        .await?;
    assert_eq!(response.status(), StatusCode::OK);
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn disabled_verification_is_hidden_and_rejected_until_admin_reenables_it()
-> Result<(), Box<dyn std::error::Error>> {
    let (app, pool) = fixture().await?;
    let path = "/api/admin/account-verification-settings";
    for (token, expected) in [
        ("", StatusCode::UNAUTHORIZED),
        ("applicant", StatusCode::FORBIDDEN),
    ] {
        let response = app
            .clone()
            .oneshot(request(
                Method::GET,
                path,
                token,
                "application/json",
                String::new(),
            ))
            .await?;
        assert_eq!(response.status(), expected);
    }
    let response = app
        .clone()
        .oneshot(request(
            Method::GET,
            path,
            "admin",
            "application/json",
            String::new(),
        ))
        .await?;
    assert_eq!(response.status(), StatusCode::OK);
    let initial: Value = serde_json::from_slice(&to_bytes(response.into_body(), 65_536).await?)?;
    assert_eq!(initial["manual_enabled"], true);
    let mut version = initial["version"].as_i64().expect("settings version");
    for enabled in [false, true] {
        let body = json!({
            "expected_version": version,
            "manual_enabled": enabled,
            "enabled": false,
            "app_id": null,
            "gateway_url": "https://openapi.alipay.com/gateway.do",
            "biz_code": "FACE",
            "timeout_secs": 10
        })
        .to_string();
        let response = app
            .clone()
            .oneshot(request(
                Method::PUT,
                path,
                "applicant",
                "application/json",
                body.clone(),
            ))
            .await?;
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        let response = app
            .clone()
            .oneshot(request(
                Method::PUT,
                path,
                "admin",
                "application/json",
                body.clone(),
            ))
            .await?;
        assert_eq!(response.status(), StatusCode::OK);
        let saved: Value = serde_json::from_slice(&to_bytes(response.into_body(), 65_536).await?)?;
        assert_eq!(saved["manual_enabled"], enabled);
        assert!(saved.get("private_key").is_none());
        version = saved["version"].as_i64().expect("saved version");
        let conflict = app
            .clone()
            .oneshot(request(
                Method::PUT,
                path,
                "admin",
                "application/json",
                body,
            ))
            .await?;
        assert_eq!(conflict.status(), StatusCode::CONFLICT);
        let response = app
            .clone()
            .oneshot(request(
                Method::GET,
                "/api/account/verifications/eligibility",
                "applicant",
                "application/json",
                String::new(),
            ))
            .await?;
        assert_eq!(response.status(), StatusCode::OK);
        let eligibility: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 65_536).await?)?;
        assert_eq!(
            eligibility["providers"],
            if enabled {
                json!(["manual"])
            } else {
                json!([])
            }
        );
        let response = app
            .clone()
            .oneshot(request(
                Method::POST,
                "/api/account/verifications",
                "applicant",
                "multipart/form-data; boundary=verification-boundary",
                upload("application/pdf", "%PDF-1.7\nfixture"),
            ))
            .await?;
        assert_eq!(
            response.status(),
            if enabled {
                StatusCode::OK
            } else {
                StatusCode::SERVICE_UNAVAILABLE
            }
        );
    }
    pool.close().await?;
    Ok(())
}
