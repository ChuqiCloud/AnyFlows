import { useQuery } from '@tanstack/react-query'

import { apiClient, ApiError } from '@/lib/api'
import {
  listOrganizationAuditLogs,
  listOrganizationUsageLogs,
} from '@/lib/api/generated/sdk.gen'

export type OrganizationUsageScope = 'organization' | 'self' | 'summary'

export type OrganizationAuditFilters = {
  actorUserId?: number
  operation?: string
  targetType?: string
  outcome?: 'succeeded' | 'denied' | 'failed'
}

function organizationActivityQueryKey(
  organizationId: number,
  resource: 'audit' | 'usage',
  parameters: object,
) {
  return ['organization', organizationId, 'activity', resource, parameters] as const
}

/** 按企业、角色裁剪范围和稳定游标读取调用日志。 */
export function useOrganizationUsageLogs(
  organizationId: number,
  scope: OrganizationUsageScope,
  before: number | undefined,
  limit: number,
  enabled: boolean,
) {
  return useQuery({
    queryKey: organizationActivityQueryKey(organizationId, 'usage', {
      before: before ?? null,
      limit,
      scope,
    }),
    enabled,
    queryFn: async ({ signal }) => {
      const { data } = await listOrganizationUsageLogs({
        client: apiClient,
        path: { organization_id: organizationId },
        query: { before, limit, scope },
        signal,
      })
      return data
    },
  })
}

/** 企业审计筛选全部由服务端执行，避免客户端误把当前页当作完整结果。 */
export function useOrganizationAuditLogs(
  organizationId: number,
  before: number | undefined,
  limit: number,
  filters: OrganizationAuditFilters,
  enabled: boolean,
) {
  return useQuery({
    queryKey: organizationActivityQueryKey(organizationId, 'audit', {
      before: before ?? null,
      filters,
      limit,
    }),
    enabled,
    queryFn: async ({ signal }) => {
      const { data } = await listOrganizationAuditLogs({
        client: apiClient,
        path: { organization_id: organizationId },
        query: {
          actor_user_id: filters.actorUserId,
          before,
          limit,
          operation: filters.operation,
          outcome: filters.outcome,
          target_type: filters.targetType,
        },
        signal,
      })
      return data
    },
  })
}

export function organizationActivityErrorStatus(error: unknown) {
  return error instanceof ApiError ? error.status : undefined
}
