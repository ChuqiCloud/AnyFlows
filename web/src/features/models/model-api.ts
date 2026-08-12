import { useInfiniteQuery } from '@tanstack/react-query'

import { apiClient } from '@/lib/api'
import { listModels } from '@/lib/api/generated/sdk.gen'
import type {
  ModelCatalogBillingMode,
  ModelCatalogCapability,
  ModelCatalogModality,
  ModelCatalogProtocol,
} from '@/lib/api/generated/types.gen'

const MODEL_PAGE_SIZE = 24

export type ModelBillingFilter = 'all' | ModelCatalogBillingMode

export type ModelCatalogFilters = {
  billingMode: ModelBillingFilter
  inputModalities: ModelCatalogModality[]
  outputModalities: ModelCatalogModality[]
  capabilities: ModelCatalogCapability[]
  protocols: ModelCatalogProtocol[]
}

export const EMPTY_MODEL_CATALOG_FILTERS: ModelCatalogFilters = {
  billingMode: 'all',
  inputModalities: [],
  outputModalities: [],
  capabilities: [],
  protocols: [],
}

/** 按模型名字典序游标读取游客基础目录或当前登录用户可用目录。 */
export function useModelCatalog(
  search: string,
  filters: ModelCatalogFilters,
  authenticated: boolean,
) {
  return useInfiniteQuery({
    // 身份状态变化时必须重新请求，不能把公开基础价和分组实际价共用缓存。
    queryKey: ['model-catalog', { authenticated, filters, search }],
    initialPageParam: undefined as string | undefined,
    queryFn: async ({ pageParam, signal }) => {
      const { data } = await listModels({
        client: apiClient,
        query: {
          after: pageParam,
          billing_mode: filters.billingMode === 'all' ? undefined : filters.billingMode,
          capability: filters.capabilities.length > 0 ? filters.capabilities : undefined,
          input_modality: filters.inputModalities.length > 0 ? filters.inputModalities : undefined,
          limit: MODEL_PAGE_SIZE,
          output_modality: filters.outputModalities.length > 0 ? filters.outputModalities : undefined,
          protocol: authenticated && filters.protocols.length > 0 ? filters.protocols : undefined,
          q: search || undefined,
        },
        signal,
      })
      return data
    },
    getNextPageParam: (page) => page.next_cursor ?? undefined,
    staleTime: 15_000,
  })
}
