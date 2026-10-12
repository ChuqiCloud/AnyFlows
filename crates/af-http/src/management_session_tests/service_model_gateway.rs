use super::*;
use af_admin::{
    ServiceAccountAuthentication, ServiceAccountModelAuthentication,
    ServiceAccountModelAuthenticator,
};
use af_domain::{
    ConcurrencyLimit, OrganizationId, OrganizationServiceAccountId, OrganizationServiceAccountKey,
    OrganizationServiceAccountLocator, OrganizationServiceAccountRuntimeIdentity,
    ServiceModelPolicy,
};
use axum::extract::{ConnectInfo, Extension};
use http::Method;
use std::{
    net::SocketAddr,
    num::NonZeroU32,
    sync::atomic::{AtomicUsize, Ordering},
};

const SECRET: &str = "sa_abababababababababababababababababababababababababababababababab";
fn snapshot() -> ServiceAccountModelAuthentication {
    ServiceAccountModelAuthentication::new(
        ServiceAccountAuthentication::new(
            OrganizationServiceAccountRuntimeIdentity::new(
                OrganizationServiceAccountLocator::new(
                    OrganizationId::new(1).unwrap(),
                    OrganizationServiceAccountId::new(2).unwrap(),
                ),
                OrganizationServiceAccountKey::new([3; 16]).unwrap(),
                OrganizationServiceAccountKey::new([4; 16]).unwrap(),
            ),
            None,
            None,
        ),
        GroupId::new(5).unwrap(),
        ServiceModelPolicy::new(vec!["gpt-5".into()]).unwrap(),
        ConcurrencyLimit::new(2).unwrap(),
        NonZeroU32::new(30).unwrap(),
    )
}
struct Auth;
impl ServiceAccountModelAuthenticator for Auth {
    fn authenticate_models<'a>(
        &'a self,
        digest: &'a af_admin::ServiceAccountKeyDigest,
    ) -> af_admin::ServiceAccountModelAuthenticationFuture<'a> {
        Box::pin(async move {
            let expected = af_admin::PresentedServiceAccountKey::parse(SECRET)
                .unwrap()
                .digest();
            if digest.as_str() == expected.as_str() {
                Ok(snapshot())
            } else {
                Err(af_admin::ServiceAccountModelAuthenticationError::InvalidKey)
            }
        })
    }
}
struct Personal;
impl af_admin::TokenAuthenticator for Personal {
    fn authenticate<'a>(
        &'a self,
        _: &'a af_admin::ApiKeyDigest,
        _: af_domain::TrustedClientIp,
    ) -> af_admin::TokenAuthenticationFuture<'a> {
        panic!("service key reached personal authenticator")
    }
}
#[derive(Default)]
struct Calls(AtomicUsize);
impl ChatService for Calls {
    fn chat_completions<'a>(
        &'a self,
        _: &'a GatewayPrincipal,
        _: Option<ConcurrencyLimit>,
        _: CanonicalRequestEnvelope,
        _: af_domain::Protocol,
        _: &'a str,
        _: af_relay::RelayDiagnosticInput,
    ) -> ChatServiceFuture<'a> {
        panic!("service identity reached personal service")
    }
    fn service_account_chat_completions<'a>(
        &'a self,
        authentication: &'a ServiceAccountModelAuthentication,
        request: CanonicalRequestEnvelope,
        _: af_domain::Protocol,
        _: &'a str,
        _: af_relay::RelayDiagnosticInput,
    ) -> ChatServiceFuture<'a> {
        assert_eq!(authentication, &snapshot());
        assert_eq!(request.canonical().model, "gpt-5");
        self.0.fetch_add(1, Ordering::Relaxed);
        Box::pin(async {
            Ok(af_relay::ChatResponse::Full {
                body: axum::body::Bytes::from_static(b"{}"),
                usage: Err(af_relay::UsageResolutionError::UnsupportedContent),
            })
        })
    }
}
struct Catalog;
impl af_admin::ModelCatalogReader for Catalog {
    fn list_for_token(
        &self,
        _: af_admin::TokenAuthentication,
    ) -> af_admin::GatewayModelListFuture<'_> {
        panic!("service used personal directory")
    }
    fn list_for_service_account(
        &self,
        authentication: ServiceAccountModelAuthentication,
    ) -> af_admin::GatewayModelListFuture<'_> {
        assert_eq!(authentication, snapshot());
        Box::pin(async {
            Ok(vec![
                af_admin::GatewayModel::new("gpt-5".into(), 0, "OpenAI".into()).unwrap(),
            ])
        })
    }
    fn list<'a>(
        &'a self,
        _: Option<SessionAuthentication>,
        _: &'a af_admin::ModelCatalogQuery,
    ) -> af_admin::ModelCatalogListFuture<'a> {
        panic!("unused catalog")
    }
    fn providers(
        &self,
        _: Option<SessionAuthentication>,
    ) -> af_admin::ModelCatalogProvidersFuture<'_> {
        panic!("unused providers")
    }
}
fn app(calls: Arc<Calls>) -> Router {
    use crate::authentication::{AuthenticationState, authenticate_api_key};
    let auth = middleware::from_fn_with_state(
        AuthenticationState::new(
            Arc::new(Personal),
            crate::QueryApiKeyPolicy::Deny,
            &af_config::ServerConfig::default(),
        )
        .with_service_authenticator(Some(Arc::new(Auth))),
        authenticate_api_key,
    );
    let personal = middleware::from_fn_with_state(
        AuthenticationState::new(
            Arc::new(Personal),
            crate::QueryApiKeyPolicy::Deny,
            &af_config::ServerConfig::default(),
        ),
        authenticate_api_key,
    );
    let groups = Arc::new(UnusedAdminGroupReader);
    let users = Arc::new(UnusedAdminUserReader);
    let state = HttpState::new(
        calls,
        Arc::new(FakeSessionAuthenticator::new()),
        groups.clone(),
        groups,
        Arc::new(UnusedAdminTokenReader),
        Arc::new(UnusedAdminTokenReader),
        users.clone(),
        users,
    )
    .with_model_catalog_reader(Arc::new(Catalog));
    Router::new()
        .route(
            "/v1/models",
            get(crate::openai_models::list_openai_models).route_layer(auth.clone()),
        )
        .route(
            "/v1/chat/completions",
            post(crate::chat_completions::chat_completions).route_layer(auth.clone()),
        )
        .route(
            "/v1/responses",
            post(crate::openai_responses::openai_responses).route_layer(auth.clone()),
        )
        .route(
            "/v1/messages",
            post(crate::anthropic_messages::anthropic_messages).route_layer(auth.clone()),
        )
        .route(
            "/v1beta/models/{model_action}",
            post(crate::gemini_generate_content::gemini_generate_content).route_layer(auth),
        )
        .route(
            "/v1/embeddings",
            post(crate::openai_embeddings::openai_embeddings).route_layer(personal),
        )
        .with_state(state)
        .layer(Extension(
            af_telemetry::RequestId::new("service-http").unwrap(),
        ))
        .layer(Extension(ConnectInfo(
            "127.0.0.1:1234".parse::<SocketAddr>().unwrap(),
        )))
}
async fn send(app: &Router, method: Method, path: &str, body: Value) -> Response {
    app.clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header(AUTHORIZATION, format!("Bearer {SECRET}"))
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap()
}
#[tokio::test]
async fn service_models_and_four_protocols_preserve_identity_and_model_policy() {
    let calls = Arc::new(Calls::default());
    let app = app(calls.clone());
    let models = send(&app, Method::GET, "/v1/models", Value::Null).await;
    assert_eq!(models.status(), StatusCode::OK);
    let models: Value =
        serde_json::from_slice(&to_bytes(models.into_body(), 8192).await.unwrap()).unwrap();
    assert_eq!(models["data"][0]["id"], "gpt-5");
    for (path, body) in [
        (
            "/v1/chat/completions",
            json!({"model":"gpt-5-high","messages":[{"role":"user","content":"hello"}]}),
        ),
        ("/v1/responses", json!({"model":"gpt-5","input":"hello"})),
        (
            "/v1/messages",
            json!({"model":"gpt-5","max_tokens":20,"messages":[{"role":"user","content":"hello"}]}),
        ),
        (
            "/v1beta/models/gpt-5:generateContent",
            json!({"contents":[{"role":"user","parts":[{"text":"hello"}]}]}),
        ),
    ] {
        assert_eq!(
            send(&app, Method::POST, path, body).await.status(),
            StatusCode::OK,
            "{path}"
        );
    }
    assert_eq!(calls.0.load(Ordering::Relaxed), 4);
    assert_eq!(
        send(
            &app,
            Method::POST,
            "/v1/chat/completions",
            json!({"model":"outside","messages":[{"role":"user","content":"hello"}]})
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(calls.0.load(Ordering::Relaxed), 4);
    assert_eq!(
        send(
            &app,
            Method::POST,
            "/v1/embeddings",
            json!({"model":"gpt-5","input":"hello"})
        )
        .await
        .status(),
        StatusCode::UNAUTHORIZED
    );
}
