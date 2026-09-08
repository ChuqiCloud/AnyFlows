use super::{
    OrganizationBudgetPeriod, OrganizationBudgetPolicyError, OrganizationBudgetScopeKind, Quota,
};

#[test]
fn organization_budget_codes_and_limits_are_closed() {
    assert_eq!(
        OrganizationBudgetScopeKind::try_from(2),
        Ok(OrganizationBudgetScopeKind::Team)
    );
    assert_eq!(
        OrganizationBudgetScopeKind::try_from(4),
        Ok(OrganizationBudgetScopeKind::Department)
    );
    assert_eq!(
        OrganizationBudgetPeriod::from_persistence(4, Some(3_600)),
        Ok(OrganizationBudgetPeriod::Fixed { seconds: 3_600 })
    );
    assert_eq!(
        OrganizationBudgetPeriod::from_persistence(1, Some(3_600)),
        Err(OrganizationBudgetPolicyError::InvalidPeriod)
    );
    assert_eq!(
        OrganizationBudgetPeriod::fixed(59),
        Err(OrganizationBudgetPolicyError::InvalidFixedPeriod)
    );
    assert_eq!(
        super::OrganizationBudgetLimit::new(Some(Quota::ZERO)),
        Err(OrganizationBudgetPolicyError::ZeroLimit)
    );
    assert!(super::OrganizationBudgetLimit::new(None).is_ok());
}
