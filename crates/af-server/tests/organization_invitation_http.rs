use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use af_account::OrganizationInvitationCipher;
use af_admin::{
    AdminGroupCreateCommand, AdminGroupWriter, AdminUserCreateCommand, AdminUserStatus,
    AdminUserWriter, DatabaseAdminGroupWriter, DatabaseAdminUserWriter,
    DatabaseOrganizationAuthorizationService, DatabaseOrganizationInvitationService,
    LoginCredentials, OrganizationAccessFuture, OrganizationAuthorizationError,
    OrganizationAuthorizationService, OrganizationInvitationError, OrganizationInvitationFuture,
    OrganizationInvitationService, SessionAuthentication, SessionAuthenticationError,
    SessionAuthenticationFuture, SessionAuthenticator, SessionLoginFuture, SessionPrincipal,
    SessionRole,
};
use af_config::CredentialEncryptionSettings;
use af_db::{
    AdminGroupRepository, AdminUserRepository, DatabaseOptions, MigrationOptions,
    OrganizationCreate, OrganizationEntitlementWrite, OrganizationMembershipCreate,
    OrganizationRepository, connect_and_migrate,
};
use af_domain::{
    GroupId, OrganizationEntitlementCapacity, OrganizationEntitlementSnapshot, OrganizationId,
    OrganizationPermission, OrganizationRole, UserId,
};
use af_http::{
    Body, Response, Router, build_organization_router,
    http::{Request, StatusCode, header::AUTHORIZATION},
    to_bytes,
};
use serde_json::{Value, json};
use tower::ServiceExt as _;

use af_http::build_organization_router;

const OWNER_TOKEN: &str = "organization-owner-token";
const VIEWER_TOKEN: &str = "organization-viewer-token";
const ACCEPT_TOKEN: &str = "organization-accept-token";

struct TestSessions {
    group_id: GroupId,
    owner_id: UserId,
    viewer_id: UserId,
}

impl SessionAuthenticator for TestSessions {
    fn login<'a>(&'a self, _credentials: &'a LoginCredentials) -> SessionLoginFuture<'a> {
        Box::pin(async { Err(SessionAuthenticationError::InvalidCredentials) })
    }

    fn authenticate<'a>(&'a self, token: &'a str) -> SessionAuthenticationFuture<'a> {
        let principal = match token {
            OWNER_TOKEN => Some(SessionPrincipal::new(self.owner_id, SessionRole::User)),
            VIEWER_TOKEN => Some(SessionPrincipal::new(self.viewer_id, SessionRole::User)),
            ACCEPT_TOKEN => Some(SessionPrincipal::new(self.viewer_id, SessionRole::User)),
            _ => None,
        };
        let group_id = self.group_id;
        Box::pin(async move {
            principal
                .map(|principal| SessionAuthentication::new(principal, group_id, now_u64() + 3_600))
                .ok_or(SessionAuthenticationError::InvalidSession)
        })
    }
}

