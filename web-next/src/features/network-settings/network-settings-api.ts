import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { apiClient } from '@/lib/api'
import {
  getAdminNetworkSettings,
  updateAdminNetworkSettings,
} from '@/lib/api/generated/sdk.gen'
import type { AdminNetworkSettingsRequestWritable } from '@/lib/api/generated/types.gen'

export const adminNetworkSettingsQueryKey = ['admin-network-settings'] as const

/** 读取全局出站网络设置，服务端只返回代理密码配置状态。 */
export function useAdminNetworkSettings() {
  return useQuery({
    queryKey: adminNetworkSettingsQueryKey,
    queryFn: async ({ signal }) => {
      const { data } = await getAdminNetworkSettings({ client: apiClient, signal })
      return data
    },
  })
}

/** 原子保存网络设置，并让当前管理页立即使用最新缓存。 */
export function useUpdateAdminNetworkSettings() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (body: AdminNetworkSettingsRequestWritable) => {
      const { data } = await updateAdminNetworkSettings({ body, client: apiClient })
      return data
    },
    onSuccess: (settings) => {
      queryClient.setQueryData(adminNetworkSettingsQueryKey, settings)
    },
  })
}
