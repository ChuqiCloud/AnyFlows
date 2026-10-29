import type {
  AdminFailedCallLog,
  AdminUsageLog,
  UsageLogProtocol,
  UserFailedCallLog,
  UserUsageLog,
} from '@/lib/api/generated/types.gen'

export type UsageLogRow = AdminUsageLog | UserUsageLog
export type FailedCallLogRow = AdminFailedCallLog | UserFailedCallLog
export type RequestLogRow = UsageLogRow | FailedCallLogRow

export const usageLogProtocols = [
  'open_ai_chat',
  'open_ai_responses',
  'anthropic',
  'gemini',
  'open_ai_embeddings',
  'open_ai_images',
  'open_ai_audio',
  'open_ai_speech',
  'jina_rerank',
  'cohere_rerank',
  'xai_video',
] satisfies UsageLogProtocol[]

export type UsageLogModeFilter = 'all' | 'stream' | 'sync' | 'legacy'

/** 时间范围按秒级时间戳圈定，缺省一侧表示不限。 */
export type UsageLogRange = { start?: number; end?: number }

/** 本页内的名称/ID 过滤；管理员日志额外支持用户、令牌、渠道、分组维度。 */
export type UsageLogFilters = {
  model: string
  protocol: string
  mode: UsageLogModeFilter
  range?: UsageLogRange
  username: string
  token: string
  channel: string
  group: string
  requestId: string
  upstreamRequestId: string
}

export type UsageLogSummary = {
  requestCount: number
  tokenCount: number
  quota: number
  averageFirstTokenMs: number | null
  /** 本页请求均摊到 60 秒窗口的每分钟请求数。 */
  rpm: number
  /** 本页 Token 均摊到 60 秒窗口的每分钟 Token 数。 */
  tpm: number
}

/** 管理员日志额外携带计费主体，普通用户响应不会暴露这些内部标识。 */
export function isAdminUsageLog(log: UsageLogRow): log is AdminUsageLog {
  return 'user_id' in log
}

/** 汇总缓存命中与缓存创建 Token，保持与后端计费明细字段一致。 */
export function usageLogCacheTokens(log: UsageLogRow) {
  return log.cache_read + log.cache_creation_5m + log.cache_creation_1h
}

export function usageLogTotalTokens(log: UsageLogRow) {
  return log.input_tokens + log.output_tokens
}

/** 管理员和企业明细返回内部字段，普通用户只返回安全错误码与文案。 */
export function isAdminFailedCallLog(log: FailedCallLogRow): log is AdminFailedCallLog {
  return 'error_kind' in log
}

export function isFailedCallLog(log: RequestLogRow): log is FailedCallLogRow {
  return 'error_code' in log
}

function inRange(log: RequestLogRow, range?: UsageLogRange) {
  return (range?.start === undefined || log.created_at >= range.start) && (range?.end === undefined || log.created_at <= range.end)
}

/** 名称类条件做不区分大小写的包含匹配，ID 类条件要求精确命中。 */
function matches(text: string | null | undefined, keyword: string) {
  return !keyword || (text ?? '').toLocaleLowerCase().includes(keyword.trim().toLocaleLowerCase())
}

function matchesId(value: number | null | undefined, keyword: string) {
  return !keyword.trim() || String(value ?? '') === keyword.trim()
}

/** 管理端按用户名、令牌、分组、渠道 ID 以及请求 ID 过滤；普通用户只能看到自己的记录，用户名维度不生效。 */
export function matchesUsageLogFilters(log: RequestLogRow, filters: UsageLogFilters) {
  const principal = isFailedCallLog(log) ? (isAdminFailedCallLog(log) ? log : undefined) : (isAdminUsageLog(log) ? log : undefined)
  const requestId = log.request_id ?? ''
  return matches(log.model, filters.model)
    && (filters.protocol === 'all' || log.protocol === filters.protocol)
    && matchesId(principal?.token_id, filters.token)
    && matchesId(principal?.group_id, filters.group)
    && matchesId(principal && 'channel_id' in principal ? principal.channel_id : undefined, filters.channel)
    && matches(principal?.username, filters.username)
    && matches(requestId, filters.requestId)
    && matches(requestId, filters.upstreamRequestId)
}

export function filterFailedCallLogs(
  logs: readonly FailedCallLogRow[],
  modelFilter: string,
  protocolFilter: string,
  range?: UsageLogRange,
) {
  const normalizedModel = modelFilter.trim().toLocaleLowerCase()
  return logs.filter((log) => (
    (!normalizedModel || log.model.toLocaleLowerCase().includes(normalizedModel))
    && (protocolFilter === 'all' || log.protocol === protocolFilter)
    && inRange(log, range)
  ))
}

export function summarizeUsageLogs(logs: readonly UsageLogRow[]): UsageLogSummary {
  let tokenCount = 0
  let quota = 0
  let firstTokenTotal = 0
  let firstTokenSamples = 0

  for (const log of logs) {
    tokenCount += usageLogTotalTokens(log)
    quota += log.quota
    if (log.first_token_ms !== null) {
      firstTokenTotal += log.first_token_ms
      firstTokenSamples += 1
    }
  }

  return {
    requestCount: logs.length,
    tokenCount,
    quota,
    averageFirstTokenMs: firstTokenSamples === 0 ? null : Math.round(firstTokenTotal / firstTokenSamples),
    rpm: Math.round(logs.length / 60 * 10) / 10,
    tpm: Math.round(tokenCount / 60),
  }
}

export function filterUsageLogs(logs: readonly UsageLogRow[], filters: UsageLogFilters) {
  return logs.filter((log) => {
    const modeMatches = filters.mode === 'all'
      || (filters.mode === 'legacy' && log.is_stream === null)
      || (filters.mode === 'stream' && log.is_stream === true)
      || (filters.mode === 'sync' && log.is_stream === false)
    const hasUpstreamId = 'upstream_request_id' in log
    const upstreamMatches = !filters.upstreamRequestId || (hasUpstreamId && log.upstream_request_id === filters.upstreamRequestId)
    return modeMatches && upstreamMatches && inRange(log, filters.range) && matchesUsageLogFilters(log, filters)
  })
}
