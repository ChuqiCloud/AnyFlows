import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { organizationQueryKey } from '@/features/organization-budgets/organization-budget-api'
import { apiClient, ApiError } from '@/lib/api'
import {
  createOrganizationApprovalRequest,
  createOrganizationApprovalTemplate,
  decideOrganizationApprovalRequest,
  getOrganizationApprovalRequest,
  listOrganizationApprovalRequests,
  listOrganizationApprovalTemplates,
} from '@/lib/api/generated/sdk.gen'
import type {
  OrganizationApprovalDecisionRequest,
  OrganizationApprovalRequestCreateRequest,
  OrganizationApprovalTemplateCreateRequest,
} from '@/lib/api/generated/types.gen'

/** 读取企业审批模板版本；是否启用由服务端概览权限投影决定。 */
export function useOrganizationApprovalTemplates(organizationId: number, enabled: boolean) {
  return useQuery({
    queryKey: organizationQueryKey(organizationId, 'approval-templates'),
    enabled,
    queryFn: async ({ signal }) => {
      const { data } = await listOrganizationApprovalTemplates({
        client: apiClient,
        path: { organization_id: organizationId },
        signal,
      })
      return data
    },
  })
}

/** 读取企业审批申请；普通成员由服务端裁剪为自己的申请。 */
export function useOrganizationApprovalRequests(organizationId: number, enabled: boolean) {
  return useQuery({
    queryKey: organizationQueryKey(organizationId, 'approval-requests'),
    enabled,
    queryFn: async ({ signal }) => {
      const { data } = await listOrganizationApprovalRequests({
        client: apiClient,
        path: { organization_id: organizationId },
        signal,
      })
      return data
    },
  })
}

/** 读取单个申请详情，冲突后由服务端事实重新确认。 */
export function useOrganizationApprovalRequest(organizationId: number, requestId: number | undefined, enabled: boolean) {
  return useQuery({
    queryKey: organizationQueryKey(organizationId, 'approval-request', requestId),
    enabled: enabled && requestId !== undefined,
    queryFn: async ({ signal }) => {
      const { data } = await getOrganizationApprovalRequest({
        client: apiClient,
        path: { organization_id: organizationId, request_id: requestId as number },
        signal,
      })
      return data
    },
  })
}

/** 创建模板版本后刷新模板与概览缓存，不做乐观写入。 */
export function useCreateOrganizationApprovalTemplate(organizationId: number) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (body: OrganizationApprovalTemplateCreateRequest) => {
      const { data } = await createOrganizationApprovalTemplate({
        body,
        client: apiClient,
        path: { organization_id: organizationId },
      })
      return data
    },
    onSuccess: () => invalidateApprovalQueries(queryClient, organizationId),
  })
}

/** 创建审批申请，服务端以请求键保证重试幂等。 */
export function useCreateOrganizationApprovalRequest(organizationId: number) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (body: OrganizationApprovalRequestCreateRequest) => {
      const { data } = await createOrganizationApprovalRequest({
        body,
        client: apiClient,
        path: { organization_id: organizationId },
      })
      return data
    },
    onSuccess: () => invalidateApprovalQueries(queryClient, organizationId),
  })
}

/** 提交审批决定时携带服务端决定键，冲突保持失败关闭。 */
export function useDecideOrganizationApprovalRequest(organizationId: number) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async ({ requestId, body }: { requestId: number; body: OrganizationApprovalDecisionRequest }) => {
      const { data } = await decideOrganizationApprovalRequest({
        body,
        client: apiClient,
        path: { organization_id: organizationId, request_id: requestId },
      })
      return data
    },
    onSuccess: () => invalidateApprovalQueries(queryClient, organizationId),
  })
}

export function organizationApprovalErrorStatus(error: unknown) {
  return error instanceof ApiError ? error.status : undefined
}

export function organizationApprovalErrorCode(error: unknown) {
  if (!(error instanceof ApiError) || typeof error.details !== 'object' || error.details === null) return undefined
  const code = (error.details as { code?: unknown }).code
  return typeof code === 'string' ? code : undefined
}

function invalidateApprovalQueries(queryClient: ReturnType<typeof useQueryClient>, organizationId: number) {
  return Promise.all([
    queryClient.invalidateQueries({ queryKey: organizationQueryKey(organizationId, 'approval-templates') }),
    queryClient.invalidateQueries({ queryKey: organizationQueryKey(organizationId, 'approval-requests') }),
    queryClient.invalidateQueries({ queryKey: organizationQueryKey(organizationId, 'approval-request') }),
    queryClient.invalidateQueries({ queryKey: organizationQueryKey(organizationId, 'overview') }),
  ])
}
