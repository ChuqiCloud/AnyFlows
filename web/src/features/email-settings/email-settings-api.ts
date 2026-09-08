import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { apiClient, ApiError } from '@/lib/api'
import {
  getAdminEmailSettings,
  sendAdminEmailTest,
  updateAdminEmailSettings,
} from '@/lib/api/generated/sdk.gen'
import type {
  AdminEmailSettingsRequestWritable,
  AdminEmailTestRequest,
} from '@/lib/api/generated/types.gen'

export const adminEmailSettingsQueryKey = ['admin-email-settings'] as const

/** 读取管理员完整 SMTP 设置，服务端只返回密码配置状态。 */
export function useAdminEmailSettings() {
  return useQuery({
    queryKey: adminEmailSettingsQueryKey,
    queryFn: async ({ signal }) => {
      const { data } = await getAdminEmailSettings({ client: apiClient, signal })
      return data
    },
  })
}

/** 原子覆盖完整 SMTP 设置，并立即同步当前查询缓存。 */
export function useUpdateAdminEmailSettings() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (body: AdminEmailSettingsRequestWritable) => {
      const { data } = await updateAdminEmailSettings({ body, client: apiClient })
      return data
    },
    onSuccess: (settings) => {
      queryClient.setQueryData(adminEmailSettingsQueryKey, settings)
    },
  })
}

/** 使用当前已保存快照发送固定正文测试邮件。 */
export function useSendAdminEmailTest() {
  return useMutation({
    mutationFn: async (body: AdminEmailTestRequest) => {
      await sendAdminEmailTest({ body, client: apiClient })
    },
  })
}

/** 判断管理 API 是否返回预期的稳定错误码。 */
export function isEmailSettingsError(error: unknown, code: string) {
  if (!(error instanceof ApiError) || typeof error.details !== 'object' || error.details === null) {
    return false
  }
  return 'code' in error.details && error.details.code === code
}
