import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { apiClient } from '@/lib/api'
import {
  getAdminDebugTrace,
  getAdminDebugTraceSettings,
  listAdminDebugTraces,
  updateAdminDebugTraceSettings,
} from '@/lib/api/generated/sdk.gen'
import type { AdminDebugTraceSettingsRequest } from '@/lib/api/generated/types.gen'

export type DebugTraceFilters = {
  outcome?: 'succeeded' | 'failed'
  model?: string
  requestId?: string
}

export const adminDebugTraceSettingsQueryKey = ['admin-debug-trace-settings'] as const

/** 读取当前实例已经应用的调试追踪设置。 */
export function useAdminDebugTraceSettings() {
  return useQuery({
    queryKey: adminDebugTraceSettingsQueryKey,
    queryFn: async ({ signal }) => {
      const { data } = await getAdminDebugTraceSettings({ client: apiClient, signal })
      return data
    },
  })
}

/** 保存设置后同步替换当前页面缓存。 */
export function useUpdateAdminDebugTraceSettings() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (body: AdminDebugTraceSettingsRequest) => {
      const { data } = await updateAdminDebugTraceSettings({ body, client: apiClient })
      return data
    },
    onSuccess: (settings) => {
      queryClient.setQueryData(adminDebugTraceSettingsQueryKey, settings)
    },
  })
}

/** 按稳定倒序游标读取一页脱敏追踪摘要。 */
export function useAdminDebugTraces(before: number | undefined, filters: DebugTraceFilters, pageSize: number) {
  return useQuery({
    queryKey: ['admin-debug-traces', { before, pageSize, ...filters }],
    queryFn: async ({ signal }) => {
      const { data } = await listAdminDebugTraces({
        client: apiClient,
        query: {
          before,
          limit: pageSize,
          outcome: filters.outcome,
          model: filters.model,
          request_id: filters.requestId,
        },
        signal,
      })
      return data
    },
  })
}

/** 仅在选中有效追踪后读取候选时间线。 */
export function useAdminDebugTrace(traceId?: number) {
  return useQuery({
    queryKey: ['admin-debug-trace', traceId],
    enabled: traceId !== undefined,
    queryFn: async ({ signal }) => {
      const { data } = await getAdminDebugTrace({
        client: apiClient,
        path: { id: traceId as number },
        signal,
      })
      return data
    },
  })
}
