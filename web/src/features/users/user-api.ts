import { useInfiniteQuery, useMutation, useQueryClient } from '@tanstack/react-query'

import { apiClient, ApiError } from '@/lib/api'
import {
  createAdminUser,
  deleteAdminUser,
  listAdminUsers,
  updateAdminUser,
} from '@/lib/api/generated/sdk.gen'
import type { AdminUserWriteRequest } from '@/lib/api/generated/types.gen'

export const adminUsersQueryKey = ['admin-users'] as const

/** 按服务端单调 ID 游标加载用户，只在管理员明确请求时读取下一页。 */
export function useAdminUsers() {
  return useInfiniteQuery({
    queryKey: adminUsersQueryKey,
    initialPageParam: undefined as number | undefined,
    queryFn: async ({ pageParam, signal }) => {
      const { data } = await listAdminUsers({
        client: apiClient,
        query: { after: pageParam, limit: 25 },
        signal,
      })
      return data
    },
    getNextPageParam: (page) => page.next_cursor ?? undefined,
  })
}

export function useCreateAdminUser() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (body: AdminUserWriteRequest) => {
      const { data } = await createAdminUser({ body, client: apiClient })
      return data
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: adminUsersQueryKey }),
  })
}

export function useUpdateAdminUser() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async ({ id, body }: { id: number; body: AdminUserWriteRequest }) => {
      const { data } = await updateAdminUser({ body, client: apiClient, path: { id } })
      return data
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: adminUsersQueryKey }),
  })
}

export function useDeleteAdminUser() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (id: number) => {
      await deleteAdminUser({ client: apiClient, path: { id } })
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: adminUsersQueryKey }),
  })
}

export function isUserConflict(error: unknown) {
  if (!(error instanceof ApiError) || typeof error.details !== 'object' || error.details === null) {
    return false
  }
  return 'code' in error.details && error.details.code === 'user_conflict'
}
