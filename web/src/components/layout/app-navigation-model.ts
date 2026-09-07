export type NavKey =
  | 'overview'
  | 'models'
  | 'playground'
  | 'videoTasks'
  | 'apiKeys'
  | 'announcements'
  | 'organizationApiKeys'
  | 'organizationActivity'
  | 'organizationBudgets'
  | 'organizationContractPrices'
  | 'organizationCredit'
  | 'organizationApprovals'
  | 'organizationMembers'
  | 'organizationDepartments'
  | 'organizationOverview'
  | 'organizationOnboarding'
  | 'organizationProvisioning'
  | 'organizationVerification'
  | 'organizationPlans'
  | 'platformOrganizations'
  | 'organizationSettings'
  | 'organizationServiceAccounts'
  | 'organizationWallet'
  | 'wallet'
  | 'subscriptions'
  | 'invitations'
  | 'profile'
  | 'users'
  | 'subscriptionManagement'
  | 'redemptionCodes'
  | 'refunds'
  | 'siteSettings'
  | 'authenticationSettings'
  | 'emailSettings'
  | 'networkSettings'
  | 'paymentSettings'
  | 'billingSettings'
  | 'groupSettings'
  | 'modelSettings'
  | 'channels'
  | 'credentials'
  | 'credentialProxies'
  | 'routes'
  | 'keys'
  | 'usage'
  | 'debugTraces'

export type AdminSectionKey = 'personal' | 'organization' | 'enterprise' | 'operations' | 'gateway' | 'platform'

export const userNavigationGroups = [
  { key: 'use', items: ['models', 'playground', 'videoTasks'] },
  { key: 'organization', items: ['organizationOverview', 'organizationOnboarding', 'organizationMembers', 'organizationDepartments', 'organizationApiKeys', 'organizationServiceAccounts', 'organizationBudgets', 'organizationContractPrices', 'organizationCredit', 'organizationApprovals', 'organizationWallet', 'organizationActivity', 'organizationVerification', 'organizationSettings'] },
  { key: 'account', items: ['apiKeys', 'wallet', 'usage', 'subscriptions', 'invitations', 'organizationProvisioning', 'profile'] },
] as const satisfies ReadonlyArray<{ key: 'use' | 'organization' | 'account'; items: readonly NavKey[] }>

export const adminWorkspaceItems = ['overview', 'usage', 'models', 'playground', 'videoTasks'] as const satisfies readonly NavKey[]

export const adminNavigationSections = [
  { key: 'personal', items: ['apiKeys', 'wallet', 'subscriptions', 'invitations', 'profile'] },
  { key: 'organization', items: ['organizationOverview', 'organizationOnboarding', 'organizationMembers', 'organizationDepartments', 'organizationApiKeys', 'organizationServiceAccounts', 'organizationBudgets', 'organizationContractPrices', 'organizationCredit', 'organizationApprovals', 'organizationWallet', 'organizationActivity', 'organizationSettings'] },
  // 企业申请、认证、目录和用户归属必须在同一个管理员分组内，避免跨页面寻找关键入口。
  { key: 'enterprise', items: ['platformOrganizations', 'organizationProvisioning', 'organizationVerification', 'users', 'organizationPlans'] },
  { key: 'operations', items: ['subscriptionManagement', 'redemptionCodes', 'refunds', 'paymentSettings', 'billingSettings'] },
  { key: 'gateway', items: ['modelSettings', 'groupSettings', 'channels', 'credentials', 'credentialProxies', 'routes'] },
  { key: 'platform', items: ['siteSettings', 'authenticationSettings', 'announcements', 'emailSettings', 'networkSettings', 'keys'] },
] as const satisfies ReadonlyArray<{ key: AdminSectionKey; items: readonly NavKey[] }>

export const adminSystemItems = ['debugTraces'] as const satisfies readonly NavKey[]

export const adminNavigationItems: readonly NavKey[] = [
  ...adminWorkspaceItems,
  ...adminNavigationSections.flatMap((section) => section.items),
  ...adminSystemItems,
]

