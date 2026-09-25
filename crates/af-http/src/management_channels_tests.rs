use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use af_admin::{
    AdminChannel, AdminChannelCreateCommand, AdminChannelCreateFuture, AdminChannelDeleteFuture,
    AdminChannelGetFuture, AdminChannelListFuture, AdminChannelListQuery, AdminChannelPage,
    AdminChannelReadError, AdminChannelReader, AdminChannelUpdateCommand, AdminChannelUpdateFuture,
    AdminChannelWriteError, AdminChannelWriter, AdminCredential, AdminCredentialCreateCommand,
    AdminCredentialCreateFuture, AdminCredentialDeleteFuture, AdminCredentialGetFuture,
    AdminCredentialListFuture, AdminCredentialListQuery, AdminCredentialMultiKeyMode,
    AdminCredentialPage, AdminCredentialQuotaDimension, AdminCredentialUpdateCommand,
    AdminCredentialUpdateFuture, AdminGroupCreateCommand, AdminGroupCreateFuture,
    AdminGroupDeleteFuture, AdminGroupGetFuture, AdminGroupListFuture, AdminGroupListQuery,
    AdminGroupReadError, AdminGroupReader, AdminGroupUpdateCommand, AdminGroupUpdateFuture,
    AdminGroupWriteError, AdminGroupWriter, AdminRoutingStatus, AdminTokenCreateCommand,
    AdminTokenCreateFuture, AdminTokenDeleteFuture, AdminTokenGetFuture, AdminTokenListFuture,
    AdminTokenListQuery, AdminTokenReadError, AdminTokenReader, AdminTokenUpdateCommand,
    AdminTokenUpdateFuture, AdminTokenWriteError, AdminTokenWriter, AdminUserCreateCommand,
    AdminUserCreateFuture, AdminUserDeleteFuture, AdminUserGetFuture, AdminUserListFuture,
    AdminUserListQuery, AdminUserReadError, AdminUserReader, AdminUserUpdateCommand,
    AdminUserUpdateFuture, AdminUserWriteError, AdminUserWriter, LoginCredentials,
    SessionAuthentication, SessionAuthenticationError, SessionAuthenticationFuture,
    SessionAuthenticator, SessionLoginFuture, SessionPrincipal, SessionRole,
};
use af_domain::{
    AfError, ChannelId, ChannelType, CredentialId, CredentialKind, GatewayPrincipal, GroupId,
    Protocol, TokenId, UserId,
};
use af_protocol::CanonicalRequestEnvelope;
use axum::{
    Router,
    body::{Body, to_bytes},
    middleware,
    routing::{get, post},
};
use http::{
    Request, StatusCode,
    header::{AUTHORIZATION, CONTENT_TYPE},
};
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::{
    AdminChannelProbe, AdminChannelProbeFuture, AdminChannelProbeOutcome, ChatService,
    ChatServiceFuture,
    chat_completions::HttpState,
    credential_usage::get_admin_credential_usage,
    management_auth::{ManagementAuthenticationState, authenticate_management_session},
    management_authorization::authorize_management_admin,
    management_channel_probe::probe_admin_channel,
    management_channel_writes::{create_admin_channel, delete_admin_channel, update_admin_channel},
    management_channels::{get_admin_channel, list_admin_channels},
    management_credential_writes::{
        create_admin_credential, delete_admin_credential, update_admin_credential,
    },
    management_credentials::{get_admin_credential, list_admin_credentials},
};

const ADMIN_TOKEN: &str = "admin-token";
const USER_TOKEN: &str = "user-token";

struct UnusedChatService;
struct UnusedAdminAccess;

struct FakeAdminChannelProbe {
    calls: AtomicUsize,
    outcome: AdminChannelProbeOutcome,
}

impl FakeAdminChannelProbe {
    fn new(outcome: AdminChannelProbeOutcome) -> Self {
        Self {
            calls: AtomicUsize::new(0),
            outcome,
        }
    }
}

impl AdminChannelProbe for FakeAdminChannelProbe {
    fn probe<'a>(&'a self, _channel_id: ChannelId) -> AdminChannelProbeFuture<'a> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        Box::pin(async move { self.outcome })
    }
}

