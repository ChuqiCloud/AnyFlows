use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use af_account::OrganizationInvitationCipher;
use af_admin::{
    AdminGroupCreateCommand, AdminGroupWriter, AdminUserCreateCommand, AdminUserStatus,
    AdminUserWriter, DatabaseAdminGroupWriter, DatabaseAdminUserWriter,
    DatabaseOrganizationAuthorizationService, DatabaseOrganizationCustomRoleService,
    DatabaseOrganizationInvitationService, DatabaseOrganizationManagementService, LoginCredentials,
    OrganizationAccessFuture, OrganizationAuthorizationError, OrganizationAuthorizationService,
    OrganizationInvitationError, OrganizationInvitationFuture, OrganizationInvitationService,
    SessionAuthentication, SessionAuthenticationError, SessionAuthenticationFuture,
    SessionAuthenticator, SessionLoginFuture, SessionPrincipal, SessionRole,
};
use af_config::CredentialEncryptionSettings;
use af_db::{
    AdminGroupRepository, AdminUserRepository, DatabaseOptions, MigrationOptions,
    OrganizationCreate, OrganizationCustomRoleRepository, OrganizationEntitlementWrite,
    OrganizationMembershipCreate, OrganizationRepository, connect_and_migrate,
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

use organization_activity_support::test_organization_activity_service;

const OWNER_TOKEN: &str = "organization-owner-token";
const VIEWER_TOKEN: &str = "organization-viewer-token";
const ADMIN_TOKEN: &str = "organization-admin-token";
const ACCEPT_TOKEN: &str = "organization-accept-token";

struct TestSessions {
    group_id: GroupId,
    owner_id: UserId,
    viewer_id: UserId,
    admin_id: UserId,
}

impl SessionAuthenticator for TestSessions {
    fn login<'a>(&'a self, _credentials: &'a LoginCredentials) -> SessionLoginFuture<'a> {
        Box::pin(async { Err(SessionAuthenticationError::InvalidCredentials) })
    }

    fn authenticate<'a>(&'a self, token: &'a str) -> SessionAuthenticationFuture<'a> {
        let principal = match token {
            OWNER_TOKEN => Some(SessionPrincipal::new(self.owner_id, SessionRole::User)),
            VIEWER_TOKEN => Some(SessionPrincipal::new(self.viewer_id, SessionRole::User)),
            ADMIN_TOKEN => Some(SessionPrincipal::new(self.admin_id, SessionRole::User)),
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
    let admin = user_writer
        .create(
            setup_principal,
            AdminUserCreateCommand::new(
                "organization-http-admin".to_owned(),
                Some("admin@example.com".to_owned()),
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
    let entitlement = test_entitlement()?;
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
    repository
        .add_membership(
            organization.id(),
            OrganizationMembershipCreate::new(
                admin.user_id(),
                OrganizationRole::Admin,
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
    let management_service = Arc::new(DatabaseOrganizationManagementService::new(Arc::clone(
        &repository,
    )));
    let custom_role_service = Arc::new(DatabaseOrganizationCustomRoleService::new(Arc::new(
        OrganizationCustomRoleRepository::new(pool.clone(), Duration::from_secs(5))?,
    )));
    let sessions = Arc::new(TestSessions {
        group_id: group.group_id(),
        owner_id: owner.user_id(),
        viewer_id: viewer.user_id(),
        admin_id: admin.user_id(),
    });
    let router = build_organization_router(
        authorization_service,
        Some(invitation_service),
        Some(management_service),
        Some(custom_role_service),
        None,
        test_organization_activity_service(),
        sessions,
    );
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

#[tokio::test]
async fn lifecycle_routes_enforce_scope_permissions_cas_and_atomic_transfer()
-> Result<(), Box<dyn std::error::Error>> {
    let (app, pool, organization_id) = sqlite_organization_router().await?;
    let members_path = format!("/api/organizations/{}/members", organization_id.get());

    let first_page = app
        .clone()
        .oneshot(authenticated_request(
            &format!("{members_path}?limit=1"),
            VIEWER_TOKEN,
        ))
        .await?;
    assert_eq!(first_page.status(), StatusCode::OK);
    let first_page = response_json(first_page).await;
    assert_eq!(first_page["members"].as_array().unwrap().len(), 1);
    let first_cursor = first_page["next_cursor"].as_i64().unwrap();
    let second_page = app
        .clone()
        .oneshot(authenticated_request(
            &format!("{members_path}?after={first_cursor}&limit=1"),
            VIEWER_TOKEN,
        ))
        .await?;
    assert_eq!(second_page.status(), StatusCode::OK);
    let second_page = response_json(second_page).await;
    assert!(second_page["members"][0]["id"].as_i64().unwrap() > first_cursor);

    let directory = app
        .clone()
        .oneshot(authenticated_request(&members_path, OWNER_TOKEN))
        .await?;
    let directory = response_json(directory).await;
    let members = directory["members"].as_array().unwrap();
    let owner = members
        .iter()
        .find(|member| member["role"] == "owner")
        .unwrap();
    let viewer = members
        .iter()
        .find(|member| member["role"] == "viewer")
        .unwrap();
    let owner_id = owner["id"].as_i64().unwrap();
    let owner_version = owner["version"].as_i64().unwrap();
    let viewer_id = viewer["id"].as_i64().unwrap();
    let viewer_version = viewer["version"].as_i64().unwrap();
    let admin_user_id = members
        .iter()
        .find(|member| member["role"] == "admin")
        .unwrap()["user_id"]
        .as_i64()
        .and_then(|value| UserId::new(value).ok())
        .unwrap();

    let viewer_write = app
        .clone()
        .oneshot(json_method_request(
            "PATCH",
            &format!("{members_path}/{viewer_id}"),
            VIEWER_TOKEN,
            &json!({
                "role": "viewer",
                "status": "active",
                "team_id": null,
                "expected_version": viewer_version
            })
            .to_string(),
        ))
        .await?;
    assert_eq!(viewer_write.status(), StatusCode::FORBIDDEN);

    let admin_owner_write = app
        .clone()
        .oneshot(json_method_request(
            "PATCH",
            &format!("{members_path}/{owner_id}"),
            ADMIN_TOKEN,
            &json!({
                "role": "owner",
                "status": "active",
                "team_id": null,
                "expected_version": owner_version
            })
            .to_string(),
        ))
        .await?;
    assert_eq!(admin_owner_write.status(), StatusCode::FORBIDDEN);

    let ordinary_promotion = app
        .clone()
        .oneshot(json_method_request(
            "PATCH",
            &format!("{members_path}/{viewer_id}"),
            OWNER_TOKEN,
            &json!({
                "role": "owner",
                "status": "active",
                "team_id": null,
                "expected_version": viewer_version
            })
            .to_string(),
        ))
        .await?;
    assert_eq!(ordinary_promotion.status(), StatusCode::CONFLICT);
    assert_eq!(
        response_json(ordinary_promotion).await["code"],
        "organization_lifecycle_transition_invalid"
    );

    let teams_path = format!("/api/organizations/{}/teams", organization_id.get());
    let viewer_team_write = app
        .clone()
        .oneshot(json_request(
            &teams_path,
            VIEWER_TOKEN,
            &json!({ "name": "Forbidden" }).to_string(),
        ))
        .await?;
    assert_eq!(viewer_team_write.status(), StatusCode::FORBIDDEN);

    let first_team = app
        .clone()
        .oneshot(json_request(
            &teams_path,
            OWNER_TOKEN,
            &json!({ "name": "Platform", "description": "Core APIs" }).to_string(),
        ))
        .await?;
    assert_eq!(first_team.status(), StatusCode::OK);
    let first_team = response_json(first_team).await;
    let first_team_id = first_team["id"].as_i64().unwrap();
    let first_team_version = first_team["version"].as_i64().unwrap();
    let second_team = app
        .clone()
        .oneshot(json_request(
            &teams_path,
            OWNER_TOKEN,
            &json!({ "name": "Operations" }).to_string(),
        ))
        .await?;
    assert_eq!(second_team.status(), StatusCode::OK);

    let team_page = app
        .clone()
        .oneshot(authenticated_request(
            &format!("{teams_path}?limit=1"),
            VIEWER_TOKEN,
        ))
        .await?;
    let team_page = response_json(team_page).await;
    assert_eq!(team_page["teams"].as_array().unwrap().len(), 1);
    let team_cursor = team_page["next_cursor"].as_i64().unwrap();
    let next_team_page = app
        .clone()
        .oneshot(authenticated_request(
            &format!("{teams_path}?after={team_cursor}&limit=1"),
            VIEWER_TOKEN,
        ))
        .await?;
    let next_team_page = response_json(next_team_page).await;
    assert!(next_team_page["teams"][0]["id"].as_i64().unwrap() > team_cursor);

    let changed_team = app
        .clone()
        .oneshot(json_method_request(
            "PATCH",
            &format!("{teams_path}/{first_team_id}"),
            OWNER_TOKEN,
            &json!({
                "name": "Platform Engineering",
                "description": "Core APIs",
                "expected_version": first_team_version
            })
            .to_string(),
        ))
        .await?;
    assert_eq!(changed_team.status(), StatusCode::OK);
    let changed_team = response_json(changed_team).await;
    let changed_team_version = changed_team["version"].as_i64().unwrap();
    let stale_team = app
        .clone()
        .oneshot(json_method_request(
            "PATCH",
            &format!("{teams_path}/{first_team_id}"),
            OWNER_TOKEN,
            &json!({
                "name": "Stale",
                "description": null,
                "expected_version": first_team_version
            })
            .to_string(),
        ))
        .await?;
    assert_eq!(stale_team.status(), StatusCode::CONFLICT);

    let assigned = app
        .clone()
        .oneshot(json_method_request(
            "PATCH",
            &format!("{members_path}/{viewer_id}"),
            OWNER_TOKEN,
            &json!({
                "role": "member",
                "status": "active",
                "team_id": first_team_id,
                "expected_version": viewer_version
            })
            .to_string(),
        ))
        .await?;
    assert_eq!(assigned.status(), StatusCode::OK);
    let assigned = response_json(assigned).await;
    let assigned_version = assigned["version"].as_i64().unwrap();

    let team_in_use = app
        .clone()
        .oneshot(json_request(
            &format!("{teams_path}/{first_team_id}/disable"),
            OWNER_TOKEN,
            &json!({ "expected_version": changed_team_version }).to_string(),
        ))
        .await?;
    assert_eq!(team_in_use.status(), StatusCode::CONFLICT);
    assert_eq!(
        response_json(team_in_use).await["code"],
        "organization_team_in_use"
    );

    let unassigned = app
        .clone()
        .oneshot(json_method_request(
            "PATCH",
            &format!("{members_path}/{viewer_id}"),
            OWNER_TOKEN,
            &json!({
                "role": "member",
                "status": "active",
                "team_id": null,
                "expected_version": assigned_version
            })
            .to_string(),
        ))
        .await?;
    let unassigned = response_json(unassigned).await;
    let target_version = unassigned["version"].as_i64().unwrap();
    let disabled = app
        .clone()
        .oneshot(json_request(
            &format!("{teams_path}/{first_team_id}/disable"),
            OWNER_TOKEN,
            &json!({ "expected_version": changed_team_version }).to_string(),
        ))
        .await?;
    assert_eq!(disabled.status(), StatusCode::OK);
    assert_eq!(response_json(disabled).await["status"], "disabled");

    let repository = OrganizationRepository::new(pool.clone(), Duration::from_secs(5))?;
    let (other_organization, other_owner) = repository
        .create(OrganizationCreate::new(
            "Other Organization".to_owned(),
            "other-organization".to_owned(),
            admin_user_id,
            test_entitlement()?,
        ))
        .await?
        .into_parts();
    let other_team = repository
        .create_team(
            other_organization.id(),
            af_db::OrganizationTeamCreate::new("Other Team".to_owned(), None),
        )
        .await?;
    let cross_member = app
        .clone()
        .oneshot(json_method_request(
            "PATCH",
            &format!("{members_path}/{}", other_owner.id().get()),
            OWNER_TOKEN,
            &json!({
                "role": "owner",
                "status": "active",
                "team_id": null,
                "expected_version": other_owner.version()
            })
            .to_string(),
        ))
        .await?;
    assert_eq!(cross_member.status(), StatusCode::NOT_FOUND);
    let cross_team = app
        .clone()
        .oneshot(json_method_request(
            "PATCH",
            &format!("{teams_path}/{}", other_team.id().get()),
            OWNER_TOKEN,
            &json!({
                "name": "Cross Organization",
                "description": null,
                "expected_version": other_team.version()
            })
            .to_string(),
        ))
        .await?;
    assert_eq!(cross_team.status(), StatusCode::NOT_FOUND);

    let transfer = app
        .clone()
        .oneshot(json_request(
            &format!(
                "/api/organizations/{}/ownership-transfer",
                organization_id.get()
            ),
            OWNER_TOKEN,
            &json!({
                "target_membership_id": viewer_id,
                "current_owner_version": owner_version,
                "target_expected_version": target_version
            })
            .to_string(),
        ))
        .await?;
    assert_eq!(transfer.status(), StatusCode::OK);
    let transfer = response_json(transfer).await;
    assert_eq!(transfer["previous_owner"]["id"], owner_id);
    assert_eq!(transfer["previous_owner"]["role"], "admin");
    assert_eq!(transfer["current_owner"]["id"], viewer_id);
    assert_eq!(transfer["current_owner"]["role"], "owner");
    let current_owner_version = transfer["current_owner"]["version"].as_i64().unwrap();

    let last_owner = app
        .oneshot(json_method_request(
            "PATCH",
            &format!("{members_path}/{viewer_id}"),
            VIEWER_TOKEN,
            &json!({
                "role": "owner",
                "status": "suspended",
                "team_id": null,
                "expected_version": current_owner_version
            })
            .to_string(),
        ))
        .await?;
    assert_eq!(last_owner.status(), StatusCode::CONFLICT);
    assert_eq!(
        response_json(last_owner).await["code"],
        "organization_last_owner"
    );

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
        admin_id: UserId::new(3).unwrap(),
    });
    let app = build_organization_router(
        Arc::new(NoOrganizations),
        Some(Arc::new(RejectionInvitations)),
        None,
        None,
        None,
        test_organization_activity_service(),
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
        admin_id: UserId::new(3).unwrap(),
    });
    let app = build_organization_router(
        Arc::new(NoOrganizations),
        None,
        None,
        None,
        None,
        test_organization_activity_service(),
        sessions,
    );
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

#[tokio::test]
async fn custom_role_api_enforces_fixed_admin_roles_cas_and_irreversible_disable()
-> Result<(), Box<dyn std::error::Error>> {
    let (app, _pool, organization_id) = sqlite_organization_router().await?;
    let roles_path = format!("/api/organizations/{}/roles", organization_id.get());

    let forbidden = app
        .clone()
        .oneshot(authenticated_request(&roles_path, VIEWER_TOKEN))
        .await?;
    assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);

    let created = app
        .clone()
        .oneshot(json_request(
            &roles_path,
            OWNER_TOKEN,
            &json!({
                "name": "Support Operator",
                "description": "Read-only support role",
                "permissions": [
                    "organization.overview.read",
                    "organization.roles.read"
                ]
            })
            .to_string(),
        ))
        .await?;
    assert_eq!(created.status(), StatusCode::OK);
    let created = response_json(created).await;
    let role_key = created["role_key"].as_str().unwrap();
    assert_eq!(role_key.len(), 32);
    assert_eq!(created["version"], 1);
    assert_eq!(created["status"], "active");

    let detail_path = format!("{roles_path}/{role_key}");
    let detail = app
        .clone()
        .oneshot(authenticated_request(&detail_path, ADMIN_TOKEN))
        .await?;
    assert_eq!(detail.status(), StatusCode::OK);
    assert_eq!(response_json(detail).await["role_key"], role_key);

    let changed = app
        .clone()
        .oneshot(json_method_request(
            "PATCH",
            &detail_path,
            ADMIN_TOKEN,
            &json!({
                "name": "Support Lead",
                "description": null,
                "permissions": [
                    "organization.overview.read",
                    "organization.audit.read"
                ],
                "expected_version": 1
            })
            .to_string(),
        ))
        .await?;
    assert_eq!(changed.status(), StatusCode::OK);
    assert_eq!(response_json(changed).await["version"], 2);

    let stale = app
        .clone()
        .oneshot(json_method_request(
            "PATCH",
            &detail_path,
            OWNER_TOKEN,
            &json!({
                "name": "Stale",
                "description": null,
                "permissions": ["organization.overview.read"],
                "expected_version": 1
            })
            .to_string(),
        ))
        .await?;
    assert_eq!(stale.status(), StatusCode::CONFLICT);
    assert_eq!(
        response_json(stale).await["code"],
        "organization_lifecycle_conflict"
    );

    let disabled = app
        .clone()
        .oneshot(json_request(
            &format!("{detail_path}/disable"),
            OWNER_TOKEN,
            &json!({"expected_version": 2}).to_string(),
        ))
        .await?;
    assert_eq!(disabled.status(), StatusCode::OK);
    let disabled = response_json(disabled).await;
    assert_eq!(disabled["status"], "disabled");
    assert_eq!(disabled["version"], 3);

    let invalid_permission = app
        .clone()
        .oneshot(json_request(
            &roles_path,
            OWNER_TOKEN,
            &json!({
                "name": "Owner Clone",
                "description": null,
                "permissions": ["organization.roles.manage"]
            })
            .to_string(),
        ))
        .await?;
    assert_eq!(invalid_permission.status(), StatusCode::BAD_REQUEST);
    Ok(())
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
    json_method_request("POST", path, token, body)
}

fn json_method_request(method: &str, path: &str, token: &str, body: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(path)
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(body.to_owned()))
        .unwrap()
}

fn test_entitlement() -> Result<OrganizationEntitlementWrite, Box<dyn std::error::Error>> {
    Ok(OrganizationEntitlementWrite::new(
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
    )?)
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
mod organization_activity_support;
