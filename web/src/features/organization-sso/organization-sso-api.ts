import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { organizationQueryKey } from '@/features/organization-budgets/organization-budget-api'
import { apiClient, ApiError } from '@/lib/api'
import {
  createOrganizationSsoDomain,
  listOrganizationSsoDomains,
  listOrganizationSsoProviders,
  getOrganizationSsoPolicy,
  updateOrganizationSsoPolicy,
  establishOrganizationSsoRecovery,
  suspendOrganizationSsoRequired,
  verifyOrganizationSsoDomain,
} from '@/lib/api/generated/sdk.gen'
import type {
  OrganizationSsoDomainCreateRequest,
  OrganizationSsoDomainVerifyRequestWritable,
  OrganizationSsoPolicyRequest,
  OrganizationSsoRecoveryRequestWritable,
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

/** 读取企业 SSO 策略与恢复就绪事实，权限不足时不发起请求。 */
export function useOrganizationSsoPolicy(organizationId: number, enabled: boolean) {
  return useQuery({
    queryKey: organizationQueryKey(organizationId, 'sso', 'policy'),
    enabled,
    queryFn: async ({ signal }) => {
      const { data } = await getOrganizationSsoPolicy({
        client: apiClient,
        path: { organization_id: organizationId },
        signal,
      })
      return data
    },
  })
}

/** 按服务端版本更新策略，成功后刷新策略与组织概览事实。 */
export function useUpdateOrganizationSsoPolicy(organizationId: number) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (body: OrganizationSsoPolicyRequest) => {
      const { data } = await updateOrganizationSsoPolicy({
        body,
        client: apiClient,
        path: { organization_id: organizationId },
      })
      return data
    },
    onSuccess: (data) => {
      queryClient.setQueryData(organizationQueryKey(organizationId, 'sso', 'policy'), data)
      void queryClient.invalidateQueries({ queryKey: organizationQueryKey(organizationId, 'overview') })
    },
  })
}

/** 登记 Owner 恢复能力；密码与二次因子只在请求体内短暂存在。 */
export function useEstablishOrganizationSsoRecovery(organizationId: number) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (body: OrganizationSsoRecoveryRequestWritable) => {
      const { data } = await establishOrganizationSsoRecovery({
        body,
        client: apiClient,
        path: { organization_id: organizationId },
      })
      return data
    },
    onSuccess: (data) => {
      queryClient.setQueryData(organizationQueryKey(organizationId, 'sso', 'policy'), data)
    },
  })
}

/** 暂停 Required 策略，保留恢复就绪事实以便后续重新启用。 */
export function useSuspendOrganizationSsoRequired(organizationId: number) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (body: OrganizationSsoRecoveryRequestWritable) => {
      const { data } = await suspendOrganizationSsoRequired({
        body,
        client: apiClient,
        path: { organization_id: organizationId },
      })
      return data
    },
    onSuccess: (data) => {
      queryClient.setQueryData(organizationQueryKey(organizationId, 'sso', 'policy'), data)
    },
  })
}

export function organizationSsoErrorCode(error: unknown) {
  if (!(error instanceof ApiError) || typeof error.details !== 'object' || error.details === null) return undefined
  const code = (error.details as { code?: unknown }).code
  return typeof code === 'string' ? code : undefined
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