impl ChatService for UnusedChatService {
    fn chat_completions<'a>(
        &'a self,
        _principal: &'a GatewayPrincipal,
        _user_concurrency: Option<af_domain::ConcurrencyLimit>,
        _request: CanonicalRequestEnvelope,
        _response_protocol: af_domain::Protocol,
        _request_id: &'a str,
        _diagnostic: af_relay::RelayDiagnosticInput,
    ) -> ChatServiceFuture<'a> {
        Box::pin(async { Err(AfError::Internal) })
    }
}

impl AdminGroupReader for UnusedAdminAccess {
    fn list<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _query: AdminGroupListQuery,
    ) -> AdminGroupListFuture<'a> {
        Box::pin(async { Err(AdminGroupReadError::Internal) })
    }

    fn get<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _group_id: GroupId,
    ) -> AdminGroupGetFuture<'a> {
        Box::pin(async { Err(AdminGroupReadError::Internal) })
    }
}

impl AdminGroupWriter for UnusedAdminAccess {
    fn create<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _command: AdminGroupCreateCommand,
    ) -> AdminGroupCreateFuture<'a> {
        Box::pin(async { Err(AdminGroupWriteError::Internal) })
    }

    fn update<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _group_id: GroupId,
        _command: AdminGroupUpdateCommand,
    ) -> AdminGroupUpdateFuture<'a> {
        Box::pin(async { Err(AdminGroupWriteError::Internal) })
    }

    fn delete<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _group_id: GroupId,
    ) -> AdminGroupDeleteFuture<'a> {
        Box::pin(async { Err(AdminGroupWriteError::Internal) })
    }
}

impl AdminTokenReader for UnusedAdminAccess {
    fn list<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _query: AdminTokenListQuery,
    ) -> AdminTokenListFuture<'a> {
        Box::pin(async { Err(AdminTokenReadError::Internal) })
    }

    fn get<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _token_id: TokenId,
    ) -> AdminTokenGetFuture<'a> {
        Box::pin(async { Err(AdminTokenReadError::Internal) })
    }
}

impl AdminTokenWriter for UnusedAdminAccess {
    fn create<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _command: AdminTokenCreateCommand,
    ) -> AdminTokenCreateFuture<'a> {
        Box::pin(async { Err(AdminTokenWriteError::Internal) })
    }

    fn update<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _token_id: TokenId,
        _command: AdminTokenUpdateCommand,
    ) -> AdminTokenUpdateFuture<'a> {
        Box::pin(async { Err(AdminTokenWriteError::Internal) })
    }

    fn delete<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _token_id: TokenId,
    ) -> AdminTokenDeleteFuture<'a> {
        Box::pin(async { Err(AdminTokenWriteError::Internal) })
    }
}

impl AdminUserReader for UnusedAdminAccess {
    fn list<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _query: AdminUserListQuery,
    ) -> AdminUserListFuture<'a> {
        Box::pin(async { Err(AdminUserReadError::Internal) })
    }

    fn get<'a>(&'a self, _principal: SessionPrincipal, _user_id: UserId) -> AdminUserGetFuture<'a> {
        Box::pin(async { Err(AdminUserReadError::Internal) })
    }
}

impl AdminUserWriter for UnusedAdminAccess {
    fn create<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _command: AdminUserCreateCommand,
    ) -> AdminUserCreateFuture<'a> {
        Box::pin(async { Err(AdminUserWriteError::Internal) })
    }

    fn update<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _user_id: UserId,
        _command: AdminUserUpdateCommand,
    ) -> AdminUserUpdateFuture<'a> {
        Box::pin(async { Err(AdminUserWriteError::Internal) })
    }

    fn delete<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _user_id: UserId,
    ) -> AdminUserDeleteFuture<'a> {
        Box::pin(async { Err(AdminUserWriteError::Internal) })
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
                        GroupId::new(1).unwrap(),
                        current_timestamp() + 60,
                    )
                })
                .ok_or(SessionAuthenticationError::InvalidSession)
        })
    }
}

struct FakeAdminChannelReader {
    calls: AtomicUsize,
}

impl FakeAdminChannelReader {
    fn new() -> Self {
        Self {
            calls: AtomicUsize::new(0),
        }
    }
}

