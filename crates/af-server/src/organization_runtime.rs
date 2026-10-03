use std::{sync::Arc, time::Duration};

use af_account::{
    ControlledOrganizationSsoDnsResolver, OrganizationInvitationCipher, SystemSecretCipher,
};
use af_admin::{
    DatabaseOrganizationActivityService, DatabaseOrganizationApprovalNotificationService,
    DatabaseOrganizationApprovalService, DatabaseOrganizationBudgetService,
    DatabaseOrganizationContractPriceService, DatabaseOrganizationCreditTermsService,
    DatabaseOrganizationCustomRoleBindingService, DatabaseOrganizationCustomRoleService,
    DatabaseOrganizationScimService, DatabaseOrganizationSsoDomainService,
    DatabaseOrganizationSsoIdentityBindingService, DatabaseOrganizationSsoLoginTransactionService,
    DatabaseOrganizationSsoPolicyService, DatabaseOrganizationSsoProviderService,
    DatabaseOrganizationSsoRecoveryService, DatabaseOrganizationTokenService,
    DatabaseOrganizationVerificationService, DatabaseOrganizationWalletService,
    DatabasePlatformAuditService, OrganizationActivityService,
    OrganizationApprovalNotificationService, OrganizationApprovalService,
    OrganizationBudgetService, OrganizationContractPriceService, OrganizationCreditTermsService,
    OrganizationCustomRoleBindingService, OrganizationCustomRoleService, OrganizationScimService,
    OrganizationSsoDomainService, OrganizationSsoIdentityBindingService,
    OrganizationSsoPolicyService, OrganizationSsoProviderService, OrganizationSsoRecoveryService,
    OrganizationTokenService, OrganizationWalletService, PlatformAuditService,
};
use af_db::{
    OrganizationApprovalNotificationRepository, OrganizationApprovalRepository,
    OrganizationAuditRepository, OrganizationBudgetRepository, OrganizationContractPriceRepository,
    OrganizationCreditInvoiceRepository, OrganizationCreditRepaymentRepository,
    OrganizationCreditTermsRepository, OrganizationCustomRoleBindingRepository,
    OrganizationCustomRoleRepository, OrganizationDepartmentMembershipRepository,
    OrganizationDepartmentRepository, OrganizationPlanOrderRepository, OrganizationPlanRepository,
    OrganizationProvisioningRepository, OrganizationRepository, OrganizationScimAuditRepository,
    OrganizationScimGroupRepository, OrganizationScimTokenRepository,
    OrganizationScimUserRepository, OrganizationServiceAccountBudgetBindingRepository,
    OrganizationServiceAccountRepository, OrganizationSsoProviderRepository,
    OrganizationTokenRepository, OrganizationVerificationRepository, OrganizationWalletRepository,
    PlatformAuditRepository,
};

use crate::BootstrapError;

/// 企业装配可复用的公共运行时依赖。
///
/// 由启动器在数据库迁移和公共认证服务初始化后创建。所有服务引用都指向后续公共
/// 路由使用的同一实例；装配函数可以克隆需要长期持有的依赖。
pub struct OrganizationRuntimeContext<'a> {
    /// 已通过校验的应用配置，仅用于受信任的服务装配。
    pub config: &'a af_config::AppConfig,
    /// 公共迁移和扩展迁移均已完成的应用连接池。
    pub database: &'a af_db::DatabasePool,
    /// 公共管理会话认证器，已应用双因素认证密文配置。
    pub session_authenticator: &'a Arc<dyn af_admin::SessionAuthenticator>,
    /// 公共个人资料与安全凭据服务。
    pub user_profile_service: &'a Arc<dyn af_admin::UserProfileService>,
    /// 仅在站点公开地址满足 WebAuthn 要求时提供。
    pub passkey_authentication_service: Option<&'a Arc<dyn af_admin::PasskeyAuthenticationService>>,
    /// 与公共认证使用相同密钥的系统秘密加密器。
    pub system_secret_cipher: &'a SystemSecretCipher,
    /// 公共调用日志仓储，供企业活动服务读取授权范围内的记录。
    pub usage_log_repository: &'a af_db::AdminUsageLogRepository,
    /// 公共请求终态仓储，包括成功和失败事实。
    pub request_outcome_repository: &'a af_db::RequestOutcomeRepository,
    /// 启动期已连接的 SCIM 限流存储；未配置 Redis 时为 None。
    pub scim_rate_limit_store: Option<Arc<dyn af_domain::OrganizationScimRateLimitStore>>,
}

