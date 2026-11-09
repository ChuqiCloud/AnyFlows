import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import type { QueryClient } from '@tanstack/react-query'

import { apiClient, ApiError } from '@/lib/api'
import {
  createAdminGroup,
  listAdminGroups,
  updateAdminGroup,
} from '@/lib/api/generated/sdk.gen'
import type { AdminGroup, AdminGroupWriteRequest } from '@/lib/api/generated/types.gen'

export const adminGroupCatalogQueryKey = ['admin-group-catalog'] as const

/** 自动遍历稳定游标，为管理表单提供完整且一致的分组目录。 */
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

export function useCreateAdminGroup() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (body: AdminGroupWriteRequest) => {
      const { data } = await createAdminGroup({ body, client: apiClient })
      return data
    },
    onSuccess: () => invalidateGroupConsumers(queryClient),
  })
}

export function useUpdateAdminGroup() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async ({ id, body }: { id: number; body: AdminGroupWriteRequest }) => {
      const { data } = await updateAdminGroup({ body, client: apiClient, path: { id } })
      return data
    },
    onSuccess: () => invalidateGroupConsumers(queryClient),
  })
}

export function useDeleteAdminGroup() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async ({ id, replacementGroupId }: { id: number; replacementGroupId?: number }) => {
      await apiClient.delete({ url: `/api/admin/groups/${id}`, query: { replacement_group_id: replacementGroupId }, security: [{ key: 'bearerAuth', scheme: 'bearer', type: 'http' }], throwOnError: true })
    },
    onSuccess: () => invalidateGroupConsumers(queryClient),
  })
}

/** Retired pools retain group ownership so ordinary traffic cannot reuse merchant supply. */
export function useMerchantPoolGroupBindings(enabled: boolean) {
  return useQuery({ queryKey: ['merchant-pool', 'group-bindings'], enabled, queryFn: async ({ signal }) => {
    const bindings = await Promise.all([undefined, 'deleted'].map(async (status) => {
      const ids: number[] = []
      let page = 1
      while (true) {
        const { data } = await apiClient.get<{ 200: { items: Array<{ group_id: number }>; page: number; page_size: number; total: number } }, unknown, true>({ url: '/api/admin/merchant-pools', query: { page, page_size: 100, status }, signal, security: [{ key: 'bearerAuth', scheme: 'bearer', type: 'http' }], throwOnError: true })
        ids.push(...data.items.map((pool) => pool.group_id))
        if (data.items.length === 0 || data.page * data.page_size >= data.total) return ids
        page += 1
      }
    }))
    return Array.from(new Set(bindings.flat()))
  } })
}

/** 分组变更同时影响管理选择器与登录用户看到的实际模型倍率。 */
function invalidateGroupConsumers(queryClient: QueryClient) {
  return Promise.all([
    queryClient.invalidateQueries({ queryKey: adminGroupCatalogQueryKey }),
    queryClient.invalidateQueries({ queryKey: ['model-catalog'] }),
    queryClient.invalidateQueries({ queryKey: ['user-api-keys'] }),
    queryClient.invalidateQueries({ queryKey: ['user-routes'] }),
    queryClient.invalidateQueries({ queryKey: ['admin-users'] }),
    queryClient.invalidateQueries({ queryKey: ['admin-tokens'] }),
  ])
}

export function groupWriteErrorCode(error: unknown) {
  if (!(error instanceof ApiError) || typeof error.details !== 'object' || error.details === null) {
    return undefined
  }
  return 'code' in error.details && typeof error.details.code === 'string'
    ? error.details.code
    : undefined
}
