import { useInfiniteQuery, useQuery } from '@tanstack/react-query'

import { apiClient } from '@/lib/api'
import { listModelProviders, listModels } from '@/lib/api/generated/sdk.gen'
import type { ModelCatalogFilters } from './model-catalog-filter-model'

export {
  EMPTY_MODEL_CATALOG_FILTERS,
  countModelCatalogFilters,
} from './model-catalog-filter-model'
export type { ModelBillingFilter, ModelCatalogFilters } from './model-catalog-filter-model'

const MODEL_PAGE_SIZE = 24

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
          provider: filters.providers.length > 0 ? filters.providers : undefined,
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

/** 独立读取当前目录完整供应商聚合，避免选项受模型分页截断。 */
export function useModelCatalogProviders(authenticated: boolean) {
  return useQuery({
    queryKey: ['model-catalog-providers', { authenticated }],
    queryFn: async ({ signal }) => {
      const { data } = await listModelProviders({ client: apiClient, signal })
      return data
    },
    staleTime: 15_000,
  })
}