impl AdminChannelReader for FakeAdminChannelReader {
    fn list_channels<'a>(
        &'a self,
        principal: SessionPrincipal,
        query: AdminChannelListQuery,
    ) -> AdminChannelListFuture<'a> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        Box::pin(async move {
            assert_eq!(principal.role(), SessionRole::Admin);
            if query.after().map(ChannelId::get) == Some(500) {
                return Err(AdminChannelReadError::Internal);
            }
            Ok(AdminChannelPage::from_parts(
                vec![sample_channel(10, "primary"), sample_channel(11, "backup")],
                Some(ChannelId::new(11).unwrap()),
            ))
        })
    }

    fn get_channel<'a>(
        &'a self,
        principal: SessionPrincipal,
        channel_id: ChannelId,
    ) -> AdminChannelGetFuture<'a> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        Box::pin(async move {
            assert_eq!(principal.role(), SessionRole::Admin);
            match channel_id.get() {
                404 => Err(AdminChannelReadError::ChannelNotFound),
                500 => Err(AdminChannelReadError::Internal),
                id => Ok(sample_channel(id, "detail")),
            }
        })
    }

    fn list_credentials<'a>(
        &'a self,
        principal: SessionPrincipal,
        channel_id: ChannelId,
        _query: AdminCredentialListQuery,
    ) -> AdminCredentialListFuture<'a> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        Box::pin(async move {
            assert_eq!(principal.role(), SessionRole::Admin);
            if channel_id.get() == 404 {
                return Err(AdminChannelReadError::ChannelNotFound);
            }
            Ok(AdminCredentialPage::from_parts(
                vec![sample_credential(channel_id, 20)],
                Some(CredentialId::new(20).unwrap()),
            ))
        })
    }

    fn get_credential<'a>(
        &'a self,
        principal: SessionPrincipal,
        channel_id: ChannelId,
        credential_id: CredentialId,
    ) -> AdminCredentialGetFuture<'a> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        Box::pin(async move {
            assert_eq!(principal.role(), SessionRole::Admin);
            if channel_id.get() == 404 {
                return Err(AdminChannelReadError::ChannelNotFound);
            }
            if credential_id.get() == 404 {
                return Err(AdminChannelReadError::CredentialNotFound);
            }
            Ok(sample_credential(channel_id, credential_id.get()))
        })
    }
}

impl AdminChannelWriter for FakeAdminChannelReader {
    fn create_channel<'a>(
        &'a self,
        principal: SessionPrincipal,
        _command: AdminChannelCreateCommand,
    ) -> AdminChannelCreateFuture<'a> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        Box::pin(async move {
            assert_eq!(principal.role(), SessionRole::Admin);
            Ok(sample_channel(90, "created"))
        })
    }

    fn update_channel<'a>(
        &'a self,
        principal: SessionPrincipal,
        channel_id: ChannelId,
        _command: AdminChannelUpdateCommand,
    ) -> AdminChannelUpdateFuture<'a> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        Box::pin(async move {
            assert_eq!(principal.role(), SessionRole::Admin);
            if channel_id.get() == 404 {
                Err(AdminChannelWriteError::ChannelNotFound)
            } else {
                Ok(sample_channel(channel_id.get(), "updated"))
            }
        })
    }

    fn delete_channel<'a>(
        &'a self,
        principal: SessionPrincipal,
        channel_id: ChannelId,
    ) -> AdminChannelDeleteFuture<'a> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        Box::pin(async move {
            assert_eq!(principal.role(), SessionRole::Admin);
            if channel_id.get() == 404 {
                Err(AdminChannelWriteError::ChannelNotFound)
            } else {
                Ok(())
            }
        })
    }

    fn create_credential<'a>(
        &'a self,
        principal: SessionPrincipal,
        channel_id: ChannelId,
        _command: AdminCredentialCreateCommand,
    ) -> AdminCredentialCreateFuture<'a> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        Box::pin(async move {
            assert_eq!(principal.role(), SessionRole::Admin);
            if channel_id.get() == 404 {
                Err(AdminChannelWriteError::ChannelNotFound)
            } else {
                Ok(sample_credential(channel_id, 91))
            }
        })
    }

    fn update_credential<'a>(
        &'a self,
        principal: SessionPrincipal,
        channel_id: ChannelId,
        credential_id: CredentialId,
        _command: AdminCredentialUpdateCommand,
    ) -> AdminCredentialUpdateFuture<'a> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        Box::pin(async move {
            assert_eq!(principal.role(), SessionRole::Admin);
            if credential_id.get() == 404 {
                Err(AdminChannelWriteError::CredentialNotFound)
            } else {
                Ok(sample_credential(channel_id, credential_id.get()))
            }
        })
    }

    fn delete_credential<'a>(
        &'a self,
        principal: SessionPrincipal,
        _channel_id: ChannelId,
        credential_id: CredentialId,
    ) -> AdminCredentialDeleteFuture<'a> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        Box::pin(async move {
            assert_eq!(principal.role(), SessionRole::Admin);
            if credential_id.get() == 404 {
                Err(AdminChannelWriteError::CredentialNotFound)
            } else {
                Ok(())
            }
        })
    }
}