/// 企业组织运行时依赖的组合边界。
///
/// 发行版可以围绕该组合对象迁出企业服务装配，同时继续复用公共认证、计费和转发
/// 基础设施。具体实现仍通过现有服务 trait 注入，路由行为不由该类型改变。
pub struct OrganizationRuntimeServices {
    pub organization_approval_notification_service:
        Arc<dyn OrganizationApprovalNotificationService>,
    pub organization_wallet_service: Arc<dyn OrganizationWalletService>,
    pub organization_budget_service: Arc<dyn OrganizationBudgetService>,
    pub organization_credit_terms_service: Arc<dyn OrganizationCreditTermsService>,
    pub organization_contract_price_repository: Arc<OrganizationContractPriceRepository>,
    pub organization_contract_price_service: Arc<dyn OrganizationContractPriceService>,
    pub organization_token_service: Arc<dyn OrganizationTokenService>,
    pub organization_scim_service: Arc<dyn OrganizationScimService>,
    pub organization_approval_service: Arc<dyn OrganizationApprovalService>,
    pub organization_provisioning_service: Arc<dyn af_admin::OrganizationProvisioningService>,
    pub organization_verification_service: Arc<dyn af_admin::OrganizationVerificationService>,
    pub organization_plan_service: Arc<dyn af_admin::OrganizationPlanService>,
    pub organization_plan_order_repository: OrganizationPlanOrderRepository,
    pub organization_repository: Arc<OrganizationRepository>,
    pub organization_activity_service: Arc<dyn OrganizationActivityService>,
    pub platform_audit_service: Arc<dyn PlatformAuditService>,
    pub organization_authorization_service: Arc<dyn af_admin::OrganizationAuthorizationService>,
    pub organization_invitation_service: Arc<dyn af_admin::OrganizationInvitationService>,
    pub organization_management_service: Arc<dyn af_admin::OrganizationManagementService>,
    pub organization_custom_role_service: Arc<dyn OrganizationCustomRoleService>,
    pub organization_custom_role_binding_service: Arc<dyn OrganizationCustomRoleBindingService>,
    pub organization_service_account_service: Arc<dyn af_admin::OrganizationServiceAccountService>,
    pub organization_service_account_runtime_authenticator:
        Arc<dyn af_admin::OrganizationServiceAccountRuntimeAuthenticator>,
    pub organization_sso_provider_service: Arc<dyn OrganizationSsoProviderService>,
    pub organization_sso_domain_service: Arc<dyn OrganizationSsoDomainService>,
    pub organization_sso_login_service: Arc<DatabaseOrganizationSsoLoginTransactionService>,
    pub organization_sso_session_service: Arc<dyn af_admin::OrganizationSsoSessionService>,
    pub organization_sso_identity_binding_service: Arc<dyn OrganizationSsoIdentityBindingService>,
    pub organization_sso_policy_service: Arc<dyn OrganizationSsoPolicyService>,
    pub organization_sso_recovery_service: Arc<dyn OrganizationSsoRecoveryService>,
}

