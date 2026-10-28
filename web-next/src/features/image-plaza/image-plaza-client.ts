import { apiBaseUrl } from '@/lib/api/endpoint'
import { getManagementSessionToken, invalidateManagementSession } from '@/lib/api/session-token'

export type ImagePlazaSettings = {
  model: string
  prompt: string
  count: number
  size: string
  quality: string
}

export type ImagePlazaResult = {
  id: string
  src: string
  revisedPrompt?: string
}

export class ImagePlazaRequestError extends Error {
  readonly kind: 'session_missing' | 'request_failed' | 'network_error' | 'invalid_response'

  constructor(kind: 'session_missing' | 'request_failed' | 'network_error' | 'invalid_response') {
    super(kind)
    this.kind = kind
    this.name = 'ImagePlazaRequestError'
  }
}

export async function generateImages(settings: ImagePlazaSettings): Promise<ImagePlazaResult[]> {
  const response = await sendImageRequest('/generations', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      model: settings.model,
      prompt: settings.prompt,
      n: settings.count,
      size: settings.size,
      quality: settings.quality,
    }),
  })
  return readImageResults(response)
}

export async function editImage(
  settings: ImagePlazaSettings,
  image: File,
  mask?: File,
): Promise<ImagePlazaResult[]> {
  const body = new FormData()
  body.set('model', settings.model)
  body.set('prompt', settings.prompt)
  body.set('n', String(settings.count))
  body.set('size', settings.size)
  body.set('quality', settings.quality)
  body.set('image', image, image.name)
  if (mask) body.set('mask', mask, mask.name)

  const response = await sendImageRequest('/edits', { method: 'POST', body })
  return readImageResults(response)
}

async function sendImageRequest(path: string, init: RequestInit) {
  const token = getManagementSessionToken()
  if (!token) throw new ImagePlazaRequestError('session_missing')

  const headers = new Headers(init.headers)
  headers.set('Authorization', `Bearer ${token}`)
  let response: Response
  try {
    const baseUrl = (apiBaseUrl || window.location.origin).replace(/\/+$/, '')
    response = await fetch(`${baseUrl}/api/playground/v1/images${path}`, {
      ...init,
      headers,
      cache: 'no-store',
      credentials: 'same-origin',
    })
  } catch {
    throw new ImagePlazaRequestError('network_error')
  }

  if (response.status === 401) invalidateManagementSession()
  if (!response.ok) throw new ImagePlazaRequestError('request_failed')
  return response
}

async function readImageResults(response: Response): Promise<ImagePlazaResult[]> {
  let payload: unknown
  try {
    payload = await response.json()
  } catch {
    throw new ImagePlazaRequestError('invalid_response')
  }
  if (!isRecord(payload) || !Array.isArray(payload.data)) {
    throw new ImagePlazaRequestError('invalid_response')
  }

  const baseUrl = apiBaseUrl || window.location.origin
  const results = payload.data.flatMap((entry): ImagePlazaResult[] => {
    if (!isRecord(entry)) return []
    const base64 = typeof entry.b64_json === 'string' ? entry.b64_json : ''
    const rawUrl = typeof entry.url === 'string' ? entry.url : ''
    const src = base64 ? `data:image/png;base64,${base64}` : safeImageUrl(rawUrl, baseUrl)
    if (!src) return []
    return [{
      id: createImageId(),
      src,
      revisedPrompt: typeof entry.revised_prompt === 'string' ? entry.revised_prompt : undefined,
    }]
  })

  if (results.length === 0) throw new ImagePlazaRequestError('invalid_response')
  return results
}

function safeImageUrl(value: string, baseUrl: string) {
  if (!value) return ''
  try {
    const url = new URL(value, baseUrl)
    return url.protocol === 'https:' || url.protocol === 'http:' ? url.href : ''
  } catch {
    return ''
  }
}

function createImageId() {
  return typeof crypto !== 'undefined' && 'randomUUID' in crypto
    ? crypto.randomUUID()
    : `${Date.now()}-${Math.random().toString(36).slice(2)}`
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}
