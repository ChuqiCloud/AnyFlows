use std::time::Duration;

use af_domain::UserId;

use super::{
    DatabasePlaygroundConversationService, PlaygroundConversationError, PlaygroundConversationId,
    PlaygroundConversationInputError, PlaygroundConversationSaveCommand,
    PlaygroundConversationService, PlaygroundShareMessage, PlaygroundShareMessageRole,
    PlaygroundShareSession, SessionPrincipal, SessionRole,
};

const CONVERSATION_ID: &str = "11111111111111111111111111111111";

#[test]
fn command_validates_identity_revision_and_derives_private_summary() {
    let command = PlaygroundConversationSaveCommand::new(
        CONVERSATION_ID.to_owned(),
        None,
        vec![session("gpt-5", &[("  first\nquestion  ", "answer")])],
    )
    .unwrap();
    let debug = format!("{command:?}");
    assert!(debug.contains("model_count"));
    assert!(!debug.contains(CONVERSATION_ID));
    assert!(!debug.contains("first"));
    assert!(!debug.contains("gpt-5"));

    for invalid_id in ["0".repeat(32), "A".repeat(32), "1".repeat(31)] {
        assert_eq!(
            PlaygroundConversationId::parse_owned(invalid_id).unwrap_err(),
            PlaygroundConversationInputError::InvalidId
        );
    }
    assert_eq!(
        PlaygroundConversationSaveCommand::new(
            CONVERSATION_ID.to_owned(),
            Some(0),
            vec![session("gpt-5", &[("q", "a")])],
        )
        .unwrap_err(),
        PlaygroundConversationInputError::InvalidRevision
    );
}

#[tokio::test]
async fn database_service_saves_lists_restores_conflicts_and_deletes_by_owner() {
    let pool = af_db::connect_and_migrate(
        &af_db::DatabaseOptions::new("sqlite::memory:").unwrap(),
        af_db::MigrationOptions::default(),
    )
    .await
    .unwrap();
    let setup = af_db::InitialSetupRepository::new(pool.clone(), Duration::from_secs(5)).unwrap();
    let af_db::InitialSetupOutcome::Initialized { user_id } = setup
        .initialize(af_db::InitialSetupRecord::new(
            "history-service-owner".to_owned(),
            "correct horse battery staple".to_owned(),
        ))
        .await
        .unwrap()
    else {
        panic!("空数据库必须完成首次安装")
    };
    let repository =
        af_db::PlaygroundConversationRepository::new(pool.clone(), Duration::from_secs(5)).unwrap();
    let service = DatabasePlaygroundConversationService::new(repository);
    let principal = SessionPrincipal::new(user_id, SessionRole::User);
    let created = service
        .save(
            principal,
            PlaygroundConversationSaveCommand::new(
                CONVERSATION_ID.to_owned(),
                None,
                vec![session(
                    "gpt-5",
                    &[("\u{0007}first\tquestion\u{001b}", "first answer")],
                )],
            )
            .unwrap(),
        )
        .await
        .unwrap();
    let (conversation_id, title, sessions, revision, _, _) = created.into_parts();
    assert_eq!(conversation_id.as_str(), CONVERSATION_ID);
    assert_eq!(title, "first question");
    assert_eq!(sessions.len(), 1);
    assert_eq!(revision, 1);

    let summaries = service.list(principal).await.unwrap();
    assert_eq!(summaries.len(), 1);
    let (listed_id, listed_title, models, listed_revision, _, _) =
        summaries.into_iter().next().unwrap().into_parts();
    assert_eq!(listed_id.as_str(), CONVERSATION_ID);
    assert_eq!(listed_title, "first question");
    assert_eq!(models, ["gpt-5"]);
    assert_eq!(listed_revision, 1);

    let updated = service
        .save(
            principal,
            PlaygroundConversationSaveCommand::new(
                CONVERSATION_ID.to_owned(),
                Some(1),
                vec![session(
                    "gpt-5",
                    &[
                        ("first question", "first answer"),
                        ("second question", "second answer"),
                    ],
                )],
            )
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(updated.into_parts().3, 2);

    assert!(matches!(
        service
            .save(
                principal,
                PlaygroundConversationSaveCommand::new(
                    CONVERSATION_ID.to_owned(),
                    Some(1),
                    vec![session("gpt-5", &[("changed", "answer")])],
                )
                .unwrap(),
            )
            .await,
        Err(PlaygroundConversationError::Conflict)
    ));

    let restored = service
        .read(principal, CONVERSATION_ID.to_owned())
        .await
        .unwrap();
    assert_eq!(restored.into_parts().2[0].messages().len(), 4);
    let other = SessionPrincipal::new(UserId::new(user_id.get() + 1).unwrap(), SessionRole::Admin);
    assert!(matches!(
        service.read(other, CONVERSATION_ID.to_owned()).await,
        Err(PlaygroundConversationError::NotFound)
    ));
    assert!(matches!(
        service.delete(other, CONVERSATION_ID.to_owned()).await,
        Err(PlaygroundConversationError::NotFound)
    ));
    service
        .delete(principal, CONVERSATION_ID.to_owned())
        .await
        .unwrap();
    assert!(matches!(
        service.read(principal, CONVERSATION_ID.to_owned()).await,
        Err(PlaygroundConversationError::NotFound)
    ));
    pool.close().await.unwrap();
}

fn session(model: &str, rounds: &[(&str, &str)]) -> PlaygroundShareSession {
    let messages = rounds
        .iter()
        .flat_map(|(user, assistant)| {
            [
                PlaygroundShareMessage::new(PlaygroundShareMessageRole::User, (*user).to_owned())
                    .unwrap(),
                PlaygroundShareMessage::new(
                    PlaygroundShareMessageRole::Assistant,
                    (*assistant).to_owned(),
                )
                .unwrap(),
            ]
        })
        .collect();
    PlaygroundShareSession::new(model.to_owned(), messages).unwrap()
}
