import { useInfiniteQuery, useMutation, useQueryClient } from '@tanstack/react-query'

import { apiClient } from '@/lib/api'
import {
  createAdminToken,
  deleteAdminToken,
  listAdminTokens,
  updateAdminToken,
} from '@/lib/api/generated/sdk.gen'
import type { AdminTokenWriteRequest } from '@/lib/api/generated/types.gen'

export const adminTokensQueryKey = ['admin-tokens'] as const

/** 按稳定 ID 游标读取令牌，避免刷新期间出现重复页或漏项。 */
export function useAdminTokens(pageSize = 50) {
  return useInfiniteQuery({
    queryKey: [...adminTokensQueryKey, { pageSize }],
    initialPageParam: undefined as number | undefined,
    queryFn: async ({ pageParam, signal }) => {
      const { data } = await listAdminTokens({
        client: apiClient,
        query: { after: pageParam, limit: pageSize },
        signal,
      })
      return data
    },
    getNextPageParam: (page) => page.next_cursor ?? undefined,
  })
}

export function useCreateAdminToken() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (body: AdminTokenWriteRequest) => {
      const { data } = await createAdminToken({ body, client: apiClient })
      return data
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: adminTokensQueryKey }),
  })
}

export function useUpdateAdminToken() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async ({ id, body }: { id: number; body: AdminTokenWriteRequest }) => {
      const { data } = await updateAdminToken({ body, client: apiClient, path: { id } })
      return data
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: adminTokensQueryKey }),
  })
}

export function useDeleteAdminToken() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (id: number) => {
      await deleteAdminToken({ client: apiClient, path: { id } })
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: adminTokensQueryKey }),
  })
}
