import type { CreateClientConfig } from './generated/client.gen'
import { apiBaseUrl } from './endpoint'
import { getManagementSessionToken } from './session-token'
import { apiFetch } from './transport'

/** 统一注入同源凭据与可选 API 前缀，业务请求不在各 feature 内重复配置。 */
export const createClientConfig: CreateClientConfig = () => ({
  ...(apiBaseUrl ? { baseUrl: apiBaseUrl } : {}),
  auth: () => getManagementSessionToken(),
  credentials: 'same-origin',
  fetch: apiFetch,
  headers: {
    Accept: 'application/json',
  },
  responseStyle: 'fields',
  throwOnError: true,
})