export const userNavigationItems: readonly NavKey[] = userNavigationGroups.flatMap((group) => group.items)

/** 企业菜单按当前成员的服务端权限投影裁剪，避免把无权页面伪装成可用功能。 */
export const organizationNavigationPermissions: Readonly<Partial<Record<NavKey, readonly string[]>>> = {
  organizationOverview: ['organization.overview.read'],
  organizationOnboarding: ['organization.profile.write'],
  organizationMembers: ['organization.overview.read'],
  organizationDepartments: ['organization.departments.read'],
  organizationApiKeys: ['organization.keys.manage_self', 'organization.keys.read_all'],
  organizationServiceAccounts: ['organization.service_accounts.read'],
  organizationBudgets: ['organization.budgets.manage'],
  organizationContractPrices: ['organization.contract_prices.read'],
  organizationCredit: ['organization.credit.read'],
  organizationApprovals: ['organization.approvals.read', 'organization.approvals.create'],
  organizationWallet: ['organization.wallet.read'],
  organizationActivity: ['organization.usage_logs.read_all', 'organization.usage_logs.read_self', 'organization.audit.read'],
  organizationVerification: ['organization.profile.write'],
  organizationSettings: ['organization.profile.write', 'organization.roles.read'],
}

/**
 * 只返回当前工作空间可用的企业入口；权限尚未加载时保留概览，避免导航闪现全部管理项。
 */
export function filterOrganizationNavigationItems(
  items: readonly NavKey[],
  permissions: readonly string[] | undefined,
  workspaceSelected: boolean,
  permissionsLoading: boolean,
): readonly NavKey[] {
  if (!workspaceSelected) {
    return []
  }
  if (permissionsLoading || permissions === undefined) {
    return items.filter((item) => item === 'organizationOverview')
  }
  const granted = new Set(permissions)
  return items.filter((item) => {
    const required = organizationNavigationPermissions[item]
    return required === undefined || required.some((permission) => granted.has(permission))
  })
}

export const activeKeyByView: Readonly<Record<string, NavKey>> = {
  overview: 'overview',
  models: 'models',
  playground: 'playground',
  'video-tasks': 'videoTasks',
  'api-keys': 'apiKeys',
  'organization-api-keys': 'organizationApiKeys',
  'organization-activity': 'organizationActivity',
  'organization-budgets': 'organizationBudgets',
  'organization-contract-prices': 'organizationContractPrices',
  'organization-credit': 'organizationCredit',
  'organization-approvals': 'organizationApprovals',
  'organization-members': 'organizationMembers',
  'organization-departments': 'organizationDepartments',
  'organization-overview': 'organizationOverview',
  'organization-onboarding': 'organizationOnboarding',
  'organization-provisioning': 'organizationProvisioning',
  'organization-verification': 'organizationVerification',
  'organization-plans': 'organizationPlans',
  'platform-organizations': 'platformOrganizations',
  'organization-settings': 'organizationSettings',
  'organization-service-accounts': 'organizationServiceAccounts',
  'organization-wallet': 'organizationWallet',
  wallet: 'wallet',
  subscriptions: 'subscriptions',
  invitations: 'invitations',
  profile: 'profile',
  users: 'users',
  'subscription-management': 'subscriptionManagement',
  'redemption-codes': 'redemptionCodes',
  refunds: 'refunds',
  'site-settings': 'siteSettings',
  announcements: 'announcements',
  'authentication-settings': 'authenticationSettings',
  'email-settings': 'emailSettings',
  'network-settings': 'networkSettings',
  'payment-settings': 'paymentSettings',
  'billing-settings': 'billingSettings',
  'group-settings': 'groupSettings',
  'model-settings': 'modelSettings',
  channels: 'channels',
  credentials: 'credentials',
  'credential-proxies': 'credentialProxies',
  routes: 'routes',
  tokens: 'keys',
  'usage-logs': 'usage',
  'debug-traces': 'debugTraces',
}
