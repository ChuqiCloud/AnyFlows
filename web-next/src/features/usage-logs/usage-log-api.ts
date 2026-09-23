import { useQuery } from '@tanstack/react-query'

import { DEFAULT_TABLE_PAGE_SIZE } from '@/components/data-table/data-table-pagination'
import { apiClient } from '@/lib/api'
import { listAdminUsageLogs, listUserUsageLogs } from '@/lib/api/generated/sdk.gen'

/** 按倒序日志 ID 游标读取固定页，刷新期间不会重复或漏读已落库记录。 */
export function useAdminUsageLogs(
  before?: number,
  failedBefore?: number,
  pageSize = DEFAULT_TABLE_PAGE_SIZE,
  enabled = true,
) {
  return useQuery({
    queryKey: ['admin-usage-logs', { before, failedBefore, pageSize }],
    enabled,
    refetchInterval: 10_000,
    queryFn: async ({ signal }) => {
      const { data } = await listAdminUsageLogs({
        client: apiClient,
        query: { before, failed_before: failedBefore, limit: pageSize },
        signal,
      })
      return data
    },
  })
}

/** 当前用户接口由服务端会话强制限定 owner，不接收客户端 user_id。 */
export function useUserUsageLogs(
  before?: number,
  failedBefore?: number,
  pageSize = DEFAULT_TABLE_PAGE_SIZE,
  enabled = true,
) {
  return useQuery({
    queryKey: ['user-usage-logs', { before, failedBefore, pageSize }],
    enabled,
    refetchInterval: 10_000,
    queryFn: async ({ signal }) => {
      const { data } = await listUserUsageLogs({
        client: apiClient,
        query: { before, failed_before: failedBefore, limit: pageSize },
        signal,
      })
      return data
    },
  })
}
