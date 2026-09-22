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

/** 侧栏板块：工作台 / 企业管理 / 系统管理（最后一档仅管理员可见）。 */
export type NavigationSectionKey = 'workspace' | 'enterprise' | 'system'

/** 工作台：模型、试验场、视频任务与账户自服务，两类角色都能看到。 */
export const userWorkspaceItems = [
  'models',
  'playground',
  'videoTasks',
  'usage',
  'apiKeys',
  'wallet',
  'subscriptions',
  'invitations',
  'organizationProvisioning',
  'profile',
] as const satisfies readonly NavKey[]

/** 管理员的工作台多一个全局概览，但不再放「申请开通企业」——那是企业管理板块的入口。 */
export const adminWorkspaceItems = [
  'overview',
  'models',
  'playground',
  'videoTasks',
  'usage',
  'apiKeys',
  'wallet',
  'subscriptions',
  'invitations',
  'profile',
] as const satisfies readonly NavKey[]

/** 企业板块里的企业自助条目，按当前成员的服务端权限裁剪。 */
export const organizationSectionItems = [
  'organizationOverview',
  'organizationOnboarding',
  'organizationMembers',
  'organizationDepartments',
  'organizationApiKeys',
  'organizationServiceAccounts',
  'organizationBudgets',
  'organizationContractPrices',
  'organizationCredit',
  'organizationApprovals',
  'organizationWallet',
  'organizationActivity',
  'organizationSettings',
] as const satisfies readonly NavKey[]

/*
 * 平台级的企业条目只对管理员开放：企业申请、认证、目录和用户归属必须和上面的企业自助项
 * 待在同一个板块里，否则管理员得跨板块找关键入口。
 */
export const enterpriseAdminItems = [
  'platformOrganizations',
  'organizationProvisioning',
  'organizationVerification',
  'users',
  'organizationPlans',
] as const satisfies readonly NavKey[]

/** 系统管理：运维、网关与平台配置，全部是管理员专属。 */
export const systemSectionItems = [
  'subscriptionManagement',
  'redemptionCodes',
  'refunds',
  'paymentSettings',
  'billingSettings',
  'modelSettings',
  'groupSettings',
  'channels',
  'credentials',
  'credentialProxies',
  'routes',
  'siteSettings',
  'authenticationSettings',
  'announcements',
  'emailSettings',
  'networkSettings',
  'keys',
  'debugTraces',
] as const satisfies readonly NavKey[]

/** 企业菜单按当前成员的服务端权限投影裁剪，避免把无权页面伪装成可用功能。 */
const organizationNavigationPermissions: Readonly<Partial<Record<NavKey, readonly string[]>>> = {
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
