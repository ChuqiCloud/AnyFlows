import { ApiError } from './errors'
import { invalidateManagementSession } from './session-token'

const nativeFetch = globalThis.fetch.bind(globalThis)

/** 在生成客户端解析响应前统一错误形态，同时保留 Query 的请求取消语义。 */
export async function apiFetch(input: RequestInfo | URL, init?: RequestInit) {
  let response: Response

  try {
    response = await nativeFetch(input, init)
  } catch (error) {
    if (error instanceof DOMException && error.name === 'AbortError') {
      throw error
    }
    throw ApiError.from(error)
  }

  if (response.ok) {
    return response
  }

  if (response.status === 401) {
    invalidateManagementSession()
  }

  throw ApiError.from(await readErrorDetails(response), response)
}

async function readErrorDetails(response: Response) {
  let text: string
  try {
    text = await response.text()
  } catch {
    return undefined
  }
  if (!text) {
    return undefined
  }

  const contentType = response.headers.get('content-type')
  if (contentType?.includes('json')) {
    try {
      return JSON.parse(text) as unknown
    } catch {
      return text
    }
  }

  return text
}
