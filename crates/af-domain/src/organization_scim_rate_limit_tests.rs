use std::num::NonZeroU32;

use super::*;

fn organization_id() -> OrganizationId {
    OrganizationId::new(7).unwrap()
}

fn rule(limit: u32) -> OrganizationScimRateLimitRule {
    OrganizationScimRateLimitRule::new(
        OrganizationScimRateLimitSubject::Organization(organization_id()),
        NonZeroU32::new(limit).unwrap(),
        OrganizationScimRateLimitWindow::new(60).unwrap(),
    )
}

#[test]
fn rate_limit_subject_keeps_token_bound_to_organization() {
    let token_key = OrganizationScimTokenKey::new("0123456789abcdef0123456789abcdef").unwrap();
    let subject = OrganizationScimRateLimitSubject::Token {
        organization_id: organization_id(),
        token_key: token_key.clone(),
    };

    assert_eq!(subject.organization_id(), organization_id());
    assert_eq!(subject.kind(), "token");
    assert_eq!(subject.token_key(), Some(&token_key));
    assert!(!format!("{subject:?}").contains("0123456789abcdef"));
}

#[test]
fn rate_limit_window_rejects_zero_negative_and_overlong_values() {
    assert_eq!(
        OrganizationScimRateLimitWindow::new(0).unwrap_err(),
        OrganizationScimRateLimitError::InvalidWindow
    );
    assert_eq!(
        OrganizationScimRateLimitWindow::from_persistence(-1).unwrap_err(),
        OrganizationScimRateLimitError::InvalidWindow
    );
    assert_eq!(
        OrganizationScimRateLimitWindow::new(MAX_ORGANIZATION_SCIM_RATE_LIMIT_WINDOW_SECONDS + 1)
            .unwrap_err(),
        OrganizationScimRateLimitError::InvalidWindow
    );
}

#[test]
fn rate_limit_admission_reports_remaining_after_atomic_increment() {
    let rule = rule(3);
    let snapshot = OrganizationScimRateLimitSnapshot::new(1, 1_060).unwrap();

    assert_eq!(
        classify_organization_scim_rate_limit(
            &rule,
            OrganizationScimRateLimitState::Ready(snapshot),
            1_000,
        ),
        OrganizationScimRateLimitDecision::Admitted {
            remaining: 1,
            reset_at_unix_seconds: 1_060,
        }
    );
}

#[test]
fn rate_limit_rejects_full_window_and_resets_expired_window() {
    let rule = rule(2);
    let full = OrganizationScimRateLimitSnapshot::new(2, 1_060).unwrap();
    assert_eq!(
        classify_organization_scim_rate_limit(
            &rule,
            OrganizationScimRateLimitState::Ready(full),
            1_000,
        ),
        OrganizationScimRateLimitDecision::Limited {
            retry_after_seconds: 60,
            reset_at_unix_seconds: 1_060,
        }
    );

    let expired = OrganizationScimRateLimitSnapshot::new(99, 999).unwrap();
    assert_eq!(
        classify_organization_scim_rate_limit(
            &rule,
            OrganizationScimRateLimitState::Ready(expired),
            1_000,
        ),
        OrganizationScimRateLimitDecision::Admitted {
            remaining: 1,
            reset_at_unix_seconds: 1_060,
        }
    );
}

#[test]
fn rate_limit_fails_closed_for_unavailable_invalid_and_bad_time() {
    let rule = rule(1);
    assert_eq!(
        classify_organization_scim_rate_limit(
            &rule,
            OrganizationScimRateLimitState::Unavailable,
            1_000,
        ),
        OrganizationScimRateLimitDecision::Rejected(
            OrganizationScimRateLimitRejection::StorageUnavailable
        )
    );
    assert_eq!(
        classify_organization_scim_rate_limit(
            &rule,
            OrganizationScimRateLimitState::Invalid,
            1_000,
        ),
        OrganizationScimRateLimitDecision::Rejected(
            OrganizationScimRateLimitRejection::InvalidFact
        )
    );
    assert_eq!(
        classify_organization_scim_rate_limit(
            &rule,
            OrganizationScimRateLimitState::Unavailable,
            0,
        ),
        OrganizationScimRateLimitDecision::Rejected(
            OrganizationScimRateLimitRejection::InvalidTimestamp
        )
    );
}
