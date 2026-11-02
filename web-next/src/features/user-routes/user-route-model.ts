import type { SelectableTokenGroup } from '../api-keys/api-key-group-model.ts'

export const MAX_USER_ROUTES = 32
export const MAX_USER_ROUTE_MEMBERS = 16
export const userRouteStrategies = ['price_first', 'stable_first', 'ordered', 'round_robin', 'weighted'] as const
export type UserRouteStrategy = typeof userRouteStrategies[number]

export type UserRouteMember = { group_id: number; priority: number; weight: number }
export type UserRoute = {
  id: number
  name: string
  strategy: UserRouteStrategy
  enabled: boolean
  max_group_attempts: number
  max_price_ratio_micros: number | null
  members: UserRouteMember[]
}
export type UserRouteWriteRequest = Omit<UserRoute, 'id'>
export type UserRouteList = { items: UserRoute[]; capacity: number }

export type UserRouteMemberDraft = { rowId: string; groupId: string; priority: string; weight: string }
export type UserRouteDraft = {
  name: string
  strategy: UserRouteStrategy
  enabled: boolean
  maxGroupAttempts: string
  maxPriceRatio: string
  members: UserRouteMemberDraft[]
}

export function defaultUserRouteDraft(route?: UserRoute): UserRouteDraft {
  return {
    name: route?.name ?? '',
    strategy: route?.strategy ?? 'price_first',
    enabled: route?.enabled ?? true,
    maxGroupAttempts: String(route?.max_group_attempts ?? 4),
    maxPriceRatio: route?.max_price_ratio_micros === null || route?.max_price_ratio_micros === undefined
      ? '' : formatUserRoutePriceRatio(route.max_price_ratio_micros),
    members: route?.members.map((member) => ({
      rowId: crypto.randomUUID(), groupId: String(member.group_id),
      priority: String(member.priority), weight: String(member.weight),
    })) ?? [],
  }
}

/** Only build a request from a complete, current authorization directory. */
export function validateUserRouteDraft(draft: UserRouteDraft, groups?: SelectableTokenGroup[]) {
  const name = draft.name.trim()
  if (!name || new TextEncoder().encode(name).length > 128 || /\p{Cc}/u.test(name)) return 'name'
  if (!userRouteStrategies.includes(draft.strategy)) return 'strategy'
  if (!groups) return 'groups'
  if (draft.members.length === 0 || draft.members.length > MAX_USER_ROUTE_MEMBERS) return 'members'
  const ids = draft.members.map((member) => Number(member.groupId))
  if (draft.members.some((member) => !/^[1-9]\d*$/.test(member.groupId))) return 'groups'
  if (new Set(ids).size !== ids.length) return 'duplicate'
  if (ids.some((id) => !Number.isSafeInteger(id) || id <= 0 || !groups.some((group) => group.id === id))) return 'groups'
  if (draft.members.some((member) => !isIntegerText(member.priority, 0, 2_147_483_647)
    || !isIntegerText(member.weight, 1, 10_000))) return 'memberValues'
  if (!isIntegerText(draft.maxGroupAttempts, 1, MAX_USER_ROUTE_MEMBERS)) return 'attempts'
  if (draft.maxPriceRatio !== '') {
    if (parseUserRoutePriceRatio(draft.maxPriceRatio) === undefined) return 'price'
  }
  return undefined
}

export function toUserRouteRequest(draft: UserRouteDraft, groups?: SelectableTokenGroup[]): UserRouteWriteRequest {
  const error = validateUserRouteDraft(draft, groups)
  if (error) throw new Error(error)
  const maxPriceRatio = draft.maxPriceRatio === '' ? null : parseUserRoutePriceRatio(draft.maxPriceRatio)
  if (maxPriceRatio === undefined) throw new Error('price')
  return {
    name: draft.name.trim(), strategy: draft.strategy, enabled: draft.enabled,
    max_group_attempts: Number(draft.maxGroupAttempts),
    max_price_ratio_micros: maxPriceRatio,
    members: draft.members.map((member) => ({
      group_id: Number(member.groupId), priority: Number(member.priority), weight: Number(member.weight),
    })),
  }
}

export function isSelectableUserRoute(value: string, routes?: UserRoute[]) {
  if (!/^[1-9]\d*$/.test(value)) return false
  const id = Number(value)
  return Number.isSafeInteger(id) && id > 0 && !!routes?.some((route) => route.id === id && route.enabled)
}

export function parseUserRoutePriceRatio(value: string): number | undefined {
  if (!/^(?:0|[1-9]\d*)(?:\.\d{1,6})?$/.test(value)) return undefined
  const [whole, fraction = ''] = value.split('.')
  const micros = Number(whole) * 1_000_000 + Number(fraction.padEnd(6, '0'))
  return Number.isSafeInteger(micros) && micros >= 0 ? micros : undefined
}

export function formatUserRoutePriceRatio(micros: number): string {
  const whole = Math.floor(micros / 1_000_000)
  const fraction = String(micros % 1_000_000).padStart(6, '0').replace(/0+$/, '')
  return fraction ? `${whole}.${fraction}` : String(whole)
}

/** Optional during rolling upgrades; older token responses have no route binding. */
export function tokenUserRouteId(token: object): number | undefined {
  const id = (token as { user_route_id?: number | null }).user_route_id
  return typeof id === 'number' && Number.isSafeInteger(id) && id > 0 ? id : undefined
}

function isIntegerText(value: string, min: number, max: number) {
  return /^\d+$/.test(value) && Number.isSafeInteger(Number(value)) && Number(value) >= min && Number(value) <= max
}
