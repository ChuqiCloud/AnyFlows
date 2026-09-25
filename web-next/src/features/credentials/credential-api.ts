import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { apiClient, ApiError } from '@/lib/api'
import {
  beginAdminOAuthAuthorization,
  completeAdminOAuthManualCallback,
  createAdminCredential,
  deleteAdminCredential,
  importAdminCredentials,
  listAdminCredentials,
  listAdminOAuthProviders,
  updateAdminCredential,
} from '@/lib/api/generated/sdk.gen'
import type {
  AdminCredential,
  AdminCredentialCreateRequestWritable,
  AdminCredentialImportResponse,
  AdminCredentialUpdateRequestWritable,
  AdminOAuthProvider,
} from '@/lib/api/generated/types.gen'

export const adminOAuthProvidersQueryKey = ['admin-oauth-providers'] as const
export const adminChannelCredentialsQueryKey = (channelId: number) => [
  'admin-channel-credentials',
  channelId,
] as const

export type AdminCredentialImportResult = AdminCredentialImportResponse

export type CredentialUsageSnapshot = {
  status: 'available' | 'unsupported' | 'unavailable'
  windows: Array<{ window_seconds: number; used_percent: number; reset_at: number | null }>
  credits_balance: string | null
  fetched_at: number | null
}

export async function getAdminCredentialUsage(channelId: number, credentialId: number, signal?: AbortSignal) {
  const result = await apiClient.get<{ 200: CredentialUsageSnapshot }, unknown, true>({
    security: [{ key: 'bearerAuth', scheme: 'bearer', type: 'http' }],
    url: '/api/admin/channels/{channel_id}/credentials/{credential_id}/usage',
    path: { channel_id: channelId, credential_id: credentialId },
    signal,
  })
  return result.data
}

export async function importAdminCredentialFiles(channelId: number, files: Array<{ name: string; content: string }>, signal?: AbortSignal) {
  const result = await importAdminCredentials({
    client: apiClient, path: { channel_id: channelId }, body: { files }, signal,
  })
  return result.data
}

export async function exportAdminCredentialFiles(channelId: number, signal?: AbortSignal) {
  const result = await apiClient.get<Blob, unknown, true>({
    security: [{ key: 'bearerAuth', scheme: 'bearer', type: 'http' }],
    url: '/api/admin/channels/{channel_id}/credentials/export',
    path: { channel_id: channelId }, parseAs: 'blob', signal,
  })
  return result.data
}
const adminChannelCredentialCatalogQueryKey = (channelId: number) => [
  'admin-channel-credentials',
  channelId,
  'complete-catalog',
] as const

const adminOAuthErrorCodes = [
  'invalid_request', 'forbidden', 'credential_not_found',
  'oauth_provider_not_configured', 'oauth_credential_provider_mismatch',
  'oauth_authorization_capacity_exceeded', 'oauth_authorization_not_found',
  'oauth_authorization_expired', 'oauth_authorization_denied',
  'oauth_upstream_timeout', 'oauth_upstream_rejected',
  'oauth_upstream_invalid_response', 'oauth_unavailable', 'internal_error',
] as const

export type AdminOAuthErrorCode = (typeof adminOAuthErrorCodes)[number] | 'unknown'

/** 读取当前启动配置已启用的 OAuth Provider 与回调能力。 */
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

/** 编辑器读取完整脱敏目录，避免分页边界遗漏已有影子或错误开放母凭据。 */
export function useAdminChannelCredentialCatalog(channelId?: number, enabled = true) {
  return useQuery({
    queryKey: adminChannelCredentialCatalogQueryKey(channelId ?? 0),
    enabled: enabled && channelId !== undefined,
    staleTime: 30_000,
    queryFn: async ({ signal }) => {
      if (channelId === undefined) throw new Error('缺少渠道 ID')
      const credentials: AdminCredential[] = []
      const seenCursors = new Set<number>()
      let after: number | undefined
      while (true) {
        const { data } = await listAdminCredentials({
          client: apiClient,
          path: { channel_id: channelId },
          query: { after, limit: 100 },
          signal,
        })
        credentials.push(...data.credentials)
        const nextCursor = data.next_cursor ?? undefined
        if (nextCursor === undefined) return credentials
        if (seenCursors.has(nextCursor)) throw new Error('凭据列表返回了重复游标')
        seenCursors.add(nextCursor)
        after = nextCursor
      }
    },
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
        body, client: apiClient, path: { channel_id: channelId }, signal,
      })
      return data
    },
    onSuccess: (_, { channelId }) => queryClient.invalidateQueries({
      queryKey: adminChannelCredentialsQueryKey(channelId),
    }),
  })
}

export function useUpdateAdminCredential() {
  const queryClient = useQueryClient()
  return useMutation({
    gcTime: 0,
    mutationFn: async ({ channelId, credentialId, body, signal }: {
      channelId: number
      credentialId: number
      body: AdminCredentialUpdateRequestWritable
      signal?: AbortSignal
    }) => {
      const { data } = await updateAdminCredential({
        body, client: apiClient,
        path: { channel_id: channelId, credential_id: credentialId }, signal,
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
    mutationFn: async ({ channelId, credentialId }: { channelId: number; credentialId: number }) => {
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

/** 为已有 OAuth 凭据创建短期授权会话。 */
export function useBeginAdminOAuthAuthorization() {
  return useMutation({
    gcTime: 0,
    mutationFn: async ({ channelId, credentialId, provider, signal }: {
      channelId: number; credentialId: number; provider: AdminOAuthProvider; signal?: AbortSignal
    }) => {
      const { data } = await beginAdminOAuthAuthorization({
        body: { provider }, client: apiClient,
        path: { channel_id: channelId, credential_id: credentialId }, signal,
      })
      return data
    },
  })
}

/** 提交完整 callback URL；调用方负责在完成后刷新脱敏凭据。 */
export function useCompleteAdminOAuthManualCallback() {
  return useMutation({
    gcTime: 0,
    mutationFn: async ({ provider, callbackUrl, signal }: {
      provider: AdminOAuthProvider; callbackUrl: string; signal?: AbortSignal
    }) => {
      const { data } = await completeAdminOAuthManualCallback({
        body: { provider, callback_url: callbackUrl }, client: apiClient, signal,
      })
      return data
    },
  })
}

/** 将 OAuth API 错误收敛为固定文案键，不展示服务端原始消息。 */
export function adminOAuthErrorCode(error: unknown): AdminOAuthErrorCode {
  if (!(error instanceof ApiError) || typeof error.details !== 'object' || error.details === null) return 'unknown'
  const code = (error.details as { code?: unknown }).code
  return typeof code === 'string' && (adminOAuthErrorCodes as readonly string[]).includes(code)
    ? code as AdminOAuthErrorCode
    : 'unknown'
}
