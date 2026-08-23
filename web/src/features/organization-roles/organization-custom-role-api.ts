import { useInfiniteQuery, useMutation, useQueryClient } from '@tanstack/react-query'

import { organizationQueryKey } from '@/features/organization-budgets/organization-budget-api'
import { apiClient, ApiError } from '@/lib/api'
import {
  createOrganizationCustomRole,
  disableOrganizationCustomRole,
  listOrganizationCustomRoles,
  updateOrganizationCustomRole,
} from '@/lib/api/generated/sdk.gen'
import type {
  OrganizationCustomRoleChangeRequest,
  OrganizationCustomRoleCreateRequest,
} from '@/lib/api/generated/types.gen'

/** 读取企业内可见的自定义角色目录，按稳定游标持续加载。 */
export function useOrganizationCustomRoles(organizationId: number, enabled = true) {
  return useInfiniteQuery({
    queryKey: organizationQueryKey(organizationId, 'custom-roles', 'pages'),
    enabled,
    initialPageParam: undefined as number | undefined,
    queryFn: async ({ pageParam, signal }) => {
      const { data } = await listOrganizationCustomRoles({
        client: apiClient,
        path: { organization_id: organizationId },
        query: { after: pageParam, limit: 100 },
        signal,
      })
      return data
    },
    getNextPageParam: (page) => page.next_cursor ?? undefined,
  })
}

/** 创建角色后刷新目录及成员投影，避免分配列表使用过期角色。 */
export function useCreateOrganizationCustomRole(organizationId: number) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (body: OrganizationCustomRoleCreateRequest) => {
      const { data } = await createOrganizationCustomRole({
        body,
        client: apiClient,
        path: { organization_id: organizationId },
      })
      return data
    },
    onSuccess: () => invalidateOrganizationCustomRoles(queryClient, organizationId),
  })
}

/** 使用服务端版本号完整替换活动自定义角色定义。 */
export function useUpdateOrganizationCustomRole(organizationId: number) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async ({ roleKey, body }: { roleKey: string; body: OrganizationCustomRoleChangeRequest }) => {
      const { data } = await updateOrganizationCustomRole({
        body,
        client: apiClient,
        path: { organization_id: organizationId, role_key: roleKey },
      })
      return data
    },
    onSuccess: () => invalidateOrganizationCustomRoles(queryClient, organizationId),
  })
}

/** 按 CAS 版本不可逆停用角色，保留审计所需的目录事实。 */
export function useDisableOrganizationCustomRole(organizationId: number) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async ({ roleKey, expectedVersion }: { roleKey: string; expectedVersion: number }) => {
      const { data } = await disableOrganizationCustomRole({
        body: { expected_version: expectedVersion },
        client: apiClient,
        path: { organization_id: organizationId, role_key: roleKey },
      })
      return data
    },
    onSuccess: () => invalidateOrganizationCustomRoles(queryClient, organizationId),
  })
}

/** 提取角色接口的闭合公开错误码，供界面显示可恢复提示。 */
export function organizationCustomRoleErrorCode(error: unknown) {
  if (!(error instanceof ApiError) || typeof error.details !== 'object' || error.details === null) {
    return undefined
  }
  const code = (error.details as { code?: unknown }).code
  return typeof code === 'string' ? code : undefined
}

function invalidateOrganizationCustomRoles(queryClient: ReturnType<typeof useQueryClient>, organizationId: number) {
  return Promise.all([
    queryClient.invalidateQueries({ queryKey: organizationQueryKey(organizationId, 'custom-roles') }),
    queryClient.invalidateQueries({ queryKey: organizationQueryKey(organizationId, 'members') }),
    queryClient.invalidateQueries({ queryKey: organizationQueryKey(organizationId, 'overview') }),
  ])
}
