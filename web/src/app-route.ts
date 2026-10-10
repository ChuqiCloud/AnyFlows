export type AppView =
  | 'extension'
  | 'account-verification'
  | 'account-verification-settings'
  | 'api'
  | 'api-keys'
  | 'announcements'
  | 'authentication-settings'
  | 'billing-settings'
  | 'channels'
  | 'credentials'
  | 'credential-proxies'
  | 'debug-traces'
  | 'email-settings'
  | 'forgot-password'
  | 'group-settings'
  | 'invitations'
  | 'models'
  | 'model-settings'
  | 'model-providers'
  | 'network-settings'
  | 'oauth-callback'
  | 'payment-settings'
  | 'playground'
  | 'profile'
  | 'public-models'
  | 'register'
  | 'redemption-codes'
  | 'refunds'
  | 'routes'
  | 'reset-password'
  | 'site-settings'
  | 'site-navigation'
  | 'share'
  | 'subscription-management'
  | 'subscriptions'
  | 'tokens'
  | 'usage-logs'
  | 'users'
  | 'video-tasks'
  | 'wallet'
  | 'overview'
  | 'login'
  | 'setup'
  | 'landing'

export type AppRoute = {
  view: AppView
  extensionPath?: string
  channelId?: number
  shareToken?: string
  resetToken?: string
  inviteCode?: string
  profileSection?: 'notifications'
}

const shareRoutePrefix = '#/share/'
const shareTokenPattern = /^sh-af-[A-Za-z0-9_-]{43}$/
const resetRoutePrefix = '#/reset-password'
const resetTokenPattern = /^[A-Za-z0-9_-]{22}\.[A-Za-z0-9_-]{43}$/
const invitationCodePattern = /^af-[A-Za-z0-9_-]{22}$/

/** 只接受单个十进制正整数查询参数，避免宽松转换产生错误渠道深链。 */
function positiveIntegerQuery(hash: string, key: string) {
  const queryStart = hash.indexOf('?')
  if (queryStart < 0) return undefined
  const values = new URLSearchParams(hash.slice(queryStart + 1)).getAll(key)
  if (values.length !== 1 || !/^[1-9]\d*$/.test(values[0])) return undefined
  const value = Number(values[0])
  return Number.isSafeInteger(value) ? value : undefined
}

/** 只接受通知偏好深链，避免把任意查询值带入个人资料页面状态。 */
function profileSectionQuery(hash: string): AppRoute['profileSection'] {
  const queryStart = hash.indexOf('?')
  if (queryStart < 0) return undefined
  const values = new URLSearchParams(hash.slice(queryStart + 1)).getAll('section')
  return values.length === 1 && values[0] === 'notifications' ? 'notifications' : undefined
}