impl OrganizationRuntimeServices {
    /// 装配迁移期间保留的内置企业实现。
    ///
    /// 默认启动入口使用此函数。私有发行版可以提供自己的装配函数，或以此为基础
    /// 逐步替换服务。该函数不会创建新的公共认证实例或运行数据库迁移。
    pub fn from_database(context: OrganizationRuntimeContext<'_>) -> Result<Self, BootstrapError> {
        let auth = context.config.auth();
        let organization_repository = OrganizationRepository::new(
            context.database.clone(),
            Duration::from_secs(auth.lookup_timeout_secs()),
        )
        .map_err(BootstrapError::OrganizationRepository)?;
        let organization_repository = Arc::new(organization_repository);
        let organization_approval_repository = Arc::new(
            OrganizationApprovalRepository::new(
                context.database.clone(),
                Duration::from_secs(auth.lookup_timeout_secs()),
            )
            .map_err(BootstrapError::OrganizationApprovalRepository)?,
        );
        let organization_provisioning_repository = Arc::new(
            OrganizationProvisioningRepository::new(
                context.database.clone(),
                Duration::from_secs(auth.lookup_timeout_secs()),
            )
            .map_err(BootstrapError::OrganizationProvisioningRepository)?,
        );
        let organization_verification_repository = Arc::new(
            OrganizationVerificationRepository::new(
                context.database.clone(),
                Duration::from_secs(auth.lookup_timeout_secs()),
            )
            .map_err(BootstrapError::OrganizationVerificationRepository)?,
        );
        let organization_plan_repository = Arc::new(
            OrganizationPlanRepository::new(
                context.database.clone(),
                Duration::from_secs(auth.lookup_timeout_secs()),
            )
            .map_err(BootstrapError::OrganizationPlanRepository)?,
        );
        let organization_plan_order_repository = OrganizationPlanOrderRepository::new(
            context.database.clone(),
            Duration::from_secs(auth.lookup_timeout_secs()),
        )
        .map_err(BootstrapError::OrganizationPlanOrderRepository)?;
        let organization_approval_notification_repository = Arc::new(
            OrganizationApprovalNotificationRepository::new(
                context.database.clone(),
                Duration::from_secs(auth.lookup_timeout_secs()),
            )
            .map_err(BootstrapError::OrganizationApprovalNotificationRepository)?,
        );
        let organization_department_repository = Arc::new(
            OrganizationDepartmentRepository::new(
                context.database.clone(),
                Duration::from_secs(auth.lookup_timeout_secs()),
            )
            .map_err(BootstrapError::OrganizationDepartmentRepository)?,
        );
        let organization_department_membership_repository = Arc::new(
            OrganizationDepartmentMembershipRepository::new(
                context.database.clone(),
                Duration::from_secs(auth.lookup_timeout_secs()),
            )
            .map_err(BootstrapError::OrganizationDepartmentMembershipRepository)?,
        );
        let organization_custom_role_repository = Arc::new(
            OrganizationCustomRoleRepository::new(
                context.database.clone(),
                Duration::from_secs(auth.lookup_timeout_secs()),
            )
            .map_err(BootstrapError::OrganizationCustomRoleRepository)?,
        );
        let organization_custom_role_binding_repository = Arc::new(
            OrganizationCustomRoleBindingRepository::new(
                context.database.clone(),
                Duration::from_secs(auth.lookup_timeout_secs()),
            )
            .map_err(BootstrapError::OrganizationCustomRoleBindingRepository)?,
        );
        let organization_service_account_repository = Arc::new(
            OrganizationServiceAccountRepository::new(
                context.database.clone(),
                Duration::from_secs(auth.lookup_timeout_secs()),
            )
            .map_err(BootstrapError::OrganizationServiceAccountRepository)?,
        );
        let organization_service_account_budget_binding_repository = Arc::new(
            OrganizationServiceAccountBudgetBindingRepository::new(
                context.database.clone(),
                Duration::from_secs(auth.lookup_timeout_secs()),
            )
            .map_err(BootstrapError::OrganizationServiceAccountBudgetBindingRepository)?,
        );
        let organization_wallet_repository = Arc::new(
            OrganizationWalletRepository::new(
                context.database.clone(),
                Duration::from_secs(auth.lookup_timeout_secs()),
            )
            .map_err(BootstrapError::OrganizationWalletRepository)?,
        );
        let organization_budget_repository = Arc::new(
            OrganizationBudgetRepository::new(
                context.database.clone(),
                Duration::from_secs(auth.lookup_timeout_secs()),
                Duration::from_secs(15 * 60),
            )
            .map_err(BootstrapError::OrganizationBudgetRepository)?,
        );
        let organization_credit_terms_repository = Arc::new(
            OrganizationCreditTermsRepository::new(
                context.database.clone(),
                Duration::from_secs(auth.lookup_timeout_secs()),
            )
            .map_err(BootstrapError::OrganizationCreditTermsRepository)?,
        );
        let organization_credit_invoice_repository = Arc::new(
            OrganizationCreditInvoiceRepository::new(
                context.database.clone(),
                Duration::from_secs(auth.lookup_timeout_secs()),
            )
            .map_err(BootstrapError::OrganizationCreditInvoiceRepository)?,
        );
        let organization_credit_repayment_repository = Arc::new(
            OrganizationCreditRepaymentRepository::new(
                context.database.clone(),
                Duration::from_secs(auth.lookup_timeout_secs()),
            )
            .map_err(BootstrapError::OrganizationCreditRepaymentRepository)?,
        );
        let organization_contract_price_repository = Arc::new(
            OrganizationContractPriceRepository::new(
                context.database.clone(),
                Duration::from_secs(auth.lookup_timeout_secs()),
            )
            .map_err(BootstrapError::OrganizationContractPriceRepository)?,
        );
        let organization_token_repository = Arc::new(
            OrganizationTokenRepository::new(
                context.database.clone(),
                Duration::from_secs(auth.lookup_timeout_secs()),
            )
            .map_err(BootstrapError::OrganizationTokenRepository)?,
        );
        let organization_scim_token_repository = Arc::new(
            OrganizationScimTokenRepository::new(
                context.database.clone(),
                Duration::from_secs(auth.lookup_timeout_secs()),
            )
            .map_err(BootstrapError::OrganizationScimTokenRepository)?,
        );
        let organization_scim_user_repository = Arc::new(
            OrganizationScimUserRepository::new(
                context.database.clone(),
                Duration::from_secs(auth.lookup_timeout_secs()),
            )
            .map_err(BootstrapError::OrganizationScimUserRepository)?,
        );
        let organization_scim_group_repository = Arc::new(
            OrganizationScimGroupRepository::new(
                context.database.clone(),
                Duration::from_secs(auth.lookup_timeout_secs()),
            )
            .map_err(BootstrapError::OrganizationScimGroupRepository)?,
        );
        let organization_scim_audit_repository = Arc::new(
            OrganizationScimAuditRepository::new(
                context.database.clone(),
                Duration::from_secs(auth.lookup_timeout_secs()),
            )
            .map_err(BootstrapError::OrganizationScimAuditRepository)?,
        );
        let organization_sso_provider_repository = OrganizationSsoProviderRepository::new(
            context.database.clone(),
            Duration::from_secs(auth.lookup_timeout_secs()),
        )
        .map_err(BootstrapError::OrganizationSsoProviderRepository)?;
        // 使用显式公共 nameserver，解析器关闭系统 hosts、缓存和 search 域影响。
        let organization_sso_dns_resolver = Arc::new(
            ControlledOrganizationSsoDnsResolver::new(
                &[
                    std::net::SocketAddr::from(([1, 1, 1, 1], 53)),
                    std::net::SocketAddr::from(([8, 8, 8, 8], 53)),
                ],
                Duration::from_secs(5),
            )
            .map_err(BootstrapError::OrganizationSsoDnsResolver)?,
        );
        let organization_invitation_cipher =
            OrganizationInvitationCipher::new(context.config.credential_encryption())
                .map_err(BootstrapError::OrganizationInvitationCipher)?;
        let organization_authorization_service: Arc<
            dyn af_admin::OrganizationAuthorizationService,
        > = Arc::new(af_admin::DatabaseOrganizationAuthorizationService::new(
            Arc::clone(&organization_repository),
        ));
        let organization_plan_service: Arc<dyn af_admin::OrganizationPlanService> =
            Arc::new(af_admin::DatabaseOrganizationPlanService::new(Arc::clone(
                &organization_plan_repository,
            )));
        let organization_activity_service: Arc<dyn OrganizationActivityService> = Arc::new(
            DatabaseOrganizationActivityService::new(
                Arc::clone(&organization_authorization_service),
                context.usage_log_repository.clone(),
                OrganizationAuditRepository::new(context.database.clone()),
                auth.session_signing_key().map(|key| key.expose()),
            )
            .map_err(BootstrapError::OrganizationActivityService)?
            .with_request_outcomes(context.request_outcome_repository.clone()),
        );
        let platform_audit_service: Arc<dyn PlatformAuditService> =
            Arc::new(DatabasePlatformAuditService::new(
                PlatformAuditRepository::new(context.database.clone()),
            ));
        // 角色目录与成员绑定共用同一服务实例，保证授权边界和配置一致。
        let organization_custom_role_service: Arc<dyn OrganizationCustomRoleService> =
            Arc::new(DatabaseOrganizationCustomRoleService::new(Arc::clone(
                &organization_custom_role_repository,
            )));
        let organization_custom_role_binding_service: Arc<
            dyn OrganizationCustomRoleBindingService,
        > = Arc::new(DatabaseOrganizationCustomRoleBindingService::new(
            organization_custom_role_binding_repository,
            Arc::clone(&organization_custom_role_service),
        ));
        let organization_service_account_service =
            Arc::new(af_admin::DatabaseOrganizationServiceAccountService::new(
                organization_service_account_repository,
                organization_service_account_budget_binding_repository,
            ));
        let organization_sso_identity_binding_service: Arc<
            dyn OrganizationSsoIdentityBindingService,
        > = Arc::new({
            let service = DatabaseOrganizationSsoIdentityBindingService::new(
                Arc::clone(context.user_profile_service),
                organization_sso_provider_repository.clone(),
            );
            match context.passkey_authentication_service.cloned() {
                Some(passkey) => service.with_passkey_authentication_service(passkey),
                None => service,
            }
        });
        let organization_sso_recovery_service: Arc<dyn OrganizationSsoRecoveryService> =
            Arc::new(DatabaseOrganizationSsoRecoveryService::new(
                Arc::clone(context.user_profile_service),
                organization_sso_provider_repository.clone(),
            ));
        let organization_sso_policy_service: Arc<dyn OrganizationSsoPolicyService> = Arc::new(
            DatabaseOrganizationSsoPolicyService::new(organization_sso_provider_repository.clone()),
        );
        let organization_sso_login_service = Arc::new(
            DatabaseOrganizationSsoLoginTransactionService::new_with_organization_repository(
                organization_sso_provider_repository.clone(),
                organization_repository.as_ref().clone(),
                context.system_secret_cipher.clone(),
                organization_invitation_cipher.clone(),
            ),
        );
        let organization_sso_login_service_trait: Arc<
            dyn af_admin::OrganizationSsoLoginTransactionService,
        > = organization_sso_login_service.clone();
        let organization_sso_session_service: Arc<dyn af_admin::OrganizationSsoSessionService> =
            Arc::new(af_admin::DatabaseOrganizationSsoSessionService::new(
                organization_sso_login_service_trait,
                Arc::clone(context.session_authenticator),
            ));
        Ok(Self {
            organization_approval_notification_service: Arc::new(
                DatabaseOrganizationApprovalNotificationService::new(
                    organization_approval_notification_repository,
                ),
            ),
            organization_wallet_service: Arc::new(DatabaseOrganizationWalletService::new(
                organization_wallet_repository,
            )),
            organization_budget_service: Arc::new(DatabaseOrganizationBudgetService::new(
                organization_budget_repository,
            )),
            organization_credit_terms_service: Arc::new(
                DatabaseOrganizationCreditTermsService::new(organization_credit_terms_repository)
                    .with_ledger_repositories(
                        organization_credit_invoice_repository,
                        organization_credit_repayment_repository,
                    ),
            ),
            organization_contract_price_repository: Arc::clone(
                &organization_contract_price_repository,
            ),
            organization_contract_price_service: Arc::new(
                DatabaseOrganizationContractPriceService::new(
                    organization_contract_price_repository,
                ),
            ),
            organization_token_service: Arc::new(DatabaseOrganizationTokenService::new(
                organization_token_repository,
            )),
            organization_scim_service: Arc::new(DatabaseOrganizationScimService::new(
                organization_scim_token_repository,
                organization_scim_user_repository,
                organization_scim_group_repository,
                organization_scim_audit_repository,
                context.scim_rate_limit_store,
            )),
            organization_approval_service: Arc::new(DatabaseOrganizationApprovalService::new(
                organization_approval_repository,
            )),
            organization_provisioning_service: Arc::new(
                af_admin::DatabaseOrganizationProvisioningService::new(
                    organization_provisioning_repository,
                    Arc::clone(&platform_audit_service),
                ),
            ),
            organization_verification_service: Arc::new(
                DatabaseOrganizationVerificationService::new(
                    organization_verification_repository,
                    Arc::clone(&organization_authorization_service),
                    Arc::clone(&platform_audit_service),
                ),
            ),
            organization_plan_service,
            organization_plan_order_repository,
            organization_repository: Arc::clone(&organization_repository),
            organization_activity_service,
            platform_audit_service,
            organization_authorization_service,
            organization_invitation_service: Arc::new(
                af_admin::DatabaseOrganizationInvitationService::new(
                    Arc::clone(&organization_repository),
                    organization_invitation_cipher.clone(),
                ),
            ),
            organization_management_service: Arc::new(
                af_admin::DatabaseOrganizationManagementService::new(Arc::clone(
                    &organization_repository,
                ))
                .with_department_repository(organization_department_repository)
                .with_department_membership_repository(
                    organization_department_membership_repository,
                ),
            ),
            organization_custom_role_service,
            organization_custom_role_binding_service,
            organization_service_account_service: organization_service_account_service.clone(),
            organization_service_account_runtime_authenticator:
                organization_service_account_service.clone(),
            organization_sso_provider_service: Arc::new(
                DatabaseOrganizationSsoProviderService::new(
                    organization_sso_provider_repository.clone(),
                    context.system_secret_cipher.clone(),
                ),
            ),
            organization_sso_domain_service: Arc::new(DatabaseOrganizationSsoDomainService::new(
                organization_sso_provider_repository.clone(),
                organization_sso_dns_resolver,
            )),
            organization_sso_login_service,
            organization_sso_session_service,
            organization_sso_identity_binding_service,
            organization_sso_policy_service,
            organization_sso_recovery_service,
        })
    }
}
