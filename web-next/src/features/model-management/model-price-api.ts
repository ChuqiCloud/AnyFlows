import { useInfiniteQuery, useMutation, useQueryClient } from '@tanstack/react-query'

import { apiClient, ApiError } from '@/lib/api'
import {
  applyAdminModelPrices,
  previewAdminLiteLlmModelPrices,
  previewAdminModelPriceExpression,
  listAdminModelPrices,
  previewAdminModelPrices,
} from '@/lib/api/generated/sdk.gen'
import type {
  AdminModelPriceBatchRequest,
  AdminModelPriceExpressionPreviewRequest,
} from '@/lib/api/generated/types.gen'

export const adminModelPriceQueryKey = ['admin-model-prices'] as const

export const modelPriceSources = ['models_dev', 'litellm'] as const
export type ModelPriceSource = typeof modelPriceSources[number]

/** 按 Canonical 游标加载正式价格，保持与模型元数据缓存相互独立。 */
export function useAdminModelPrices() {
  return useInfiniteQuery({
    queryKey: adminModelPriceQueryKey,
    initialPageParam: undefined as string | undefined,
    queryFn: async ({ pageParam, signal }) => {
      const { data } = await listAdminModelPrices({
        client: apiClient,
        query: { after: pageParam, limit: 100 },
        signal,
      })
      return data
    },
    getNextPageParam: (page) => page.next_cursor ?? undefined,
  })
}

/** 按固定来源标识读取公开价表，不接收前端 URL 或厂商覆盖。 */
export function usePreviewAdminModelPrices() {
  return useMutation({
    mutationFn: async (source: ModelPriceSource) => {
      const request = source === 'litellm'
        ? previewAdminLiteLlmModelPrices
        : previewAdminModelPrices
      const { data } = await request({ client: apiClient })
      return data
    },
  })
}

/** 使用服务端生产计费链路试算未保存表达式，前端不自行重算金额。 */
export function usePreviewAdminModelPriceExpression() {
  return useMutation({
    mutationFn: async (body: AdminModelPriceExpressionPreviewRequest) => {
      const { data } = await previewAdminModelPriceExpression({ body, client: apiClient })
      return data
    },
  })
}

/** 原子应用管理员明确选中的完整价格草稿。 */
export function useApplyAdminModelPrices() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (body: AdminModelPriceBatchRequest) => {
      const { data } = await applyAdminModelPrices({ body, client: apiClient })
      return data
    },
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: adminModelPriceQueryKey }),
        queryClient.invalidateQueries({ queryKey: ['admin-model-metadata'] }),
        queryClient.invalidateQueries({ queryKey: ['model-catalog'] }),
      ])
    },
  })
}

export function modelPriceErrorCode(error: unknown) {
  if (!(error instanceof ApiError) || typeof error.details !== 'object' || error.details === null) {
    return undefined
  }
  return 'code' in error.details && typeof error.details.code === 'string'
    ? error.details.code
    : undefined
}
