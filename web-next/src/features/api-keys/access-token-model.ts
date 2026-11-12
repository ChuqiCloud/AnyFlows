import { z } from 'zod'
import { ApiError } from '@/lib/api'
import type { AccessToken, AccessTokenWrite } from './access-token-api'

export const dateChoices = [7, 30, 90, 180, 365] as const
export const ipEntry = z.union([z.ipv4(), z.ipv6(), z.cidrv4(), z.cidrv6()])
export const accessTokenDraftKey = 'anyflows.access-token.pending-draft'

export function clearPendingDraft() {
  try { window.sessionStorage.removeItem(accessTokenDraftKey) } catch { /* The temporary draft contains no credential. */ }
}

export type AccessTokenDraft = {
  owner_id: number
  token_id?: number
  name: string
  scopes: string[]
  expiryChoice: number | 'custom' | 'never' | 'unchanged'
  customDate: string
  allowIpsText: string
  enabled: boolean
}

export function loadPendingDraft(): AccessTokenDraft | undefined {
  try {
    const raw = window.sessionStorage.getItem(accessTokenDraftKey)
    if (!raw) return undefined
    const value = JSON.parse(raw) as Partial<AccessTokenDraft>
    if (typeof value.owner_id !== 'number' || typeof value.name !== 'string' || !Array.isArray(value.scopes) || typeof value.allowIpsText !== 'string') return undefined
    return {
      owner_id: value.owner_id,
      token_id: typeof value.token_id === 'number' ? value.token_id : undefined,
      name: value.name,
      scopes: value.scopes.filter((scope): scope is string => typeof scope === 'string'),
      expiryChoice: value.expiryChoice === 'custom' || value.expiryChoice === 'never' || value.expiryChoice === 'unchanged' || typeof value.expiryChoice === 'number' ? value.expiryChoice : 30,
      customDate: typeof value.customDate === 'string' ? value.customDate : '',
      allowIpsText: value.allowIpsText,
      enabled: value.enabled !== false,
    }
  } catch {
    return undefined
  }
}

export function needsElevation(current: AccessToken, next: AccessTokenWrite) {
  if (next.scopes.some((scope) => !current.scopes.includes(scope))) return true
  if (!current.enabled && next.enabled) return true
  if (current.expires_at !== null && (next.expires_at === null || next.expires_at > current.expires_at)) return true
  if (current.allow_ips !== null && next.allow_ips === null) return true
  if (current.allow_ips !== null && next.allow_ips?.some((ip) => !current.allow_ips?.includes(ip))) return true
  return false
}

export function errorCode(error: unknown) {
  if (!(error instanceof ApiError) || typeof error.details !== 'object' || error.details === null) return undefined
  const details = error.details
  if ('code' in details && typeof details.code === 'string') return details.code
  if ('error' in details && typeof details.error === 'object' && details.error !== null && 'code' in details.error && typeof details.error.code === 'string') return details.error.code
  return undefined
}

export function accessTokenErrorKey(error: unknown) {
  switch (errorCode(error)) {
    case 'access_token_conflict': return 'accessTokens.errors.conflict'
    case 'insufficient_scope':
    case 'access_token_forbidden': return 'accessTokens.errors.scopeDenied'
    case 'access_token_limit_reached': return 'accessTokens.errors.capacity'
    case 'access_tokens_disabled': return 'accessTokens.errors.disabled'
    case 'access_token_issuance_disabled': return 'accessTokens.errors.issuanceDisabled'
    case 'access_token_rate_limited': return 'accessTokens.errors.rateLimited'
    default: return 'accessTokens.errors.save'
  }
}

export function expiryTimestamp(days: number | 'never' | 'custom', custom: string) {
  if (days === 'never') return null
  if (days === 'custom') {
    const parsed = new Date(custom)
    return Number.isFinite(parsed.getTime()) ? Math.floor(parsed.getTime() / 1000) : undefined
  }
  return Math.floor(Date.now() / 1000) + days * 24 * 60 * 60
}

export function dateInputValue(value: number | null | undefined) {
  if (value === null || value === undefined) return ''
  const date = new Date(value * 1000)
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')}T${String(date.getHours()).padStart(2, '0')}:${String(date.getMinutes()).padStart(2, '0')}`
}

export function timestamp(value: number | null, locale: string, empty: string) {
  if (value === null) return empty
  return new Intl.DateTimeFormat(locale, { dateStyle: 'medium', timeStyle: 'short' }).format(value * 1000)
}

