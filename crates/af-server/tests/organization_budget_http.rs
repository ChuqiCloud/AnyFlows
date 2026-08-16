use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use af_admin::{
    AdminGroupCreateCommand, AdminGroupWriter, AdminUserCreateCommand, AdminUserStatus,
    AdminUserWriter, DatabaseAdminGroupWriter, DatabaseAdminUserWriter,
    DatabaseOrganizationAuthorizationService, DatabaseOrganizationBudgetService, LoginCredentials,
    SessionAuthentication, SessionAuthenticationError, SessionAuthenticationFuture,
    SessionAuthenticator, SessionLoginFuture, SessionPrincipal, SessionRole,
};
use af_db::{
    AdminGroupRepository, AdminUserRepository, DatabaseOptions, MigrationOptions,
    OrganizationBillingPrincipal, OrganizationBudgetPolicyWrite, OrganizationBudgetRepository,
    OrganizationBudgetScope, OrganizationCreate, OrganizationEntitlementWrite,
    OrganizationMembershipCreate, OrganizationRepository, OrganizationWalletAdjustmentWrite,
    OrganizationWalletRepository, connect_and_migrate,
};
use af_domain::{
    BillingReservationId, GroupId, OrganizationBudgetLimit, OrganizationBudgetPeriod,
    OrganizationEntitlementCapacity, OrganizationEntitlementSnapshot, OrganizationId,
    OrganizationRole, Quota, QuotaDelta, UserId, WalletEventId,
};
use af_http::{
    Body, Response, Router, build_organization_budget_router,
    http::{Request, StatusCode, header::AUTHORIZATION},
    to_bytes,
};
use serde_json::{Value, json};
use tower::ServiceExt as _;

const OWNER_TOKEN: &str = "organization-budget-owner";
const BILLING_TOKEN: &str = "organization-budget-billing";
const MEMBER_TOKEN: &str = "organization-budget-member";

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

#[tokio::test]
async fn sqlite_budget_routes_enforce_rbac_validation_remaining_and_org_boundary()
-> Result<(), Box<dyn std::error::Error>> {
    let (
        app,
        pool,
        organization,
        owner,
        _billing,
        member,
        member_membership,
        budget,
        wallet,
        groups,
    ) = sqlite_budget_router().await?;
    let path = format!("/api/organizations/{}/budgets", organization.get());

    let forbidden = app
        .clone()
        .oneshot(authenticated_request(&path, MEMBER_TOKEN))
        .await?;
    assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);
    assert_eq!(response_json(forbidden).await["code"], "forbidden");

    let created = app
        .clone()
        .oneshot(json_request(
            &path,
            OWNER_TOKEN,
            &json!({
                "scope": {"kind": "organization"},
                "period": {"kind": "fixed", "seconds": 3600},
                "quota_limit": 100,
                "effective_from": 1_700_000_000
            })
            .to_string(),
        ))
        .await?;
    assert_eq!(created.status(), StatusCode::CREATED);

    let invalid = app
        .clone()
        .oneshot(json_request(
            &path,
            BILLING_TOKEN,
            &json!({
                "scope": {"kind": "organization"},
                "period": {"kind": "fixed", "seconds": 1},
                "quota_limit": 100,
                "effective_from": 1_700_000_001
            })
            .to_string(),
        ))
        .await?;
    assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);

    let conflict = app
        .clone()
        .oneshot(json_request(
            &path,
            OWNER_TOKEN,
            &json!({
                "scope": {"kind": "organization"},
                "period": {"kind": "fixed", "seconds": 3600},
                "quota_limit": 100,
                "effective_from": 1_700_000_000
            })
            .to_string(),
        ))
        .await?;
    assert_eq!(conflict.status(), StatusCode::CONFLICT);

    wallet
        .adjust(OrganizationWalletAdjustmentWrite::new(
            WalletEventId::new([1; 16])?,
            organization,
            owner,
            QuotaDelta::new(100)?,
            "预算测试充值".to_owned(),
        )?)
        .await?;
    let reservation_id = BillingReservationId::new([2; 16])?;
    budget
        .reserve(
            reservation_id,
            OrganizationBillingPrincipal::new(organization, member_membership, None, member),
            Quota::new(30)?,
        )
        .await?;

    let overview = app
        .clone()
        .oneshot(authenticated_request(&path, BILLING_TOKEN))
        .await?;
    assert_eq!(overview.status(), StatusCode::OK);
    let overview = response_json(overview).await;
    assert_eq!(overview["windows"][0]["limit_snapshot"], 100);
    assert_eq!(overview["windows"][0]["used_quota"], 0);
    assert_eq!(overview["windows"][0]["frozen_quota"], 30);
    assert_eq!(overview["windows"][0]["remaining_quota"], 70);

    let (other_organization, other_membership) = groups
        .create(OrganizationCreate::new(
            "Other Budget Organization".to_owned(),
            "other-budget-organization".to_owned(),
            owner,
            test_entitlement()?,
        ))
        .await?
        .into_parts();
    let other_policy = OrganizationBudgetPolicyWrite::new(
        other_organization.id(),
        OrganizationBudgetScope::Organization,
        OrganizationBudgetPeriod::Fixed { seconds: 3600 },
        OrganizationBudgetLimit::new(Some(Quota::new(100)?))?,
        af_admin::DatabaseTimestamp::from_unix_timestamp(1_700_000_000)?,
        owner,
    );
    budget.create_policy(other_policy).await?;
    wallet
        .adjust(OrganizationWalletAdjustmentWrite::new(
            WalletEventId::new([3; 16])?,
            other_organization.id(),
            owner,
            QuotaDelta::new(100)?,
            "跨企业预算测试充值".to_owned(),
        )?)
        .await?;
    let other_reservation_id = BillingReservationId::new([4; 16])?;
    budget
        .reserve(
            other_reservation_id,
            OrganizationBillingPrincipal::new(
                other_organization.id(),
                other_membership.id(),
                None,
                owner,
            ),
            Quota::new(10)?,
        )
        .await?;

    let reservation_path = format!(
        "{path}/reservations/{}",
        other_reservation_id.persistence_key()
    );
    let cross_organization = app
        .oneshot(authenticated_request(&reservation_path, OWNER_TOKEN))
        .await?;
    assert_eq!(cross_organization.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        response_json(cross_organization).await["code"],
        "organization_not_found"
    );

    pool.close().await?;
    Ok(())
}

