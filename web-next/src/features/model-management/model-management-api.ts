import { useInfiniteQuery, useMutation, useQueryClient } from '@tanstack/react-query'

import { apiClient, ApiError } from '@/lib/api'
import {
  createAdminModel,
  deleteAdminModel,
  listAdminModels,
  updateAdminModel,
} from '@/lib/api/generated/sdk.gen'
import type { AdminModelCreateRequest, AdminModelUpdateRequest } from '@/lib/api/generated/types.gen'

export const adminModelMetadataQueryKey = ['admin-model-metadata'] as const

/** 按服务端单调 ID 游标加载模型元数据，避免与用户模型目录缓存混用。 */
export function useAdminModelMetadata(pageSize = 25) {
  return useInfiniteQuery({
    queryKey: [...adminModelMetadataQueryKey, { pageSize }],
    initialPageParam: undefined as number | undefined,
    queryFn: async ({ pageParam, signal }) => {
      const { data } = await listAdminModels({
        client: apiClient,
        query: { after: pageParam, limit: pageSize },
        signal,
      })
      return data
    },
    getNextPageParam: (page) => page.next_cursor ?? undefined,
  })
}

export function useCreateAdminModel() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (body: AdminModelCreateRequest) => {
      const { data } = await createAdminModel({ body, client: apiClient })
      return data
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: adminModelMetadataQueryKey }),
  })
}

export function useUpdateAdminModel() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async ({ id, body }: { id: number; body: AdminModelUpdateRequest }) => {
      const { data } = await updateAdminModel({ body, client: apiClient, path: { id } })
      return data
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: adminModelMetadataQueryKey }),
  })
}

export function useDeleteAdminModel() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (id: number) => {
      await deleteAdminModel({ client: apiClient, path: { id } })
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: adminModelMetadataQueryKey }),
  })
}

export function isModelConflict(error: unknown) {
  if (!(error instanceof ApiError) || typeof error.details !== 'object' || error.details === null) {
    return false
  }
  return 'code' in error.details && error.details.code === 'model_conflict'
}