/** 解析临时 hash 路由，并严格限制公开分享令牌的可接受格式。 */
export function routeFromHash(hash: string): AppRoute {
  const extensionPath = hash.slice(1).split('?')[0]
  if (/^\/console\/extensions\/[a-z0-9-]+(?:\/[a-z0-9-]+)*$/.test(extensionPath)) return { view: 'extension', extensionPath }
  if (hash.startsWith(shareRoutePrefix)) {
    const token = hash.slice(shareRoutePrefix.length)
    return {
      view: 'share',
      shareToken: shareTokenPattern.test(token) ? token : undefined,
    }
  }
  if (hash.startsWith(resetRoutePrefix)) {
    const queryStart = hash.indexOf('?')
    const query = queryStart >= 0 ? new URLSearchParams(hash.slice(queryStart + 1)) : undefined
    const token = query?.get('token') ?? undefined
    return {
      view: 'reset-password',
      resetToken: token && resetTokenPattern.test(token) ? token : undefined,
    }
  }
  if (hash === '#/console/credentials' || hash.startsWith('#/console/credentials?')) {
    return { view: 'credentials', channelId: positiveIntegerQuery(hash, 'channel') }
  }
  if (hash.startsWith('#/console/channels')) return { view: 'channels' }
  if (hash.startsWith('#/console/proxies')) return { view: 'credential-proxies' }
  if (hash.startsWith('#/console/debug-traces')) return { view: 'debug-traces' }
  if (hash.startsWith('#/console/redemption-codes')) return { view: 'redemption-codes' }
  if (hash.startsWith('#/console/refunds')) return { view: 'refunds' }
  if (hash.startsWith('#/console/subscription-management')) return { view: 'subscription-management' }
  if (hash.startsWith('#/console/subscriptions')) return { view: 'subscriptions' }
  if (hash.startsWith('#/console/routes')) return { view: 'routes' }
  if (hash.startsWith('#/console/system-settings/navigation')) return { view: 'site-navigation' }
  if (hash.startsWith('#/console/system-settings/site')) return { view: 'site-settings' }
  if (hash.startsWith('#/console/system-settings/announcements')) return { view: 'announcements' }
  if (hash.startsWith('#/console/system-settings/authentication')) return { view: 'authentication-settings' }
  if (hash.startsWith('#/console/system-settings/email')) return { view: 'email-settings' }
  if (hash.startsWith('#/console/system-settings/network')) return { view: 'network-settings' }
  if (hash.startsWith('#/console/system-settings/payment')) return { view: 'payment-settings' }
  if (hash.startsWith('#/console/system-settings/billing')) return { view: 'billing-settings' }
  if (hash.startsWith('#/console/system-settings/groups')) return { view: 'group-settings' }
  if (hash.startsWith('#/console/system-settings/models')) return { view: 'model-settings' }
  if (hash.startsWith('#/console/system-settings/model-providers')) return { view: 'model-providers' }
  if (hash.startsWith('#/console/system-settings/account-verification')) return { view: 'account-verification-settings' }
  if (hash.startsWith('#/console/api-keys')) return { view: 'api-keys' }
  if (hash.startsWith('#/console/wallet')) return { view: 'wallet' }
  if (hash.startsWith('#/console/models')) return { view: 'models' }
  if (hash.startsWith('#/console/playground')) return { view: 'playground' }
  if (hash.startsWith('#/console/video-tasks')) return { view: 'video-tasks' }
  if (hash.startsWith('#/console/invitations')) return { view: 'invitations' }
  if (hash.startsWith('#/console/profile')) {
    const section = profileSectionQuery(hash)
    return section ? { view: 'profile', profileSection: section } : { view: 'profile' }
  }
  if (hash.startsWith('#/console/tokens')) return { view: 'tokens' }
  if (hash.startsWith('#/console/usage-logs')) return { view: 'usage-logs' }
  if (hash.startsWith('#/console/users')) return { view: 'users' }
  if (hash.startsWith('#/console/registration')) return { view: 'authentication-settings' }
  if (hash.startsWith('#/console/account-verification')) return { view: 'account-verification' }
  if (hash.startsWith('#/console')) return { view: 'overview' }
  if (hash.startsWith('#/models')) return { view: 'public-models' }
  if (hash === '#/api' || hash.startsWith('#/api?')) return { view: 'api' }
  if (hash.startsWith('#/register')) {
    const queryStart = hash.indexOf('?')
    const query = queryStart >= 0 ? new URLSearchParams(hash.slice(queryStart + 1)) : undefined
    const inviteCode = query?.get('invite') ?? undefined
    return inviteCode && invitationCodePattern.test(inviteCode)
      ? { view: 'register', inviteCode }
      : { view: 'register' }
  }
  if (hash.startsWith('#/forgot-password')) return { view: 'forgot-password' }
  if (hash === '#/oauth/callback' || hash.startsWith('#/oauth/callback?')) {
    return { view: 'oauth-callback' }
  }
  if (hash.startsWith('#/login')) return { view: 'login' }
  if (hash.startsWith('#/setup')) return { view: 'setup' }
  return { view: 'landing' }
}
