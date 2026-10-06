use std::{
    collections::HashSet,
    sync::{Arc, Barrier},
    thread,
    time::{Duration, Instant},
};

use af_domain::{ChannelId, CredentialId, UserId};
use url::Url;

use super::{
    MAX_OAUTH_AUTHORIZATION_SESSION_TTL, MAX_PENDING_OAUTH_AUTHORIZATIONS,
    OAuthAuthorizationCallback, OAuthAuthorizationContext, OAuthAuthorizationError,
    OAuthAuthorizationRequest, OAuthAuthorizationSessionStore, OAuthAuthorizationStart,
    UpstreamOAuthProvider, authorization::code_challenge,
};

const CALLBACK_URI: &str = "http://127.0.0.1:43123/oauth/callback";
const AUTHORIZATION_CODE: &str = "authorization-code-marker";

fn authorization_context() -> OAuthAuthorizationContext {
    OAuthAuthorizationContext::new(
        UserId::new(11).unwrap(),
        Some(ChannelId::new(22).unwrap()),
        Some(CredentialId::new(33).unwrap()),
    )
    .unwrap()
}

fn authorization_request() -> OAuthAuthorizationRequest {
    request_with(
        "public-client-marker",
        "https://accounts.example.com/oauth/authorize?audience=anyflows",
        CALLBACK_URI,
        "openid profile offline_access",
    )
    .unwrap()
}

fn request_with(
    client_id: &str,
    authorization_endpoint: &str,
    redirect_uri: &str,
    scope: &str,
) -> Result<OAuthAuthorizationRequest, OAuthAuthorizationError> {
    OAuthAuthorizationRequest::new(
        UpstreamOAuthProvider::ClaudeCode,
        authorization_context(),
        client_id.to_owned(),
        authorization_endpoint.to_owned(),
        redirect_uri.to_owned(),
        scope.to_owned(),
    )
}

fn query_value(start: &OAuthAuthorizationStart, key: &str) -> String {
    start
        .authorization_url()
        .query_pairs()
        .find_map(|(candidate, value)| (candidate == key).then(|| value.into_owned()))
        .unwrap()
}

fn callback_url(redirect_uri: &Url, state: &str, outcome_key: &str, outcome_value: &str) -> Url {
    let mut url = redirect_uri.clone();
    url.query_pairs_mut()
        .append_pair("state", state)
        .append_pair(outcome_key, outcome_value);
    url
}

fn callback_with_code(
    provider: UpstreamOAuthProvider,
    redirect_uri: &Url,
    state: &str,
) -> OAuthAuthorizationCallback {
    OAuthAuthorizationCallback::from_redirect_url(
        provider,
        callback_url(redirect_uri, state, "code", AUTHORIZATION_CODE),
    )
    .unwrap()
}

#[test]
fn rfc_7636_s256_official_vector_is_stable() {
    assert_eq!(
        code_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
        "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
    );
}

#[test]
fn authorization_start_contains_pkce_and_unique_state_without_verifier() {
    let store = OAuthAuthorizationSessionStore::with_defaults();
    let request = authorization_request();
    let first = store.begin(&request).unwrap();
    let second = store.begin(&request).unwrap();
    let first_state = query_value(&first, "state");
    let second_state = query_value(&second, "state");
    let challenge = query_value(&first, "code_challenge");

    assert_eq!(first_state.len(), 43);
    assert_eq!(challenge.len(), 43);
    assert_ne!(first_state, second_state);
    assert_ne!(first_state, challenge);
    assert_eq!(query_value(&first, "response_type"), "code");
    assert_eq!(query_value(&first, "client_id"), "public-client-marker");
    assert_eq!(query_value(&first, "redirect_uri"), CALLBACK_URI);
    assert_eq!(
        query_value(&first, "scope"),
        "openid profile offline_access"
    );
    assert_eq!(query_value(&first, "code_challenge_method"), "S256");
    assert_eq!(query_value(&first, "audience"), "anyflows");
    assert!(
        first
            .authorization_url()
            .query_pairs()
            .all(|(key, _)| key != "code_verifier")
    );
    assert_eq!(store.pending_count().unwrap(), 2);
}

