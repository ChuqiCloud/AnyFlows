import { getManagementSessionToken, invalidateManagementSession } from '@/lib/api/session-token'
import { apiUrl } from '@/lib/api/endpoint'

export type ApiExplorerOperation = {
  operation_id: string
  method: string
  path: string
  tag: string
  summary: string
  description?: string
  auth_mode: 'none' | 'session' | 'access_token' | 'api_key' | 'special'
  supports_access_token?: boolean
  required_scopes?: string[]
  interactive_session_required?: boolean
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
export type ApiExplorerRunResult = {
  status: number
  ok: boolean
  duration_ms: number
  request_id?: string
  body: string
  diagnostic?: 'access' | 'verification' | 'quota' | 'model' | 'request' | 'server' | 'unknown'
  next_step?: string
  action?: { path: string; label: string }
}

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

export async function runApiExplorerOperation(operation: ApiExplorerOperation, input: { pathParams: Record<string, string>; queryParams: Record<string, string>; body?: string; apiKey?: string; accessToken?: string; useAccessToken?: boolean; signal?: AbortSignal }) {
  if (!operation.debuggable) throw new Error('This operation is documentation-only')
  const path = operation.path.replace(/\{([^}]+)\}/g, (_, name: string) => {
    const value = input.pathParams[name]?.trim()
    if (!value) throw new Error(`Missing path parameter: ${name}`)
    return encodeURIComponent(value)
  })
  const url = new URL(apiUrl(path), window.location.origin)
  Object.entries(input.queryParams).forEach(([key, value]) => { if (value.trim()) url.searchParams.set(key, value.trim()) })
  const useAccessToken = operation.auth_mode === 'access_token' || (operation.auth_mode === 'session' && input.useAccessToken === true)
  if (useAccessToken && (!operation.supports_access_token || !input.accessToken?.trim())) {
    throw new Error('A personal access token with the required scopes is needed')
  }
  const requestHeaders: Record<string, string> = useAccessToken
    ? { Authorization: `Bearer ${input.accessToken!.trim()}` }
    : operation.auth_mode === 'api_key'
    ? input.apiKey?.trim() ? { 'x-api-key': input.apiKey.trim() } : (() => { throw new Error('API key is required') })()
    : operation.auth_mode === 'session' ? headers() : {}
  const body = input.body?.trim()
  if (body) requestHeaders['Content-Type'] = 'application/json'
  const started = performance.now()
  const response = await fetch(url, { method: operation.method, headers: requestHeaders, body: operation.method === 'GET' || operation.method === 'HEAD' ? undefined : body, credentials: useAccessToken ? 'omit' : 'same-origin', signal: input.signal })
  if (response.status === 401 && operation.auth_mode === 'session' && !useAccessToken) invalidateManagementSession()
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

const secretField = /(?:api[_-]?key|access[_-]?token|password|secret|credential|authorization)/i

function sampleValue(schema: unknown, property = '', depth = 0): unknown {
  if (!schema || typeof schema !== 'object' || depth > 5) return undefined
  const value = schema as Record<string, unknown>
  if (Array.isArray(value.enum) && value.enum.length > 0) return value.enum[0]
  if (value.example !== undefined && !secretField.test(property)) return value.example
  if (value.default !== undefined && !secretField.test(property)) return value.default
  if (value.type === 'object' || value.properties) {
    const properties = value.properties && typeof value.properties === 'object'
      ? value.properties as Record<string, unknown>
      : {}
    const result = Object.fromEntries(Object.entries(properties)
      .filter(([name]) => !secretField.test(name))
      .map(([name, child]) => [name, sampleValue(child, name, depth + 1)])
      .filter(([, child]) => child !== undefined))
    return Object.keys(result).length ? result : undefined
  }
  if (value.type === 'array') {
    if (property === 'messages') return [{ role: 'user', content: 'Hello' }]
    const item = sampleValue(value.items, '', depth + 1)
    return item === undefined ? [] : [item]
  }
  if (value.type === 'string' || value.type === undefined) {
    if (property === 'model') return 'your-model'
    if (property === 'prompt' || property === 'input' || property === 'content') return 'Hello'
    if (property === 'role') return 'user'
    if (property === 'stream') return undefined
    return 'example'
  }
  if (value.type === 'integer' || value.type === 'number') return value.minimum ?? 1
  if (value.type === 'boolean') return false
  return undefined
}

/** Give the request editor a safe example without inserting credentials. */
export function suggestedRequestBody(detail: ApiExplorerDetail | undefined) {
  if (!detail) return ''
  const path = detail.operation.path.toLowerCase()
  let example: unknown
  if (path.endsWith('/chat/completions')) {
    example = { model: 'your-model', messages: [{ role: 'user', content: 'Hello' }] }
  } else if (path.endsWith('/responses')) {
    example = { model: 'your-model', input: 'Hello' }
  } else if (path.endsWith('/messages')) {
    example = { model: 'your-model', max_tokens: 512, messages: [{ role: 'user', content: 'Hello' }] }
  } else {
    const openapi = detail.openapi
    const body = openapi.requestBody && typeof openapi.requestBody === 'object'
      ? openapi.requestBody as Record<string, unknown>
      : undefined
    const content = body?.content && typeof body.content === 'object'
      ? body.content as Record<string, unknown>
      : undefined
    const json = content?.['application/json'] && typeof content['application/json'] === 'object'
      ? content['application/json'] as Record<string, unknown>
      : undefined
    example = sampleValue(json?.schema)
  }
  return example && typeof example === 'object' ? JSON.stringify(example, null, 2) : '{\n  \n}'
}
