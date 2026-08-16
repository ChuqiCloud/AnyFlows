use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use af_admin::{
    AdminGroupCreateCommand, AdminGroupWriter, AdminUserCreateCommand, AdminUserStatus,
    AdminUserWriter, DatabaseAdminGroupWriter, DatabaseAdminUserWriter,
    DatabaseOrganizationAuthorizationService, DatabaseOrganizationTokenService, LoginCredentials,
    SessionAuthentication, SessionAuthenticationError, SessionAuthenticationFuture,
    SessionAuthenticator, SessionLoginFuture, SessionPrincipal, SessionRole,
};
use af_db::{
    AdminGroupRepository, AdminUserRepository, DatabaseOptions, MigrationOptions,
    OrganizationCreate, OrganizationEntitlementWrite, OrganizationMembershipChange,
    OrganizationMembershipCreate, OrganizationRepository, OrganizationTeamCreate,
    OrganizationTokenLookupOutcome, OrganizationTokenRepository, connect_and_migrate,
};
use af_domain::{
    GroupId, OrganizationEntitlementCapacity, OrganizationEntitlementSnapshot, OrganizationId,
    OrganizationMembershipId, OrganizationMembershipStatus, OrganizationRole, TokenId, UserId,
};
use af_http::{
    Body, Response, Router, build_organization_token_router,
    http::{Method, Request, StatusCode, header::AUTHORIZATION},
    to_bytes,
};
use serde_json::{Value, json};
use tower::ServiceExt as _;

const OWNER_TOKEN: &str = "organization-token-owner";
const ADMIN_TOKEN: &str = "organization-token-admin";
const BILLING_TOKEN: &str = "organization-token-billing";
const MEMBER_TOKEN: &str = "organization-token-member";

struct TestSessions {
    group_id: GroupId,
    owner_id: UserId,
    admin_id: UserId,
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
            ADMIN_TOKEN => Some(SessionPrincipal::new(self.admin_id, SessionRole::User)),
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

struct Fixture {
    app: Router,
    pool: af_db::DatabasePool,
    organization_id: OrganizationId,
    other_organization_id: OrganizationId,
    member_id: UserId,
    member_membership_id: OrganizationMembershipId,
    organization_repository: OrganizationRepository,
    token_repository: Arc<OrganizationTokenRepository>,
}

#[tokio::test]
async fn sqlite_organization_token_routes_close_rbac_capacity_and_lifecycle_boundaries()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = fixture().await?;
    let base = format!(
        "/api/organizations/{}/tokens",
        fixture.organization_id.get()
    );