fn router(reader: Arc<FakeAdminChannelReader>) -> Router {
    router_with_probe(
        reader,
        Some(Arc::new(FakeAdminChannelProbe::new(
            AdminChannelProbeOutcome::Healthy,
        ))),
    )
}

fn router_with_probe(
    reader: Arc<FakeAdminChannelReader>,
    probe: Option<Arc<dyn AdminChannelProbe>>,
) -> Router {
    let session_authenticator: Arc<dyn SessionAuthenticator> = Arc::new(FakeSessionAuthenticator);
    let unused = Arc::new(UnusedAdminAccess);
    let collection_authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(Arc::clone(&session_authenticator)),
        authenticate_management_session,
    );
    let item_authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(Arc::clone(&session_authenticator)),
        authenticate_management_session,
    );
    let credential_collection_authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(Arc::clone(&session_authenticator)),
        authenticate_management_session,
    );
    let credential_item_authentication = middleware::from_fn_with_state(
        ManagementAuthenticationState::new(Arc::clone(&session_authenticator)),
        authenticate_management_session,
    );
    let channel_reader: Arc<dyn AdminChannelReader> = reader.clone();
    let channel_writer: Arc<dyn AdminChannelWriter> = reader;
    Router::new()
        .route(
            "/api/admin/channels",
            get(list_admin_channels)
                .post(create_admin_channel)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(collection_authentication),
        )
        .route(
            "/api/admin/channels/{id}",
            get(get_admin_channel)
                .put(update_admin_channel)
                .delete(delete_admin_channel)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(item_authentication.clone()),
        )
        .route(
            "/api/admin/channels/{id}/probe",
            post(probe_admin_channel)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(item_authentication),
        )
        .route(
            "/api/admin/channels/{channel_id}/credentials",
            get(list_admin_credentials)
                .post(create_admin_credential)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(credential_collection_authentication),
        )
        .route(
            "/api/admin/channels/{channel_id}/credentials/{credential_id}",
            get(get_admin_credential)
                .put(update_admin_credential)
                .delete(delete_admin_credential)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(credential_item_authentication),
        )
        .route(
            "/api/admin/channels/{channel_id}/credentials/{credential_id}/usage",
            get(get_admin_credential_usage)
                .route_layer(middleware::from_fn(authorize_management_admin))
                .route_layer(middleware::from_fn_with_state(
                    ManagementAuthenticationState::new(Arc::clone(&session_authenticator)),
                    authenticate_management_session,
                )),
        )
        .with_state(
            HttpState::new(
                Arc::new(UnusedChatService),
                session_authenticator,
                unused.clone(),
                unused.clone(),
                unused.clone(),
                unused.clone(),
                unused.clone(),
                unused,
            )
            .with_admin_channel_reader(channel_reader)
            .with_admin_channel_writer(channel_writer)
            .with_admin_channel_probe(probe),
        )
}

#[tokio::test]
async fn credential_usage_denies_users_and_reports_unsupported_kind() {
    let app = router(Arc::new(FakeAdminChannelReader::new()));
    let path = "/api/admin/channels/10/credentials/20/usage";
    let forbidden = app
        .clone()
        .oneshot(authorized_request(path, USER_TOKEN))
        .await
        .unwrap();
    assert_error(forbidden, StatusCode::FORBIDDEN, "forbidden").await;

    let response = app
        .oneshot(authorized_request(path, ADMIN_TOKEN))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response_json(response).await["status"], "unsupported");
}

