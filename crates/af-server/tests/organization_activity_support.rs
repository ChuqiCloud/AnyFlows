use std::sync::Arc;

use af_admin::{
    AdminUsageLogListQuery, OrganizationActivityError, OrganizationActivityService,
    OrganizationAuditChangeSummary, OrganizationAuditListFuture, OrganizationAuditListQuery,
    OrganizationAuditRecordFuture, OrganizationAuditRequestContext, OrganizationContext,
    OrganizationUsageLogFuture, OrganizationUsageLogScope, SessionPrincipal,
};
use af_db::OrganizationAuditOutcome;
use af_domain::OrganizationId;

struct TestOrganizationActivityService;

impl OrganizationActivityService for TestOrganizationActivityService {
    fn list_usage<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _organization_id: OrganizationId,
        _scope: OrganizationUsageLogScope,
        _query: AdminUsageLogListQuery,
    ) -> OrganizationUsageLogFuture<'a> {
        Box::pin(async { Err(OrganizationActivityError::Internal) })
    }

    fn list_usage_as_platform<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _organization_id: OrganizationId,
        _query: AdminUsageLogListQuery,
        _platform_reason: String,
        _request: OrganizationAuditRequestContext,
    ) -> OrganizationUsageLogFuture<'a> {
        Box::pin(async { Err(OrganizationActivityError::Internal) })
    }

    fn list_audit<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _organization_id: OrganizationId,
        _query: OrganizationAuditListQuery,
        _request: OrganizationAuditRequestContext,
    ) -> OrganizationAuditListFuture<'a> {
        Box::pin(async { Err(OrganizationActivityError::Internal) })
    }

    fn list_audit_as_platform<'a>(
        &'a self,
        _principal: SessionPrincipal,
        _organization_id: OrganizationId,
        _query: OrganizationAuditListQuery,
        _platform_reason: String,
        _request: OrganizationAuditRequestContext,
    ) -> OrganizationAuditListFuture<'a> {
        Box::pin(async { Err(OrganizationActivityError::Internal) })
    }

    fn record_member_action<'a>(
        &'a self,
        _context: OrganizationContext,
        _request: OrganizationAuditRequestContext,
        _permission_code: &'static str,
        _operation: &'static str,
        _target_type: &'static str,
        _target_identifier: String,
        _outcome: OrganizationAuditOutcome,
        _summary: Option<OrganizationAuditChangeSummary>,
    ) -> OrganizationAuditRecordFuture<'a> {
        Box::pin(async { Ok(()) })
    }
}

pub fn test_organization_activity_service() -> Arc<dyn OrganizationActivityService> {
    Arc::new(TestOrganizationActivityService)
}
