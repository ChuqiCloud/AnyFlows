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
  | 'accountVerificationReviews'
  | 'invitationSettings'
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
  | 'clientProfiles'
  | 'credentials'
  | 'credentialProxies'
  | 'routes'
  | 'keys'
  | 'usage'
  | 'debugTraces'
  | 'runtimeStatus'

export type AdminSectionKey = 'operations' | 'gateway' | 'platform'

export const userNavigationGroups = [
  { key: 'workspace', items: ['models', 'playground', 'videoTasks', 'usage', 'apiKeys', 'wallet', 'subscriptions', 'invitations', 'accountVerification', 'profile'] },
] as const satisfies ReadonlyArray<{ key: 'workspace'; items: readonly NavKey[] }>

export const adminWorkspaceItems = ['overview', 'models', 'playground', 'videoTasks', 'usage', 'apiKeys', 'wallet', 'subscriptions', 'invitations', 'accountVerification', 'profile'] as const satisfies readonly NavKey[]

export const adminNavigationSections = [
  { key: 'operations', items: ['subscriptionManagement', 'redemptionCodes', 'refunds', 'paymentSettings', 'billingSettings'] },
  { key: 'gateway', items: ['modelSettings', 'modelProviders', 'groupSettings', 'channels', 'clientProfiles', 'credentials', 'credentialProxies', 'routes'] },
  { key: 'platform', items: ['users', 'accountVerificationReviews', 'invitationSettings', 'siteSettings', 'siteNavigation', 'authenticationSettings', 'announcements', 'emailSettings', 'networkSettings', 'accountVerificationSettings', 'keys'] },
] as const satisfies ReadonlyArray<{ key: AdminSectionKey; items: readonly NavKey[] }>

export const adminSystemItems = ['runtimeStatus', 'debugTraces'] as const satisfies readonly NavKey[]

export const adminNavigationItems: readonly NavKey[] = [
  ...adminWorkspaceItems,
  ...adminNavigationSections.flatMap((section) => section.items),
  ...adminSystemItems,
]

export const userNavigationItems: readonly NavKey[] = userNavigationGroups.flatMap((group) => group.items)

export const activeKeyByView: Readonly<Record<string, NavKey>> = {
  overview: 'overview',
  models: 'models',
  playground: 'playground',
  'video-tasks': 'videoTasks',
  'api-keys': 'apiKeys',
  'account-verification': 'accountVerification',
  'account-verification-settings': 'accountVerificationSettings',
  'account-verification-reviews': 'accountVerificationReviews',
  'invitation-settings': 'invitationSettings',
  wallet: 'wallet',
  subscriptions: 'subscriptions',
  invitations: 'invitations',
  profile: 'profile',
  users: 'users',
  'subscription-management': 'subscriptionManagement',
  'redemption-codes': 'redemptionCodes',
  refunds: 'refunds',
  'site-settings': 'siteSettings',
  'site-navigation': 'siteNavigation',
  announcements: 'announcements',
  'authentication-settings': 'authenticationSettings',
  'email-settings': 'emailSettings',
  'network-settings': 'networkSettings',
  'payment-settings': 'paymentSettings',
  'billing-settings': 'billingSettings',
  'group-settings': 'groupSettings',
  'model-settings': 'modelSettings',
  'model-providers': 'modelProviders',
  channels: 'channels',
  'client-profiles': 'clientProfiles',
  credentials: 'credentials',
  'credential-proxies': 'credentialProxies',
  routes: 'routes',
  tokens: 'keys',
  'usage-logs': 'usage',
  'debug-traces': 'debugTraces',
  'runtime-status': 'runtimeStatus',
}
