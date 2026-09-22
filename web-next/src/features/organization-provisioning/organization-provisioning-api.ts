import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { apiClient, ApiError } from '@/lib/api'
import {
  downloadAdminOrganizationProvisioningMaterial as downloadAdminOrganizationProvisioningMaterialRequest,
  decideOrganizationProvisioningRequest,
  getActiveOrganizationProvisioningRequest,
  getOrganizationProvisioningRequest,
  listAdminOrganizationProvisioningMaterials,
  listAdminOrganizationProvisioningRequests,
} from '@/lib/api/generated/sdk.gen'
import type {
  OrganizationProvisioningDecisionRequest,
  OrganizationProvisioningEntitlement,
  OrganizationProvisioningRequest,
  OrganizationProvisioningRequestListResponse,
  OrganizationProvisioningStatus,
} from '@/lib/api/generated/types.gen'

export type OrganizationProvisioningCreateInput = {
  idempotency_key: string
  organization_name: string
  organization_slug: string
  business_reason: string
}

export type { OrganizationProvisioningDecisionRequest, OrganizationProvisioningEntitlement, OrganizationProvisioningRequest, OrganizationProvisioningStatus }

export type OrganizationProvisioningMaterialDraft = { id: string; kind: string; file?: File }
export type OrganizationProvisioningMaterial = {
  id: number
  request_key: string
  kind: string
  file_name: string
  content_type: string
  size_bytes: number
  content_available: boolean
  created_at: number
}

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
    mutationFn: async (body: OrganizationProvisioningCreateInput & { materials: OrganizationProvisioningMaterialDraft[] }) => {
      const form = new FormData()
      form.append('metadata', JSON.stringify({
        idempotency_key: body.idempotency_key,
        organization_name: body.organization_name,
        organization_slug: body.organization_slug,
        business_reason: body.business_reason,
        materials: body.materials.map((material, index) => ({ kind: material.kind.trim(), file_field: String(index) })),
      }))
      body.materials.forEach((material, index) => {
        if (material.file) form.append(`file:${index}`, material.file, material.file.name)
      })
      const result = await apiClient.post<OrganizationProvisioningRequest>({
        url: '/api/account/organization-provisioning',
        body: form,
        // FormData 已包含动态 file:<index> 部件，必须绕过共享客户端的 JSON 序列化器。
        bodySerializer: null,
        headers: { 'Content-Type': null },
        security: [{ key: 'bearerAuth', scheme: 'bearer', type: 'http' }],
      }) as unknown as { data: OrganizationProvisioningRequest }
      return result.data
    },
    onSuccess: (data) => {
      queryClient.setQueryData(organizationProvisioningQueryKey, data)
    },
  })
}

export function useAdminOrganizationProvisioningMaterials(requestKey: string | undefined) {
  return useQuery({
    queryKey: [...provisioningRoot, 'materials', requestKey],
    enabled: requestKey !== undefined,
    retry: false,
    queryFn: async ({ signal }) => {
      const result = await listAdminOrganizationProvisioningMaterials({ client: apiClient, path: { request_key: requestKey ?? '' }, signal }) as unknown as { data: OrganizationProvisioningMaterial[] }
      return result.data
    },
  })
}

export async function downloadAdminOrganizationProvisioningMaterial(requestKey: string, materialId: number) {
  const result = await downloadAdminOrganizationProvisioningMaterialRequest({
    client: apiClient,
    path: { request_key: requestKey, material_id: materialId },
    parseAs: 'blob',
  }) as unknown as { data: Blob; response: Response }
  return { blob: result.data, response: result.response }
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
