import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { apiClient } from '@/lib/api'
import {
  getAdminAnalyticsExportStatus,
  replayAdminAnalyticsExport,
} from '@/lib/api/generated/sdk.gen'

/** 读取分析导出状态，独立于看板业务指标缓存。 */
export function useAnalyticsExportStatus() {
  return useQuery({
    queryKey: ['admin-analytics-export-status'],
    queryFn: async ({ signal }) => {
      const { data } = await getAdminAnalyticsExportStatus({ client: apiClient, signal })
      return data
    },
  })
}
/** 有界重放 outbox，成功后刷新状态而不刷新看板历史快照。 */
export function useReplayAnalyticsExport() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (limit: number) => {
      const { data } = await replayAdminAnalyticsExport({
        client: apiClient,
        body: { limit },
      })
      return data
    },
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['admin-analytics-export-status'] })
    },
  })
}
