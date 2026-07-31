import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import type { QueryClient } from '@tanstack/react-query'

import { apiClient, ApiError } from '@/lib/api'
import {
  createAdminGroup,
  deleteAdminGroup,
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
    mutationFn: async (id: number) => {
      await deleteAdminGroup({ client: apiClient, path: { id } })
    },
    onSuccess: () => invalidateGroupConsumers(queryClient),
  })
}

/** 分组变更同时影响管理选择器与登录用户看到的实际模型倍率。 */
function invalidateGroupConsumers(queryClient: QueryClient) {
  return Promise.all([
    queryClient.invalidateQueries({ queryKey: adminGroupCatalogQueryKey }),
    queryClient.invalidateQueries({ queryKey: ['model-catalog'] }),
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
