import type {
  AdminRoute,
  AdminRouteMode,
  AdminRouteStrategy,
  AdminRouteWriteRequest,
} from '@/lib/api/generated/types.gen'

export type RouteMappingRow = {
  id: string
  source: string
  target: string
}

export type RouteCandidateRow = {
  id: string
  channelId: number
  credentialId: number
  weight: string
  enabled: boolean
  stats?: RouteCandidateStats
}

export type RouteCandidateStats = {
  successCount: number
  failCount: number
  totalLatencyMs: number
  cooldownLevel: number
}

export type RouteFormValues = {
  name: string
  modelPattern: string
  mode: AdminRouteMode
  strategy: AdminRouteStrategy
  enabled: boolean
  modelMapping: RouteMappingRow[]
  candidates: RouteCandidateRow[]
}

export type RouteFormErrorCode =
  | 'invalidCandidates'
  | 'invalidMapping'
  | 'invalidName'
  | 'invalidPattern'
  | 'mappingTooLarge'
  | 'tooManyCandidates'

export type RouteFormErrors = Partial<Record<'candidates' | 'mapping' | 'modelPattern' | 'name', RouteFormErrorCode>>

let localRowSequence = 0

/** 创建仅用于当前表单生命周期的稳定行标识。 */
export function createRouteRowId(prefix: 'candidate' | 'mapping') {
  localRowSequence += 1
  return `${prefix}-${Date.now()}-${localRowSequence}`
}

export function defaultRouteFormValues(route?: AdminRoute): RouteFormValues {
  const candidates = route?.channels
    .slice()
    .sort((left, right) => right.priority - left.priority || left.id - right.id)
    .map((candidate) => ({
      id: `candidate-${candidate.id}`,
      channelId: candidate.channel_id,
      credentialId: candidate.credential_id,
      weight: String(candidate.weight),
      enabled: candidate.enabled,
      stats: {
        successCount: candidate.success_count,
        failCount: candidate.fail_count,
        totalLatencyMs: candidate.total_latency_ms,
        cooldownLevel: candidate.cooldown_level,
      },
    })) ?? []

  return {
    name: route?.name ?? '',
    modelPattern: route?.model_pattern ?? '',
    mode: route?.mode ?? 'pattern',
    strategy: route?.strategy ?? 'weighted',
    enabled: route?.enabled ?? true,
    modelMapping: Object.entries(route?.model_mapping ?? {}).map(([source, target], index) => ({
      id: `mapping-${index}`,
      source,
      target: mappingValueText(target),
    })),
    candidates,
  }
}

/** 前端复现公开写入边界，服务端仍是最终校验来源。 */
export function validateRouteForm(values: RouteFormValues): RouteFormErrors {
  const errors: RouteFormErrors = {}
  if (!validText(values.name, 128)) errors.name = 'invalidName'
  if (!validPattern(values.modelPattern, values.mode)) errors.modelPattern = 'invalidPattern'

  const mapping = Object.create(null) as Record<string, string>
  for (const row of values.modelMapping) {
    if (
      !validPattern(row.source, 'pattern')
      || !validText(row.target, 256)
      || row.target.trim() !== row.target
      || row.source in mapping
    ) {
      errors.mapping = 'invalidMapping'
      break
    }
    mapping[row.source] = row.target
  }
  if (!errors.mapping && encodedBytes(mapping) > 16 * 1024) errors.mapping = 'mappingTooLarge'

  const seenCandidates = new Set<string>()
  if (values.candidates.length > 64) errors.candidates = 'tooManyCandidates'
  for (const candidate of values.candidates) {
    const key = `${candidate.channelId}:${candidate.credentialId}`
    const weight = parseNonNegativeInt32(candidate.weight)
    if (!positiveSafeInteger(candidate.channelId)
      || !positiveSafeInteger(candidate.credentialId)
      || weight === undefined
      || seenCandidates.has(key)) {
      errors.candidates = 'invalidCandidates'
      break
    }
    seenCandidates.add(key)
  }
  return errors
}

/** 按可视顺序生成优先级，列表顶部始终拥有最高优先级。 */
export function toRouteWriteRequest(values: RouteFormValues): AdminRouteWriteRequest {
  const errors = validateRouteForm(values)
  if (Object.keys(errors).length > 0) throw new Error('智能路由表单未通过校验')
  return {
    name: values.name,
    model_pattern: values.modelPattern,
    mode: values.mode,
    strategy: values.strategy,
    model_mapping: Object.fromEntries(values.modelMapping.map((row) => [row.source, row.target])),
    enabled: values.enabled,
    channels: values.candidates.map((candidate, index) => ({
      channel_id: candidate.channelId,
      credential_id: candidate.credentialId,
      priority: values.candidates.length - index,
      weight: parseNonNegativeInt32(candidate.weight) ?? 0,
      enabled: candidate.enabled,
    })),
  }
}

/** 快捷启停使用完整写入模型，避免遗漏候选运行配置。 */
export function routeToWriteRequest(route: AdminRoute, enabled = route.enabled): AdminRouteWriteRequest {
  return {
    name: route.name,
    model_pattern: route.model_pattern,
    mode: route.mode,
    strategy: route.strategy,
    model_mapping: route.model_mapping,
    enabled,
    channels: route.channels.map((candidate) => ({
      channel_id: candidate.channel_id,
      credential_id: candidate.credential_id,
      priority: candidate.priority,
      weight: candidate.weight,
      enabled: candidate.enabled,
    })),
  }
}

function validText(value: string, maximumBytes: number) {
  return value.length > 0
    && !Array.from(value).some((character) => {
      const code = character.charCodeAt(0)
      return code <= 0x1f || code === 0x7f
    })
    && encodedBytes(value) <= maximumBytes
}

function validPattern(value: string, mode: AdminRouteMode) {
  if (!validText(value, 255) || value.trim() !== value) return false
  if (mode === 'explicit_group') return !value.startsWith('re:')
  if (!value.startsWith('re:')) return true
  if (value.length <= 3) return false
  try {
    RegExp(value.slice(3))
    return true
  } catch {
    return false
  }
}

function parseNonNegativeInt32(value: string) {
  if (!/^(0|[1-9][0-9]*)$/.test(value)) return undefined
  const parsed = Number(value)
  return Number.isSafeInteger(parsed) && parsed <= 2_147_483_647 ? parsed : undefined
}

function positiveSafeInteger(value: number) {
  return Number.isSafeInteger(value) && value > 0
}

function encodedBytes(value: unknown) {
  const text = typeof value === 'string' ? value : JSON.stringify(value) ?? ''
  return new TextEncoder().encode(text).length
}

function mappingValueText(value: unknown) {
  if (typeof value === 'string') return value
  if (value === null || value === undefined) return ''
  if (typeof value === 'number' || typeof value === 'boolean') return String(value)
  return JSON.stringify(value)
}
