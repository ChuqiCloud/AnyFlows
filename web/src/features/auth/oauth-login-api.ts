import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { publicSiteSettingsQueryKey } from '@/features/site-settings/site-settings-api'
import { apiClient } from '@/lib/api'
import {
  exchangeOAuthLoginTicket,
  getAdminGitHubOAuthLoginSettings,
  startGitHubOAuthLogin,
  updateAdminGitHubOAuthLoginSettings,
} from '@/lib/api/generated/sdk.gen'
import type { AdminOAuthLoginProviderSettingsRequest } from '@/lib/api/generated/types.gen'

export const adminGitHubOAuthLoginSettingsQueryKey = ['admin-github-oauth-login-settings'] as const

/** 请求服务端生成的固定 GitHub 授权地址，前端不拼接 OAuth 参数。 */
export async function beginGitHubOAuthLogin() {
  const { data } = await startGitHubOAuthLogin({ client: apiClient })
  return data
}

/** 用短期单次票据交换现有 bearer 会话。 */
export async function exchangeOAuthTicket(ticket: string) {
  const { data } = await exchangeOAuthLoginTicket({
    body: { ticket },
    client: apiClient,
  })
  return data
}

/** 读取脱敏后的 GitHub OAuth App 管理设置。 */
export function useAdminGitHubOAuthLoginSettings() {
  return useQuery({
    queryKey: adminGitHubOAuthLoginSettingsQueryKey,
    queryFn: async ({ signal }) => {
      const { data } = await getAdminGitHubOAuthLoginSettings({ client: apiClient, signal })
      return data
    },
  })
}

/** 覆盖 GitHub OAuth App 设置，成功后重新读取公开 Provider 投影。 */
export function useUpdateAdminGitHubOAuthLoginSettings() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: async (body: AdminOAuthLoginProviderSettingsRequest) => {
      const { data } = await updateAdminGitHubOAuthLoginSettings({ body, client: apiClient })
      return data
    },
    onSuccess: (settings) => {
      queryClient.setQueryData(adminGitHubOAuthLoginSettingsQueryKey, settings)
      void queryClient.invalidateQueries({ queryKey: publicSiteSettingsQueryKey })
    },
  })
}
