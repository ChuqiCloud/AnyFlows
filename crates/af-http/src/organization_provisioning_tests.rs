use super::organization_provisioning::{
    DecisionKindRequest, EntitlementRequest, ProvisioningResponse, parse_admin_query,
};
use af_domain::OrganizationProvisioningStatus;
use serde_json::json;

#[test]
fn admin_query_defaults_and_status_filter_are_stable() {
    let query = parse_admin_query(Some("status=pending&limit=20&before=42"))
        .expect("有效的管理员游标查询应能解析");
    assert_eq!(query.before(), Some(42));
    assert_eq!(query.limit(), 20);
    assert_eq!(
        query.status(),
        Some(OrganizationProvisioningStatus::Pending)
    );

    let default = parse_admin_query(None).expect("默认查询应能构造");
    assert_eq!(default.before(), None);
    assert_eq!(default.limit(), 50);
    assert_eq!(default.status(), None);
}

#[test]
fn admin_query_rejects_duplicates_invalid_cursor_and_unknown_fields() {
    for query in [
        "limit=10&limit=20",
        "before=0",
        "limit=101",
        "status=unknown",
        "unexpected=value",
    ] {
        assert!(
            parse_admin_query(Some(query)).is_err(),
            "查询应被拒绝: {query}"
        );
    }
}

#[test]
fn decision_and_entitlement_wire_names_are_snake_case() {
    let decision: DecisionKindRequest =
        serde_json::from_value(json!("approve")).expect("批准枚举应使用稳定 snake_case");
    assert!(matches!(decision, DecisionKindRequest::Approve));
    let entitlement: EntitlementRequest = serde_json::from_value(json!({
        "seat_limit": 10,
        "team_limit": 3,
        "organization_key_limit": 5,
        "team_management_enabled": true,
        "sso_enabled": false,
        "custom_roles_enabled": false,
        "audit_retention_days": 30,
        "valid_from": 1,
        "valid_until": 2,
        "grace_until": null
    }))
    .expect("授权快照字段应能解析");
    assert_eq!(entitlement.seat_limit, 10);
    assert_eq!(entitlement.grace_until, None);
}

#[test]
fn provisioning_response_does_not_serialize_unknown_fields() {
    let value = serde_json::to_value(ProvisioningResponse {
        id: 1,
        request_key: "00112233445566778899aabbccddeeff".to_owned(),
        idempotency_key: "ffeeddccbbaa99887766554433221100".to_owned(),
        applicant_user_id: 7,
        organization_name: "Acme".to_owned(),
        organization_slug: "acme".to_owned(),
        business_reason: "团队协作".to_owned(),
        status: super::organization_provisioning::StatusResponse::Pending,
        version: 1,
        reviewer_user_id: None,
        decision_reason: None,
        organization_id: None,
        created_at: 1,
        updated_at: 1,
        decided_at: None,
        replayed: false,
    })
    .expect("响应应能序列化");
    assert_eq!(value["status"], "pending");
    assert!(value.get("secret").is_none());
}