async fn sqlite_organization_router()
-> Result<(Router, af_db::DatabasePool, OrganizationId), Box<dyn std::error::Error>> {
    let pool = connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let group_repository = AdminGroupRepository::new(pool.clone(), Duration::from_secs(5))?;
    let group_writer = DatabaseAdminGroupWriter::new(group_repository);
    let setup_principal = SessionPrincipal::new(UserId::new(900).unwrap(), SessionRole::Admin);
    let group = group_writer
        .create(
            setup_principal,
            AdminGroupCreateCommand::new(
                "organization-http-test".to_owned(),
                "Organization HTTP Test".to_owned(),
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

    let user_repository = AdminUserRepository::new(pool.clone(), Duration::from_secs(5))?;
    let user_writer = DatabaseAdminUserWriter::new(user_repository);
    let owner = user_writer
        .create(
            setup_principal,
            AdminUserCreateCommand::new(
                "organization-http-owner".to_owned(),
                Some("owner@example.com".to_owned()),
                None,
                SessionRole::User,
                AdminUserStatus::Enabled,
                group.group_id(),
                0,
                None,
                None,
            )?,
        )
        .await?;
    let viewer = user_writer
        .create(
            setup_principal,
            AdminUserCreateCommand::new(
                "organization-http-viewer".to_owned(),
                Some("viewer@example.com".to_owned()),
                None,
                SessionRole::User,
                AdminUserStatus::Enabled,
                group.group_id(),
                0,
                None,
                None,
            )?,
        )
        .await?;

    let repository = Arc::new(OrganizationRepository::new(
        pool.clone(),
        Duration::from_secs(5),
    )?);
    let entitlement = OrganizationEntitlementWrite::new(
        OrganizationEntitlementSnapshot::new(
            OrganizationEntitlementCapacity::new(10, 10, 2)?,
            true,
            false,
            false,
            90,
            946_684_800,
            4_102_444_800,
            Some(4_102_444_800),
        )?,
        "contract".to_owned(),
        None,
        None,
    )?;
    let (organization, _) = repository
        .create(OrganizationCreate::new(
            "HTTP Test Organization".to_owned(),
            "http-test-organization".to_owned(),
            owner.user_id(),
            entitlement,
        ))
        .await?
        .into_parts();
    repository
        .add_membership(
            organization.id(),
            OrganizationMembershipCreate::new(
                viewer.user_id(),
                OrganizationRole::Viewer,
                None,
                Some(owner.user_id()),
            ),
        )
        .await?;

    let settings: CredentialEncryptionSettings = serde_json::from_value(json!({
        "key_id": "organization-http-test-key",
        "key": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
    }))?;
    let cipher = OrganizationInvitationCipher::new(&settings)?;
    let authorization_service = Arc::new(DatabaseOrganizationAuthorizationService::new(
        Arc::clone(&repository),
    ));
    let invitation_service = Arc::new(DatabaseOrganizationInvitationService::new(
        Arc::clone(&repository),
        cipher,
    ));
    let sessions = Arc::new(TestSessions {
        group_id: group.group_id(),
        owner_id: owner.user_id(),
        viewer_id: viewer.user_id(),
    });
    let router =
        build_organization_router(authorization_service, Some(invitation_service), sessions);
    Ok((router, pool, organization.id()))
}

#[tokio::test]
async fn invitation_routes_require_members_manage_and_batch_replay_is_redacted()
-> Result<(), Box<dyn std::error::Error>> {
    let (app, pool, organization_id) = sqlite_organization_router().await?;
    let forbidden = app
        .clone()
        .oneshot(authenticated_request(
            &format!("/api/organizations/{}/invitations", organization_id.get()),
            VIEWER_TOKEN,
        ))
        .await?;
    assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);
    assert_eq!(response_json(forbidden).await["code"], "forbidden");

    let body = json!({
        "batch_key": "11111111111111111111111111111111",
        "items": [{
            "email": "invitee@example.com",
            "role": "member",
            "expires_at": now() + 7_200
        }]
    })
    .to_string();
    let created = app
        .clone()
        .oneshot(json_request(
            &format!(
                "/api/organizations/{}/invitations/batches",
                organization_id.get()
            ),
            OWNER_TOKEN,
            &body,
        ))
        .await?;
    assert_eq!(created.status(), StatusCode::OK);
    let created = response_json(created).await;
    assert_eq!(created["replayed"], false);
    assert_eq!(created["invitations"].as_array().unwrap().len(), 1);
    assert_no_invitation_token(&created);

    let replayed = app
        .oneshot(json_request(
            &format!(
                "/api/organizations/{}/invitations/batches",
                organization_id.get()
            ),
            OWNER_TOKEN,
            &body,
        ))
        .await?;
    assert_eq!(replayed.status(), StatusCode::OK);
    let replayed = response_json(replayed).await;
    assert_eq!(replayed["replayed"], true);
    assert_no_invitation_token(&replayed);
    pool.close().await?;
    Ok(())
}

struct RejectionInvitations;

impl OrganizationInvitationService for RejectionInvitations {
    fn create<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _organization_id: OrganizationId,
        _command: af_admin::OrganizationInvitationCommand,
    ) -> OrganizationInvitationFuture<'a, af_admin::IssuedOrganizationInvitation> {
        Box::pin(async { Err(OrganizationInvitationError::Internal) })
    }

    fn create_batch<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _organization_id: OrganizationId,
        _command: af_admin::OrganizationInvitationBatchCommand,
    ) -> OrganizationInvitationFuture<'a, af_admin::OrganizationInvitationBatchResult> {
        Box::pin(async { Err(OrganizationInvitationError::Internal) })
    }

    fn preview_batch<'a>(
        &'a self,
        _organization_id: OrganizationId,
        _command: af_admin::OrganizationInvitationPreviewCommand,
    ) -> OrganizationInvitationFuture<'a, af_admin::OrganizationInvitationPreviewResult> {
        Box::pin(async { Err(OrganizationInvitationError::Internal) })
    }

    fn list<'a>(
        &'a self,
        _organization_id: OrganizationId,
    ) -> OrganizationInvitationFuture<'a, Vec<af_db::OrganizationInvitationRecord>> {
        Box::pin(async { Err(OrganizationInvitationError::Internal) })
    }

    fn revoke<'a>(
        &'a self,
        _organization_id: OrganizationId,
        _invitation_key: af_domain::OrganizationInvitationKey,
    ) -> OrganizationInvitationFuture<'a, af_db::OrganizationInvitationRecord> {
        Box::pin(async { Err(OrganizationInvitationError::Internal) })
    }

    fn resend<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _organization_id: OrganizationId,
        _invitation_key: af_domain::OrganizationInvitationKey,
    ) -> OrganizationInvitationFuture<'a, af_admin::IssuedOrganizationInvitation> {
        Box::pin(async { Err(OrganizationInvitationError::Internal) })
    }

    fn accept<'a>(
        &'a self,
        _principal: SessionPrincipal,
        token: String,
    ) -> OrganizationInvitationFuture<'a, af_admin::AcceptedOrganizationInvitation> {
        let error = match token.as_str() {
            "not-found" => OrganizationInvitationError::NotFound,
            "conflict" => OrganizationInvitationError::Conflict,
            "expired" => OrganizationInvitationError::Expired,
            _ => OrganizationInvitationError::EmailMismatch,
        };
        Box::pin(async move { Err(error) })
    }
}

