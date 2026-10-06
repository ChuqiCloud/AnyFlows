import { QueryClient } from '@tanstack/react-query'

import { ApiError } from '@/lib/api/errors'

/** 创建隔离的 QueryClient，便于测试和未来多入口复用。 */
export function createQueryClient() {
  return new QueryClient({
    defaultOptions: {
      queries: {
        gcTime: 5 * 60 * 1000,
        retry: (failureCount, error) => {
          // 客户端错误应直接交给页面处理，仅重试网络错误和服务端错误。
          if (error instanceof ApiError && error.isClientError) {
            return false
          }
          return failureCount < 2
        },
        staleTime: 30 * 1000,
      },
      mutations: {
        retry: false,
      },
    },
  })
}

export const appQueryClient = createQueryClient()
