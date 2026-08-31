import { apiClient } from '@/lib/api'
import { discoverOrganizationSso as discoverOrganizationSsoRequest } from '@/lib/api/generated/sdk.gen'

/** 请求服务端判断企业邮箱或公开标识是否具备通用 SSO 登录资格。 */
export async function discoverOrganizationSso(identifier: string) {
  const { data } = await discoverOrganizationSsoRequest({
    body: { identifier },
    client: apiClient,
  })
  return data
}
