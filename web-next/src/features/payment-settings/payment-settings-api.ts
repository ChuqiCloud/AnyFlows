import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { apiClient } from '@/lib/api'
import type {
  AdminPaymentSettings,
  AdminPaymentSettingsRequest,
} from './payment-settings-types'

export const adminPaymentSettingsQueryKey = ['admin-payment-settings'] as const

const bearerSecurity = [{ key: 'bearerAuth', scheme: 'bearer', type: 'http' }] as const

/** 读取支付配置的脱敏管理视图。 */
export function useAdminPaymentSettings() {
  return useQuery({
    queryKey: adminPaymentSettingsQueryKey,
    queryFn: async ({ signal }) => {
      const { data } = await apiClient.get<{ 200: AdminPaymentSettings }, unknown, true>({
        security: bearerSecurity,
        signal,
        throwOnError: true,
        url: '/api/admin/payment-settings',
      })
      return data
    },
  })
}

/** 原子保存支付配置，并以服务端返回快照覆盖当前缓存。 */
export function useUpdateAdminPaymentSettings() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (body: AdminPaymentSettingsRequest) => {
      const { data } = await apiClient.put<{ 200: AdminPaymentSettings }, unknown, true>({
        body,
        headers: { 'Content-Type': 'application/json' },
        security: bearerSecurity,
        throwOnError: true,
        url: '/api/admin/payment-settings',
      })
      return data
    },
    onSuccess: (settings) => {
      queryClient.setQueryData(adminPaymentSettingsQueryKey, settings)
    },
  })
}
