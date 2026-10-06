import type { PlaygroundErrorKind } from './playground-types'

function asRecord(value: unknown): Record<string, unknown> | undefined {
  return typeof value === 'object' && value !== null
    ? value as Record<string, unknown>
    : undefined
}

function responseErrorCode(error: Record<string, unknown>) {
  const responseBody = error.responseBody
  if (typeof responseBody !== 'string' || responseBody.length > 64 * 1024) return undefined

  try {
    const parsed = asRecord(JSON.parse(responseBody))
    const details = asRecord(parsed?.error)
    return typeof details?.code === 'string' ? details.code : undefined
  } catch {
    return undefined
  }
}

function streamErrorCode(error: Record<string, unknown>) {
  if (typeof error.code === 'string') return error.code
  const nested = asRecord(error.error)
  return typeof nested?.code === 'string' ? nested.code : undefined
}

/** 把 SDK 错误收敛为不携带响应正文或密钥的稳定界面分类。 */
export function classifyPlaygroundError(error: unknown): PlaygroundErrorKind {
  const details = asRecord(error)
  const statusCode = typeof details?.statusCode === 'number' ? details.statusCode : undefined
  const code = details ? responseErrorCode(details) ?? streamErrorCode(details) : undefined

  if (statusCode === 401 || code === 'invalid_api_key' || code === 'authentication_error') {
    return 'invalid_key'
  }
  if (code === 'insufficient_quota') return 'insufficient_quota'
  if (statusCode === 429 || code === 'rate_limit_exceeded' || code === 'rate_limit_error') {
    return 'rate_limited'
  }
  if (statusCode === 404 || code === 'model_not_found') return 'model_unavailable'
  if (statusCode === 400 || statusCode === 413 || code === 'invalid_prompt') {
    return 'invalid_request'
  }
  if (statusCode === 502 || statusCode === 503 || statusCode === 504) {
    return 'upstream_unavailable'
  }
  if (code === 'server_error') return 'upstream_unavailable'
  return 'unknown'
}
