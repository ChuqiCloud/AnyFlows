import type {
  VideoFailureCode,
  VideoTaskApiError,
  VideoTaskPollResponse,
  VideoTaskRequest,
  VideoTaskListItem,
  VideoTaskListPage,
  VideoTaskStatus,
} from './video-task-types'
import { apiUrl } from '@/lib/api/endpoint'
import { getManagementSessionToken, invalidateManagementSession } from '@/lib/api/session-token'

const MAX_RESPONSE_BYTES = 1024 * 1024
const taskIdPattern = /^[0-9a-f]{32}$/
const historyCursorPattern = taskIdPattern

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
  idempotencyKey: string,
  request: VideoTaskRequest,
  signal?: AbortSignal,
) {
  const authorization = sessionAuthorization()
  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
    'Idempotency-Key': idempotencyKey,
  }
  if (authorization) headers.Authorization = authorization
  const response = await fetch(apiUrl('/api/playground/v1/videos/generations'), {
    method: 'POST',
    headers,
    body: JSON.stringify(request),
    cache: 'no-store',
    signal,
  })
  const payload = await readJson(response)
  if (!response.ok) throw requestError(response.status, payload)
  if (!isRecord(payload)) throw requestError(503, undefined)
  const requestId = readString(payload, 'request_id')
  if (!taskIdPattern.test(requestId)) {
    throw requestError(503, undefined)
  }
  return requestId
}

/** 查询只读取 owner-scoped 本地任务，并允许服务端重取原任务的短期结果地址。 */
export async function pollVideoTask(
  taskId: string,
  signal?: AbortSignal,
): Promise<VideoTaskPollResponse> {
  if (!taskIdPattern.test(taskId)) throw requestError(404, undefined)
  const authorization = sessionAuthorization()
  const response = await fetch(apiUrl(`/api/playground/v1/videos/${encodeURIComponent(taskId)}`), {
    headers: authorization ? { Authorization: authorization } : {},
    cache: 'no-store',
    signal,
  })
  const payload = await readJson(response)
  if (!response.ok) throw requestError(response.status, payload)
  return parsePollResponse(payload)
}

/** 列表只读取 owner-scoped 持久化摘要，不会触发上游轮询。 */
export async function listVideoTasks(
  before?: string,
  signal?: AbortSignal,
): Promise<VideoTaskListPage> {
  if (before !== undefined && !historyCursorPattern.test(before)) {
    throw requestError(400, undefined)
  }
  const query = new URLSearchParams({ limit: '20' })
  if (before) query.set('before', before)
  const authorization = sessionAuthorization()
  const response = await fetch(apiUrl(`/api/playground/v1/videos?${query.toString()}`), {
    headers: authorization ? { Authorization: authorization } : {},
    cache: 'no-store',
    signal,
  })
  const payload = await readJson(response)
  if (!response.ok) throw requestError(response.status, payload)
  return parseListPage(payload)
}

function sessionAuthorization() {
  const token = getManagementSessionToken()
  return token ? `Bearer ${token}` : ''
}

export function videoTaskError(error: unknown): VideoTaskApiError {
  if (error instanceof VideoTaskRequestError) return error.detail
  if (error instanceof DOMException && error.name === 'AbortError') {
    return { code: 'cancelled', message: 'Request cancelled.', status: 0 }
  }
  return { code: 'network_error', message: 'Network request failed.', status: 0 }
}

async function readJson(response: Response): Promise<unknown> {
  if (response.status === 401) invalidateManagementSession()
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
    const code = readString(payload.error, 'code')
    if (!['invalid_argument', 'failed_precondition', 'service_unavailable'].includes(code)) {
      throw requestError(503, undefined)
    }
    return {
      status,
      error: {
        code: code as VideoFailureCode,
        message: readString(payload.error, 'message'),
      },
    }
  }
  return { status }
}

function parseListPage(payload: unknown): VideoTaskListPage {
  if (!isRecord(payload) || !Array.isArray(payload.data) || payload.data.length > 20) {
    throw requestError(503, undefined)
  }
  const nextCursor = payload.next_cursor
  if (nextCursor !== null && nextCursor !== undefined
    && (typeof nextCursor !== 'string' || !historyCursorPattern.test(nextCursor))) {
    throw requestError(503, undefined)
  }
  return {
    data: payload.data.map(parseListItem),
    next_cursor: typeof nextCursor === 'string' ? nextCursor : undefined,
  }
}

function parseListItem(value: unknown): VideoTaskListItem {
  if (!isRecord(value)) throw requestError(503, undefined)
  const id = readString(value, 'id')
  const model = readString(value, 'model')
  const rawStatus = value.status
  const progress = value.progress_basis_points
  const createdAt = value.created_at
  const updatedAt = value.updated_at
  if (
    !taskIdPattern.test(id)
    || model.length > 256
    || typeof rawStatus !== 'string'
    || !['pending', 'done', 'expired', 'failed'].includes(rawStatus)
    || !Number.isInteger(progress)
    || Number(progress) < 0
    || Number(progress) > 10_000
    || !validTimestamp(createdAt)
    || !validTimestamp(updatedAt)
    || Number(updatedAt) < Number(createdAt)
  ) {
    throw requestError(503, undefined)
  }
  return {
    id,
    model,
    status: rawStatus as VideoTaskStatus,
    progress_basis_points: Number(progress),
    created_at: Number(createdAt),
    updated_at: Number(updatedAt),
  }
}

function validTimestamp(value: unknown) {
  return Number.isSafeInteger(value) && Number(value) >= 0
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
