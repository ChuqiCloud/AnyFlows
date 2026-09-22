import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { apiClient, ApiError } from '@/lib/api'
import {
  createUserToken,
  deleteUserToken,
  listUserTokens,
  updateUserToken,
} from '@/lib/api/generated/sdk.gen'
import type { ManagementError, UserTokenWriteRequest } from '@/lib/api/generated/types.gen'

export const apiKeysQueryKey = ['user-api-keys'] as const

/** 单个用户最多 32 把 Key，一页读取即可获得准确容量状态。 */
export function useApiKeys() {
  return useQuery({
    queryKey: apiKeysQueryKey,
    queryFn: async ({ signal }) => {
      const { data } = await listUserTokens({
        client: apiClient,
        query: { limit: 100 },
        signal,
      })
      return data
    },
  })
}

export function useCreateApiKey() {
  const queryClient = useQueryClient()
  return useMutation({
    gcTime: 0,
    mutationFn: async (body: UserTokenWriteRequest) => {
      const { data } = await createUserToken({ body, client: apiClient })
      return data
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: apiKeysQueryKey }),
    onError: (error) => {
      if (hasManagementErrorCode(error, 'token_limit_reached')) {
        void queryClient.invalidateQueries({ queryKey: apiKeysQueryKey })
      }
    },
  })
}

export function useUpdateApiKey() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async ({ id, body }: { id: number; body: UserTokenWriteRequest }) => {
      const { data } = await updateUserToken({ body, client: apiClient, path: { id } })
      return data
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: apiKeysQueryKey }),
  })
}

export function useDeleteApiKey() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (id: number) => {
      await deleteUserToken({ client: apiClient, path: { id } })
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: apiKeysQueryKey }),
  })
}

export function hasManagementErrorCode(error: unknown, code: ManagementError['code']) {
  if (!(error instanceof ApiError) || typeof error.details !== 'object' || error.details === null) {
    return false
  }
  return 'code' in error.details && error.details.code === code
}
