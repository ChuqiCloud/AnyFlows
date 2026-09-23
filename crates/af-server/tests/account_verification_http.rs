use std::sync::Arc;

use af_admin::{
    AccountVerificationService, DatabasePlatformAuditService, LoginCredentials,
    SessionAuthentication, SessionAuthenticationError, SessionAuthenticationFuture,
    SessionAuthenticator, SessionLoginFuture, SessionPrincipal, SessionRole,
};
use af_db::{AccountVerificationRepository, DatabaseOptions, MigrationOptions, PlatformAuditRepository, entity::{groups, users}};
use af_domain::{GroupId, UserId};
use af_http::{Body, Router, build_account_verification_router, http::{Method, Request, StatusCode}, to_bytes};
use sea_orm::{ActiveModelTrait, Set};
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
            Ok(SessionAuthentication::new(SessionPrincipal::new(self.users[index], role), self.group, 4_102_444_800))
        })
    }
}

async fn fixture() -> Result<(Router, af_db::DatabasePool), Box<dyn std::error::Error>> {
    let pool = af_db::connect_and_migrate(&DatabaseOptions::new("sqlite::memory:")?, MigrationOptions::default()).await?;
    let group = groups::ActiveModel {
        name: Set("verification-http".to_owned()), display_name: Set("Verification HTTP".to_owned()),
        flags: Set(json!({})), ..Default::default()
    }.insert(pool.connection()).await?;
    let mut ids = Vec::new();
    for index in 0..3 {
        let user = users::ActiveModel {
            username: Set(format!("verification-http-{index}")),
            role: Set(if index == 2 { 1 } else { 0 }), status: Set(1),
            default_group_id: Set(group.id), aff_code: Set(format!("verification-http-aff-{index}")),
            settings: Set(json!({})), ..Default::default()
        }.insert(pool.connection()).await?;
        ids.push(UserId::new(user.id)?);
    }
    let sessions = Arc::new(Sessions { group: GroupId::new(group.id)?, users: ids.try_into().expect("three users") });
    let audit = Arc::new(DatabasePlatformAuditService::new(PlatformAuditRepository::new(pool.clone())));
    let service = Arc::new(AccountVerificationService::new(AccountVerificationRepository::new(pool.clone()), audit));
    Ok((build_account_verification_router(service, sessions), pool))
}

fn request(method: Method, path: &str, token: &str, content_type: &str, body: String) -> Request<Body> {
    let mut builder = Request::builder().method(method).uri(path).header("content-type", content_type);
    if !token.is_empty() {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    builder.body(Body::from(body)).expect("request")
}

fn upload(content_type: &str, bytes: &str) -> String {
    let metadata = json!({"kind":"enterprise","subject_name":"Example Enterprise","summary":"Enterprise identity review","materials":[{"kind":"business_document","file_field":"0"}]}).to_string();
    format!("--verification-boundary\r\nContent-Disposition: form-data; name=\"metadata\"\r\n\r\n{metadata}\r\n--verification-boundary\r\nContent-Disposition: form-data; name=\"file:0\"; filename=\"license.pdf\"\r\nContent-Type: {content_type}\r\n\r\n{bytes}\r\n--verification-boundary--\r\n")
}

#[tokio::test]
async fn multipart_review_and_material_download_remain_accessible_only_to_authorized_users() -> Result<(), Box<dyn std::error::Error>> {
    let (app, pool) = fixture().await?;
    let base = "/api/account/verifications";
    let response = app.clone().oneshot(request(Method::POST, base, "applicant", "multipart/form-data; boundary=verification-boundary", upload("application/pdf", "%PDF-1.7\nfixture"))).await?;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let submitted: Value = serde_json::from_slice(&to_bytes(response.into_body(), 65_536).await?)?;
    let id = submitted["id"].as_i64().expect("case id");
    let detail_path = format!("{base}/{id}");
    let response = app.clone().oneshot(request(Method::GET, &detail_path, "applicant", "application/json", String::new())).await?;
    assert_eq!(response.status(), StatusCode::OK);
    let detail: Value = serde_json::from_slice(&to_bytes(response.into_body(), 65_536).await?)?;
    let material = detail["materials"][0]["id"].as_i64().expect("material id");
    assert!(detail["materials"][0].get("content_bytes").is_none());
    let download_path = format!("{detail_path}/materials/{material}");
    for (token, path, expected) in [
        ("", detail_path.as_str(), StatusCode::UNAUTHORIZED),
        ("other", detail_path.as_str(), StatusCode::NOT_FOUND),
        ("other", download_path.as_str(), StatusCode::NOT_FOUND),
        ("applicant", "/api/admin/account-verifications", StatusCode::FORBIDDEN),
    ] {
        let response = app.clone().oneshot(request(Method::GET, path, token, "application/json", String::new())).await?;
        assert_eq!(response.status(), expected);
    }
    let decision_path = format!("/api/admin/account-verifications/{id}/decision");
    let body = json!({"expected_version":1,"status":4}).to_string();
    let response = app.clone().oneshot(request(Method::POST, &decision_path, "applicant", "application/json", body.clone())).await?;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let response = app.clone().oneshot(request(Method::POST, &decision_path, "admin", "application/json", body.clone())).await?;
    assert_eq!(response.status(), StatusCode::OK);
    let response = app.clone().oneshot(request(Method::POST, &decision_path, "admin", "application/json", body)).await?;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let response = app.clone().oneshot(request(Method::GET, &format!("{base}/eligibility"), "applicant", "application/json", String::new())).await?;
    let eligibility: Value = serde_json::from_slice(&to_bytes(response.into_body(), 65_536).await?)?;
    assert_eq!(eligibility["can_apply_for_organization"], true);
    let response = app.clone().oneshot(request(Method::GET, &download_path, "applicant", "application/json", String::new())).await?;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert_eq!(response.headers()["x-content-type-options"], "nosniff");
    assert!(response.headers()["content-disposition"].to_str()?.starts_with("attachment;"));
    assert_eq!(to_bytes(response.into_body(), 65_536).await?.as_ref(), b"%PDF-1.7\nfixture");
    pool.close().await?;
    Ok(())
}

#[tokio::test]
async fn invalid_materials_and_ambiguous_pagination_are_rejected_at_http_boundary() -> Result<(), Box<dyn std::error::Error>> {
    let (app, pool) = fixture().await?;
    let response = app.clone().oneshot(request(Method::POST, "/api/account/verifications", "applicant", "multipart/form-data; boundary=verification-boundary", upload("image/png", "%PDF-1.7\nfixture"))).await?;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    for query in ["limit=0", "limit=51", "limit=2&limit=3", "before=0", "status=2", "unknown=1"] {
        let response = app.clone().oneshot(request(Method::GET, &format!("/api/account/verifications?{query}"), "applicant", "application/json", String::new())).await?;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{query}");
    }
    pool.close().await?;
    Ok(())
}
