import { useMemo } from 'react'
import { useMutation, useQueries, useQuery, useQueryClient } from '@tanstack/react-query'

import { apiClient, ApiError } from '@/lib/api'
import {
  createAdminRoute,
  deleteAdminRoute,
  listAdminChannels,
  listAdminCredentials,
  listAdminRoutes,
  updateAdminRoute,
} from '@/lib/api/generated/sdk.gen'
import type {
  AdminChannel,
  AdminCredential,
  AdminRoute,
  AdminRouteWriteRequest,
} from '@/lib/api/generated/types.gen'

export const adminRoutesQueryKey = ['admin-routes'] as const
const adminRouteChannelsQueryKey = ['admin-channels', 'route-catalog'] as const
const adminRouteCredentialsQueryKey = (channelId: number) => [
  'admin-channel-credentials',
  channelId,
  'route-catalog',
] as const

/** 读取完整路由目录，避免编辑页在游标分页之间遗漏规则。 */
export function useAdminRoutes() {
  return useQuery({
    queryKey: adminRoutesQueryKey,
    queryFn: async ({ signal }) => {
      const routes: AdminRoute[] = []
      const seenCursors = new Set<number>()
      let after: number | undefined
      while (true) {
        const { data } = await listAdminRoutes({
          client: apiClient,
          query: { after, limit: 100 },
          signal,
        })
        routes.push(...data.routes)
        const nextCursor = data.next_cursor ?? undefined
        if (nextCursor === undefined) return routes
        if (seenCursors.has(nextCursor)) throw new Error('路由列表返回了重复游标')
        seenCursors.add(nextCursor)
        after = nextCursor
      }
    },
  })
}

/** 编辑器按需读取完整渠道目录，候选选择不依赖渠道页是否已翻页。 */
export function useAdminRouteChannelCatalog(enabled: boolean) {
  return useQuery({
    queryKey: adminRouteChannelsQueryKey,
    enabled,
    staleTime: 30_000,
    queryFn: async ({ signal }) => {
      const channels: AdminChannel[] = []
      const seenCursors = new Set<number>()
      let after: number | undefined
      while (true) {
        const { data } = await listAdminChannels({
          client: apiClient,
          query: { after, limit: 100 },
          signal,
        })
        channels.push(...data.channels)
        const nextCursor = data.next_cursor ?? undefined
        if (nextCursor === undefined) return channels
        if (seenCursors.has(nextCursor)) throw new Error('渠道列表返回了重复游标')
        seenCursors.add(nextCursor)
        after = nextCursor
      }
    },
  })
}

/** 为每个渠道读取脱敏凭据元数据，绝不读取或缓存凭据明文。 */
export function useAdminRouteCredentialCatalog(
  channelIds: readonly number[],
  enabled: boolean,
) {
  const uniqueChannelIds = useMemo(
    () => [...new Set(channelIds)].filter((id) => Number.isSafeInteger(id) && id > 0).sort((left, right) => left - right),
    [channelIds],
  )
  const queries = useQueries({
    queries: uniqueChannelIds.map((channelId) => ({
      queryKey: adminRouteCredentialsQueryKey(channelId),
      enabled,
      staleTime: 30_000,
      queryFn: async ({ signal }: { signal: AbortSignal }) => {
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
    })),
  })
  const credentialsByChannel = useMemo(() => {
    const result: Record<number, AdminCredential[]> = {}
    uniqueChannelIds.forEach((channelId, index) => {
      const credentials = queries[index]?.data
      if (credentials) result[channelId] = credentials
    })
    return result
  }, [queries, uniqueChannelIds])
  return { queries, channelIds: uniqueChannelIds, credentialsByChannel }
}

export function useCreateAdminRoute() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (body: AdminRouteWriteRequest) => {
      const { data } = await createAdminRoute({ body, client: apiClient })
      return data
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: adminRoutesQueryKey }),
  })
}

export function useUpdateAdminRoute() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async ({ id, body }: { id: number; body: AdminRouteWriteRequest }) => {
      const { data } = await updateAdminRoute({ body, client: apiClient, path: { id } })
      return data
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: adminRoutesQueryKey }),
  })
}

export function useDeleteAdminRoute() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (id: number) => {
      await deleteAdminRoute({ client: apiClient, path: { id } })
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: adminRoutesQueryKey }),
  })
}

/** 将服务端管理错误收敛为稳定文案键，原始诊断不进入页面。 */
export function routeWriteErrorCode(error: unknown) {
  if (!(error instanceof ApiError) || typeof error.details !== 'object' || error.details === null) {
    return 'unknown'
  }
  return 'code' in error.details && typeof error.details.code === 'string'
    ? error.details.code
    : 'unknown'
}
