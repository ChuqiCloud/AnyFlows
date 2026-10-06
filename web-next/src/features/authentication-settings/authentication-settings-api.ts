import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { apiClient } from '@/lib/api'
import {
  getAdminAuthenticationSettings,
  updateAdminAuthenticationSettings,
} from '@/lib/api/generated/sdk.gen'
import type {
  AdminAuthenticationSettingsRequest,
  PublicSiteSettings,
} from '@/lib/api/generated/types.gen'
import { publicSiteSettingsQueryKey } from '@/features/site-settings/site-settings-api'

export const adminAuthenticationSettingsQueryKey = ['admin-authentication-settings'] as const

/** 读取管理员完整认证与注册设置。 */
export function useAdminAuthenticationSettings() {
  return useQuery({
    queryKey: adminAuthenticationSettingsQueryKey,
    queryFn: async ({ signal }) => {
      const { data } = await getAdminAuthenticationSettings({ client: apiClient, signal })
      return data
    },
  })
}

/** 原子覆盖认证设置，并同步公开能力缓存。 */
export function useUpdateAdminAuthenticationSettings() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: async (body: AdminAuthenticationSettingsRequest) => {
      const { data } = await updateAdminAuthenticationSettings({ body, client: apiClient })
      return data
    },
    onSuccess: (settings) => {
      queryClient.setQueryData(adminAuthenticationSettingsQueryKey, settings)
      queryClient.setQueryData<PublicSiteSettings>(publicSiteSettingsQueryKey, (current) => (
        current
          ? {
              ...current,
              authentication: {
                ...current.authentication,
                password_login_enabled: settings.password_login_enabled,
                registration_enabled: settings.registration_enabled,
                registration_email_required: settings.email_required,
              },
            }
          : current
      ))
      void queryClient.invalidateQueries({ queryKey: publicSiteSettingsQueryKey })
    },
  })
}
