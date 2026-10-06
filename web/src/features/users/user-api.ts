import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { apiClient, ApiError } from '@/lib/api'
import {
  adjustAdminWallet,
  createAdminUser,
  deleteAdminUser,
  getAdminUser,
  listAdminWalletEntries,
  listAdminUsers,
  updateAdminUser,
} from '@/lib/api/generated/sdk.gen'
import type {
  AdminUserCreateRequest,
  AdminUserUpdateRequest,
  AdminWalletAdjustmentRequest,
} from '@/lib/api/generated/types.gen'

export const adminUsersQueryKey = ['admin-users'] as const
export const adminWalletQueryKey = ['admin-wallet'] as const

/** 仅在管理员明确打开用户资料时读取单个非敏感账户快照。 */
export function useAdminUser(userId?: number) {
  return useQuery({
    queryKey: [...adminUsersQueryKey, userId],
    enabled: userId !== undefined,
    queryFn: async ({ signal }) => {
      const { data } = await getAdminUser({
        client: apiClient,
        path: { id: userId as number },
        signal,
      })
      return data
    },
  })
}

/** 按服务端单调 ID 游标加载用户，只在管理员明确请求时读取下一页。 */
export function useAdminUsers(pageSize = 25) {
  return useInfiniteQuery({
    queryKey: [...adminUsersQueryKey, { pageSize }],
    initialPageParam: undefined as number | undefined,
    queryFn: async ({ pageParam, signal }) => {
      const { data } = await listAdminUsers({
        client: apiClient,
        query: { after: pageParam, limit: pageSize },
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
    mutationFn: async (body: AdminUserCreateRequest) => {
      const { data } = await createAdminUser({ body, client: apiClient })
      return data
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: adminUsersQueryKey }),
  })
}

export function useUpdateAdminUser() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async ({ id, body }: { id: number; body: AdminUserUpdateRequest }) => {
      const { data } = await updateAdminUser({ body, client: apiClient, path: { id } })
      return data
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: adminUsersQueryKey }),
  })
}

/** 按账本 ID 倒序读取历史，下一页只包含更早的不可变事件。 */
export function useAdminWalletEntries(userId: number | undefined, enabled: boolean) {
  return useInfiniteQuery({
    queryKey: [...adminWalletQueryKey, userId],
    enabled: enabled && userId !== undefined,
    initialPageParam: undefined as number | undefined,
    queryFn: async ({ pageParam, signal }) => {
      if (userId === undefined) throw new Error('钱包账本查询缺少用户 ID')
      const { data } = await listAdminWalletEntries({
        client: apiClient,
        path: { id: userId },
        query: { before: pageParam, limit: 25 },
        signal,
      })
      return data
    },
    getNextPageParam: (page) => page.next_cursor ?? undefined,
  })
}

export function useAdjustAdminWallet(userId: number) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (body: AdminWalletAdjustmentRequest) => {
      const { data, response } = await adjustAdminWallet({
        body,
        client: apiClient,
        path: { id: userId },
      })
      return { entry: data, replayed: response.status === 200 }
    },
    onSuccess: () => Promise.all([
      queryClient.invalidateQueries({ queryKey: adminUsersQueryKey }),
      queryClient.invalidateQueries({ queryKey: [...adminWalletQueryKey, userId] }),
    ]),
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
  return userWriteErrorCode(error) === 'user_conflict'
}

export function userWriteErrorCode(error: unknown) {
  if (!(error instanceof ApiError) || typeof error.details !== 'object' || error.details === null) {
    return undefined
  }
  return 'code' in error.details && typeof error.details.code === 'string'
    ? error.details.code
    : undefined
}