#[tokio::test]
async fn admin_probe_returns_only_status_and_latency() {
    for (outcome, expected_status) in [
        (AdminChannelProbeOutcome::Healthy, "healthy"),
        (AdminChannelProbeOutcome::Unhealthy, "unhealthy"),
        (AdminChannelProbeOutcome::TimedOut, "timeout"),
    ] {
        let reader = Arc::new(FakeAdminChannelReader::new());
        let probe = Arc::new(FakeAdminChannelProbe::new(outcome));
        let probe_port: Arc<dyn AdminChannelProbe> = probe.clone();
        let app = router_with_probe(reader, Some(probe_port));

        let response = app
            .oneshot(write_request(
                "POST",
                "/api/admin/channels/10/probe",
                ADMIN_TOKEN,
                &json!({}),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let response = response_json(response).await;
        assert_eq!(response["status"], expected_status);
        assert!(response["latency_ms"].is_u64());
        assert_eq!(response.as_object().unwrap().len(), 2);
        assert_eq!(probe.calls.load(Ordering::Relaxed), 1);
    }
}

#[tokio::test]
async fn probe_requires_admin_and_enabled_service() {
    let reader = Arc::new(FakeAdminChannelReader::new());
    let probe = Arc::new(FakeAdminChannelProbe::new(
        AdminChannelProbeOutcome::Healthy,
    ));
    let probe_port: Arc<dyn AdminChannelProbe> = probe.clone();
    let app = router_with_probe(Arc::clone(&reader), Some(probe_port));

    let forbidden = app
        .oneshot(write_request(
            "POST",
            "/api/admin/channels/10/probe",
            USER_TOKEN,
            &json!({}),
        ))
        .await
        .unwrap();
    assert_error(forbidden, StatusCode::FORBIDDEN, "forbidden").await;
    assert_eq!(probe.calls.load(Ordering::Relaxed), 0);

    let unavailable = router_with_probe(reader, None)
        .oneshot(write_request(
            "POST",
            "/api/admin/channels/10/probe",
            ADMIN_TOKEN,
            &json!({}),
        ))
        .await
        .unwrap();
    assert_error(
        unavailable,
        StatusCode::SERVICE_UNAVAILABLE,
        "probe_unavailable",
    )
    .await;
}

#[tokio::test]
async fn admin_can_create_update_and_delete_channels_and_credentials() {
    let reader = Arc::new(FakeAdminChannelReader::new());
    let app = router(Arc::clone(&reader));
    let channel_body = json!({
        "name": "private-channel",
        "type": "openai",
        "protocol": "openai_chat",
        "base_url": "https://private.example.com/v1",
        "timeout_secs": 60,
        "status": "disabled",
        "weight": 10,
        "priority": 20,
        "auto_ban": true,
        "pool_mode": true,
        "client_simulation_profile": null,
        "client_simulation_risk_accepted": false,
        "client_simulation_body_profile": null,
        "client_simulation_body_risk_accepted": false,
        "responses_websocket_enabled": false,
        "models": ["public"],
        "group_ids": [1],
        "model_mapping": {"public": "upstream"},
        "param_override": {
            "temperature": 0.25,
            "top_p": 0.8,
            "max_output_tokens": 4096,
            "stop_sequences": ["stop"]
        },
        "header_override": {"x-private": "header-secret-canary"},
        "settings": {"private": "settings-secret-canary"},
        "tag": null
    });
    let created = app
        .clone()
        .oneshot(write_request(
            "POST",
            "/api/admin/channels",
            ADMIN_TOKEN,
            &channel_body,
        ))
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let created = response_json(created).await;
    assert_eq!(created["id"], 90);
    assert_eq!(created["timeout_secs"], 60);
    assert_eq!(created["pool_mode"], false);
    assert_eq!(
        created["client_simulation_profile"],
        serde_json::Value::Null
    );
    assert_eq!(
        created["client_simulation_body_profile"],
        serde_json::Value::Null
    );
    assert_eq!(created["responses_websocket_enabled"], false);
    for secret in ["header-secret-canary", "settings-secret-canary"] {
        assert!(!created.to_string().contains(secret));
    }

    let mut gemini_body = channel_body.clone();
    let gemini_object = gemini_body
        .as_object_mut()
        .expect("Gemini 测试渠道正文必须是对象");
    gemini_object.insert("name".to_owned(), json!("gemini"));
    gemini_object.insert("type".to_owned(), json!("gemini"));
    gemini_object.insert("protocol".to_owned(), json!("gemini"));
    let gemini_created = app
        .clone()
        .oneshot(write_request(
            "POST",
            "/api/admin/channels",
            ADMIN_TOKEN,
            &gemini_body,
        ))
        .await
        .unwrap();
    assert_eq!(gemini_created.status(), StatusCode::CREATED);

    let mut anthropic_body = channel_body.clone();
    {
        let anthropic_object = anthropic_body
            .as_object_mut()
            .expect("Anthropic 测试渠道正文必须是对象");
        anthropic_object.insert("name".to_owned(), json!("anthropic-simulation"));
        anthropic_object.insert("type".to_owned(), json!("anthropic"));
        anthropic_object.insert("protocol".to_owned(), json!("anthropic"));
        anthropic_object.insert(
            "client_simulation_profile".to_owned(),
            json!("anthropic_cli_headers_v1"),
        );
    }
    let rejected_risk = app
        .clone()
        .oneshot(write_request(
            "POST",
            "/api/admin/channels",
            ADMIN_TOKEN,
            &anthropic_body,
        ))
        .await
        .unwrap();
    assert_error(rejected_risk, StatusCode::BAD_REQUEST, "invalid_request").await;
    anthropic_body
        .as_object_mut()
        .expect("Anthropic 测试渠道正文必须是对象")
        .insert("client_simulation_risk_accepted".to_owned(), json!(true));
    let anthropic_created = app
        .clone()
        .oneshot(write_request(
            "POST",
            "/api/admin/channels",
            ADMIN_TOKEN,
            &anthropic_body,
        ))
        .await
        .unwrap();
    assert_eq!(anthropic_created.status(), StatusCode::CREATED);

    let mut anthropic_body_patch = anthropic_body.clone();
    anthropic_body_patch
        .as_object_mut()
        .expect("Anthropic 测试渠道正文必须是对象")
        .insert(
            "client_simulation_body_profile".to_owned(),
            json!("anthropic_cli_system_date_v1"),
        );
    let rejected_body_risk = app
        .clone()
        .oneshot(write_request(
            "POST",
            "/api/admin/channels",
            ADMIN_TOKEN,
            &anthropic_body_patch,
        ))
        .await
        .unwrap();
    assert_error(
        rejected_body_risk,
        StatusCode::BAD_REQUEST,
        "invalid_request",
    )
    .await;
    anthropic_body_patch
        .as_object_mut()
        .expect("Anthropic 测试渠道正文必须是对象")
        .insert(
            "client_simulation_body_risk_accepted".to_owned(),
            json!(true),
        );
    let body_patch_created = app
        .clone()
        .oneshot(write_request(
            "POST",
            "/api/admin/channels",
            ADMIN_TOKEN,
            &anthropic_body_patch,
        ))
        .await
        .unwrap();
    assert_eq!(body_patch_created.status(), StatusCode::CREATED);

    for (field, invalid) in [
        ("param_override", json!({"messages": []})),
        ("param_override", json!({"max_output_tokens": 0})),
        ("timeout_secs", json!(0)),
        ("timeout_secs", json!(901)),
        ("header_override", json!({"content-type": "text/plain"})),
        ("header_override", json!({"x-request-id": "untrusted"})),
        ("responses_websocket_enabled", json!(true)),
    ] {
        let mut invalid_body = channel_body.clone();
        invalid_body
            .as_object_mut()
            .expect("测试渠道正文必须是对象")
            .insert(field.to_owned(), invalid);
        let calls_before_rejection = reader.calls.load(Ordering::Relaxed);
        let rejected = app
            .clone()
            .oneshot(write_request(
                "POST",
                "/api/admin/channels",
                ADMIN_TOKEN,
                &invalid_body,
            ))
            .await
            .unwrap();
        assert_error(rejected, StatusCode::BAD_REQUEST, "invalid_request").await;
        assert_eq!(
            reader.calls.load(Ordering::Relaxed),
            calls_before_rejection,
            "无效请求覆盖不得进入写端口"
        );
    }

    let mut channel_update_body = channel_body.clone();
    let update_object = channel_update_body
        .as_object_mut()
        .expect("测试渠道正文必须是对象");
    update_object.remove("header_override");
    update_object.remove("settings");
    let updated = app
        .clone()
        .oneshot(write_request(
            "PUT",
            "/api/admin/channels/90",
            ADMIN_TOKEN,
            &channel_update_body,
        ))
        .await
        .unwrap();
    assert_eq!(response_json(updated).await["name"], "updated");

    let credential_body = json!({
        "kind": "api_key",
        "secret": {
            "kind": "api_key",
            "api_key": "credential-secret-canary"
        },
        "status": "disabled",
        "multi_key_mode": "random",
        "priority": 10,
        "weight": 20,
        "concurrency": null,
        "load_factor_micros": null,
        "rate_multiplier_micros": null,
        "schedulable": false,
        "parent_id": null,
        "quota_dimension": "global",
        "proxy_id": null,
        "oauth_provider": null,
        "oauth_account_key": null,
        "oauth_project_id": null
    });
    let credential = app
        .clone()
        .oneshot(write_request(
            "POST",
            "/api/admin/channels/90/credentials",
            ADMIN_TOKEN,
            &credential_body,
        ))
        .await
        .unwrap();
    assert_eq!(credential.status(), StatusCode::CREATED);
    let credential = response_json(credential).await;
    assert_eq!(credential["id"], 91);
    assert!(!credential.to_string().contains("credential-secret-canary"));

    let mut missing_secret = credential_body.clone();
    missing_secret
        .as_object_mut()
        .expect("测试凭据正文必须是对象")
        .remove("secret");
    let calls_before_rejection = reader.calls.load(Ordering::Relaxed);
    let rejected = app
        .clone()
        .oneshot(write_request(
            "PUT",
            "/api/admin/channels/90/credentials/91",
            ADMIN_TOKEN,
            &missing_secret,
        ))
        .await
        .unwrap();
    assert_error(rejected, StatusCode::BAD_REQUEST, "invalid_request").await;
    assert_eq!(
        reader.calls.load(Ordering::Relaxed),
        calls_before_rejection,
        "缺少 secret 的请求不得进入写端口"
    );

    let mut update_body = credential_body.clone();
    update_body
        .as_object_mut()
        .expect("测试凭据正文必须是对象")
        .insert("secret".to_owned(), Value::Null);
    let updated = app
        .clone()
        .oneshot(write_request(
            "PUT",
            "/api/admin/channels/90/credentials/91",
            ADMIN_TOKEN,
            &update_body,
        ))
        .await
        .unwrap();
    assert_eq!(response_json(updated).await["id"], 91);

    for uri in [
        "/api/admin/channels/90/credentials/91",
        "/api/admin/channels/90",
    ] {
        let deleted = app
            .clone()
            .oneshot(write_request("DELETE", uri, ADMIN_TOKEN, &json!({})))
            .await
            .unwrap();
        assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
    }
    assert_eq!(reader.calls.load(Ordering::Relaxed), 9);
}

#[tokio::test]
async fn admin_can_read_channel_and_credential_snapshots() {
    let reader = Arc::new(FakeAdminChannelReader::new());
    let app = router(Arc::clone(&reader));

    let list = app
        .clone()
        .oneshot(authorized_request(
            "/api/admin/channels?after=1&limit=2",
            ADMIN_TOKEN,
        ))
        .await
        .unwrap();
    assert_eq!(list.status(), StatusCode::OK);
    let list = response_json(list).await;
    assert_eq!(list["channels"][0]["id"], 10);
    assert_eq!(list["channels"][0]["type"], "openai");
    assert_eq!(list["channels"][0]["status"], "enabled");
    assert_eq!(list["next_cursor"], 11);

    let detail = app
        .clone()
        .oneshot(authorized_request("/api/admin/channels/12", ADMIN_TOKEN))
        .await
        .unwrap();
    assert_eq!(response_json(detail).await["name"], "detail");

    let credentials = app
        .clone()
        .oneshot(authorized_request(
            "/api/admin/channels/10/credentials?limit=1",
            ADMIN_TOKEN,
        ))
        .await
        .unwrap();
    let credentials = response_json(credentials).await;
    assert_eq!(credentials["credentials"][0]["kind"], "api_key");
    assert_eq!(credentials["credentials"][0]["multi_key_mode"], "random");
    assert_eq!(credentials["credentials"][0]["oauth_revision"], 0);
    assert_eq!(credentials["credentials"][0]["oauth_token_pending"], false);
    assert_eq!(credentials["credentials"][0]["blocks_spark_shadow"], true);
    assert!(!credentials.to_string().contains("secret"));

    let credential = app
        .oneshot(authorized_request(
            "/api/admin/channels/10/credentials/21",
            ADMIN_TOKEN,
        ))
        .await
        .unwrap();
    assert_eq!(response_json(credential).await["id"], 21);
    assert_eq!(reader.calls.load(Ordering::Relaxed), 4);
}

#[tokio::test]
async fn authorization_inputs_and_not_found_errors_are_closed() {
    let reader = Arc::new(FakeAdminChannelReader::new());
    let app = router(Arc::clone(&reader));

    let forbidden = app
        .clone()
        .oneshot(authorized_request("/api/admin/channels", USER_TOKEN))
        .await
        .unwrap();
    assert_error(forbidden, StatusCode::FORBIDDEN, "forbidden").await;
    assert_eq!(reader.calls.load(Ordering::Relaxed), 0);

    for uri in [
        "/api/admin/channels?limit=0",
        "/api/admin/channels?after=1&after=2",
        "/api/admin/channels/-1",
        "/api/admin/channels/1/credentials?after=%FF",
        "/api/admin/channels/1/credentials/abc",
    ] {
        let response = app
            .clone()
            .oneshot(authorized_request(uri, ADMIN_TOKEN))
            .await
            .unwrap();
        assert_error(response, StatusCode::BAD_REQUEST, "invalid_request").await;
    }
    assert_eq!(reader.calls.load(Ordering::Relaxed), 0);

    let channel_missing = app
        .clone()
        .oneshot(authorized_request("/api/admin/channels/404", ADMIN_TOKEN))
        .await
        .unwrap();
    assert_error(channel_missing, StatusCode::NOT_FOUND, "channel_not_found").await;
    let credential_missing = app
        .oneshot(authorized_request(
            "/api/admin/channels/1/credentials/404",
            ADMIN_TOKEN,
        ))
        .await
        .unwrap();
    assert_error(
        credential_missing,
        StatusCode::NOT_FOUND,
        "credential_not_found",
    )
    .await;
}

fn sample_channel(id: i64, name: &str) -> AdminChannel {
    AdminChannel::from_parts(
        ChannelId::new(id).unwrap(),
        name.to_owned(),
        ChannelType::OpenAi,
        Protocol::OpenAiChat,
        Some("https://api.example.com/v1".to_owned()),
        Some(af_domain::ChannelTimeout::new(60).unwrap()),
        AdminRoutingStatus::Enabled,
        10,
        20,
        true,
        vec!["gpt-public".to_owned()],
        vec![GroupId::new(1).unwrap()],
        json!({"gpt-public": "gpt-upstream"}),
        json!({"temperature": 0}),
        Some(1_000),
        25,
        Some("primary".to_owned()),
        1_000,
        2_000,
    )
}

fn sample_credential(channel_id: ChannelId, id: i64) -> AdminCredential {
    AdminCredential::from_parts(
        CredentialId::new(id).unwrap(),
        channel_id,
        CredentialKind::ApiKey,
        AdminRoutingStatus::Enabled,
        Some(AdminCredentialMultiKeyMode::Random),
        30,
        40,
        Some(2),
        Some(1_100_000),
        Some(900_000),
        true,
        None,
        None,
        None,
        None,
        false,
        None,
        None,
        None,
        AdminCredentialQuotaDimension::Global,
        None,
        Some("example-oauth".to_owned()),
        false,
        Some("account-public-id".to_owned()),
        Some("project-public-id".to_owned()),
        0,
        None,
        1_000,
        2_000,
    )
}

fn authorized_request(uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .uri(uri)
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap()
}

fn write_request(method: &str, uri: &str, token: &str, body: &Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(AUTHORIZATION, format!("Bearer {token}"))
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(body).unwrap()))
        .unwrap()
}

async fn assert_error(response: axum::response::Response, status: StatusCode, code: &str) {
    assert_eq!(response.status(), status);
    assert_eq!(response_json(response).await["code"], code);
}

async fn response_json(response: axum::response::Response) -> Value {
    serde_json::from_slice(&to_bytes(response.into_body(), 64 * 1024).await.unwrap()).unwrap()
}

fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
