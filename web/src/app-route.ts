export type AppView =
  | 'api-keys'
  | 'authentication-settings'
  | 'channels'
  | 'email-settings'
  | 'forgot-password'
  | 'models'
  | 'playground'
  | 'profile'
  | 'public-models'
  | 'register'
  | 'reset-password'
  | 'site-settings'
  | 'share'
  | 'tokens'
  | 'usage-logs'
  | 'users'
  | 'overview'
  | 'login'
  | 'setup'
  | 'landing'

export type AppRoute = {
  view: AppView
  shareToken?: string
  resetToken?: string
}

const shareRoutePrefix = '#/share/'
const shareTokenPattern = /^sh-af-[A-Za-z0-9_-]{43}$/
const resetRoutePrefix = '#/reset-password'
const resetTokenPattern = /^[A-Za-z0-9_-]{22}\.[A-Za-z0-9_-]{43}$/

/** 解析临时 hash 路由，并严格限制公开分享令牌的可接受格式。 */
export function routeFromHash(hash: string): AppRoute {
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
  if (hash.startsWith('#/console/channels')) return { view: 'channels' }
  if (hash.startsWith('#/console/system-settings/site')) return { view: 'site-settings' }
  if (hash.startsWith('#/console/system-settings/authentication')) return { view: 'authentication-settings' }
  if (hash.startsWith('#/console/system-settings/email')) return { view: 'email-settings' }
  if (hash.startsWith('#/console/api-keys')) return { view: 'api-keys' }
  if (hash.startsWith('#/console/models')) return { view: 'models' }
  if (hash.startsWith('#/console/playground')) return { view: 'playground' }
  if (hash.startsWith('#/console/profile')) return { view: 'profile' }
  if (hash.startsWith('#/console/tokens')) return { view: 'tokens' }
  if (hash.startsWith('#/console/usage-logs')) return { view: 'usage-logs' }
  if (hash.startsWith('#/console/users')) return { view: 'users' }
  if (hash.startsWith('#/console/registration')) return { view: 'authentication-settings' }
  if (hash.startsWith('#/console')) return { view: 'overview' }
  if (hash.startsWith('#/models')) return { view: 'public-models' }
  if (hash.startsWith('#/register')) return { view: 'register' }
  if (hash.startsWith('#/forgot-password')) return { view: 'forgot-password' }
  if (hash.startsWith('#/login')) return { view: 'login' }
  if (hash.startsWith('#/setup')) return { view: 'setup' }
  return { view: 'landing' }
}
