import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { apiClient, ApiError } from '@/lib/api'
import {
  createOrganizationProvisioningRequest,
  decideOrganizationProvisioningRequest,
  getActiveOrganizationProvisioningRequest,
  getOrganizationProvisioningRequest,
  listAdminOrganizationProvisioningRequests,
} from '@/lib/api/generated/sdk.gen'
import type {
  OrganizationProvisioningCreateRequest,
  OrganizationProvisioningDecisionRequest,
  OrganizationProvisioningEntitlement,
  OrganizationProvisioningRequest,
  OrganizationProvisioningRequestListResponse,
  OrganizationProvisioningStatus,
} from '@/lib/api/generated/types.gen'

export type { OrganizationProvisioningCreateRequest, OrganizationProvisioningDecisionRequest, OrganizationProvisioningEntitlement, OrganizationProvisioningRequest, OrganizationProvisioningStatus }

const provisioningRoot = ['organization-provisioning'] as const
export const organizationProvisioningQueryKey = [...provisioningRoot, 'active'] as const
export const organizationProvisioningAdminQueryKey = [...provisioningRoot, 'admin'] as const

/** 读取当前账号唯一仍在处理中的申请；没有申请时将 404 收敛为空状态。 */
export function useActiveOrganizationProvisioning() {
  return useQuery({
    queryKey: organizationProvisioningQueryKey,
    queryFn: async ({ signal }) => {
      try {
        const result = await getActiveOrganizationProvisioningRequest({ client: apiClient, signal }) as unknown as { data: OrganizationProvisioningRequest }
        return result.data
      } catch (error) {
        if (error instanceof ApiError && error.status === 404) return null
        throw error
      }
    },
  })
}

/** 按申请键恢复本人最近一次申请，适用于刷新页面或网络重试后的状态确认。 */
export function useOrganizationProvisioningRequest(requestKey: string | undefined, enabled = true) {
  const queryRequestKey = requestKey ?? ''
  return useQuery({
    queryKey: [...provisioningRoot, 'request', requestKey],
    enabled: enabled && requestKey !== undefined,
    queryFn: async ({ signal }) => {
      const result = await getOrganizationProvisioningRequest({ client: apiClient, path: { request_key: queryRequestKey }, signal }) as unknown as { data: OrganizationProvisioningRequest }
      return result.data
    },
  })
}

/** 提交申请时保留同一幂等键，网络超时后重复提交只恢复服务端事实。 */
export function useCreateOrganizationProvisioning() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (body: OrganizationProvisioningCreateRequest) => {
      const result = await createOrganizationProvisioningRequest({ client: apiClient, body }) as unknown as { data: OrganizationProvisioningRequest }
      return result.data
    },
    onSuccess: (data) => {
      queryClient.setQueryData(organizationProvisioningQueryKey, data)
    },
  })
}

/** 平台管理员读取申请队列，筛选和游标均交由服务端校验。 */
export function useAdminOrganizationProvisioning(status?: OrganizationProvisioningStatus) {
  return useQuery({
    queryKey: [...organizationProvisioningAdminQueryKey, status ?? 'all'],
    queryFn: async ({ signal }) => {
      const result = await listAdminOrganizationProvisioningRequests({ client: apiClient, query: { limit: 50, status }, signal }) as unknown as { data: OrganizationProvisioningRequestListResponse }
      return result.data
    },
  })
}

/** 管理员决定后刷新队列，避免在客户端推导企业开通成功。 */
export function useDecideOrganizationProvisioning() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async ({ requestKey, body }: { requestKey: string; body: OrganizationProvisioningDecisionRequest }) => {
      const result = await decideOrganizationProvisioningRequest({ client: apiClient, path: { request_key: requestKey }, body }) as unknown as { data: OrganizationProvisioningRequest }
      return result.data
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: organizationProvisioningAdminQueryKey }),
  })
}

export function organizationProvisioningErrorStatus(error: unknown) {
  return error instanceof ApiError ? error.status : undefined
}

export function organizationProvisioningErrorCode(error: unknown) {
  if (!(error instanceof ApiError) || typeof error.details !== 'object' || error.details === null) return undefined
  const code = (error.details as { code?: unknown }).code
  return typeof code === 'string' ? code : undefined
}
