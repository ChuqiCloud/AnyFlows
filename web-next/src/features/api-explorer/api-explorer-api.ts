import { getManagementSessionToken, invalidateManagementSession } from '@/lib/api/session-token'
import { apiUrl } from '@/lib/api/endpoint'

export type ApiExplorerOperation = {
  operation_id: string
  method: string
  path: string
  tag: string
  summary: string
  description?: string
  auth_mode: 'none' | 'session' | 'api_key' | 'special'
  access_scope: 'public' | 'gateway' | 'session' | 'organization' | 'admin'
  debuggable: boolean
}

export type ApiExplorerCatalog = {
  operations: ApiExplorerOperation[]
  tags: string[]
  total: number
  page: number
  page_size: number
  has_more: boolean
  viewer: 'guest' | 'user' | 'admin'
}

export type ApiExplorerDetail = { operation: ApiExplorerOperation; openapi: Record<string, unknown> }
export type ApiExplorerRunResult = { status: number; ok: boolean; duration_ms: number; request_id?: string; body: string }

function headers(): Record<string, string> {
  const token = getManagementSessionToken()
  return token ? { Authorization: `Bearer ${token}` } : {}
}

async function readJson<T>(response: Response) {
  if (response.status === 401 && getManagementSessionToken()) invalidateManagementSession()
  if (!response.ok) throw new Error(`HTTP ${response.status}`)
  return response.json() as Promise<T>
}

export async function fetchApiExplorerCatalog(search: string, tag: string, page = 1, signal?: AbortSignal) {
  const params = new URLSearchParams({ page: String(page), page_size: '60' })
  if (search.trim()) params.set('q', search.trim())
  if (tag) params.set('tag', tag)
  const response = await fetch(apiUrl(`/api/explorer/catalog?${params}`), { headers: headers(), credentials: 'same-origin', signal })
  return readJson<ApiExplorerCatalog>(response)
}

export async function fetchApiExplorerDetail(operationId: string, signal?: AbortSignal) {
  const response = await fetch(apiUrl(`/api/explorer/catalog/${encodeURIComponent(operationId)}`), { headers: headers(), credentials: 'same-origin', signal })
  return readJson<ApiExplorerDetail>(response)
}

export async function runApiExplorerOperation(operation: ApiExplorerOperation, input: { pathParams: Record<string, string>; queryParams: Record<string, string>; body?: string; apiKey?: string; signal?: AbortSignal }) {
  if (!operation.debuggable) throw new Error('This operation is documentation-only')
  const path = operation.path.replace(/\{([^}]+)\}/g, (_, name: string) => {
    const value = input.pathParams[name]?.trim()
    if (!value) throw new Error(`Missing path parameter: ${name}`)
    return encodeURIComponent(value)
  })
  const url = new URL(apiUrl(path), window.location.origin)
  Object.entries(input.queryParams).forEach(([key, value]) => { if (value.trim()) url.searchParams.set(key, value.trim()) })
  const requestHeaders: Record<string, string> = operation.auth_mode === 'api_key'
    ? input.apiKey?.trim() ? { 'x-api-key': input.apiKey.trim() } : (() => { throw new Error('API key is required') })()
    : operation.auth_mode === 'session' ? headers() : {}
  const body = input.body?.trim()
  if (body) requestHeaders['Content-Type'] = 'application/json'
  const started = performance.now()
  const response = await fetch(url, { method: operation.method, headers: requestHeaders, body: operation.method === 'GET' || operation.method === 'HEAD' ? undefined : body, credentials: 'same-origin', signal: input.signal })
  if (response.status === 401 && operation.auth_mode === 'session') invalidateManagementSession()
  const text = await response.text()
  return { status: response.status, ok: response.ok, duration_ms: Math.round(performance.now() - started), request_id: response.headers.get('x-request-id') ?? undefined, body: text.length > 512_000 ? `${text.slice(0, 512_000)}\n…` : text }
}

export function operationParameterNames(detail: ApiExplorerDetail | undefined) {
  const parameters = Array.isArray(detail?.openapi.parameters) ? detail.openapi.parameters : []
  const parsed = parameters.flatMap((parameter) => {
    if (!parameter || typeof parameter !== 'object') return []
    const item = parameter as Record<string, unknown>
    return typeof item.name === 'string' && (item.in === 'path' || item.in === 'query') ? [{ name: item.name, location: item.in as 'path' | 'query', required: item.required === true }] : []
  })
  const pathParameters = [...(detail?.operation.path.matchAll(/\{([^}]+)\}/g) ?? [])]
    .map((match) => ({ name: match[1], location: 'path' as const, required: true }))
  return [...new Map([...pathParameters, ...parsed].map((item) => [`${item.location}:${item.name}`, item])).values()]
}
