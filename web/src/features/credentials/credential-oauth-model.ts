import type {
  AdminCredential,
  AdminOAuthProvider,
  AdminOAuthProviderStatus,
} from '@/lib/api/generated/types.gen'

export const adminOAuthProviders = ['claude_code', 'codex', 'gemini', 'antigravity'] as const

export type OAuthCredentialBaseline = {
  revision: number
}

/** 将数据库中的开放 provider 字符串收敛为管理 API 支持的闭合集合。 */
export function asAdminOAuthProvider(value: string | null): AdminOAuthProvider | undefined {
  return adminOAuthProviders.find((provider) => provider === value)
}

/** 返回凭据当前绑定 provider 的启动配置状态。 */
export function oauthProviderStatus(
  providers: readonly AdminOAuthProviderStatus[],
  credential: AdminCredential,
) {
  const provider = asAdminOAuthProvider(credential.oauth_provider)
  return provider === undefined
    ? undefined
    : providers.find((status) => status.provider === provider)
}

/** 只接受与本次授权固定 redirect URI 同源、同路径且带查询参数的完整回调地址。 */
export function isValidOAuthCallbackUrl(value: string, redirectUri: string) {
  if (value.length === 0 || value.length > 16 * 1024 || value.trim() !== value) return false
  try {
    const callback = new URL(value)
    const redirect = new URL(redirectUri)
    const states = callback.searchParams.getAll('state')
    const codes = callback.searchParams.getAll('code')
    const errors = callback.searchParams.getAll('error')
    const hasCode = codes.length === 1 && codes[0].length > 0
    const hasError = errors.length === 1 && errors[0].length > 0
    return callback.protocol === redirect.protocol
      && callback.origin === redirect.origin
      && callback.pathname === redirect.pathname
      && callback.username === ''
      && callback.password === ''
      && states.length === 1
      && states[0].length > 0
      && hasCode !== hasError
      && callback.hash === ''
  } catch {
    return false
  }
}

/** 仅当目标凭据绑定了本次 provider 且 OAuth 专用版本推进时认定自动回调完成。 */
export function oauthCredentialConnectionUpdated(
  credential: AdminCredential,
  provider: AdminOAuthProvider,
  baseline: OAuthCredentialBaseline,
) {
  if (credential.kind !== 'oauth' || credential.oauth_provider !== provider) return false
  return credential.oauth_revision > baseline.revision
}

/** 将毫秒截止时间转换为稳定的向上取整剩余秒数。 */
export function oauthAuthorizationRemainingSeconds(expiresAt: number, now = Date.now()) {
  return Math.max(0, Math.ceil((expiresAt - now) / 1_000))
}
