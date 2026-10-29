use super::{
    MAX_MODEL_NAME_BYTES, MAX_TOKEN_MODEL_ALLOWLIST_COUNT, MAX_TOKEN_MODEL_ALLOWLIST_TEXT_BYTES,
    TokenModelPolicy, TokenModelPolicyError,
};

#[test]
fn unrestricted_policy_allows_any_canonical_model() {
    let policy = TokenModelPolicy::unrestricted();

    assert!(policy.allows("gpt-test"));
    assert!(policy.allows("custom/model:latest"));
}

#[test]
fn restricted_policy_uses_case_sensitive_exact_matching() {
    let policy = TokenModelPolicy::try_from_allowlist(vec![
        "gpt-test".to_owned(),
        "gpt-test".to_owned(),
        "Custom-Model".to_owned(),
    ])
    .unwrap();

    assert!(policy.allows("gpt-test"));
    assert!(policy.allows("Custom-Model"));
    assert!(!policy.allows("GPT-TEST"));
    assert!(!policy.allows("gpt-test "));
    assert_eq!(format!("{policy:?}"), "TokenModelPolicy(<redacted>)");
}

#[test]
fn independent_model_restrictions_only_narrow_access() {
    let group = TokenModelPolicy::try_from_allowlist(vec!["a".into(), "b".into()]).unwrap();
    let key = TokenModelPolicy::try_from_allowlist(vec!["b".into(), "c".into()]).unwrap();
    let combined = group.intersect(&key);
    assert!(!combined.allows("a"));
    assert!(combined.allows("b"));
    assert!(!combined.allows("c"));
    assert_eq!(TokenModelPolicy::unrestricted().intersect(&key), key);
    assert_eq!(group.intersect(&TokenModelPolicy::unrestricted()), group);

    let disjoint =
        group.intersect(&TokenModelPolicy::try_from_allowlist(vec!["c".into()]).unwrap());
    for model in ["a", "b", "c", "other"] {
        assert!(!disjoint.allows(model));
    }
}

#[test]
fn restricted_policy_accepts_exact_name_and_total_byte_boundaries() {
    let boundary_model = "x".repeat(MAX_MODEL_NAME_BYTES);
    let policy = TokenModelPolicy::try_from_allowlist(vec![
        boundary_model.clone();
        MAX_TOKEN_MODEL_ALLOWLIST_TEXT_BYTES
            / MAX_MODEL_NAME_BYTES
    ])
    .unwrap();

    assert!(policy.allows(&boundary_model));
}

#[test]
fn restricted_policy_rejects_invalid_shapes_and_capacity_overflow() {
    let invalid = [
        Vec::new(),
        vec![String::new()],
        vec![" leading-space".to_owned()],
        vec!["trailing-space ".to_owned()],
        vec!["line\nbreak".to_owned()],
        vec!["x".repeat(MAX_MODEL_NAME_BYTES + 1)],
        vec![
            "x".repeat(MAX_MODEL_NAME_BYTES);
            MAX_TOKEN_MODEL_ALLOWLIST_TEXT_BYTES / MAX_MODEL_NAME_BYTES + 1
        ],
        vec!["model".to_owned(); MAX_TOKEN_MODEL_ALLOWLIST_COUNT + 1],
    ];

    for models in invalid {
        assert_eq!(
            TokenModelPolicy::try_from_allowlist(models),
            Err(TokenModelPolicyError::InvalidAllowlist)
        );
    }
}
