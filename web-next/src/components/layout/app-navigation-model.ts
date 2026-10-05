export type NavKey =
  | 'overview'
  | 'models'
  | 'playground'
  | 'videoTasks'
  | 'apiKeys'
  | 'announcements'
  | 'wallet'
  | 'subscriptions'
  | 'invitations'
  | 'accountVerification'
  | 'accountVerificationSettings'
  | 'profile'
  | 'users'
  | 'subscriptionManagement'
  | 'redemptionCodes'
  | 'refunds'
  | 'siteSettings'
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
  | 'credentials'
  | 'credentialProxies'
  | 'routes'
  | 'keys'
  | 'usage'
  | 'debugTraces'

/** 工作台向登录用户开放，系统管理仅向管理员开放。 */
export type NavigationSectionKey = 'workspace' | 'system'

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
  'accountVerification',
  'profile',
] as const satisfies readonly NavKey[]

/** 管理员工作台包含全局概览。 */
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
  'accountVerification',
  'profile',
] as const satisfies readonly NavKey[]

/** 系统管理：运维、网关与平台配置，全部是管理员专属。 */
export const systemSectionItems = [
  'users',
  'subscriptionManagement',
  'redemptionCodes',
  'refunds',
  'paymentSettings',
  'billingSettings',
  'modelSettings',
  'modelProviders',
  'groupSettings',
  'channels',
  'credentials',
  'credentialProxies',
  'routes',
  'siteSettings',
  'siteNavigation',
  'authenticationSettings',
  'announcements',
  'emailSettings',
  'networkSettings',
  'accountVerificationSettings',
  'keys',
  'debugTraces',
] as const satisfies readonly NavKey[]