async fn sqlite_budget_router() -> Result<
    (
        Router,
        af_db::DatabasePool,
        OrganizationId,
        UserId,
        UserId,
        UserId,
        af_domain::OrganizationMembershipId,
        Arc<OrganizationBudgetRepository>,
        Arc<OrganizationWalletRepository>,
        OrganizationRepository,
    ),
    Box<dyn std::error::Error>,
> {
    let pool = connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let group_repository = AdminGroupRepository::new(pool.clone(), Duration::from_secs(5))?;
    let setup_principal = SessionPrincipal::new(UserId::new(900)?, SessionRole::Admin);
    let group = DatabaseAdminGroupWriter::new(group_repository)
        .create(
            setup_principal,
            AdminGroupCreateCommand::new(
                "organization-budget-http".to_owned(),
                "Organization Budget HTTP".to_owned(),
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
    let owner = create_user(&users, setup_principal, group.group_id(), "budget-owner").await?;
    let billing = create_user(&users, setup_principal, group.group_id(), "budget-billing").await?;
    let member = create_user(&users, setup_principal, group.group_id(), "budget-member").await?;
    let organization_repository =
        OrganizationRepository::new(pool.clone(), Duration::from_secs(5))?;
    let (organization, _) = organization_repository
        .create(OrganizationCreate::new(
            "Budget Test Organization".to_owned(),
            "budget-test-organization".to_owned(),
            owner.user_id(),
            test_entitlement()?,
        ))
        .await?
        .into_parts();
    let _billing_membership = organization_repository
        .add_membership(
            organization.id(),
            OrganizationMembershipCreate::new(
                billing.user_id(),
                OrganizationRole::Billing,
                None,
                Some(owner.user_id()),
            ),
        )
        .await?;
    let member_membership = organization_repository
        .add_membership(
            organization.id(),
            OrganizationMembershipCreate::new(
                member.user_id(),
                OrganizationRole::Member,
                None,
                Some(owner.user_id()),
            ),
        )
        .await?;
    let authorization = Arc::new(DatabaseOrganizationAuthorizationService::new(Arc::new(
        organization_repository.clone(),
    )));
    let budget = Arc::new(OrganizationBudgetRepository::new(
        pool.clone(),
        Duration::from_secs(5),
        Duration::from_secs(900),
    )?);
    let wallet = Arc::new(OrganizationWalletRepository::new(
        pool.clone(),
        Duration::from_secs(5),
    )?);
    let sessions = Arc::new(TestSessions {
        group_id: group.group_id(),
        owner_id: owner.user_id(),
        billing_id: billing.user_id(),
        member_id: member.user_id(),
    });
    let service = Arc::new(DatabaseOrganizationBudgetService::new(Arc::clone(&budget)));
    Ok((
        build_organization_budget_router(authorization, service, sessions),
        pool,
        organization.id(),
        owner.user_id(),
        billing.user_id(),
        member.user_id(),
        member_membership.id(),
        budget,
        wallet,
        organization_repository,
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
