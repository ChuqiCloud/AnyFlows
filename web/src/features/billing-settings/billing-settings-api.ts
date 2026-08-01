import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { apiClient } from '@/lib/api'
import {
  getAdminBalanceAlertSettings,
  updateAdminBalanceAlertSettings,
} from '@/lib/api/generated/sdk.gen'
import type { AdminBalanceAlertSettingsRequest } from '@/lib/api/generated/types.gen'

export const adminBalanceAlertSettingsQueryKey = ['admin-balance-alert-settings'] as const

/** 读取余额预警全局设置。 */
export function useAdminBalanceAlertSettings() {
  return useQuery({
    queryKey: adminBalanceAlertSettingsQueryKey,
    queryFn: async ({ signal }) => {
      const { data } = await getAdminBalanceAlertSettings({ client: apiClient, signal })
      return data
    },
  })
}

/** 保存完整余额预警设置并同步当前缓存。 */
export function useUpdateAdminBalanceAlertSettings() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (body: AdminBalanceAlertSettingsRequest) => {
      const { data } = await updateAdminBalanceAlertSettings({ body, client: apiClient })
      return data
    },
    onSuccess: (settings) => {
      queryClient.setQueryData(adminBalanceAlertSettingsQueryKey, settings)
    },
  })
}
