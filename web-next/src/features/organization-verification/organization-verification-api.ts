import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { apiClient, ApiError } from '@/lib/api'
import {
  decideOrganizationVerification,
  downloadAdminOrganizationVerificationMaterial as downloadAdminOrganizationVerificationMaterialRequest,
  getOrganizationVerification,
  listAdminOrganizationVerifications,
} from '@/lib/api/generated/sdk.gen'
import type {
  OrganizationVerificationDecisionRequest,
  OrganizationVerificationListResponse,
  OrganizationVerificationResponse,
  OrganizationVerificationStatus,
} from '@/lib/api/generated/types.gen'

export type {
  OrganizationVerificationDecisionRequest,
  OrganizationVerificationListResponse,
  OrganizationVerificationResponse,
  OrganizationVerificationStatus,
}

export type OrganizationVerificationMaterialDraft = {
  kind: string
  file?: File
}

const verificationRoot = ['organization-verification'] as const

export function useOrganizationVerification(organizationId: number, enabled = true) {
  return useQuery({
    queryKey: [...verificationRoot, organizationId],
    enabled,
    retry: false,
    queryFn: async ({ signal }) => {
      try {
        const { data } = await getOrganizationVerification({ client: apiClient, path: { organization_id: organizationId }, signal })
        return data
      } catch (error) {
        if (error instanceof ApiError && error.status === 404) return null
        throw error
      }
    },
  })
}

export function useSubmitOrganizationVerification(organizationId: number) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (body: { summary: string; materials: OrganizationVerificationMaterialDraft[] }) => {
      const form = new FormData()
      form.append('summary', body.summary)
      form.append('materials', JSON.stringify(body.materials.map((material, index) => ({ kind: material.kind.trim(), file_field: String(index) }))))
      body.materials.forEach((material, index) => {
        if (material.file) form.append(`file:${index}`, material.file, material.file.name)
      })
      const result = await apiClient.post<OrganizationVerificationResponse>({
        url: '/api/organizations/{organization_id}/verification',
        path: { organization_id: organizationId },
        body: form,
        // FormData 包含动态 file:<index> 部件，必须保留 multipart 边界，不能走默认 JSON 序列化。
        bodySerializer: null,
        headers: { 'Content-Type': null },
        security: [{ key: 'bearerAuth', scheme: 'bearer', type: 'http' }],
      }) as unknown as { data: OrganizationVerificationResponse }
      return result.data
    },
    onSuccess: (data) => queryClient.setQueryData([...verificationRoot, organizationId], data),
  })
}

/** 使用平台管理员权限下载单份材料，正文不写入 React Query 或浏览器持久化。 */
export async function downloadAdminOrganizationVerificationMaterial(caseId: number, materialId: number) {
  const result = await downloadAdminOrganizationVerificationMaterialRequest({
    client: apiClient,
    path: { case_id: caseId, material_id: materialId },
    parseAs: 'blob',
  }) as unknown as { data: Blob; response: Response }
  return { blob: result.data, response: result.response }
}

export function useAdminOrganizationVerifications(status?: OrganizationVerificationStatus) {
  return useInfiniteQuery({
    queryKey: [...verificationRoot, 'admin', status ?? 'all'],
    initialPageParam: undefined as number | undefined,
    queryFn: async ({ pageParam, signal }) => {
      const { data } = await listAdminOrganizationVerifications({ client: apiClient, query: { status, before: pageParam, limit: 50 }, signal })
      return data
    },
    getNextPageParam: (page) => page.next_cursor ?? undefined,
    retry: false,
  })
}

export function useDecideOrganizationVerification() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async ({ caseId, body }: { caseId: number; body: OrganizationVerificationDecisionRequest }) => {
      const { data } = await decideOrganizationVerification({ client: apiClient, path: { case_id: caseId }, body })
      return data
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: [...verificationRoot, 'admin'] }),
  })
}

export function organizationVerificationErrorCode(error: unknown) {
  if (!(error instanceof ApiError) || typeof error.details !== 'object' || error.details === null) return undefined
  const code = (error.details as { code?: unknown }).code
  return typeof code === 'string' ? code : undefined
}