    let forbidden = fixture
        .app
        .clone()
        .oneshot(json_request(
            Method::POST,
            &base,
            BILLING_TOKEN,
            token_body("billing-key", "enabled"),
        ))
        .await?;
    assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);

    let created = fixture
        .app
        .clone()
        .oneshot(json_request(
            Method::POST,
            &base,
            MEMBER_TOKEN,
            token_body("member-key-1", "enabled"),
        ))
        .await?;
    assert_eq!(created.status(), StatusCode::CREATED);
    let created = response_json(created).await;
    let first_key = created["api_key"]
        .as_str()
        .expect("创建响应必须返回明文 Key");
    assert!(first_key.starts_with("sk-af-"));
    assert_eq!(first_key.len(), 49);
    let first_id = parse_token_id(&created["token"]["id"])?;

    let own_list = fixture
        .app
        .clone()
        .oneshot(authenticated_request(Method::GET, &base, MEMBER_TOKEN))
        .await?;
    assert_eq!(own_list.status(), StatusCode::OK);
    let own_list_bytes = to_bytes(own_list.into_body(), usize::MAX).await?;
    assert!(!String::from_utf8_lossy(&own_list_bytes).contains(first_key));
    let own_list: Value = serde_json::from_slice(&own_list_bytes)?;
    assert_eq!(own_list["tokens"].as_array().map(Vec::len), Some(1));
    assert_eq!(own_list["tokens"][0]["id"], first_id.get());

    let directory_path = format!("{base}/directory");
    let member_directory = fixture
        .app
        .clone()
        .oneshot(authenticated_request(
            Method::GET,
            &directory_path,
            MEMBER_TOKEN,
        ))
        .await?;
    assert_eq!(member_directory.status(), StatusCode::FORBIDDEN);

    let admin_directory = fixture
        .app
        .clone()
        .oneshot(authenticated_request(
            Method::GET,
            &directory_path,
            ADMIN_TOKEN,
        ))
        .await?;
    assert_eq!(admin_directory.status(), StatusCode::OK);
    let admin_directory = response_json(admin_directory).await;
    assert_eq!(
        admin_directory["tokens"][0]["owner_user_id"],
        fixture.member_id.get()
    );
    assert!(admin_directory["tokens"][0].get("api_key").is_none());

    let first_path = format!("{base}/{}", first_id.get());
    let admin_cannot_use_self_read = fixture
        .app
        .clone()
        .oneshot(authenticated_request(Method::GET, &first_path, ADMIN_TOKEN))
        .await?;
    assert_eq!(admin_cannot_use_self_read.status(), StatusCode::NOT_FOUND);

    let disabled_by_admin = fixture
        .app
        .clone()
        .oneshot(authenticated_request(
            Method::POST,
            &format!("{first_path}/disable"),
            ADMIN_TOKEN,
        ))
        .await?;
    assert_eq!(disabled_by_admin.status(), StatusCode::OK);
    assert_eq!(response_json(disabled_by_admin).await["status"], "disabled");

    let duplicate_query = fixture
        .app
        .clone()
        .oneshot(authenticated_request(
            Method::GET,
            &format!("{base}?limit=1&limit=2"),
            MEMBER_TOKEN,
        ))
        .await?;
    assert_eq!(duplicate_query.status(), StatusCode::BAD_REQUEST);

    let second = create_token(&fixture.app, &base, MEMBER_TOKEN, "member-key-2").await?;
    let team = fixture
        .organization_repository
        .create_team(
            fixture.organization_id,
            OrganizationTeamCreate::new("Gateway".to_owned(), None),
        )
        .await?;
    let moved = fixture
        .organization_repository
        .change_membership(
            fixture.organization_id,
            fixture.member_membership_id,
            OrganizationMembershipChange::new(
                OrganizationRole::Member,
                OrganizationMembershipStatus::Active,
                Some(team.id()),
                1,
            ),
        )
        .await?;
    assert_token_status(
        &fixture.token_repository,
        fixture.organization_id,
        second,
        2,
    )
    .await?;

    let third = create_token(&fixture.app, &base, MEMBER_TOKEN, "member-key-3").await?;
    let suspended = fixture
        .organization_repository
        .change_membership(
            fixture.organization_id,
            fixture.member_membership_id,
            OrganizationMembershipChange::new(
                OrganizationRole::Member,
                OrganizationMembershipStatus::Suspended,
                Some(team.id()),
                moved.version(),
            ),
        )
        .await?;
    assert_token_status(&fixture.token_repository, fixture.organization_id, third, 2).await?;
    let resumed = fixture
        .organization_repository
        .change_membership(
            fixture.organization_id,
            fixture.member_membership_id,
            OrganizationMembershipChange::new(
                OrganizationRole::Member,
                OrganizationMembershipStatus::Active,
                Some(team.id()),
                suspended.version(),
            ),
        )
        .await?;
    assert!(resumed.status().is_active());
    assert_token_status(&fixture.token_repository, fixture.organization_id, third, 2).await?;

    create_token(&fixture.app, &base, OWNER_TOKEN, "owner-key").await?;
    let at_capacity = fixture
        .app
        .clone()
        .oneshot(json_request(
            Method::POST,
            &base,
            OWNER_TOKEN,
            token_body("owner-key-over-limit", "enabled"),
        ))
        .await?;
    assert_eq!(at_capacity.status(), StatusCode::CONFLICT);
    assert_eq!(
        response_json(at_capacity).await["code"],
        "token_limit_reached"
    );

    let cross_organization = fixture
        .app
        .clone()
        .oneshot(authenticated_request(
            Method::GET,
            &format!(
                "/api/organizations/{}/tokens",
                fixture.other_organization_id.get()
            ),
            MEMBER_TOKEN,
        ))
        .await?;
    assert_eq!(cross_organization.status(), StatusCode::NOT_FOUND);

    fixture.pool.close().await?;
    Ok(())
}

