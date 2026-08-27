use super::*;

fn organization_id() -> OrganizationId {
    OrganizationId::new(7).unwrap()
}

fn token_key() -> OrganizationScimTokenKey {
    OrganizationScimTokenKey::new("0123456789abcdef0123456789abcdef").unwrap()
}

fn request_fingerprint() -> OrganizationScimRequestFingerprint {
    OrganizationScimRequestFingerprint::new([0x42; 32]).unwrap()
}

fn resource() -> OrganizationScimAuditResource {
    let resource_id = OrganizationScimResourceId::new("external-user-1").unwrap();
    let digest = OrganizationScimResourceDigest::from_resource(
        organization_id(),
        OrganizationScimResourceKind::Users,
        &resource_id,
    );
    OrganizationScimAuditResource::new(
        OrganizationScimResourceKind::Users,
        digest,
        OrganizationScimResourceVersion::new(3).unwrap(),
    )
}

#[test]
fn resource_digest_is_stable_and_redacted() {
    let resource_id = OrganizationScimResourceId::new("external-user-1").unwrap();
    let digest = OrganizationScimResourceDigest::from_resource(
        organization_id(),
        OrganizationScimResourceKind::Users,
        &resource_id,
    );
    let restored =
        OrganizationScimResourceDigest::from_persistence_key(&digest.persistence_key()).unwrap();

    assert_eq!(digest, restored);
    assert_eq!(digest.persistence_key().len(), 64);
    assert_eq!(
        format!("{digest:?}"),
        "OrganizationScimResourceDigest(<redacted>)"
    );
    assert!(!format!("{digest:?}").contains("external-user-1"));
}

#[test]
fn audit_facts_require_closed_shapes_and_never_store_resource_text() {
    let facts = OrganizationScimAuditFacts::new(
        organization_id(),
        Some(token_key()),
        OrganizationScimAuditEventType::ResourceSynchronized,
        OrganizationScimAuditResult::Applied,
        Some(resource()),
        request_fingerprint(),
        None,
        None,
    )
    .unwrap();

    assert_eq!(facts.event_type().code(), "resource_synchronized");
    assert_eq!(facts.result().code(), "applied");
    assert_eq!(facts.resource().unwrap().version().get(), 3);
    let debug = format!("{facts:?}");
    assert!(!debug.contains("external-user-1"));
    assert!(!debug.contains("alice@example.com"));
    assert!(!debug.contains("request body"));
}

#[test]
fn audit_facts_reject_mismatched_resource_result_and_rate_limit_metadata() {
    let missing_resource = OrganizationScimAuditFacts::new(
        organization_id(),
        None,
        OrganizationScimAuditEventType::LifecycleTransition,
        OrganizationScimAuditResult::Applied,
        None,
        request_fingerprint(),
        None,
        None,
    );
    assert_eq!(
        missing_resource.unwrap_err(),
        OrganizationScimAuditFactsError::InvalidResourceProjection
    );

    let missing_reason = OrganizationScimAuditFacts::new(
        organization_id(),
        None,
        OrganizationScimAuditEventType::RequestRejected,
        OrganizationScimAuditResult::Rejected,
        None,
        request_fingerprint(),
        None,
        None,
    );
    assert_eq!(
        missing_reason.unwrap_err(),
        OrganizationScimAuditFactsError::InvalidResultShape
    );

    let invalid_retry_shape = OrganizationScimAuditFacts::new(
        organization_id(),
        None,
        OrganizationScimAuditEventType::RequestRejected,
        OrganizationScimAuditResult::Rejected,
        None,
        request_fingerprint(),
        Some(OrganizationScimAuditReason::InvalidToken),
        Some(5),
    );
    assert_eq!(
        invalid_retry_shape.unwrap_err(),
        OrganizationScimAuditFactsError::InvalidResultShape
    );
}

#[test]
fn audit_facts_reject_zero_request_fingerprint() {
    assert_eq!(
        OrganizationScimRequestFingerprint::new([0; 32]).unwrap_err(),
        OrganizationScimAuditFactsError::InvalidRequestFingerprint
    );
}
