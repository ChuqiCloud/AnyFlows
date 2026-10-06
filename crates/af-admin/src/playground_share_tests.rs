use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use std::time::Duration;

use af_domain::UserId;

use super::{
    DatabasePlaygroundShareService, MAX_PLAYGROUND_SHARE_MESSAGE_BYTES,
    MAX_PLAYGROUND_SHARE_MESSAGES, MAX_PLAYGROUND_SHARE_SESSIONS, PlaygroundShareCreateCommand,
    PlaygroundShareError, PlaygroundShareInputError, PlaygroundShareMessage,
    PlaygroundShareMessageRole, PlaygroundShareService, PlaygroundShareSession,
    PresentedPlaygroundShareToken, SessionPrincipal, SessionRole,
    playground_share_snapshot::sessions_from_snapshot,
    playground_share_token::issued_from_test_entropy,
};

#[test]
fn share_token_format_digest_and_debug_are_stable() {
    let issued = issued_from_test_entropy([0x42; 32]);
    let token_text = issued.token().expose_secret().to_owned();
    assert!(token_text.starts_with("sh-af-"));
    assert_eq!(token_text.len(), 49);
    assert_eq!(issued.digest().as_str().len(), 64);
    assert_eq!(format!("{issued:?}"), "<redacted>");
    assert!(!format!("{:?}", issued.digest()).contains(issued.digest().as_str()));

    let parsed = PresentedPlaygroundShareToken::parse_owned(token_text.clone()).unwrap();
    assert_eq!(parsed.digest().as_str(), issued.digest().as_str());
    assert_eq!(
        serde_json::to_string(&parsed).unwrap(),
        format!("\"{token_text}\"")
    );
    assert!(!format!("{parsed:?}").contains(&token_text));
}

#[test]
fn malformed_share_tokens_are_rejected() {
    let valid_random = URL_SAFE_NO_PAD.encode([0x24; 32]);
    for token in [
        String::new(),
        format!("sk-af-{valid_random}"),
        "sh-af-short".to_owned(),
        format!("sh-af-{}=", &valid_random[..42]),
        format!("sh-af-{}*", &valid_random[..42]),
    ] {
        assert!(PresentedPlaygroundShareToken::parse_owned(token).is_err());
    }
}

#[test]
fn snapshot_accepts_only_complete_unique_model_sessions() {
    let first = session("gpt-5", "question", "answer");
    let second = session("claude-sonnet", "question", "different answer");
    let command = PlaygroundShareCreateCommand::new(7, vec![first, second]).unwrap();
    let debug = format!("{command:?}");
    assert!(debug.contains("session_count"));
    assert!(!debug.contains("question"));
    assert!(!debug.contains("gpt-5"));
    let sessions = sessions_from_snapshot(command.into_snapshot().unwrap()).unwrap();
    assert_eq!(sessions.len(), 2);
    assert_eq!(sessions[0].messages()[0].content(), "question");

    assert_eq!(
        PlaygroundShareCreateCommand::new(2, vec![session("gpt-5", "q", "a")]).unwrap_err(),
        PlaygroundShareInputError::InvalidTtl
    );
    assert_eq!(
        PlaygroundShareCreateCommand::new(
            1,
            vec![session("gpt-5", "q", "a"), session("gpt-5", "q", "b")],
        )
        .unwrap_err(),
        PlaygroundShareInputError::InvalidSnapshot
    );
    assert_eq!(MAX_PLAYGROUND_SHARE_SESSIONS, 4);
    assert_eq!(MAX_PLAYGROUND_SHARE_MESSAGES, 256);
}

