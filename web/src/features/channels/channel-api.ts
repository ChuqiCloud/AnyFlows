import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { apiClient, ApiError } from '@/lib/api'
import {
  beginAdminOAuthAuthorization,
  completeAdminOAuthManualCallback,
  createAdminChannel,
  createAdminCredential,
  deleteAdminCredential,
  listAdminChannels,
  listAdminCredentials,
  listAdminOAuthProviders,
  probeAdminChannel,
  updateAdminChannel,
  updateAdminCredential,
} from '@/lib/api/generated/sdk.gen'
import type {
  AdminChannelCreateRequestWritable,
  AdminChannelUpdateRequestWritable,
  AdminCredentialCreateRequestWritable,
  AdminCredentialUpdateRequestWritable,
  AdminOAuthProvider,
} from '@/lib/api/generated/types.gen'

export const adminChannelsQueryKey = ['admin-channels'] as const
export const adminOAuthProvidersQueryKey = ['admin-oauth-providers'] as const
export const adminChannelCredentialsQueryKey = (channelId: number) => [
  'admin-channel-credentials',
  channelId,
] as const

const adminOAuthErrorCodes = [
  'invalid_request',
  'forbidden',
  'credential_not_found',
  'oauth_provider_not_configured',
  'oauth_credential_provider_mismatch',
  'oauth_authorization_capacity_exceeded',
  'oauth_authorization_not_found',
  'oauth_authorization_expired',
  'oauth_authorization_denied',
  'oauth_upstream_timeout',
  'oauth_upstream_rejected',
  'oauth_upstream_invalid_response',
  'oauth_unavailable',
  'internal_error',
] as const

export type AdminOAuthErrorCode = (typeof adminOAuthErrorCodes)[number] | 'unknown'

/** 读取当前启动配置已启用的 OAuth provider 与回调能力。 */
export function useAdminOAuthProviders(enabled = true) {
  return useQuery({
    queryKey: adminOAuthProvidersQueryKey,
    enabled,
    staleTime: 30_000,
    queryFn: async ({ signal }) => {
      const { data } = await listAdminOAuthProviders({ client: apiClient, signal })
      return data
    },
  })
}

/** 按服务端游标加载渠道，页面只在用户明确请求时继续翻页。 */
export function useAdminChannels() {
  return useInfiniteQuery({
    queryKey: adminChannelsQueryKey,
    initialPageParam: undefined as number | undefined,
    queryFn: async ({ pageParam, signal }) => {
      const { data } = await listAdminChannels({
        client: apiClient,
        query: { after: pageParam, limit: 50 },
        signal,
      })
      return data
    },
    getNextPageParam: (page) => page.next_cursor ?? undefined,
  })
}

export function useCreateAdminChannel() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (body: AdminChannelCreateRequestWritable) => {
      const { data } = await createAdminChannel({ body, client: apiClient })
      return data
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: adminChannelsQueryKey }),
  })
}

/** 按服务端游标加载指定渠道的脱敏凭据元数据。 */
export function useAdminChannelCredentials(channelId?: number) {
  return useInfiniteQuery({
    queryKey: adminChannelCredentialsQueryKey(channelId ?? 0),
    enabled: channelId !== undefined,
    initialPageParam: undefined as number | undefined,
    queryFn: async ({ pageParam, signal }) => {
      if (channelId === undefined) throw new Error('缺少渠道 ID')
      const { data } = await listAdminCredentials({
        client: apiClient,
        path: { channel_id: channelId },
        query: { after: pageParam, limit: 50 },
        signal,
      })
      return data
    },
    getNextPageParam: (page) => page.next_cursor ?? undefined,
  })
}

export function useCreateAdminCredential() {
  const queryClient = useQueryClient()
  return useMutation({
    gcTime: 0,
    mutationFn: async ({ channelId, body, signal }: {
      channelId: number
      body: AdminCredentialCreateRequestWritable
      signal?: AbortSignal
    }) => {
      const { data } = await createAdminCredential({
        body,
        client: apiClient,
        path: { channel_id: channelId },
        signal,
      })
      return data
    },
    onSuccess: (_, { channelId }) => {
      void queryClient.invalidateQueries({ queryKey: adminChannelCredentialsQueryKey(channelId) })
    },
  })
}

export function useUpdateAdminCredential() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async ({ channelId, credentialId, body }: {
      channelId: number
      credentialId: number
      body: AdminCredentialUpdateRequestWritable
    }) => {
      const { data } = await updateAdminCredential({
        body,
        client: apiClient,
        path: { channel_id: channelId, credential_id: credentialId },
      })
      return data
    },
    onSuccess: (_, { channelId }) => queryClient.invalidateQueries({
      queryKey: adminChannelCredentialsQueryKey(channelId),
    }),
  })
}

export function useDeleteAdminCredential() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async ({ channelId, credentialId }: {
      channelId: number
      credentialId: number
    }) => {
      await deleteAdminCredential({
        client: apiClient,
        path: { channel_id: channelId, credential_id: credentialId },
      })
    },
    onSuccess: (_, { channelId }) => queryClient.invalidateQueries({
      queryKey: adminChannelCredentialsQueryKey(channelId),
    }),
  })
}

/** 为一条已有 OAuth 凭据创建短期授权会话。 */
export function useBeginAdminOAuthAuthorization() {
  return useMutation({
    gcTime: 0,
    mutationFn: async ({ channelId, credentialId, provider, signal }: {
      channelId: number
      credentialId: number
      provider: AdminOAuthProvider
      signal?: AbortSignal
    }) => {
      const { data } = await beginAdminOAuthAuthorization({
        body: { provider },
        client: apiClient,
        path: { channel_id: channelId, credential_id: credentialId },
        signal,
      })
      return data
    },
  })
}

/** 提交完整 callback URL；调用方必须先清空敏感 mutation，再刷新凭据。 */
export function useCompleteAdminOAuthManualCallback() {
  return useMutation({
    gcTime: 0,
    mutationFn: async ({ provider, callbackUrl, signal }: {
      provider: AdminOAuthProvider
      callbackUrl: string
      signal?: AbortSignal
    }) => {
      const { data } = await completeAdminOAuthManualCallback({
        body: { provider, callback_url: callbackUrl },
        client: apiClient,
        signal,
      })
      return data
    },
  })
}

/** 将 OAuth API 错误收敛为前端固定文案键，不展示服务端原始消息。 */
export function adminOAuthErrorCode(error: unknown): AdminOAuthErrorCode {
  if (!(error instanceof ApiError) || typeof error.details !== 'object' || error.details === null) {
    return 'unknown'
  }
  const details = error.details as { code?: unknown }
  if (typeof details.code !== 'string') return 'unknown'
  return (adminOAuthErrorCodes as readonly string[]).includes(details.code)
    ? details.code as AdminOAuthErrorCode
    : 'unknown'
}

export function useUpdateAdminChannel() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async ({ id, body }: { id: number; body: AdminChannelUpdateRequestWritable }) => {
      const { data } = await updateAdminChannel({ body, client: apiClient, path: { id } })
      return data
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: adminChannelsQueryKey }),
  })
}

export function useProbeAdminChannel() {
  return useMutation({
    mutationFn: async (id: number) => {
      const { data } = await probeAdminChannel({ client: apiClient, path: { id } })
      return data
    },
  })
}
