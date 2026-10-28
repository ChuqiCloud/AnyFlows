import { getManagementSessionToken, invalidateManagementSession } from '@/lib/api/session-token'

export type ImageRequestOptions = {
  model: string
  prompt: string
  n: number
  size: string
  quality: string
  responseFormat: 'url' | 'b64_json'
}

export type GeneratedImage = {
  url?: string
  b64_json?: string
  revised_prompt?: string
}

export type ImageResponse = {
  created?: number
  data?: GeneratedImage[]
  usage?: Record<string, unknown>
}

function apiRoot() {
  const configured = import.meta.env.VITE_API_BASE_URL?.trim().replace(/\/+$/, '')
  return configured || window.location.origin
}

async function sessionFetch(input: RequestInfo | URL, init: RequestInit = {}) {
  const headers = new Headers(init.headers)
  const token = getManagementSessionToken()
  if (token) headers.set('Authorization', `Bearer ${token}`)
  const response = await fetch(input, { ...init, headers, credentials: 'same-origin' })
  if (response.status === 401) invalidateManagementSession()
  return response
}

async function readResponse(response: Response): Promise<ImageResponse> {
  if (!response.ok) {
    const text = await response.text().catch(() => '')
    throw new Error(text || `Image request failed (${response.status})`)
  }
  return response.json() as Promise<ImageResponse>
}

export async function generateImage(options: ImageRequestOptions) {
  const response = await sessionFetch(`${apiRoot()}/api/playground/v1/images/generations`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      model: options.model,
      prompt: options.prompt,
      n: options.n,
      size: options.size,
      quality: options.quality,
      response_format: options.responseFormat,
    }),
  })
  return readResponse(response)
}

export async function editImage(file: File, options: ImageRequestOptions, mask?: File) {
  const body = new FormData()
  body.append('model', options.model)
  body.append('prompt', options.prompt)
  body.append('image', file, file.name || 'image.png')
  body.append('n', String(options.n))
  body.append('size', options.size)
  body.append('quality', options.quality)
  body.append('response_format', options.responseFormat)
  if (mask) body.append('mask', mask, mask.name || 'mask.png')
  const response = await sessionFetch(`${apiRoot()}/api/playground/v1/images/edits`, {
    method: 'POST',
    body,
  })
  return readResponse(response)
}

export function imageSource(image: GeneratedImage) {
  if (image.url) return image.url
  if (image.b64_json) return `data:image/png;base64,${image.b64_json}`
  return undefined
}
