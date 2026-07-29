import { useInfiniteQuery } from '@tanstack/react-query'

import { apiClient } from '@/lib/api'
import { listModels } from '@/lib/api/generated/sdk.gen'
import type { ModelCatalogBillingMode } from '@/lib/api/generated/types.gen'

const MODEL_PAGE_SIZE = 24

export type ModelBillingFilter = 'all' | ModelCatalogBillingMode

/** 按模型名字典序游标读取游客基础目录或当前登录用户可用目录。 */
export function useModelCatalog(
  search: string,
  billingMode: ModelBillingFilter,
  authenticated: boolean,
) {
  return useInfiniteQuery({
    // 身份状态变化时必须重新请求，不能把公开基础价和分组实际价共用缓存。
    queryKey: ['model-catalog', { authenticated, billingMode, search }],
    initialPageParam: undefined as string | undefined,
    queryFn: async ({ pageParam, signal }) => {
      const { data } = await listModels({
        client: apiClient,
        query: {
          after: pageParam,
          billing_mode: billingMode === 'all' ? undefined : billingMode,
          limit: MODEL_PAGE_SIZE,
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