struct NoOrganizations;

impl OrganizationAuthorizationService for NoOrganizations {
    fn list<'a>(&'a self, _principal: SessionPrincipal) -> af_admin::OrganizationListFuture<'a> {
        Box::pin(async { Ok(Vec::new()) })
    }

    fn context<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _organization_id: OrganizationId,
        _permission: OrganizationPermission,
    ) -> OrganizationAccessFuture<'a> {
        Box::pin(async { Err(OrganizationAuthorizationError::Forbidden) })
    }
}

#[tokio::test]
async fn acceptance_maps_all_rejection_facts_to_one_public_error() {
    let sessions = Arc::new(TestSessions {
        group_id: GroupId::new(1).unwrap(),
        owner_id: UserId::new(1).unwrap(),
        viewer_id: UserId::new(2).unwrap(),
    });
    let app = build_organization_router(
        Arc::new(NoOrganizations),
        Some(Arc::new(RejectionInvitations)),
        sessions,
    );
    for token in ["not-found", "conflict", "expired", "email-mismatch"] {
        let response = app
            .clone()
            .oneshot(json_request(
                "/api/account/organization-invitations/accept",
                ACCEPT_TOKEN,
                &json!({ "token": token }).to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);
        assert_eq!(
            response_json(response).await["code"],
            "organization_invitation_rejected"
        );
    }
}

#[tokio::test]
async fn organization_directory_remains_available_without_invitation_service() {
    let sessions = Arc::new(TestSessions {
        group_id: GroupId::new(1).unwrap(),
        owner_id: UserId::new(1).unwrap(),
        viewer_id: UserId::new(2).unwrap(),
    });
    let app = build_organization_router(Arc::new(NoOrganizations), None, sessions);
    let response = app
        .oneshot(authenticated_request(
            "/api/account/organizations",
            OWNER_TOKEN,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response_json(response).await["organizations"], json!([]));
}

fn authenticated_request(path: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(path)
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap()
}

fn json_request(path: &str, token: &str, body: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(path)
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(body.to_owned()))
        .unwrap()
}

async fn response_json(response: Response) -> Value {
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap()
}

fn assert_no_invitation_token(body: &Value) {
    assert!(body.to_string().find("oinv_").is_none(), "{body}");
    assert!(body.get("token").is_none(), "{body}");
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

fn now_u64() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
