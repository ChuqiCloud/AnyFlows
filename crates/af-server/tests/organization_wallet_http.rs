use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use af_admin::{
    AdminGroupCreateCommand, AdminGroupWriter, AdminUserCreateCommand, AdminUserStatus,
    AdminUserWriter, DatabaseAdminGroupWriter, DatabaseAdminUserWriter,
    DatabaseOrganizationAuthorizationService, DatabaseOrganizationWalletService, LoginCredentials,
    SessionAuthentication, SessionAuthenticationError, SessionAuthenticationFuture,
    SessionAuthenticator, SessionLoginFuture, SessionPrincipal, SessionRole,
};
use af_db::{
    AdminGroupRepository, AdminUserRepository, DatabaseOptions, MigrationOptions,
    OrganizationCreate, OrganizationEntitlementWrite, OrganizationMembershipCreate,
    OrganizationRepository, OrganizationWalletRepository, connect_and_migrate,
};
use af_domain::{
    GroupId, OrganizationEntitlementCapacity, OrganizationEntitlementSnapshot, OrganizationId,
    OrganizationRole, UserId,
};
use af_http::{
    Body, Response, Router, build_organization_wallet_router,
    http::{Request, StatusCode, header::AUTHORIZATION},
    to_bytes,
};
use serde_json::{Value, json};
use tower::ServiceExt as _;

const OWNER_TOKEN: &str = "organization-wallet-owner";
const BILLING_TOKEN: &str = "organization-wallet-billing";
const MEMBER_TOKEN: &str = "organization-wallet-member";

struct TestSessions {
    group_id: GroupId,
    owner_id: UserId,
    billing_id: UserId,
    member_id: UserId,
}

impl SessionAuthenticator for TestSessions {
    fn login<'a>(&'a self, _credentials: &'a LoginCredentials) -> SessionLoginFuture<'a> {
        Box::pin(async { Err(SessionAuthenticationError::InvalidCredentials) })
    }

