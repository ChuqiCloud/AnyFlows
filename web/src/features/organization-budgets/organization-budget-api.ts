import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import type { QueryClient } from '@tanstack/react-query'

import { apiClient, ApiError } from '@/lib/api'
import {
  createOrganizationBudgetPolicy,
  getOrganizationBudgets,
  getOrganizationOverview,
  listAccountOrganizations,
  listOrganizationMembers,
  listOrganizationTeams,
} from '@/lib/api/generated/sdk.gen'
import type {
  OrganizationBudgetPolicyCreateRequest,
  OrganizationMembershipResponse,
  OrganizationTeamResponse,
} from '@/lib/api/generated/types.gen'

export const organizationDirectoryQueryKey = ['organization-directory'] as const
const organizationQueryRoot = ['organization'] as const

export function organizationQueryKey(organizationId: number, ...resources: readonly unknown[]) {
  return [...organizationQueryRoot, organizationId, ...resources] as const
}

/** 读取当前账号可进入的企业空间，不把平台管理员身份当作企业成员。 */
export function useOrganizationDirectory() {
  return useQuery({
    queryKey: organizationDirectoryQueryKey,
    queryFn: async ({ signal }) => {
      const { data } = await listAccountOrganizations({ client: apiClient, signal })
      return data.organizations
    },
  })
}

/** 先读取服务端权限投影，预算查询和编辑都依赖该结果。 */
export function useOrganizationOverview(organizationId: number, enabled = true) {
  return useQuery({
    queryKey: organizationQueryKey(organizationId, 'overview'),
    enabled,
    queryFn: async ({ signal }) => {
      const { data } = await getOrganizationOverview({
        client: apiClient,
        path: { organization_id: organizationId },
        signal,
      })
      return data
    },
  })
}

/** 仅在服务端权限投影允许时读取策略历史与当前窗口。 */
export function useOrganizationBudgets(organizationId: number, enabled: boolean) {
  return useQuery({
    queryKey: organizationQueryKey(organizationId, 'budgets'),
    enabled,
    queryFn: async ({ signal }) => {
      const { data } = await getOrganizationBudgets({
        client: apiClient,
        path: { organization_id: organizationId },
        signal,
      })
      return data
    },
  })
}

/** 遍历完整团队目录，为预算作用域提供真实且稳定的选择项。 */
export function useOrganizationTeamCatalog(organizationId: number, enabled: boolean) {
  return useQuery({
    queryKey: organizationQueryKey(organizationId, 'teams'),
    enabled,
    queryFn: ({ signal }) => loadOrganizationTeams(organizationId, signal),
  })
}

/** 遍历完整成员目录，策略写入只允许选择活动成员。 */
export function useOrganizationMemberCatalog(organizationId: number, enabled: boolean) {
  return useQuery({
    queryKey: organizationQueryKey(organizationId, 'members'),
    enabled,
    queryFn: ({ signal }) => loadOrganizationMembers(organizationId, signal),
  })
}

/** 创建不可变策略版本，成功后重新读取服务端生成的历史与窗口。 */
export function useCreateOrganizationBudgetPolicy(organizationId: number) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (body: OrganizationBudgetPolicyCreateRequest) => {
      const { data } = await createOrganizationBudgetPolicy({
        body,
        client: apiClient,
        path: { organization_id: organizationId },
      })
      return data
    },
    onSuccess: () => queryClient.invalidateQueries({
      queryKey: organizationQueryKey(organizationId, 'budgets'),
    }),
  })
}

/** 切换企业前取消旧请求并移除全部组织级缓存，避免跨企业短暂串屏。 */
export async function clearOrganizationWorkspaceCache(
  queryClient: QueryClient,
  organizationId: number,
) {
  const queryKey = [...organizationQueryRoot, organizationId] as const
  await queryClient.cancelQueries({ queryKey })
  queryClient.removeQueries({ queryKey })
}

/** 提取预算接口的稳定公开错误码，包含生成契约尚未收窄的预算写入错误。 */
export function organizationBudgetErrorCode(error: unknown) {
  if (!(error instanceof ApiError) || typeof error.details !== 'object' || error.details === null) {
    return undefined
  }
  const code = (error.details as { code?: unknown }).code
  return typeof code === 'string' ? code : undefined
}

export function organizationBudgetErrorStatus(error: unknown) {
  return error instanceof ApiError ? error.status : undefined
}

async function loadOrganizationTeams(organizationId: number, signal: AbortSignal) {
  const teams: OrganizationTeamResponse[] = []
  const seenCursors = new Set<number>()
  let after: number | undefined
  while (true) {
    const { data } = await listOrganizationTeams({
      client: apiClient,
      path: { organization_id: organizationId },
      query: { after, limit: 100 },
      signal,
    })
    teams.push(...data.teams)
    const nextCursor = data.next_cursor ?? undefined
    if (nextCursor === undefined) return teams
    if (seenCursors.has(nextCursor)) throw new Error('企业团队目录返回了重复游标')
    seenCursors.add(nextCursor)
    after = nextCursor
  }
}

async function loadOrganizationMembers(organizationId: number, signal: AbortSignal) {
  const members: OrganizationMembershipResponse[] = []
  const seenCursors = new Set<number>()
  let after: number | undefined
  while (true) {
    const { data } = await listOrganizationMembers({
      client: apiClient,
      path: { organization_id: organizationId },
      query: { after, limit: 100 },
      signal,
    })
    members.push(...data.members)
    const nextCursor = data.next_cursor ?? undefined
    if (nextCursor === undefined) return members
    if (seenCursors.has(nextCursor)) throw new Error('企业成员目录返回了重复游标')
    seenCursors.add(nextCursor)
    after = nextCursor
  }
}