#[test]
fn request_validation_rejects_unsafe_endpoints_and_parameters() {
    assert_eq!(
        OAuthAuthorizationContext::new(
            UserId::new(1).unwrap(),
            None,
            Some(CredentialId::new(2).unwrap())
        )
        .unwrap_err(),
        OAuthAuthorizationError::InvalidContext
    );
    assert_eq!(
        request_with(
            "client",
            "http://accounts.example.com/authorize",
            CALLBACK_URI,
            "openid"
        )
        .unwrap_err(),
        OAuthAuthorizationError::InvalidAuthorizationEndpoint
    );
    for redirect_uri in [
        "http://localhost:43123/callback",
        "http://127.0.0.2:43123/callback",
        "http://192.168.1.2:43123/callback",
        "http://127.0.0.1/callback",
        "https://127.0.0.1:43123/callback",
        "http://127.0.0.1:43123/callback?preset=true",
    ] {
        assert_eq!(
            request_with(
                "client",
                "https://accounts.example.com/authorize",
                redirect_uri,
                "openid"
            )
            .unwrap_err(),
            OAuthAuthorizationError::InvalidRedirectUri
        );
    }
    assert!(
        request_with(
            "client",
            "https://accounts.example.com/authorize",
            "http://[::1]:43123/callback",
            "openid"
        )
        .is_ok()
    );
    assert_eq!(
        request_with(
            "client",
            "https://accounts.example.com/authorize?state=preset",
            CALLBACK_URI,
            "openid"
        )
        .unwrap_err(),
        OAuthAuthorizationError::ReservedParameter
    );
    for client_id in ["client id", " client", "client\n", "客户端"] {
        assert_eq!(
            request_with(
                client_id,
                "https://accounts.example.com/authorize",
                CALLBACK_URI,
                "openid"
            )
            .unwrap_err(),
            OAuthAuthorizationError::InvalidClientId
        );
    }
    for scope in ["", "openid  profile", "openid\tprofile", "用户"] {
        assert_eq!(
            request_with(
                "client",
                "https://accounts.example.com/authorize",
                CALLBACK_URI,
                scope
            )
            .unwrap_err(),
            OAuthAuthorizationError::InvalidScope
        );
    }
}

#[test]
fn authorization_url_has_a_hard_length_limit() {
    let endpoint = format!(
        "https://accounts.example.com/authorize?padding={}",
        "a".repeat(8 * 1_024)
    );
    let request = request_with("client", &endpoint, CALLBACK_URI, "openid").unwrap();
    let store = OAuthAuthorizationSessionStore::with_defaults();

    assert_eq!(
        store.begin(&request).unwrap_err(),
        OAuthAuthorizationError::AuthorizationUrlTooLong
    );
    assert_eq!(store.pending_count().unwrap(), 0);
}

#[test]
fn callback_parser_rejects_ambiguous_or_incomplete_outcomes() {
    let state = "a".repeat(43);
    let base = Url::parse(CALLBACK_URI).unwrap();
    let cases = [
        (
            format!("state={state}"),
            OAuthAuthorizationError::MalformedCallback,
        ),
        (
            format!("state={state}&code=ok&error=denied"),
            OAuthAuthorizationError::MalformedCallback,
        ),
        (
            format!("state={state}&code=one&code=two"),
            OAuthAuthorizationError::MalformedCallback,
        ),
        (
            format!("state={state}&state={state}&code=ok"),
            OAuthAuthorizationError::MalformedCallback,
        ),
        (
            "state=short&code=ok".to_owned(),
            OAuthAuthorizationError::InvalidState,
        ),
    ];
    for (query, expected) in cases {
        let mut url = base.clone();
        url.set_query(Some(&query));
        assert_eq!(
            OAuthAuthorizationCallback::from_redirect_url(UpstreamOAuthProvider::ClaudeCode, url)
                .unwrap_err(),
            expected
        );
    }
}

#[test]
fn successful_callback_returns_code_and_verifier_once() {
    let store = OAuthAuthorizationSessionStore::with_defaults();
    let start = store.begin(&authorization_request()).unwrap();
    let state = query_value(&start, "state");
    let challenge = query_value(&start, "code_challenge");
    let callback_url = callback_url(start.redirect_uri(), &state, "code", AUTHORIZATION_CODE);
    let callback = OAuthAuthorizationCallback::from_redirect_url(
        UpstreamOAuthProvider::ClaudeCode,
        callback_url.clone(),
    )
    .unwrap();
    let grant = store.consume(callback).unwrap();

    assert_eq!(grant.provider(), UpstreamOAuthProvider::ClaudeCode);
    assert_eq!(grant.context(), authorization_context());
    assert_eq!(grant.redirect_uri().as_str(), CALLBACK_URI);
    assert_eq!(grant.state(), state);
    assert_eq!(grant.authorization_code(), AUTHORIZATION_CODE);
    assert_eq!(grant.code_verifier().len(), 43);
    assert_eq!(code_challenge(grant.code_verifier()), challenge);
    assert_eq!(store.pending_count().unwrap(), 0);

    let replay = OAuthAuthorizationCallback::from_redirect_url(
        UpstreamOAuthProvider::ClaudeCode,
        callback_url,
    )
    .unwrap();
    assert_eq!(
        store.consume(replay).unwrap_err(),
        OAuthAuthorizationError::SessionNotFound
    );
}

