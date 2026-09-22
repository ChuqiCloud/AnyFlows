import { apiClient } from '@/lib/api'
import {
  discoverOrganizationSso as discoverOrganizationSsoRequest,
  startPublicOrganizationSsoLogin as startPublicOrganizationSsoLoginRequest,
} from '@/lib/api/generated/sdk.gen'

export type OrganizationSsoPublicLoginStart = {
  protocol: 'oidc'
  expires_at: number
  authorization_url: string
} | {
  protocol: 'saml'
  expires_at: number
  action: string
  saml_request: string
  relay_state: string
}

/** 请求服务端判断企业邮箱或公开标识是否具备通用 SSO 登录资格。 */
export async function discoverOrganizationSso(identifier: string) {
  const { data } = await discoverOrganizationSsoRequest({
    body: { identifier },
    client: apiClient,
  })
  return data
}

/** 创建一次性企业 SSO 事务，并只接受服务端闭合的浏览器动作。 */
export async function startPublicOrganizationSsoLogin(
  identifier: string,
): Promise<OrganizationSsoPublicLoginStart> {
  const { data } = await startPublicOrganizationSsoLoginRequest({
    body: { identifier },
    client: apiClient,
  })
  if (!isPublicLoginStart(data)) throw new Error('企业 SSO 启动响应无效')
  return data
}

function isPublicLoginStart(value: unknown): value is OrganizationSsoPublicLoginStart {
  if (!isRecord(value)
    || typeof value.expires_at !== 'number'
    || !Number.isSafeInteger(value.expires_at)) return false
  if (value.protocol === 'oidc') {
    return typeof value.authorization_url === 'string'
      && isSafeOidcAuthorizationUrl(value.authorization_url)
      && Object.keys(value).length === 3
  }
  return value.protocol === 'saml'
    && typeof value.action === 'string'
    && typeof value.saml_request === 'string'
    && value.saml_request.length > 0
    && typeof value.relay_state === 'string'
    && value.relay_state.length > 0
    && Object.keys(value).length === 5
}

function isSafeOidcAuthorizationUrl(value: string) {
  try {
    const url = new URL(value)
    return url.protocol === 'https:'
      && url.username === ''
      && url.password === ''
      && url.hash === ''
  } catch {
    return false
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}
