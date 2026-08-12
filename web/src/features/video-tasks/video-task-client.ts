import type {
  VideoTaskApiError,
  VideoTaskPollResponse,
  VideoTaskRequest,
  VideoTaskStatus,
} from './video-task-types'

const MAX_RESPONSE_BYTES = 1024 * 1024
const taskIdPattern = /^[0-9a-f]{32}$/

class VideoTaskRequestError extends Error {
  readonly detail: VideoTaskApiError

  constructor(detail: VideoTaskApiError) {
    super(detail.message)
    this.name = 'VideoTaskRequestError'
    this.detail = detail
  }
}

/** 提交任务时不做自动重试，结果未知由调用方复用原幂等键显式恢复。 */
export async function submitVideoTask(
  apiKey: string,
  idempotencyKey: string,
  request: VideoTaskRequest,
  signal?: AbortSignal,
) {
  const response = await fetch('/v1/videos/generations', {
    method: 'POST',
    headers: {
      Authorization: `Bearer ${apiKey}`,
      'Content-Type': 'application/json',
      'Idempotency-Key': idempotencyKey,
    },
    body: JSON.stringify(request),
    cache: 'no-store',
    signal,
  })
  const payload = await readJson(response)
  if (!response.ok) throw requestError(response.status, payload)
  const requestId = readString(payload, 'request_id')
  if (!taskIdPattern.test(requestId)) {
    throw requestError(503, undefined)
  }
  return requestId
}

/** 查询只读取 owner-scoped 本地任务，并允许服务端重取原任务的短期结果地址。 */
export async function pollVideoTask(
  apiKey: string,
  taskId: string,
  signal?: AbortSignal,
): Promise<VideoTaskPollResponse> {
  if (!taskIdPattern.test(taskId)) throw requestError(404, undefined)
  const response = await fetch(`/v1/videos/${encodeURIComponent(taskId)}`, {
    headers: { Authorization: `Bearer ${apiKey}` },
    cache: 'no-store',
    signal,
  })
  const payload = await readJson(response)
  if (!response.ok) throw requestError(response.status, payload)
  return parsePollResponse(payload)
}

export function videoTaskError(error: unknown): VideoTaskApiError {
  if (error instanceof VideoTaskRequestError) return error.detail
  if (error instanceof DOMException && error.name === 'AbortError') {
    return { code: 'cancelled', message: 'Request cancelled.', status: 0 }
  }
  return { code: 'network_error', message: 'Network request failed.', status: 0 }
}

async function readJson(response: Response): Promise<unknown> {
  const text = await response.text()
  if (new TextEncoder().encode(text).byteLength > MAX_RESPONSE_BYTES) {
    throw requestError(503, undefined)
  }
  try {
    return JSON.parse(text) as unknown
  } catch {
    throw requestError(response.ok ? 503 : response.status, undefined)
  }
}

function parsePollResponse(payload: unknown): VideoTaskPollResponse {
  if (!isRecord(payload)) throw requestError(503, undefined)
  const rawStatus = payload.status
  if (typeof rawStatus !== 'string' || !['pending', 'done', 'expired', 'failed'].includes(rawStatus)) {
    throw requestError(503, undefined)
  }
  const status = rawStatus as VideoTaskStatus
  if (status === 'done') {
    if (!isRecord(payload.video)) throw requestError(503, undefined)
    const url = readString(payload.video, 'url')
    const duration = payload.video.duration
    const respectModeration = payload.video.respect_moderation
    const model = readString(payload, 'model')
    if (
      !url.startsWith('https://')
      || !Number.isInteger(duration)
      || Number(duration) < 1
      || Number(duration) > 15
      || respectModeration !== true
    ) {
      throw requestError(503, undefined)
    }
    return {
      status,
      model,
      video: { url, duration: Number(duration), respect_moderation: true },
    }
  }
  if (status === 'failed') {
    if (!isRecord(payload.error)) throw requestError(503, undefined)
    return {
      status,
      error: {
        code: readString(payload.error, 'code'),
        message: readString(payload.error, 'message'),
      },
    }
  }
  return { status }
}

function requestError(status: number, payload: unknown) {
  const error = isRecord(payload) && isRecord(payload.error) ? payload.error : undefined
  const code = error && typeof error.code === 'string' ? error.code : 'upstream_unavailable'
  const message = error && typeof error.message === 'string'
    ? error.message
    : 'The service is temporarily unavailable. Please retry later.'
  return new VideoTaskRequestError({ code, message, status })
}

function readString(record: Record<string, unknown>, key: string) {
  const value = record[key]
  if (typeof value !== 'string' || value.length === 0) throw requestError(503, undefined)
  return value
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}
