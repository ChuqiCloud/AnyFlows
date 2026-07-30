use std::{error::Error, sync::Arc, time::Duration};

use af_account::SystemSecretCipher;
use af_admin::{
    DatabaseAdminEmailSettingsService, EmailDelivery, EmailDeliveryError, EmailDeliveryFuture,
    EmailDeliveryRequest, LoginCredentials, SessionAuthentication, SessionAuthenticationError,
    SessionAuthenticationFuture, SessionAuthenticator, SessionLoginFuture, SessionPrincipal,
    SessionRole,
};
use af_config::{CREDENTIAL_ENCRYPTION_KEY_BYTES, CredentialEncryptionSettings};
use af_db::{DatabaseOptions, EmailSettingsRepository, MigrationOptions};
use af_domain::{GroupId, UserId};
use axum::body::{Body, to_bytes};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use http::{
    Request, StatusCode,
    header::{AUTHORIZATION, CACHE_CONTROL, CONTENT_TYPE},
};
use serde_json::{Value, json};
use std::sync::Mutex;
use tower::ServiceExt;

use crate::email_settings::build_email_settings_router;

const ADMIN_TOKEN: &str = "email-admin-token";
const USER_TOKEN: &str = "email-user-token";
const SMTP_PASSWORD: &str = "private smtp password";

#[derive(Clone, Debug, Eq, PartialEq)]
struct ObservedTestMail {
    recipient: String,
    username: Option<String>,
    password: Option<String>,
}

struct MutableDelivery {
    result: Mutex<Result<(), EmailDeliveryError>>,
    requests: Mutex<Vec<ObservedTestMail>>,
}

impl MutableDelivery {
    fn new() -> Self {
        Self {
            result: Mutex::new(Ok(())),
            requests: Mutex::new(Vec::new()),
        }
    }

    fn fail(&self) {
        *self.result.lock().unwrap() = Err(EmailDeliveryError::Failed);
    }

    fn requests(&self) -> Vec<ObservedTestMail> {
        self.requests.lock().unwrap().clone()
    }
}

impl EmailDelivery for MutableDelivery {
    fn send_test<'a>(&'a self, request: EmailDeliveryRequest) -> EmailDeliveryFuture<'a> {
        self.requests.lock().unwrap().push(ObservedTestMail {
            recipient: request.recipient().to_owned(),
            username: request.username().map(str::to_owned),
            password: request.password().map(str::to_owned),
        });
        let result = *self.result.lock().unwrap();
        Box::pin(async move { result })
    }
}

struct FakeSessionAuthenticator;

impl SessionAuthenticator for FakeSessionAuthenticator {
    fn login<'a>(&'a self, _credentials: &'a LoginCredentials) -> SessionLoginFuture<'a> {
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

async fn fixture()
-> Result<(af_db::DatabasePool, Arc<MutableDelivery>, axum::Router), Box<dyn Error>> {
    let pool = af_db::connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let repository = EmailSettingsRepository::new(pool.clone(), Duration::from_secs(5))?;
    let delivery = Arc::new(MutableDelivery::new());
    let service = Arc::new(DatabaseAdminEmailSettingsService::new(
        repository,
        cipher(),
        delivery.clone(),
    ));
    let router = build_email_settings_router(service, Arc::new(FakeSessionAuthenticator));
    Ok((pool, delivery, router))
}

fn cipher() -> SystemSecretCipher {
    let settings: CredentialEncryptionSettings = serde_json::from_value(json!({
        "key_id": "email-http-test-key",
        "key": URL_SAFE_NO_PAD.encode([0x42; CREDENTIAL_ENCRYPTION_KEY_BYTES])
    }))
    .unwrap();
    SystemSecretCipher::new(&settings).unwrap()
}

#[tokio::test]
async fn admin_can_update_read_and_test_email_without_secret_echo() -> Result<(), Box<dyn Error>> {
    let (pool, delivery, app) = fixture().await?;

    let initial = app
        .clone()
        .oneshot(json_request(
            "GET",
            "/api/admin/email-settings",
            Value::Null,
            Some(ADMIN_TOKEN),
        ))
        .await?;
    assert_eq!(initial.status(), StatusCode::OK);
    assert_eq!(initial.headers()[CACHE_CONTROL], "no-store");
    assert_eq!(response_json(initial).await?["enabled"], false);

    let updated = app
        .clone()
        .oneshot(json_request(
            "PUT",
            "/api/admin/email-settings",
            settings_body(Some(SMTP_PASSWORD)),
            Some(ADMIN_TOKEN),
        ))
        .await?;
    assert_eq!(updated.status(), StatusCode::OK);
    assert_eq!(updated.headers()[CACHE_CONTROL], "no-store");
    let updated_body = response_json(updated).await?;
    assert_eq!(updated_body["enabled"], true);
    assert_eq!(updated_body["password_configured"], true);
    assert_eq!(updated_body["delivery_ready"], true);
    assert_eq!(updated_body["version"], 2);
    assert!(updated_body.get("password").is_none());
    assert!(!updated_body.to_string().contains(SMTP_PASSWORD));

    let kept = app
        .clone()
        .oneshot(json_request(
            "PUT",
            "/api/admin/email-settings",
            settings_body(None),
            Some(ADMIN_TOKEN),
        ))
        .await?;
    assert_eq!(response_json(kept).await?["version"], 3);

    let tested = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/admin/email-settings/test",
            json!({"recipient": "recipient@example.com"}),
            Some(ADMIN_TOKEN),
        ))
        .await?;
    assert_eq!(tested.status(), StatusCode::NO_CONTENT);
    assert_eq!(tested.headers()[CACHE_CONTROL], "no-store");
    assert!(to_bytes(tested.into_body(), 1024).await?.is_empty());
    assert_eq!(
        delivery.requests(),
        vec![ObservedTestMail {
            recipient: "recipient@example.com".to_owned(),
            username: Some("mailer@example.com".to_owned()),
            password: Some(SMTP_PASSWORD.to_owned()),
        }]
    );

