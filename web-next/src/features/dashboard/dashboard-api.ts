import { useQuery } from '@tanstack/react-query'

import { apiClient } from '@/lib/api'
import { getAdminDashboard } from '@/lib/api/generated/sdk.gen'

/** 读取管理员最近 24 小时看板快照，刷新时保留当前数据避免界面跳变。 */
export function useAdminDashboard() {
  return useQuery({
    queryKey: ['admin-dashboard'],
    queryFn: async ({ signal }) => {
      const { data } = await getAdminDashboard({ client: apiClient, signal })
      return data
    },
  })
}
