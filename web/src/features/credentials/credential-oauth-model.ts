import type {
  AdminCredential,
  AdminOAuthProvider,
  AdminOAuthProviderStatus,
} from '@/lib/api/generated/types.gen'

export const adminOAuthProviders = ['claude_code', 'codex', 'gemini', 'antigravity'] as const

export type OAuthCredentialBaseline = {
  revision: number
}

export type OAuthWizardPhase = 'waiting' | 'error' | 'expired'

export type PersistedOAuthWizardState = {
  channelId: number
  credentialId: number
  provider: AdminOAuthProvider
  phase: OAuthWizardPhase
}

const oauthWizardStoragePrefix = 'anyflows.credentials.oauth-wizard.v1'

/** 仅使用当前标签页保存向导阶段；授权 URL、state、code 和 token 永不写入存储。 */
export function readOAuthWizardState(channelId: number, credentialId: number): PersistedOAuthWizardState | undefined {
  const storage = getOAuthWizardStorage()
  if (!storage) return undefined
  const key = oauthWizardStorageKey(channelId, credentialId)
  try {
    const raw = storage.getItem(key)
    if (!raw) return undefined
    const parsed: unknown = JSON.parse(raw)
    if (!isPersistedOAuthWizardState(parsed, channelId, credentialId)) {
      storage.removeItem(key)
      return undefined
    }
    return parsed
  } catch {
    try { storage.removeItem(key) } catch { /* 存储策略拒绝访问时仅退化为内存状态。 */ }
    return undefined
  }
}

/** 写入可恢复的非敏感阶段，存储不可用时不影响当前页面授权。 */
export function writeOAuthWizardState(state: PersistedOAuthWizardState) {
  const storage = getOAuthWizardStorage()
  if (!storage) return
  try {
    storage.setItem(oauthWizardStorageKey(state.channelId, state.credentialId), JSON.stringify(state))
  } catch {
    // 隐私模式或存储配额不足时保留页面内存状态。
  }
}

/** 清除当前凭据的恢复状态，避免已删除或已完成授权再次显示旧阶段。 */
export function clearOAuthWizardState(channelId: number, credentialId: number) {
  const storage = getOAuthWizardStorage()
  if (!storage) return
  try { storage.removeItem(oauthWizardStorageKey(channelId, credentialId)) } catch { /* 存储不可用时无需补偿。 */ }
}

function oauthWizardStorageKey(channelId: number, credentialId: number) {
  return `${oauthWizardStoragePrefix}.${channelId}.${credentialId}`
}

function getOAuthWizardStorage(): Storage | undefined {
  try { return globalThis.sessionStorage } catch { return undefined }
}

function isPersistedOAuthWizardState(value: unknown, channelId: number, credentialId: number): value is PersistedOAuthWizardState {
  if (typeof value !== 'object' || value === null) return false
  const candidate = value as Record<string, unknown>
  return candidate.channelId === channelId
    && candidate.credentialId === credentialId
    && asAdminOAuthProvider(typeof candidate.provider === 'string' ? candidate.provider : '') !== undefined
    && (candidate.phase === 'waiting' || candidate.phase === 'error' || candidate.phase === 'expired')
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

/** 仅当目标凭据已退出待授权状态、绑定本次 provider 且版本推进时认定回调完成。 */
export function oauthCredentialConnectionUpdated(
  credential: AdminCredential,
  provider: AdminOAuthProvider,
  baseline: OAuthCredentialBaseline,
) {
  if (credential.kind !== 'oauth'
    || credential.oauth_token_pending
    || credential.oauth_provider !== provider) return false
  return credential.oauth_revision > baseline.revision
}

/** 将毫秒截止时间转换为稳定的向上取整剩余秒数。 */
export function oauthAuthorizationRemainingSeconds(expiresAt: number, now = Date.now()) {
  return Math.max(0, Math.ceil((expiresAt - now) / 1_000))
}
