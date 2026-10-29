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
  blob?: Blob
  requestId?: string
  revisedPrompt?: string
}

export type ImagePlazaHistoryRecord = {
  id: string
  model: string
  prompt: string
  revised_prompt?: string | null
  request_id?: string | null
  format: 'png' | 'jpeg' | 'webp'
  byte_size: number
  thumbnail_format?: 'png' | 'jpeg' | 'webp' | null
  thumbnail_byte_size?: number | null
  created_at: number
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
  const outputFormat = typeof payload.output_format === 'string' ? payload.output_format : ''
  const requestId = response.headers.get('x-request-id') ?? undefined
  const results = payload.data.flatMap((entry): ImagePlazaResult[] => {
    if (!isRecord(entry)) return []
    const base64 = typeof entry.b64_json === 'string' ? entry.b64_json : ''
    const rawUrl = typeof entry.url === 'string' ? entry.url : ''
    const blob = base64 ? base64ImageBlob(base64, imageMimeType(outputFormat, base64)) : undefined
    const src = blob ? URL.createObjectURL(blob) : safeImageUrl(rawUrl, baseUrl)
    if (!src) return []
    return [{
      id: createImageId(),
      src,
      blob,
      requestId,
      revisedPrompt: typeof entry.revised_prompt === 'string' ? entry.revised_prompt : undefined,
    }]
  })

  if (results.length === 0) throw new ImagePlazaRequestError('invalid_response')
  return results
}

function imageMimeType(format: string, base64: string) {
  switch (format.toLowerCase()) {
    case 'png': return 'image/png'
    case 'jpeg':
    case 'jpg': return 'image/jpeg'
    case 'webp': return 'image/webp'
  }
  if (base64.startsWith('iVBORw0KGgo')) return 'image/png'
  if (base64.startsWith('/9j/')) return 'image/jpeg'
  if (base64.startsWith('UklGR')) return 'image/webp'
  return ''
}

function base64ImageBlob(base64: string, mimeType: string): Blob | undefined {
  if (!mimeType) return
  try {
    const binary = atob(base64)
    const bytes = new Uint8Array(binary.length)
    for (let index = 0; index < binary.length; index += 1) {
      bytes[index] = binary.charCodeAt(index)
    }
    return new Blob([bytes], { type: mimeType })
  } catch {
    return
  }
}

function safeImageUrl(value: string, baseUrl: string) {
  if (!value) return ''
  try {
    const url = new URL(value, baseUrl)
    const securePage = window.location.protocol === 'https:'
    return url.protocol === 'https:' || (!securePage && url.protocol === 'http:') ? url.href : ''
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

export async function loadImageHistory(): Promise<ImagePlazaHistoryRecord[]> {
  const response = await sendHistoryRequest('', { method: 'GET' })
  let payload: unknown
  try {
    payload = await response.json()
  } catch {
    throw new ImagePlazaRequestError('invalid_response')
  }
  if (!isRecord(payload) || !Array.isArray(payload.images)) {
    throw new ImagePlazaRequestError('invalid_response')
  }
  return payload.images.filter(isHistoryRecord)
}

export async function saveImageHistory(
  image: ImagePlazaResult,
  model: string,
  prompt: string,
): Promise<ImagePlazaHistoryRecord> {
  const body = new FormData()
  body.set('model', model)
  body.set('prompt', prompt)
  if (image.revisedPrompt) body.set('revised_prompt', image.revisedPrompt)
  if (image.requestId) body.set('request_id', image.requestId)

  if (image.blob) {
    body.set('image', image.blob, `image-${image.id}.${imageFileExtension(image.blob.type)}`)
    const thumbnail = await createThumbnail(image.blob)
    if (thumbnail) body.set('thumbnail', thumbnail, `thumbnail-${image.id}.${imageFileExtension(thumbnail.type)}`)
  } else {
    body.set('image_url', image.src)
  }

  const response = await sendHistoryRequest('', { method: 'POST', body })
  let payload: unknown
  try {
    payload = await response.json()
  } catch {
    throw new ImagePlazaRequestError('invalid_response')
  }
  if (!isHistoryRecord(payload)) throw new ImagePlazaRequestError('invalid_response')
  return payload
}

export async function loadImageHistoryImage(imageId: string, thumbnail: boolean) {
  const variant = thumbnail ? 'thumbnail' : 'image'
  const response = await sendHistoryRequest(`/${encodeURIComponent(imageId)}/${variant}`, { method: 'GET' })
  return response.blob()
}

export async function clearImageHistory() {
  await sendHistoryRequest('', { method: 'DELETE' })
}

export async function deleteImageHistory(imageId: string) {
  await sendHistoryRequest(`/${encodeURIComponent(imageId)}`, { method: 'DELETE' })
}

async function sendHistoryRequest(path: string, init: RequestInit) {
  const token = getManagementSessionToken()
  if (!token) throw new ImagePlazaRequestError('session_missing')

  const headers = new Headers(init.headers)
  headers.set('Authorization', `Bearer ${token}`)
  let response: Response
  try {
    const baseUrl = (apiBaseUrl || window.location.origin).replace(/\/+$/, '')
    response = await fetch(`${baseUrl}/api/playground/image-history${path}`, {
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

async function createThumbnail(image: Blob): Promise<Blob | undefined> {
  let bitmap: ImageBitmap | undefined
  try {
    bitmap = await createImageBitmap(image)
    const scale = Math.min(1, 384 / Math.max(bitmap.width, bitmap.height))
    const canvas = document.createElement('canvas')
    canvas.width = Math.max(1, Math.round(bitmap.width * scale))
    canvas.height = Math.max(1, Math.round(bitmap.height * scale))
    const context = canvas.getContext('2d')
    if (!context) return
    context.drawImage(bitmap, 0, 0, canvas.width, canvas.height)
    const thumbnail = await new Promise<Blob | null>((resolve) => {
      canvas.toBlob(resolve, 'image/webp', 0.78)
    })
    if (thumbnail && thumbnail.size <= 512 * 1024) return thumbnail
    return undefined
  } catch {
    return
  } finally {
    bitmap?.close()
  }
}

function imageFileExtension(mimeType: string) {
  if (mimeType === 'image/jpeg') return 'jpg'
  if (mimeType === 'image/webp') return 'webp'
  return 'png'
}

function isHistoryRecord(value: unknown): value is ImagePlazaHistoryRecord {
  if (!isRecord(value)) return false
  return typeof value.id === 'string'
    && typeof value.model === 'string'
    && typeof value.prompt === 'string'
    && ['png', 'jpeg', 'webp'].includes(String(value.format))
    && typeof value.byte_size === 'number'
    && typeof value.created_at === 'number'
}
