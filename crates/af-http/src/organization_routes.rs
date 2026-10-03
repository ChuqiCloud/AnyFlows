//! 企业 HTTP 路由边界。
//!
//! 该模块只负责把企业服务装配成已经带有会话边界的路由树。公共路由组合器
//! 不需要了解企业处理器的具体实现，发行版可以用 `HttpExtensions` 替换这组路由。

use std::sync::Arc;

use af_admin::{
    OrganizationActivityService, OrganizationApprovalNotificationService,
    OrganizationApprovalService, OrganizationBudgetService, OrganizationContractPriceService,
    OrganizationCreditTermsService, OrganizationPlanService, OrganizationScimService,
    OrganizationTokenService, OrganizationWalletService, PlatformAuditService,
    PlatformOrganizationDirectoryService, PlatformOrganizationManagementService,
    SessionAuthenticator, UserTopupService,
};
use af_config::ServerConfig;
use axum::Router;

use crate::organization_activity::build_organization_activity_router;
use crate::organization_approval::build_organization_approval_router;
use crate::organization_approval_notifications::build_organization_approval_notification_router;
use crate::organization_budget::build_organization_budget_router;
use crate::organization_contract_price::build_organization_contract_price_router;
use crate::organization_credit_terms::build_organization_credit_terms_router;
use crate::organization_plan::build_organization_plan_router;
use crate::organization_provisioning::build_organization_provisioning_router;
use crate::organization_scim_audit::build_organization_scim_audit_router;
use crate::organization_service_account_runtime::build_organization_service_account_runtime_router;
use crate::organization_sso_login::{
    OrganizationSsoPublicLoginProtection, build_organization_sso_login_router_with_protection,
};
use crate::organization_tokens::build_organization_token_router;
use crate::organization_verification::build_organization_verification_router;
use crate::organization_wallet::build_organization_wallet_router;
use crate::organizations::build_organization_router_with_domain;
use crate::platform_audit::build_platform_audit_router;
use crate::platform_organizations::build_platform_organization_router;
use crate::scim::build_scim_router;

/// 企业 HTTP 路由所需的服务集合。
///
/// 将组织、企业 SSO、服务账号和平台组织服务集中在一个注入对象中。
/// 发行版可以构造该集合，也可以通过 `HttpExtension` 提供替代实现。
pub struct OrganizationHttpServices {
    pub approval_notification: Arc<dyn OrganizationApprovalNotificationService>,
    pub authorization: Option<Arc<dyn af_admin::OrganizationAuthorizationService>>,
    pub activity: Option<Arc<dyn OrganizationActivityService>>,
    pub wallet: Option<Arc<dyn OrganizationWalletService>>,
    pub budget: Option<Arc<dyn OrganizationBudgetService>>,
    pub contract_price: Option<Arc<dyn OrganizationContractPriceService>>,
    pub credit_terms: Option<Arc<dyn OrganizationCreditTermsService>>,
    pub token: Option<Arc<dyn OrganizationTokenService>>,
    pub scim: Option<Arc<dyn OrganizationScimService>>,
    pub approval: Option<Arc<dyn OrganizationApprovalService>>,
    pub provisioning: Option<Arc<dyn af_admin::OrganizationProvisioningService>>,
    pub verification: Option<Arc<dyn af_admin::OrganizationVerificationService>>,
    pub plan: Option<Arc<dyn OrganizationPlanService>>,
    pub plan_order: Option<Arc<dyn af_admin::OrganizationPlanOrderService>>,
    pub invitation: Option<Arc<dyn af_admin::OrganizationInvitationService>>,
    pub management: Option<Arc<dyn af_admin::OrganizationManagementService>>,
    pub custom_role: Option<Arc<dyn af_admin::OrganizationCustomRoleService>>,
    pub custom_role_binding: Option<Arc<dyn af_admin::OrganizationCustomRoleBindingService>>,
    pub service_account: Option<Arc<dyn af_admin::OrganizationServiceAccountService>>,
    pub service_account_runtime_authenticator:
        Option<Arc<dyn af_admin::OrganizationServiceAccountRuntimeAuthenticator>>,
    pub platform_audit: Option<Arc<dyn PlatformAuditService>>,
    pub platform_organization_services: Option<(
        Arc<dyn PlatformOrganizationDirectoryService>,
        Arc<dyn PlatformOrganizationManagementService>,
    )>,
    pub sso_provider: Option<Arc<dyn af_admin::OrganizationSsoProviderService>>,
    pub sso_policy: Option<Arc<dyn af_admin::OrganizationSsoPolicyService>>,
    pub sso_domain: Option<Arc<dyn af_admin::OrganizationSsoDomainService>>,
    pub sso_login: Option<Arc<dyn af_admin::OrganizationSsoLoginTransactionService>>,
    pub sso_public_login_rate_limit_store:
        Option<Arc<dyn af_domain::OrganizationSsoPublicLoginRateLimitStore>>,
    pub sso_public_login_fingerprint_secret: [u8; 32],
    pub sso_session: Option<Arc<dyn af_admin::OrganizationSsoSessionService>>,
    pub sso_identity_binding: Option<Arc<dyn af_admin::OrganizationSsoIdentityBindingService>>,
    pub sso_recovery: Option<Arc<dyn af_admin::OrganizationSsoRecoveryService>>,
}

pub(crate) struct OrganizationHttpRoutes {
    pub(crate) public: Router,
    pub(crate) management: Router,
}

