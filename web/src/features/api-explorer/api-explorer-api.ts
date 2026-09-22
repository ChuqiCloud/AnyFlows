import { getManagementSessionToken, invalidateManagementSession } from '@/lib/api/session-token'

export type ApiExplorerAuthMode = 'none' | 'session' | 'api_key' | 'special'
export type ApiExplorerAccessScope = 'public' | 'gateway' | 'session' | 'organization' | 'admin'

export type ApiExplorerOperation = {
  operation_id: string
  method: string
  path: string
  tag: string
  summary: string
  description?: string
  auth_mode: ApiExplorerAuthMode
  access_scope: ApiExplorerAccessScope
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

export type ApiExplorerDetail = {
  operation: ApiExplorerOperation
  openapi: Record<string, unknown>
}

export type ApiExplorerRunResult = {
  status: number
  ok: boolean
  duration_ms: number
  request_id?: string
  body: string
}

function sessionHeaders(): Record<string, string> {
  const token = getManagementSessionToken()
  return token ? { Authorization: `Bearer ${token}` } : {}
}

async function readJson<T>(response: Response): Promise<T> {
  if (response.status === 401 && getManagementSessionToken()) invalidateManagementSession()
  if (!response.ok) {
    throw new Error(`HTTP ${response.status}`)
  }
  return response.json() as Promise<T>
}

export async function fetchApiExplorerCatalog(
  search: string,
  tag: string,
  page = 1,
  signal?: AbortSignal,
) {
  const params = new URLSearchParams({ page: String(page), page_size: '60' })
  if (search.trim()) params.set('q', search.trim())
  if (tag) params.set('tag', tag)
  const response = await fetch(`/api/explorer/catalog?${params.toString()}`, {
    headers: sessionHeaders(),
    credentials: 'same-origin',
    signal,
  })
  return readJson<ApiExplorerCatalog>(response)
}

export async function fetchApiExplorerDetail(operationId: string, signal?: AbortSignal) {
  const response = await fetch(`/api/explorer/catalog/${encodeURIComponent(operationId)}`, {
    headers: sessionHeaders(),
    credentials: 'same-origin',
    signal,
  })
  return readJson<ApiExplorerDetail>(response)
}

function pathParameters(path: string) {
  return [...path.matchAll(/\{([^}]+)\}/g)].map((match) => match[1])
}

export async function runApiExplorerOperation(
  operation: ApiExplorerOperation,
  input: {
    pathParams: Record<string, string>
    queryParams: Record<string, string>
    body?: string
    apiKey?: string
    signal?: AbortSignal
  },
): Promise<ApiExplorerRunResult> {
  if (!operation.debuggable) throw new Error('This operation is documentation-only')
  const urlPath = operation.path.replace(/\{([^}]+)\}/g, (_, name: string) => {
    const value = input.pathParams[name]?.trim()
    if (!value) throw new Error(`Missing path parameter: ${name}`)
    return encodeURIComponent(value)
  })
  const url = new URL(urlPath, window.location.origin)
  Object.entries(input.queryParams).forEach(([key, value]) => {
    if (value.trim()) url.searchParams.set(key, value.trim())
  })
  const headers: Record<string, string> = {}
  if (operation.auth_mode === 'session') {
    Object.assign(headers, sessionHeaders())
  } else if (operation.auth_mode === 'api_key') {
    const apiKey = input.apiKey?.trim()
    if (!apiKey) throw new Error('API key is required')
    headers.Authorization = `Bearer ${apiKey}`
  }
  const body = input.body?.trim()
  if (body) headers['Content-Type'] = 'application/json'
  const startedAt = performance.now()
  const response = await fetch(url, {
    method: operation.method,
    headers,
    body: operation.method === 'GET' || operation.method === 'HEAD' ? undefined : body,
    credentials: 'same-origin',
    signal: input.signal,
  })
  if (response.status === 401 && operation.auth_mode === 'session') invalidateManagementSession()
  const responseBody = await response.text()
  return {
    status: response.status,
    ok: response.ok,
    duration_ms: Math.round(performance.now() - startedAt),
    request_id: response.headers.get('x-request-id') ?? undefined,
    body: responseBody.length > 512_000 ? `${responseBody.slice(0, 512_000)}\n…` : responseBody,
  }
}

export function operationParameterNames(detail: ApiExplorerDetail | undefined) {
  const parameters = Array.isArray(detail?.openapi.parameters) ? detail.openapi.parameters : []
  return parameters
    .map((parameter) => {
      if (!parameter || typeof parameter !== 'object') return undefined
      const record = parameter as Record<string, unknown>
      return typeof record.name === 'string' && (record.in === 'query' || record.in === 'path')
        ? { name: record.name, location: record.in as 'query' | 'path', required: record.required === true }
        : undefined
    })
    .filter((parameter): parameter is { name: string; location: 'query' | 'path'; required: boolean } => Boolean(parameter))
}

export function operationPathParameterNames(operation: ApiExplorerOperation) {
  return pathParameters(operation.path)
}