#[test]
fn concurrent_callback_consumption_has_exactly_one_winner() {
    let store = Arc::new(OAuthAuthorizationSessionStore::with_defaults());
    let start = store.begin(&authorization_request()).unwrap();
    let state = query_value(&start, "state");
    let callback_url = callback_url(start.redirect_uri(), &state, "code", AUTHORIZATION_CODE);
    let barrier = Arc::new(Barrier::new(2));
    let handles = (0..2)
        .map(|_| {
            let store = Arc::clone(&store);
            let barrier = Arc::clone(&barrier);
            let callback = OAuthAuthorizationCallback::from_redirect_url(
                UpstreamOAuthProvider::ClaudeCode,
                callback_url.clone(),
            )
            .unwrap();
            thread::spawn(move || {
                barrier.wait();
                store.consume(callback)
            })
        })
        .collect::<Vec<_>>();

    let mut successes = 0;
    let mut replay_errors = 0;
    for handle in handles {
        match handle.join().unwrap() {
            Ok(grant) => {
                successes += 1;
                assert_eq!(grant.authorization_code(), AUTHORIZATION_CODE);
            }
            Err(OAuthAuthorizationError::SessionNotFound) => replay_errors += 1,
            Err(error) => panic!("并发消费返回了意外错误：{error}"),
        }
    }

    assert_eq!(successes, 1);
    assert_eq!(replay_errors, 1);
    assert_eq!(store.pending_count().unwrap(), 0);
}

#[test]
fn denied_callbacks_consume_but_mismatched_bindings_preserve_the_session() {
    let request = authorization_request();

    let store = OAuthAuthorizationSessionStore::with_defaults();
    let start = store.begin(&request).unwrap();
    let state = query_value(&start, "state");
    let denied = OAuthAuthorizationCallback::from_redirect_url(
        UpstreamOAuthProvider::ClaudeCode,
        callback_url(start.redirect_uri(), &state, "error", "access_denied"),
    )
    .unwrap();
    assert_eq!(
        store.consume(denied).unwrap_err(),
        OAuthAuthorizationError::ProviderDenied
    );
    assert_eq!(store.pending_count().unwrap(), 0);

    let store = OAuthAuthorizationSessionStore::with_defaults();
    let start = store.begin(&request).unwrap();
    let state = query_value(&start, "state");
    let wrong_provider =
        callback_with_code(UpstreamOAuthProvider::Codex, start.redirect_uri(), &state);
    assert_eq!(
        store.consume(wrong_provider).unwrap_err(),
        OAuthAuthorizationError::ProviderMismatch
    );
    assert_eq!(store.pending_count().unwrap(), 1);
    let correct = callback_with_code(
        UpstreamOAuthProvider::ClaudeCode,
        start.redirect_uri(),
        &state,
    );
    store.consume(correct).unwrap();
    assert_eq!(store.pending_count().unwrap(), 0);

    let store = OAuthAuthorizationSessionStore::with_defaults();
    let start = store.begin(&request).unwrap();
    let state = query_value(&start, "state");
    let wrong_redirect = callback_with_code(
        UpstreamOAuthProvider::ClaudeCode,
        &Url::parse("http://127.0.0.1:43123/other-callback").unwrap(),
        &state,
    );
    assert_eq!(
        store.consume(wrong_redirect).unwrap_err(),
        OAuthAuthorizationError::RedirectUriMismatch
    );
    assert_eq!(store.pending_count().unwrap(), 1);
    let correct = callback_with_code(
        UpstreamOAuthProvider::ClaudeCode,
        start.redirect_uri(),
        &state,
    );
    store.consume(correct).unwrap();
    assert_eq!(store.pending_count().unwrap(), 0);
}

#[test]
fn manual_callback_requires_the_authorization_owner() {
    let store = OAuthAuthorizationSessionStore::with_defaults();
    let start = store.begin(&authorization_request()).unwrap();
    let state = query_value(&start, "state");
    let callback = callback_with_code(
        UpstreamOAuthProvider::ClaudeCode,
        start.redirect_uri(),
        &state,
    );
    assert_eq!(
        store
            .consume_for(callback, UserId::new(12).unwrap())
            .unwrap_err(),
        OAuthAuthorizationError::PrincipalMismatch
    );
    assert_eq!(store.pending_count().unwrap(), 1);

    let callback = callback_with_code(
        UpstreamOAuthProvider::ClaudeCode,
        start.redirect_uri(),
        &state,
    );
    store
        .consume_for(callback, authorization_context().user_id())
        .unwrap();
    assert_eq!(store.pending_count().unwrap(), 0);
}

