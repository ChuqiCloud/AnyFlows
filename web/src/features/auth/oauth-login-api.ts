import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { publicSiteSettingsQueryKey } from '@/features/site-settings/site-settings-api'
import { apiClient } from '@/lib/api'
import {
  exchangeOAuthLoginTicket,
  getAdminDiscordOAuthLoginSettings,
  getAdminGitHubOAuthLoginSettings,
  startDiscordOAuthLogin,
  startGitHubOAuthLogin,
  updateAdminDiscordOAuthLoginSettings,
  updateAdminGitHubOAuthLoginSettings,
} from '@/lib/api/generated/sdk.gen'
import type { AdminOAuthLoginProviderSettingsRequest } from '@/lib/api/generated/types.gen'

export type OAuthLoginProvider = 'github' | 'discord'

export const adminOAuthLoginSettingsQueryKey = (provider: OAuthLoginProvider) =>
  ['admin-oauth-login-settings', provider] as const

/** 请求服务端生成的固定内置 Provider 授权地址，前端不拼接 OAuth 参数。 */
export async function beginOAuthLogin(provider: OAuthLoginProvider) {
  const { data } = provider === 'github'
    ? await startGitHubOAuthLogin({ client: apiClient })
    : await startDiscordOAuthLogin({ client: apiClient })
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

/** 读取脱敏后的内置 OAuth App 管理设置。 */
export function useAdminOAuthLoginSettings(provider: OAuthLoginProvider) {
  return useQuery({
    queryKey: adminOAuthLoginSettingsQueryKey(provider),
    queryFn: async ({ signal }) => {
      const { data } = provider === 'github'
        ? await getAdminGitHubOAuthLoginSettings({ client: apiClient, signal })
        : await getAdminDiscordOAuthLoginSettings({ client: apiClient, signal })
      return data
    },
  })
}

/** 覆盖内置 OAuth App 设置，成功后重新读取公开 Provider 投影。 */
export function useUpdateAdminOAuthLoginSettings(provider: OAuthLoginProvider) {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: async (body: AdminOAuthLoginProviderSettingsRequest) => {
      const { data } = provider === 'github'
        ? await updateAdminGitHubOAuthLoginSettings({ body, client: apiClient })
        : await updateAdminDiscordOAuthLoginSettings({ body, client: apiClient })
      return data
    },
    onSuccess: (settings) => {
      queryClient.setQueryData(adminOAuthLoginSettingsQueryKey(provider), settings)
      void queryClient.invalidateQueries({ queryKey: publicSiteSettingsQueryKey })
    },
  })
}
