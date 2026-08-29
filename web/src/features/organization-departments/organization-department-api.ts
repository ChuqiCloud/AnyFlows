import { useInfiniteQuery, useMutation, useQueryClient } from '@tanstack/react-query'

import { organizationQueryKey } from '@/features/organization-budgets/organization-budget-api'
import { apiClient, ApiError } from '@/lib/api'
import {
  createOrganizationDepartment,
  disableOrganizationDepartment,
  listOrganizationDepartments,
  moveOrganizationDepartment,
} from '@/lib/api/generated/sdk.gen'
import type {
  OrganizationDepartmentCreateRequest,
  OrganizationDepartmentDisableRequest,
  OrganizationDepartmentMoveRequest,
} from '@/lib/api/generated/types.gen'

/** 读取企业部门目录；游标由服务端返回的 sort_order:id 原样传递。 */
export function useOrganizationDepartments(organizationId: number, enabled = true) {
  return useInfiniteQuery({
    queryKey: organizationQueryKey(organizationId, 'departments', 'pages'),
    enabled,
    initialPageParam: undefined as string | undefined,
    queryFn: async ({ pageParam, signal }) => {
      const { data } = await listOrganizationDepartments({
        client: apiClient,
        path: { organization_id: organizationId },
        query: { after: pageParam, limit: 25 },
        signal,
      })
      return data
    },
    getNextPageParam: (page) => page.next_cursor ?? undefined,
  })
}

/** 创建部门后刷新部门目录与企业概览容量。 */
export function useCreateOrganizationDepartment(organizationId: number) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (body: OrganizationDepartmentCreateRequest) => {
      const { data } = await createOrganizationDepartment({
        body,
        client: apiClient,
        path: { organization_id: organizationId },
      })
      return data
    },
    onSuccess: () => invalidateDepartments(queryClient, organizationId),
  })
}

/** 移动部门子树时携带服务端返回的版本，冲突不做乐观覆盖。 */
export function useMoveOrganizationDepartment(organizationId: number) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async ({ departmentId, body }: { departmentId: number; body: OrganizationDepartmentMoveRequest }) => {
      const { data } = await moveOrganizationDepartment({
        body,
        client: apiClient,
        path: { department_id: departmentId, organization_id: organizationId },
      })
      return data
    },
    onSuccess: () => invalidateDepartments(queryClient, organizationId),
  })
}

/** 部门禁用为不可逆软状态，成功后同步目录和企业容量投影。 */
export function useDisableOrganizationDepartment(organizationId: number) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async ({ departmentId, body }: { departmentId: number; body: OrganizationDepartmentDisableRequest }) => {
      const { data } = await disableOrganizationDepartment({
        body,
        client: apiClient,
        path: { department_id: departmentId, organization_id: organizationId },
      })
      return data
    },
    onSuccess: () => invalidateDepartments(queryClient, organizationId),
  })
}

export function organizationDepartmentErrorStatus(error: unknown) {
  return error instanceof ApiError ? error.status : undefined
}

function invalidateDepartments(queryClient: ReturnType<typeof useQueryClient>, organizationId: number) {
  return Promise.all([
    queryClient.invalidateQueries({ queryKey: organizationQueryKey(organizationId, 'departments') }),
    queryClient.invalidateQueries({ queryKey: organizationQueryKey(organizationId, 'overview') }),
  ])
}
