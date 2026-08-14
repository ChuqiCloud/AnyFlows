export type NavKey =
  | 'overview'
  | 'models'
  | 'playground'
  | 'videoTasks'
  | 'apiKeys'
  | 'wallet'
  | 'subscriptions'
  | 'invitations'
  | 'profile'
  | 'users'
  | 'subscriptionManagement'
  | 'redemptionCodes'
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

export type AdminSectionKey = 'personal' | 'operations' | 'gateway' | 'platform' | 'observability'

export const userNavigationGroups = [
  { key: 'use', items: ['models', 'playground', 'videoTasks'] },
  { key: 'account', items: ['apiKeys', 'wallet', 'usage', 'subscriptions', 'invitations', 'profile'] },
] as const satisfies ReadonlyArray<{ key: 'use' | 'account'; items: readonly NavKey[] }>

export const adminWorkspaceItems = ['overview', 'models', 'playground', 'videoTasks'] as const satisfies readonly NavKey[]

export const adminNavigationSections = [
  { key: 'personal', items: ['apiKeys', 'wallet', 'subscriptions', 'invitations', 'profile'] },
  { key: 'operations', items: ['users', 'subscriptionManagement', 'redemptionCodes', 'paymentSettings', 'billingSettings'] },
  { key: 'gateway', items: ['modelSettings', 'groupSettings', 'channels', 'credentials', 'credentialProxies', 'routes'] },
  { key: 'platform', items: ['siteSettings', 'authenticationSettings', 'emailSettings', 'networkSettings', 'keys'] },
  { key: 'observability', items: ['usage', 'debugTraces'] },
] as const satisfies ReadonlyArray<{ key: AdminSectionKey; items: readonly NavKey[] }>

export const adminNavigationItems: readonly NavKey[] = [
  ...adminWorkspaceItems,
  ...adminNavigationSections.flatMap((section) => section.items),
]

export const userNavigationItems: readonly NavKey[] = userNavigationGroups.flatMap((group) => group.items)

export const activeKeyByView: Readonly<Record<string, NavKey>> = {
  overview: 'overview',
  models: 'models',
  playground: 'playground',
  'video-tasks': 'videoTasks',
  'api-keys': 'apiKeys',
  wallet: 'wallet',
  subscriptions: 'subscriptions',
  invitations: 'invitations',
  profile: 'profile',
  users: 'users',
  'subscription-management': 'subscriptionManagement',
  'redemption-codes': 'redemptionCodes',
  'site-settings': 'siteSettings',
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
