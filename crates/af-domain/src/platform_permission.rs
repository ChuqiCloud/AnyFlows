/// Platform permissions shared by administration and extension audit records.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PlatformPermission {
    UserDirectoryReadAll,
    DashboardReadAll,
    PlatformAuditReadAll,
    OrganizationsReadAll,
    OrganizationUsageLogsReadAll,
    OrganizationAuditReadAll,
    OrganizationsManage,
    CustomOAuth2ProvidersManage,
    ModelProvidersManage,
    AccountVerificationsReadAll,
    AccountVerificationsManage,
}

impl PlatformPermission {
    /// Stable permission code used by authorization and audit storage.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::UserDirectoryReadAll => "platform.user_directory.read_all",
            Self::DashboardReadAll => "platform.dashboard.read_all",
            Self::PlatformAuditReadAll => "platform.audit.read_all",
            Self::OrganizationsReadAll => "platform.organizations.read_all",
            Self::OrganizationUsageLogsReadAll => "platform.organization_usage_logs.read_all",
            Self::OrganizationAuditReadAll => "platform.organization_audit.read_all",
            Self::OrganizationsManage => "platform.organizations.manage",
            Self::CustomOAuth2ProvidersManage => "platform.custom_oauth2_providers.manage",
            Self::ModelProvidersManage => "platform.model_providers.manage",
            Self::AccountVerificationsReadAll => "platform.account_verifications.read_all",
            Self::AccountVerificationsManage => "platform.account_verifications.manage",
        }
    }
}