pub(crate) fn build_organization_routes(
    services: OrganizationHttpServices,
    session_authenticator: Arc<dyn SessionAuthenticator>,
    user_topup_service: Option<Arc<dyn UserTopupService>>,
    config: &ServerConfig,
) -> OrganizationHttpRoutes {
    let OrganizationHttpServices {
        approval_notification,
        authorization,
        activity,
        wallet,
        budget,
        contract_price,
        credit_terms,
        token,
        scim,
        approval,
        provisioning,
        verification,
        plan,
        plan_order,
        invitation,
        management,
        custom_role,
        custom_role_binding,
        service_account,
        service_account_runtime_authenticator,
        platform_audit,
        platform_organization_services,
        sso_provider,
        sso_policy,
        sso_domain,
        sso_login,
        sso_public_login_rate_limit_store,
        sso_public_login_fingerprint_secret,
        sso_session,
        sso_identity_binding,
        sso_recovery,
    } = services;
    let mut public_routes = build_organization_approval_notification_router(
        approval_notification,
        Arc::clone(&session_authenticator),
    );
    let mut management_routes = Router::new();

    management_routes = match sso_login.clone() {
        Some(service) => {
            management_routes.merge(build_organization_sso_login_router_with_protection(
                service,
                sso_session,
                Some(OrganizationSsoPublicLoginProtection::new(
                    crate::client_ip::ClientIpResolver::from_config(config),
                    sso_public_login_rate_limit_store,
                    sso_public_login_fingerprint_secret,
                )),
            ))
        }
        None => management_routes,
    };

    management_routes = match (authorization.clone(), activity.clone()) {
        (Some(authorization), Some(activity)) => {
            management_routes.merge(build_organization_router_with_domain(
                authorization,
                invitation,
                management,
                custom_role,
                custom_role_binding,
                activity,
                Arc::clone(&session_authenticator),
                sso_provider,
                sso_policy,
                sso_domain,
                sso_login,
                sso_identity_binding,
                sso_recovery,
                service_account,
            ))
        }
        _ => management_routes,
    };
    management_routes = management_routes.merge(build_organization_provisioning_router(
        provisioning,
        Arc::clone(&session_authenticator),
    ));
    management_routes = management_routes.merge(build_organization_verification_router(
        verification,
        Arc::clone(&session_authenticator),
    ));
    management_routes = management_routes.merge(build_organization_plan_router(
        plan,
        plan_order,
        Arc::clone(&session_authenticator),
        user_topup_service.clone(),
    ));

    management_routes = match (authorization.clone(), contract_price, activity.clone()) {
        (Some(authorization), Some(contract_price), Some(activity)) => {
            management_routes.merge(build_organization_contract_price_router(
                authorization,
                contract_price,
                activity,
                Arc::clone(&session_authenticator),
            ))
        }
        _ => management_routes,
    };
    management_routes = match (authorization.clone(), credit_terms, activity.clone()) {
        (Some(authorization), Some(credit_terms), Some(activity)) => {
            management_routes.merge(build_organization_credit_terms_router(
                authorization,
                credit_terms,
                activity,
                Arc::clone(&session_authenticator),
            ))
        }
        _ => management_routes,
    };
    management_routes = match (authorization.clone(), wallet, activity.clone()) {
        (Some(authorization), Some(wallet), Some(activity)) => {
            management_routes.merge(build_organization_wallet_router(
                authorization,
                wallet,
                user_topup_service,
                activity,
                Arc::clone(&session_authenticator),
            ))
        }
        _ => management_routes,
    };
    management_routes = match (authorization.clone(), token, activity.clone()) {
        (Some(authorization), Some(token), Some(activity)) => {
            management_routes.merge(build_organization_token_router(
                authorization,
                token,
                activity,
                Arc::clone(&session_authenticator),
            ))
        }
        _ => management_routes,
    };
    management_routes = match (authorization.clone(), budget, activity.clone()) {
        (Some(authorization), Some(budget), Some(activity)) => {
            management_routes.merge(build_organization_budget_router(
                authorization,
                budget,
                activity,
                Arc::clone(&session_authenticator),
            ))
        }
        _ => management_routes,
    };
    management_routes = match (authorization.clone(), approval, activity.clone()) {
        (Some(authorization), Some(approval), Some(activity)) => {
            management_routes.merge(build_organization_approval_router(
                authorization,
                approval,
                activity,
                Arc::clone(&session_authenticator),
            ))
        }
        _ => management_routes,
    };
    management_routes = match activity {
        Some(activity) => management_routes.merge(build_organization_activity_router(
            activity,
            Arc::clone(&session_authenticator),
        )),
        None => management_routes,
    };
    management_routes = match (scim.clone(), authorization.clone()) {
        (Some(scim), Some(authorization)) => {
            management_routes.merge(build_organization_scim_audit_router(
                scim,
                authorization,
                Arc::clone(&session_authenticator),
            ))
        }
        _ => management_routes,
    };
    public_routes = match scim {
        Some(scim) => public_routes.merge(build_scim_router(scim)),
        None => public_routes,
    };
    public_routes = match service_account_runtime_authenticator {
        Some(authenticator) => public_routes.merge(
            build_organization_service_account_runtime_router(authenticator),
        ),
        None => public_routes,
    };
    management_routes = match platform_audit {
        Some(service) => management_routes.merge(build_platform_audit_router(
            service,
            Arc::clone(&session_authenticator),
        )),
        None => management_routes,
    };
    management_routes = match platform_organization_services {
        Some((directory, management)) => {
            management_routes.merge(build_platform_organization_router(
                directory,
                management,
                Arc::clone(&session_authenticator),
            ))
        }
        None => management_routes,
    };

    OrganizationHttpRoutes {
        public: public_routes,
        management: management_routes,
    }
}
