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

export type UsageLogSummary = {
  requestCount: number
  tokenCount: number
  quota: number
  averageFirstTokenMs: number | null
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

export function filterFailedCallLogs(
  logs: readonly FailedCallLogRow[],
  modelFilter: string,
  protocolFilter: string,
) {
  const normalizedModel = modelFilter.trim().toLocaleLowerCase()
  return logs.filter((log) => (
    (!normalizedModel || log.model.toLocaleLowerCase().includes(normalizedModel))
    && (protocolFilter === 'all' || log.protocol === protocolFilter)
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
  }
}

export function filterUsageLogs(
  logs: readonly UsageLogRow[],
  modelFilter: string,
  protocolFilter: string,
  modeFilter: UsageLogModeFilter,
) {
  const normalizedModel = modelFilter.trim().toLocaleLowerCase()
  return logs.filter((log) => {
    const modelMatches = !normalizedModel || log.model?.toLocaleLowerCase().includes(normalizedModel)
    const protocolMatches = protocolFilter === 'all' || log.protocol === protocolFilter
    const modeMatches = modeFilter === 'all'
      || (modeFilter === 'legacy' && log.is_stream === null)
      || (modeFilter === 'stream' && log.is_stream === true)
      || (modeFilter === 'sync' && log.is_stream === false)
    return modelMatches && protocolMatches && modeMatches
  })
}
