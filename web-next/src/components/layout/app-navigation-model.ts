export type NavKey =
  | 'overview'
  | 'models'
  | 'playground'
  | 'videoTasks'
  | 'imagePlaza'
  | 'apiKeys'
  | 'announcements'
  | 'wallet'
  | 'subscriptions'
  | 'invitations'
  | 'accountVerification'
  | 'accountVerificationSettings'
  | 'accountVerificationReviews'
  | 'invitationSettings'
  | 'profile'
  | 'users'
  | 'subscriptionManagement'
  | 'redemptionCodes'
  | 'refunds'
  | 'siteSettings'
  | 'about'
  | 'siteNavigation'
  | 'authenticationSettings'
  | 'emailSettings'
  | 'networkSettings'
  | 'paymentSettings'
  | 'billingSettings'
  | 'groupSettings'
  | 'modelSettings'
  | 'modelProviders'
  | 'channels'
  | 'clientProfiles'
  | 'credentials'
  | 'credentialProxies'
  | 'routes'
  | 'keys'
  | 'usage'
  | 'debugTraces'
  | 'runtimeStatus'

/** 工作台向登录用户开放，系统管理仅向管理员开放。 */
export type NavigationSectionKey = 'workspace' | 'system'

/** 工作台：模型、试验场、视频任务与账户自服务，两类角色都能看到。 */
export const userWorkspaceItems = [
  'overview',
  'playground',
  'videoTasks',
  'imagePlaza',
  'usage',
  'apiKeys',
  'wallet',
  'invitations',
  'accountVerification',
  'profile',
] as const satisfies readonly NavKey[]

/** 管理员工作台包含全局概览。 */
export const adminWorkspaceItems = [
  'overview',
  'playground',
  'videoTasks',
  'imagePlaza',
  'usage',
  'apiKeys',
  'wallet',
  'invitations',
  'accountVerification',
  'profile',
] as const satisfies readonly NavKey[]

/** 系统管理：运维、网关与平台配置，全部是管理员专属。 */
export const systemSectionItems = [
  'users',
  'accountVerificationReviews',
  'invitationSettings',
  'subscriptionManagement',
  'redemptionCodes',
  'refunds',
  'paymentSettings',
  'billingSettings',
  'modelSettings',
  'modelProviders',
  'groupSettings',
  'channels',
  'clientProfiles',
  'credentials',
  'credentialProxies',
  'routes',
  'siteSettings',
  'about',
  'siteNavigation',
  'authenticationSettings',
  'announcements',
  'emailSettings',
  'networkSettings',
  'accountVerificationSettings',
  'keys',
  'runtimeStatus',
  'debugTraces',
] as const satisfies readonly NavKey[]

/** 系统入口按用途折叠，路由命中时自动展开所在分组；顺序即侧栏展示顺序：业务 → 渠道 → 费用 → 站点 → 开发。 */
export type SystemNavigationGroupKey = 'users' | 'gateway' | 'commerce' | 'site'

export const systemPinnedItems = ['keys', 'runtimeStatus', 'debugTraces'] as const satisfies readonly NavKey[]

export const systemNavigationGroups = {
  users: ['users', 'accountVerificationReviews', 'invitationSettings'],
  gateway: ['channels', 'routes', 'modelSettings', 'modelProviders', 'groupSettings', 'clientProfiles', 'credentials', 'credentialProxies'],
  commerce: ['subscriptionManagement', 'redemptionCodes', 'refunds', 'paymentSettings', 'billingSettings'],
  site: ['siteSettings', 'about', 'siteNavigation', 'announcements', 'emailSettings', 'authenticationSettings', 'accountVerificationSettings', 'networkSettings'],
} as const satisfies Record<SystemNavigationGroupKey, readonly NavKey[]>
