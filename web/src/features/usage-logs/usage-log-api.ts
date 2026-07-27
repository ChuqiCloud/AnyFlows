import { useQuery } from '@tanstack/react-query'

import { apiClient } from '@/lib/api'
import { listAdminUsageLogs } from '@/lib/api/generated/sdk.gen'

const PAGE_SIZE = 25

/** 按倒序日志 ID 游标读取固定页，刷新期间不会重复或漏读已落库记录。 */
export function useAdminUsageLogs(before?: number) {
  return useQuery({
    queryKey: ['admin-usage-logs', { before }],
    queryFn: async ({ signal }) => {
      const { data } = await listAdminUsageLogs({
        client: apiClient,
        query: { before, limit: PAGE_SIZE },
        signal,
      })
      return data
    },
  })
}
