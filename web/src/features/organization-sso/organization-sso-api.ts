import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { organizationQueryKey } from '@/features/organization-budgets/organization-budget-api'
import { apiClient } from '@/lib/api'
import {
  createOrganizationSsoDomain,
  listOrganizationSsoDomains,
  listOrganizationSsoProviders,
  verifyOrganizationSsoDomain,
} from '@/lib/api/generated/sdk.gen'
import type {
  OrganizationSsoDomainCreateRequest,
  OrganizationSsoDomainVerifyRequestWritable,
} from '@/lib/api/generated/types.gen'

/** 按企业权限读取脱敏 SSO Provider；未授权时不发起请求。 */
export function useOrganizationSsoProviders(organizationId: number, enabled: boolean) {
  return useQuery({
    queryKey: organizationQueryKey(organizationId, 'sso', 'providers'),
    enabled,
    queryFn: async ({ signal }) => {
      const { data } = await listOrganizationSsoProviders({
        client: apiClient,
        path: { organization_id: organizationId },
        signal,
      })
      return data.providers
    },
  })
}

/** 按企业范围读取域名验证事实；未授权时不发起请求。 */
export function useOrganizationSsoDomains(organizationId: number, enabled: boolean) {
  return useQuery({
    queryKey: organizationQueryKey(organizationId, 'sso', 'domains'),
    enabled,
    queryFn: async ({ signal }) => {
      const { data } = await listOrganizationSsoDomains({
        client: apiClient,
        path: { organization_id: organizationId },
        signal,
      })
      return data.domains
    },
  })
}

/** 创建域名证明后只把一次性 TXT 材料交给当前页面，不写入查询缓存。 */
export function useCreateOrganizationSsoDomain(organizationId: number) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (body: OrganizationSsoDomainCreateRequest) => {
      const { data } = await createOrganizationSsoDomain({
        body,
        client: apiClient,
        path: { organization_id: organizationId },
      })
      return data
    },
    onSuccess: () => invalidateOrganizationSsoDomains(queryClient, organizationId),
  })
}

/** 使用服务端 DNS 解析和一次性证明令牌完成域名验证。 */
export function useVerifyOrganizationSsoDomain(organizationId: number) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async ({ domainId, body }: { domainId: number; body: OrganizationSsoDomainVerifyRequestWritable }) => {
      const { data } = await verifyOrganizationSsoDomain({
        body,
        client: apiClient,
        path: { organization_id: organizationId, domain_id: domainId },
      })
      return data
    },
    onSuccess: () => invalidateOrganizationSsoDomains(queryClient, organizationId),
  })
}

function invalidateOrganizationSsoDomains(queryClient: ReturnType<typeof useQueryClient>, organizationId: number) {
  return Promise.all([
    queryClient.invalidateQueries({ queryKey: organizationQueryKey(organizationId, 'sso', 'domains') }),
    queryClient.invalidateQueries({ queryKey: organizationQueryKey(organizationId, 'overview') }),
  ])
}
