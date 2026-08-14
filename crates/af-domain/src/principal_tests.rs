use super::{ChannelId, GatewayPrincipal, GroupId, PrincipalIdError, TokenId, UserId};

#[test]
fn principal_ids_accept_only_positive_values() {
    assert_eq!(TokenId::new(0), Err(PrincipalIdError::NonPositive));
    assert_eq!(UserId::new(-1), Err(PrincipalIdError::NonPositive));
    assert_eq!(
        GroupId::try_from(i64::MIN),
        Err(PrincipalIdError::NonPositive)
    );

    assert_eq!(TokenId::new(1).unwrap().get(), 1);
    assert_eq!(UserId::try_from(i64::MAX).unwrap().get(), i64::MAX);
    assert_eq!(GroupId::new(42).unwrap().get(), 42);
    assert_eq!(ChannelId::new(7).unwrap().get(), 7);
}

#[test]
fn gateway_principal_preserves_validated_identifiers() {
    let token_id = TokenId::new(11).unwrap();
    let user_id = UserId::new(22).unwrap();
    let group_id = GroupId::new(33).unwrap();
    let principal = GatewayPrincipal::new(token_id, user_id, group_id);

    assert_eq!(principal.token_id(), token_id);
    assert_eq!(principal.user_id(), user_id);
    assert_eq!(principal.group_id(), group_id);
    assert!(!principal.is_playground());

    let playground = GatewayPrincipal::playground(token_id, user_id, group_id);
    assert!(playground.is_playground());
    assert_ne!(playground, principal);
}

#[test]
fn principal_debug_output_redacts_all_identifiers() {
    let principal = GatewayPrincipal::new(
        TokenId::new(100_001).unwrap(),
        UserId::new(200_002).unwrap(),
        GroupId::new(300_003).unwrap(),
    );

    for rendered in [
        format!("{:?}", principal.token_id()),
        format!("{:?}", principal.user_id()),
        format!("{:?}", principal.group_id()),
        format!("{:?}", ChannelId::new(400_004).unwrap()),
        format!("{principal:?}"),
    ] {
        assert!(rendered.contains("<redacted>"));
        assert!(!rendered.contains("100001"));
        assert!(!rendered.contains("200002"));
        assert!(!rendered.contains("300003"));
        assert!(!rendered.contains("400004"));
    }
}
