import { useInfiniteQuery, useMutation, useQueryClient } from '@tanstack/react-query'

import { apiClient } from '@/lib/api'
import {
  createAdminChannel,
  listAdminChannels,
  probeAdminChannel,
  updateAdminChannel,
} from '@/lib/api/generated/sdk.gen'
import type {
  AdminChannelCreateRequestWritable,
  AdminChannelUpdateRequestWritable,
} from '@/lib/api/generated/types.gen'

export const adminChannelsQueryKey = ['admin-channels'] as const

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