#[test]
fn messages_and_round_trips_enforce_visibility_bounds() {
    assert_eq!(
        PlaygroundShareMessage::new(PlaygroundShareMessageRole::User, String::new()).unwrap_err(),
        PlaygroundShareInputError::InvalidMessage
    );
    assert_eq!(
        PlaygroundShareMessage::new(
            PlaygroundShareMessageRole::Assistant,
            "x".repeat(MAX_PLAYGROUND_SHARE_MESSAGE_BYTES + 1),
        )
        .unwrap_err(),
        PlaygroundShareInputError::InvalidMessage
    );

    let incomplete = vec![
        PlaygroundShareMessage::new(PlaygroundShareMessageRole::User, "question".to_owned())
            .unwrap(),
    ];
    assert_eq!(
        PlaygroundShareSession::new("gpt-5".to_owned(), incomplete).unwrap_err(),
        PlaygroundShareInputError::InvalidSession
    );
    let reversed = vec![
        PlaygroundShareMessage::new(PlaygroundShareMessageRole::Assistant, "answer".to_owned())
            .unwrap(),
        PlaygroundShareMessage::new(PlaygroundShareMessageRole::User, "question".to_owned())
            .unwrap(),
    ];
    assert_eq!(
        PlaygroundShareSession::new("gpt-5".to_owned(), reversed).unwrap_err(),
        PlaygroundShareInputError::InvalidSession
    );
}

#[test]
fn persisted_snapshot_rejects_unknown_version_and_fields() {
    for snapshot in [
        serde_json::json!({"version": 2, "sessions": []}),
        serde_json::json!({"version": 1, "sessions": [], "secret": "hidden"}),
        serde_json::json!({
            "version": 1,
            "sessions": [{
                "model": "gpt-5",
                "messages": [
                    {"role": "user", "content": "q", "usage": 1},
                    {"role": "assistant", "content": "a"}
                ]
            }]
        }),
    ] {
        assert_eq!(
            sessions_from_snapshot(snapshot).unwrap_err(),
            PlaygroundShareInputError::InvalidSnapshot
        );
    }
}

#[tokio::test]
async fn database_service_creates_reads_and_revokes_with_owner_boundary() {
    let pool = af_db::connect_and_migrate(
        &af_db::DatabaseOptions::new("sqlite::memory:").unwrap(),
        af_db::MigrationOptions::default(),
    )
    .await
    .unwrap();
    let setup = af_db::InitialSetupRepository::new(pool.clone(), Duration::from_secs(5)).unwrap();
    let af_db::InitialSetupOutcome::Initialized { user_id } = setup
        .initialize(af_db::InitialSetupRecord::new(
            "share-service-owner".to_owned(),
            "correct horse battery staple".to_owned(),
        ))
        .await
        .unwrap()
    else {
        panic!("空数据库必须完成首次安装")
    };
    let repository =
        af_db::PlaygroundShareRepository::new(pool.clone(), Duration::from_secs(5)).unwrap();
    let service = DatabasePlaygroundShareService::new(repository);
    let principal = SessionPrincipal::new(user_id, SessionRole::User);
    let issued = service
        .create(
            principal,
            PlaygroundShareCreateCommand::new(1, vec![session("gpt-5", "q", "a")]).unwrap(),
        )
        .await
        .unwrap();
    let (token, _, _) = issued.into_parts();
    let token = token.expose_secret().to_owned();
    let view = service.read(token.clone()).await.unwrap();
    let (sessions, _, _) = view.into_parts();
    assert_eq!(sessions[0].model(), "gpt-5");

    let other = SessionPrincipal::new(UserId::new(user_id.get() + 1).unwrap(), SessionRole::User);
    assert_eq!(
        service.revoke(other, token.clone()).await,
        Err(PlaygroundShareError::NotFound)
    );
    service.revoke(principal, token.clone()).await.unwrap();
    assert!(matches!(
        service.read(token).await,
        Err(PlaygroundShareError::NotFound)
    ));
    pool.close().await.unwrap();
}

fn session(model: &str, user: &str, assistant: &str) -> PlaygroundShareSession {
    PlaygroundShareSession::new(
        model.to_owned(),
        vec![
            PlaygroundShareMessage::new(PlaygroundShareMessageRole::User, user.to_owned()).unwrap(),
            PlaygroundShareMessage::new(
                PlaygroundShareMessageRole::Assistant,
                assistant.to_owned(),
            )
            .unwrap(),
        ],
    )
    .unwrap()
}
