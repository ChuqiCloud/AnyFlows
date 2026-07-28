import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { apiClient } from '@/lib/api'
import {
  createAdminChannel,
  createAdminCredential,
  deleteAdminCredential,
  listAdminChannels,
  listAdminCredentials,
  listAdminGroups,
  probeAdminChannel,
  updateAdminChannel,
  updateAdminCredential,
} from '@/lib/api/generated/sdk.gen'
import type {
  AdminChannelCreateRequestWritable,
  AdminChannelUpdateRequestWritable,
  AdminCredentialCreateRequestWritable,
  AdminCredentialUpdateRequestWritable,
  AdminGroup,
} from '@/lib/api/generated/types.gen'

export const adminChannelsQueryKey = ['admin-channels'] as const
export const adminGroupCatalogQueryKey = ['admin-group-catalog'] as const
export const adminChannelCredentialsQueryKey = (channelId: number) => [
  'admin-channel-credentials',
  channelId,
] as const

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

/** 自动遍历稳定游标，给渠道表单提供完整分组选择目录。 */
export function useAdminGroupCatalog() {
  return useQuery({
    queryKey: adminGroupCatalogQueryKey,
    staleTime: 60_000,
    queryFn: async ({ signal }) => {
      const groups: AdminGroup[] = []
      const seenCursors = new Set<number>()
      let after: number | undefined
      while (true) {
        const { data } = await listAdminGroups({
          client: apiClient,
          query: { after, limit: 100 },
          signal,
        })
        groups.push(...data.groups)
        const nextCursor = data.next_cursor ?? undefined
        if (nextCursor === undefined) return groups
        if (seenCursors.has(nextCursor)) throw new Error('分组列表返回了重复游标')
        seenCursors.add(nextCursor)
        after = nextCursor
      }
    },
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
    mutationFn: async ({ channelId, body }: {
      channelId: number
      body: AdminCredentialCreateRequestWritable
    }) => {
      const { data } = await createAdminCredential({
        body,
        client: apiClient,
        path: { channel_id: channelId },
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
