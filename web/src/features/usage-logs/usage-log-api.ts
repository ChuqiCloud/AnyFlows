import { useQuery } from '@tanstack/react-query'

import { apiClient } from '@/lib/api'
import { listAdminUsageLogs, listUserUsageLogs } from '@/lib/api/generated/sdk.gen'

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

/** 当前用户接口由服务端会话强制限定 owner，不接收客户端 user_id。 */
export function useUserUsageLogs(before?: number) {
  return useQuery({
    queryKey: ['user-usage-logs', { before }],
    queryFn: async ({ signal }) => {
      const { data } = await listUserUsageLogs({
        client: apiClient,
        query: { before, limit: PAGE_SIZE },
        signal,
      })
      return data
    },
  })
}
