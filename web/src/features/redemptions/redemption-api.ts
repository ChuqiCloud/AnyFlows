import { useInfiniteQuery, useMutation, useQueryClient } from '@tanstack/react-query'

import { apiClient, ApiError } from '@/lib/api'
import {
  createAdminRedemptionBatch,
  disableAdminRedemptionBatch,
  listAdminRedemptionBatches,
} from '@/lib/api/generated/sdk.gen'
import type {
  AdminRedemptionBatchCreateRequest,
  AdminRedemptionBatchDisableRequest,
} from '@/lib/api/generated/types.gen'

export const adminRedemptionBatchesQueryKey = ['admin-redemption-batches'] as const

/** 按批次主键倒序加载管理列表，下一页只读取更早的批次。 */
export function useAdminRedemptionBatches() {
  return useInfiniteQuery({
    queryKey: adminRedemptionBatchesQueryKey,
    initialPageParam: undefined as number | undefined,
    queryFn: async ({ pageParam, signal }) => {
      const { data } = await listAdminRedemptionBatches({
        client: apiClient,
        query: { before: pageParam, limit: 25 },
        signal,
      })
      return data
    },
    getNextPageParam: (page) => page.next_cursor ?? undefined,
  })
}

/** 创建响应包含仅展示一次的明文，禁用缓存保留。 */
export function useCreateAdminRedemptionBatch() {
  const queryClient = useQueryClient()
  return useMutation({
    gcTime: 0,
    mutationFn: async (body: AdminRedemptionBatchCreateRequest) => {
      const { data } = await createAdminRedemptionBatch({ body, client: apiClient })
      return data
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: adminRedemptionBatchesQueryKey }),
  })
}

export function useDisableAdminRedemptionBatch() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async ({
      batchId,
      body,
    }: {
      batchId: string
      body: AdminRedemptionBatchDisableRequest
    }) => {
      const { data } = await disableAdminRedemptionBatch({
        body,
        client: apiClient,
        path: { batch_id: batchId },
      })
      return data
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: adminRedemptionBatchesQueryKey }),
  })
}

export function redemptionErrorCode(error: unknown) {
  if (!(error instanceof ApiError) || typeof error.details !== 'object' || error.details === null) {
    return undefined
  }
  return 'code' in error.details && typeof error.details.code === 'string'
    ? error.details.code
    : undefined
}