    drop(app);
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn authentication_and_delivery_errors_use_stable_sanitized_codes()
-> Result<(), Box<dyn Error>> {
    let (pool, delivery, app) = fixture().await?;

    let unauthorized = app
        .clone()
        .oneshot(json_request(
            "GET",
            "/api/admin/email-settings",
            Value::Null,
            None,
        ))
        .await?;
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

    let forbidden = app
        .clone()
        .oneshot(json_request(
            "GET",
            "/api/admin/email-settings",
            Value::Null,
            Some(USER_TOKEN),
        ))
        .await?;
    assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);

    let not_configured = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/admin/email-settings/test",
            json!({"recipient": "private-recipient@example.com"}),
            Some(ADMIN_TOKEN),
        ))
        .await?;
    assert_eq!(not_configured.status(), StatusCode::CONFLICT);
    let body = response_json(not_configured).await?;
    assert_eq!(body["code"], "email_not_configured");
    assert!(!body.to_string().contains("private-recipient"));

    let invalid = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/admin/email-settings/test",
            json!({
                "recipient": "recipient@example.com",
                "subject": "must not be accepted"
            }),
            Some(ADMIN_TOKEN),
        ))
        .await?;
    assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);

    app.clone()
        .oneshot(json_request(
            "PUT",
            "/api/admin/email-settings",
            settings_body(Some(SMTP_PASSWORD)),
            Some(ADMIN_TOKEN),
        ))
        .await?;
    delivery.fail();
    let failed = app
        .clone()
        .oneshot(json_request(
            "POST",
            "/api/admin/email-settings/test",
            json!({"recipient": "private-recipient@example.com"}),
            Some(ADMIN_TOKEN),
        ))
        .await?;
    assert_eq!(failed.status(), StatusCode::BAD_GATEWAY);
    let failed_body = response_json(failed).await?;
    assert_eq!(failed_body["code"], "email_delivery_failed");
    let rendered = failed_body.to_string();
    assert!(!rendered.contains("private-recipient"));
    assert!(!rendered.contains(SMTP_PASSWORD));

    drop(app);
    pool.close().await?;
    Ok(())
}

fn settings_body(password: Option<&str>) -> Value {
    let mut body = json!({
        "enabled": true,
        "host": "smtp.example.com",
        "port": 587,
        "tls_mode": "start_tls",
        "username": "mailer@example.com",
        "from_address": "from@example.com",
        "from_name": "AnyFlows",
        "reply_to": "reply@example.com",
        "timeout_seconds": 15
    });
    if let Some(password) = password {
        body["password"] = Value::String(password.to_owned());
    }
    body
}

fn json_request(method: &str, uri: &str, body: Value, token: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder().method(method).uri(uri);
    if method != "GET" {
        builder = builder.header(CONTENT_TYPE, "application/json");
    }
    if let Some(token) = token {
        builder = builder.header(AUTHORIZATION, format!("Bearer {token}"));
    }
    builder
        .body(if method == "GET" {
            Body::empty()
        } else {
            Body::from(serde_json::to_vec(&body).unwrap())
        })
        .unwrap()
}

async fn response_json(response: axum::response::Response) -> Result<Value, Box<dyn Error>> {
    let body = to_bytes(response.into_body(), 64 * 1024).await?;
    Ok(serde_json::from_slice(&body)?)
}

fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
