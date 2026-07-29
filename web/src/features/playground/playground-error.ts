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

/** 把 SDK 错误收敛为不携带响应正文或密钥的稳定界面分类。 */
export function classifyPlaygroundError(error: unknown): PlaygroundErrorKind {
  const details = asRecord(error)
  const statusCode = typeof details?.statusCode === 'number' ? details.statusCode : undefined
  const code = details ? responseErrorCode(details) : undefined

  if (statusCode === 401) return 'invalid_key'
  if (statusCode === 429 && code === 'insufficient_quota') return 'insufficient_quota'
  if (statusCode === 429) return 'rate_limited'
  if (statusCode === 404) return 'model_unavailable'
  if (statusCode === 400 || statusCode === 413) return 'invalid_request'
  if (statusCode === 502 || statusCode === 503 || statusCode === 504) {
    return 'upstream_unavailable'
  }
  return 'unknown'
}
