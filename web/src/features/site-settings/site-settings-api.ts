import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { apiClient, ApiError } from '@/lib/api'
import {
  getAdminSiteSettings,
  getPublicSiteSettings,
  updateAdminSiteSettings,
  updateAdminSiteNavigation,
} from '@/lib/api/generated/sdk.gen'
import type {
  AdminSiteSettingsRequest,
  AdminSiteNavigationRequest,
  ManagementError,
  PublicSiteSettings,
} from '@/lib/api/generated/types.gen'

export const publicSiteSettingsQueryKey = ['public-site-settings'] as const
export const adminSiteSettingsQueryKey = ['admin-site-settings'] as const

/** 读取游客可见的站点身份、品牌与认证能力。 */
export function usePublicSiteSettings() {
  return useQuery({
    queryKey: publicSiteSettingsQueryKey,
    staleTime: 30_000,
    retry: false,
    queryFn: async ({ signal }) => {
      const { data } = await getPublicSiteSettings({ client: apiClient, signal })
      return data
    },
  })
}

/** 读取管理员可编辑的完整站点设置。 */
export function useAdminSiteSettings() {
  return useQuery({
    queryKey: adminSiteSettingsQueryKey,
    queryFn: async ({ signal }) => {
      const { data } = await getAdminSiteSettings({ client: apiClient, signal })
      return data
    },
  })
}

/** 原子覆盖站点设置，并同步已存在的公开品牌缓存。 */
export function useUpdateAdminSiteSettings() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: async (body: AdminSiteSettingsRequest) => {
      const { data } = await updateAdminSiteSettings({ body, client: apiClient })
      return data
    },
    onSuccess: (settings) => {
      queryClient.setQueryData(adminSiteSettingsQueryKey, settings)
      queryClient.setQueryData<PublicSiteSettings>(publicSiteSettingsQueryKey, (current) => (
        current
          ? {
              ...current,
              site_name: settings.site_name,
              public_base_url: settings.public_base_url,
              brand: settings.brand,
              balance_display: settings.balance_display,
            }
          : current
      ))
      void queryClient.invalidateQueries({ queryKey: publicSiteSettingsQueryKey })
    },
  })
}

export function useUpdateSiteNavigation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (body: AdminSiteNavigationRequest) => {
      const { data } = await updateAdminSiteNavigation({ body, client: apiClient })
      return data
    },
    onSuccess: (settings) => {
      queryClient.setQueryData(adminSiteSettingsQueryKey, settings)
      queryClient.setQueryData<PublicSiteSettings>(publicSiteSettingsQueryKey, (current) => current ? {
        ...current, navigation: settings.navigation,
      } : current)
    },
  })
}

/** 提取站点设置写入的稳定管理错误码。 */
export function siteSettingsErrorCode(error: unknown): ManagementError['code'] | undefined {
  if (!(error instanceof ApiError) || typeof error.details !== 'object' || error.details === null) {
    return undefined
  }
  return 'code' in error.details && typeof error.details.code === 'string'
    ? error.details.code as ManagementError['code']
    : undefined
}
