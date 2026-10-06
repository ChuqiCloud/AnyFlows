import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { publicSiteSettingsQueryKey } from '@/features/site-settings/site-settings-api'
import { apiClient } from '@/lib/api'
import {
  exchangeOAuthLoginTicket,
  getAdminDiscordOAuthLoginSettings,
  getAdminGitHubOAuthLoginSettings,
  getAdminLinuxDoLoginSettings,
  getAdminOidcLoginSettings,
  getAdminWeChatOAuthLoginSettings,
  getAdminTelegramOAuthLoginSettings,
  getAdminGoogleOAuthLoginSettings,
  startDiscordOAuthLogin,
  startCustomOAuth2Login,
  startGitHubOAuthLogin,
  startLinuxDoLogin,
  startOidcLogin,
  startWeChatOAuthLogin,
  startTelegramLogin,
  startGoogleLogin,
  updateAdminDiscordOAuthLoginSettings,
  updateAdminGitHubOAuthLoginSettings,
  updateAdminLinuxDoLoginSettings,
  updateAdminOidcLoginSettings,
  updateAdminWeChatOAuthLoginSettings,
  updateAdminTelegramOAuthLoginSettings,
  updateAdminGoogleOAuthLoginSettings,
} from '@/lib/api/generated/sdk.gen'
import type { AdminOAuthLoginProviderSettingsRequest } from '@/lib/api/generated/types.gen'

export type BuiltinOAuthLoginProvider = 'github' | 'discord' | 'oidc' | 'linuxdo' | 'wechat' | 'telegram' | 'google'
export type CustomOAuthLoginProvider = `custom_${string}`
export type OAuthLoginProvider = BuiltinOAuthLoginProvider | CustomOAuthLoginProvider

const startOAuthLogin = {
  github: () => startGitHubOAuthLogin({ client: apiClient }),
  discord: () => startDiscordOAuthLogin({ client: apiClient }),
  oidc: () => startOidcLogin({ client: apiClient }),
  linuxdo: () => startLinuxDoLogin({ client: apiClient }),
  wechat: () => startWeChatOAuthLogin({ client: apiClient }),
  telegram: () => startTelegramLogin({ client: apiClient }),
  google: () => startGoogleLogin({ client: apiClient }),
}

const getAdminOAuthLoginSettings = {
  github: (signal: AbortSignal) => getAdminGitHubOAuthLoginSettings({ client: apiClient, signal }),
  discord: (signal: AbortSignal) => getAdminDiscordOAuthLoginSettings({ client: apiClient, signal }),
  oidc: (signal: AbortSignal) => getAdminOidcLoginSettings({ client: apiClient, signal }),
  linuxdo: (signal: AbortSignal) => getAdminLinuxDoLoginSettings({ client: apiClient, signal }),
  wechat: (signal: AbortSignal) => getAdminWeChatOAuthLoginSettings({ client: apiClient, signal }),
  telegram: (signal: AbortSignal) => getAdminTelegramOAuthLoginSettings({ client: apiClient, signal }),
  google: (signal: AbortSignal) => getAdminGoogleOAuthLoginSettings({ client: apiClient, signal }),
}

const updateAdminOAuthLoginSettings = {
  github: (body: AdminOAuthLoginProviderSettingsRequest) =>
    updateAdminGitHubOAuthLoginSettings({ body, client: apiClient }),
  discord: (body: AdminOAuthLoginProviderSettingsRequest) =>
    updateAdminDiscordOAuthLoginSettings({ body, client: apiClient }),
  oidc: (body: AdminOAuthLoginProviderSettingsRequest) =>
    updateAdminOidcLoginSettings({ body, client: apiClient }),
  linuxdo: (body: AdminOAuthLoginProviderSettingsRequest) =>
    updateAdminLinuxDoLoginSettings({ body, client: apiClient }),
  wechat: (body: AdminOAuthLoginProviderSettingsRequest) =>
    updateAdminWeChatOAuthLoginSettings({ body, client: apiClient }),
  telegram: (body: AdminOAuthLoginProviderSettingsRequest) =>
    updateAdminTelegramOAuthLoginSettings({ body, client: apiClient }),
  google: (body: AdminOAuthLoginProviderSettingsRequest) =>
    updateAdminGoogleOAuthLoginSettings({ body, client: apiClient }),
}

export const adminOAuthLoginSettingsQueryPrefix = ['admin-oauth-login-settings'] as const

export const adminOAuthLoginSettingsQueryKey = (provider: BuiltinOAuthLoginProvider) =>
  [...adminOAuthLoginSettingsQueryPrefix, provider] as const

/** 请求服务端生成的固定 Provider 授权地址，前端不拼接 OAuth 参数。 */
export async function beginOAuthLogin(provider: OAuthLoginProvider) {
  if (isCustomOAuthLoginProvider(provider)) {
    const { data } = await startCustomOAuth2Login({
      client: apiClient,
      path: { provider_key: provider },
    })
    return data
  }
  const { data } = await startOAuthLogin[provider]()
  return data
}

function isCustomOAuthLoginProvider(
  provider: OAuthLoginProvider,
): provider is CustomOAuthLoginProvider {
  return provider.startsWith('custom_')
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
export function useAdminOAuthLoginSettings(provider: BuiltinOAuthLoginProvider) {
  return useQuery({
    queryKey: adminOAuthLoginSettingsQueryKey(provider),
    queryFn: async ({ signal }) => {
      const { data } = await getAdminOAuthLoginSettings[provider](signal)
      return data
    },
  })
}

/** 覆盖内置 OAuth App 设置，成功后重新读取公开 Provider 投影。 */
export function useUpdateAdminOAuthLoginSettings(provider: BuiltinOAuthLoginProvider) {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: async (body: AdminOAuthLoginProviderSettingsRequest) => {
      const { data } = await updateAdminOAuthLoginSettings[provider](body)
      return data
    },
    onSuccess: (settings) => {
      queryClient.setQueryData(adminOAuthLoginSettingsQueryKey(provider), settings)
      void queryClient.invalidateQueries({ queryKey: publicSiteSettingsQueryKey })
    },
  })
}