    fn authenticate<'a>(&'a self, token: &'a str) -> SessionAuthenticationFuture<'a> {
        let principal = match token {
            OWNER_TOKEN => Some(SessionPrincipal::new(self.owner_id, SessionRole::User)),
            BILLING_TOKEN => Some(SessionPrincipal::new(self.billing_id, SessionRole::User)),
            MEMBER_TOKEN => Some(SessionPrincipal::new(self.member_id, SessionRole::User)),
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

/// 真实 SQLite 链路必须同时守住企业 RBAC、调账幂等和只追加账本边界。
#[tokio::test]
async fn sqlite_wallet_routes_enforce_permissions_idempotency_and_balance_guards()
-> Result<(), Box<dyn std::error::Error>> {
    let (app, pool, organization_id) = sqlite_wallet_router().await?;
    let wallet_path = format!("/api/organizations/{}/wallet", organization_id.get());
    let entries_path = format!("{wallet_path}/entries");
    let adjustments_path = format!("{wallet_path}/adjustments");

    let forbidden = app
        .clone()
        .oneshot(authenticated_request(&wallet_path, MEMBER_TOKEN))
        .await?;
    assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);
    assert_eq!(response_json(forbidden).await["code"], "forbidden");

    let empty = app
        .clone()
        .oneshot(authenticated_request(&wallet_path, BILLING_TOKEN))
        .await?;
    assert_eq!(empty.status(), StatusCode::OK);
    assert_eq!(empty.headers()["cache-control"], "no-store");
    let empty = response_json(empty).await;
    assert_eq!(empty["balance"], 0);
    assert_eq!(empty["used_quota"], 0);
    assert_eq!(empty["frozen_quota"], 0);
    assert_eq!(empty["version"], 1);

    let positive_body = json!({
        "event_id": "11111111111111111111111111111111",
        "quota_delta": 100,
        "reason": "合同预存额度"
    })
    .to_string();
    let created = app
        .clone()
        .oneshot(json_request(&adjustments_path, OWNER_TOKEN, &positive_body))
        .await?;
    assert_eq!(created.status(), StatusCode::CREATED);
    let created = response_json(created).await;
    assert_eq!(created["replayed"], false);
    assert_eq!(created["entry"]["balance_before"], 0);
    assert_eq!(created["entry"]["balance_after"], 100);
    assert_eq!(created["entry"]["entry_type"], "admin_adjustment");

    let replayed = app
        .clone()
        .oneshot(json_request(&adjustments_path, OWNER_TOKEN, &positive_body))
        .await?;
    assert_eq!(replayed.status(), StatusCode::OK);
    assert_eq!(response_json(replayed).await["replayed"], true);

    let conflicting = app
        .clone()
        .oneshot(json_request(
            &adjustments_path,
            OWNER_TOKEN,
            &json!({
                "event_id": "11111111111111111111111111111111",
                "quota_delta": 101,
                "reason": "合同预存额度"
            })
            .to_string(),
        ))
        .await?;
    assert_eq!(conflicting.status(), StatusCode::CONFLICT);
    assert_eq!(
        response_json(conflicting).await["code"],
        "wallet_event_conflict"
    );

    let debited = app
        .clone()
        .oneshot(json_request(
            &adjustments_path,
            BILLING_TOKEN,
            &json!({
                "event_id": "22222222222222222222222222222222",
                "quota_delta": -30,
                "reason": "账务冲正"
            })
            .to_string(),
        ))
        .await?;
    assert_eq!(debited.status(), StatusCode::CREATED);
    assert_eq!(response_json(debited).await["entry"]["balance_after"], 70);

    let insufficient = app
        .clone()
        .oneshot(json_request(
            &adjustments_path,
            BILLING_TOKEN,
            &json!({
                "event_id": "33333333333333333333333333333333",
                "quota_delta": -100,
                "reason": "越界扣减"
            })
            .to_string(),
        ))
        .await?;
    assert_eq!(insufficient.status(), StatusCode::CONFLICT);
    assert_eq!(
        response_json(insufficient).await["code"],
        "wallet_insufficient_quota"
    );

    let first_page = app
        .clone()
        .oneshot(authenticated_request(
            &format!("{entries_path}?limit=1"),
            BILLING_TOKEN,
        ))
        .await?;
    assert_eq!(first_page.status(), StatusCode::OK);
    let first_page = response_json(first_page).await;
    assert_eq!(first_page["entries"].as_array().unwrap().len(), 1);
    assert_eq!(first_page["entries"][0]["quota_delta"], -30);
    let cursor = first_page["next_cursor"].as_i64().unwrap();
    let second_page = app
        .clone()
        .oneshot(authenticated_request(
            &format!("{entries_path}?before={cursor}&limit=1"),
            BILLING_TOKEN,
        ))
        .await?;
    let second_page = response_json(second_page).await;
    assert_eq!(second_page["entries"][0]["quota_delta"], 100);
    assert!(second_page["next_cursor"].is_null());

    let summary = app
        .oneshot(authenticated_request(&wallet_path, BILLING_TOKEN))
        .await?;
    let summary = response_json(summary).await;
    assert_eq!(summary["balance"], 70);
    assert_eq!(summary["version"], 3);

    pool.close().await?;
    Ok(())
}

async fn sqlite_wallet_router()
-> Result<(Router, af_db::DatabasePool, OrganizationId), Box<dyn std::error::Error>> {
    let pool = connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let setup_principal = SessionPrincipal::new(UserId::new(900)?, SessionRole::Admin);
    let group_repository = AdminGroupRepository::new(pool.clone(), Duration::from_secs(5))?;
    let group = DatabaseAdminGroupWriter::new(group_repository)
        .create(
            setup_principal,
            AdminGroupCreateCommand::new(
                "organization-wallet-http".to_owned(),
                "Organization Wallet HTTP".to_owned(),
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
    let owner = create_user(&users, setup_principal, group.group_id(), "wallet-owner").await?;
    let billing = create_user(&users, setup_principal, group.group_id(), "wallet-billing").await?;
    let member = create_user(&users, setup_principal, group.group_id(), "wallet-member").await?;

    let organization_repository = Arc::new(OrganizationRepository::new(
        pool.clone(),
        Duration::from_secs(5),
    )?);
    let (organization, _) = organization_repository
        .create(OrganizationCreate::new(
            "Wallet Test Organization".to_owned(),
            "wallet-test-organization".to_owned(),
            owner.user_id(),
            test_entitlement()?,
        ))
        .await?
        .into_parts();
    for (user_id, role) in [
        (billing.user_id(), OrganizationRole::Billing),
        (member.user_id(), OrganizationRole::Member),
    ] {
        organization_repository
            .add_membership(
                organization.id(),
                OrganizationMembershipCreate::new(user_id, role, None, Some(owner.user_id())),
            )
            .await?;
    }

    let authorization = Arc::new(DatabaseOrganizationAuthorizationService::new(Arc::clone(
        &organization_repository,
    )));
    let wallet = Arc::new(DatabaseOrganizationWalletService::new(Arc::new(
        OrganizationWalletRepository::new(pool.clone(), Duration::from_secs(5))?,
    )));
    let sessions = Arc::new(TestSessions {
        group_id: group.group_id(),
        owner_id: owner.user_id(),
        billing_id: billing.user_id(),
        member_id: member.user_id(),
    });
    Ok((
        build_organization_wallet_router(authorization, wallet, None, sessions),
        pool,
        organization.id(),
    ))
}

async fn create_user(
    writer: &DatabaseAdminUserWriter,
    principal: SessionPrincipal,
    group_id: GroupId,
    username: &str,
) -> Result<af_admin::AdminUser, af_admin::AdminUserWriteError> {
    writer
        .create(
            principal,
            AdminUserCreateCommand::new(
                username.to_owned(),
                None,
                None,
                SessionRole::User,
                AdminUserStatus::Enabled,
                group_id,
                0,
                None,
                None,
            )?,
        )
        .await
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

fn now_u64() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
