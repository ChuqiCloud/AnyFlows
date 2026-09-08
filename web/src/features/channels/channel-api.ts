import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { apiClient } from '@/lib/api'
import {
  createAdminChannel,
  getAdminChannel,
  listAdminChannels,
  probeAdminChannel,
  updateAdminChannel,
} from '@/lib/api/generated/sdk.gen'
import type {
  AdminChannelCreateRequestWritable,
  AdminChannelUpdateRequestWritable,
} from '@/lib/api/generated/types.gen'

export const adminChannelsQueryKey = ['admin-channels'] as const
export const adminChannelQueryKey = (channelId: number) => ['admin-channel', channelId] as const

/** 按服务端游标加载渠道，页面只在用户明确请求时继续翻页。 */
export function useAdminChannels(pageSize = 50) {
  return useInfiniteQuery({
    queryKey: [...adminChannelsQueryKey, { pageSize }],
    initialPageParam: undefined as number | undefined,
    queryFn: async ({ pageParam, signal }) => {
      const { data } = await listAdminChannels({
        client: apiClient,
        query: { after: pageParam, limit: pageSize },
        signal,
      })
      return data
    },
    getNextPageParam: (page) => page.next_cursor ?? undefined,
  })
}

/** 深链渠道不在当前列表页时按 ID 补读，仍由服务端执行管理员权限校验。 */
export function useAdminChannel(channelId?: number, enabled = true) {
  return useQuery({
    queryKey: adminChannelQueryKey(channelId ?? 0),
    enabled: enabled && channelId !== undefined,
    queryFn: async ({ signal }) => {
      if (channelId === undefined) throw new Error('缺少渠道 ID')
      const { data } = await getAdminChannel({ client: apiClient, path: { id: channelId }, signal })
      return data
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