#[test]
fn ttl_cleanup_and_capacity_are_enforced_at_the_boundary() {
    let ttl = Duration::from_secs(60);
    let store = OAuthAuthorizationSessionStore::new(1, ttl).unwrap();
    let request = authorization_request();
    let now = Instant::now();
    let _first_start = store.begin_at(&request, now).unwrap();

    assert_eq!(
        store.begin_at(&request, now).unwrap_err(),
        OAuthAuthorizationError::CapacityExceeded
    );
    assert_eq!(store.pending_count_at(now).unwrap(), 1);
    assert_eq!(store.cleanup_expired_at(now + ttl).unwrap(), 1);
    assert_eq!(store.pending_count_at(now + ttl).unwrap(), 0);

    let start = store.begin_at(&request, now + ttl).unwrap();
    assert_eq!(start.expires_at(), now + ttl + ttl);
    let state = query_value(&start, "state");
    let callback = callback_with_code(
        UpstreamOAuthProvider::ClaudeCode,
        start.redirect_uri(),
        &state,
    );
    assert_eq!(
        store.consume_at(callback, now + ttl + ttl).unwrap_err(),
        OAuthAuthorizationError::SessionExpired
    );
    assert_eq!(store.pending_count_at(now + ttl + ttl).unwrap(), 0);
}

#[test]
fn store_configuration_rejects_unbounded_values() {
    assert_eq!(
        OAuthAuthorizationSessionStore::new(0, Duration::from_secs(60)).unwrap_err(),
        OAuthAuthorizationError::InvalidSessionCapacity
    );
    assert_eq!(
        OAuthAuthorizationSessionStore::new(
            MAX_PENDING_OAUTH_AUTHORIZATIONS + 1,
            Duration::from_secs(60)
        )
        .unwrap_err(),
        OAuthAuthorizationError::InvalidSessionCapacity
    );
    for ttl in [
        Duration::from_secs(59),
        MAX_OAUTH_AUTHORIZATION_SESSION_TTL + Duration::from_secs(1),
        Duration::from_secs(60) + Duration::from_nanos(1),
    ] {
        assert_eq!(
            OAuthAuthorizationSessionStore::new(1, ttl).unwrap_err(),
            OAuthAuthorizationError::InvalidSessionTtl
        );
    }
}

#[test]
fn concurrent_begins_keep_states_unique_and_never_exceed_capacity() {
    const CAPACITY: usize = 32;
    const WORKERS: usize = 64;

    let store =
        Arc::new(OAuthAuthorizationSessionStore::new(CAPACITY, Duration::from_secs(60)).unwrap());
    let request = Arc::new(authorization_request());
    let barrier = Arc::new(Barrier::new(WORKERS));
    let handles = (0..WORKERS)
        .map(|_| {
            let store = Arc::clone(&store);
            let request = Arc::clone(&request);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                store
                    .begin(request.as_ref())
                    .map(|start| query_value(&start, "state"))
            })
        })
        .collect::<Vec<_>>();

    let mut states = HashSet::new();
    let mut capacity_errors = 0;
    for handle in handles {
        match handle.join().unwrap() {
            Ok(state) => assert!(states.insert(state)),
            Err(OAuthAuthorizationError::CapacityExceeded) => capacity_errors += 1,
            Err(error) => panic!("并发创建返回了意外错误：{error}"),
        }
    }

    assert_eq!(states.len(), CAPACITY);
    assert_eq!(capacity_errors, WORKERS - CAPACITY);
    assert_eq!(store.pending_count().unwrap(), CAPACITY);
}

#[test]
fn debug_outputs_never_expose_oauth_material() {
    let request = authorization_request();
    let request_debug = format!("{request:?}");
    assert!(!request_debug.contains("public-client-marker"));
    assert!(!request_debug.contains("accounts.example.com"));
    assert!(!request_debug.contains("offline_access"));

    let store = OAuthAuthorizationSessionStore::with_defaults();
    let start = store.begin(&request).unwrap();
    let state = query_value(&start, "state");
    let start_debug = format!("{start:?}");
    assert!(!start_debug.contains(&state));
    assert!(!start_debug.contains(CALLBACK_URI));

    let callback = callback_with_code(
        UpstreamOAuthProvider::ClaudeCode,
        start.redirect_uri(),
        &state,
    );
    let callback_debug = format!("{callback:?}");
    assert!(!callback_debug.contains(&state));
    assert!(!callback_debug.contains(AUTHORIZATION_CODE));
    assert!(!callback_debug.contains("state="));
    assert!(!callback_debug.contains("code="));

    let grant = store.consume(callback).unwrap();
    let verifier = grant.code_verifier().to_owned();
    let grant_debug = format!("{grant:?}");
    assert!(!grant_debug.contains(&state));
    assert!(!grant_debug.contains(CALLBACK_URI));
    assert!(!grant_debug.contains(AUTHORIZATION_CODE));
    assert!(!grant_debug.contains(&verifier));
    assert!(!format!("{store:?}").contains(&state));
}