async fn fixture() -> Result<Fixture, Box<dyn std::error::Error>> {
    let pool = connect_and_migrate(
        &DatabaseOptions::new("sqlite::memory:")?,
        MigrationOptions::default(),
    )
    .await?;
    let groups = AdminGroupRepository::new(pool.clone(), Duration::from_secs(5))?;
    let setup_principal = SessionPrincipal::new(UserId::new(900)?, SessionRole::Admin);
    let group = DatabaseAdminGroupWriter::new(groups)
        .create(
            setup_principal,
            AdminGroupCreateCommand::new(
                "organization-token-http".to_owned(),
                "Organization Token HTTP".to_owned(),
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
    let owner = create_user(&users, setup_principal, group.group_id(), "key-owner").await?;
    let admin = create_user(&users, setup_principal, group.group_id(), "key-admin").await?;
    let billing = create_user(&users, setup_principal, group.group_id(), "key-billing").await?;
    let member = create_user(&users, setup_principal, group.group_id(), "key-member").await?;
    let organization_repository =
        OrganizationRepository::new(pool.clone(), Duration::from_secs(5))?;
    let (organization, _) = organization_repository
        .create(OrganizationCreate::new(
            "Token Test Organization".to_owned(),
            "token-test-organization".to_owned(),
            owner.user_id(),
            test_entitlement()?,
        ))
        .await?
        .into_parts();
    organization_repository
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
    organization_repository
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
    let (other_organization, _) = organization_repository
        .create(OrganizationCreate::new(
            "Other Token Organization".to_owned(),
            "other-token-organization".to_owned(),
            owner.user_id(),
            test_entitlement()?,
        ))
        .await?
        .into_parts();

    let authorization = Arc::new(DatabaseOrganizationAuthorizationService::new(Arc::new(
        organization_repository.clone(),
    )));
    let token_repository = Arc::new(OrganizationTokenRepository::new(
        pool.clone(),
        Duration::from_secs(5),
    )?);
    let token_service = Arc::new(DatabaseOrganizationTokenService::new(Arc::clone(
        &token_repository,
    )));
    let sessions = Arc::new(TestSessions {
        group_id: group.group_id(),
        owner_id: owner.user_id(),
        admin_id: admin.user_id(),
        billing_id: billing.user_id(),
        member_id: member.user_id(),
    });
    Ok(Fixture {
        app: build_organization_token_router(authorization, token_service, sessions),
        pool,
        organization_id: organization.id(),
        other_organization_id: other_organization.id(),
        member_id: member.user_id(),
        member_membership_id: member_membership.id(),
        organization_repository,
        token_repository,
    })
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
            OrganizationEntitlementCapacity::new(10, 10, 4)?,
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

async fn create_token(
    app: &Router,
    base: &str,
    session_token: &str,
    name: &str,
) -> Result<TokenId, Box<dyn std::error::Error>> {
    let response = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            base,
            session_token,
            token_body(name, "enabled"),
        ))
        .await?;
    assert_eq!(response.status(), StatusCode::CREATED);
    parse_token_id(&response_json(response).await["token"]["id"])
}

async fn assert_token_status(
    repository: &OrganizationTokenRepository,
    organization_id: OrganizationId,
    token_id: TokenId,
    expected: i16,
) -> Result<(), Box<dyn std::error::Error>> {
    let record = repository.get(organization_id, None, token_id).await?;
    let OrganizationTokenLookupOutcome::Found(record) = record else {
        panic!("企业 API Key 必须存在");
    };
    assert_eq!(record.token().status(), expected);
    Ok(())
}

fn parse_token_id(value: &Value) -> Result<TokenId, Box<dyn std::error::Error>> {
    let raw = value.as_i64().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, "Key 标识必须是整数")
    })?;
    Ok(TokenId::new(raw)?)
}

fn token_body(name: &str, status: &str) -> String {
    json!({
        "name": name,
        "status": status,
        "remain_quota": 100,
        "unlimited_quota": false,
        "expired_at": null,
        "model_limits": null,
        "allow_ips": null,
        "rate_limit_5h": null,
        "rate_limit_1d": null,
        "rate_limit_7d": null,
        "max_requests": null
    })
    .to_string()
}

fn authenticated_request(method: Method, path: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(path)
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap()
}

fn json_request(method: Method, path: &str, token: &str, body: String) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(path)
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(body))
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
