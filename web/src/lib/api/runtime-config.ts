import type { CreateClientConfig } from './generated/client.gen'
import { getManagementSessionToken } from './session-token'
import { apiFetch } from './transport'

/** 统一注入同源凭据与可选 API 前缀，业务请求不在各 feature 内重复配置。 */
export const createClientConfig: CreateClientConfig = () => {
  const baseUrl = import.meta.env.VITE_API_BASE_URL?.trim().replace(/\/+$/, '')

  return {
    ...(baseUrl ? { baseUrl } : {}),
    auth: () => getManagementSessionToken(),
    credentials: 'same-origin',
    fetch: apiFetch,
    headers: {
      Accept: 'application/json',
    },
    responseStyle: 'fields',
    throwOnError: true,
  }
}
