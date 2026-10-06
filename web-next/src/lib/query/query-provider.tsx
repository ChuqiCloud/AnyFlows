import type { PropsWithChildren } from 'react'
import { QueryClientProvider } from '@tanstack/react-query'

import { appQueryClient } from './query-client'

/** 为应用提供统一的服务端状态缓存与重试策略。 */
export function QueryProvider({ children }: PropsWithChildren) {
  return <QueryClientProvider client={appQueryClient}>{children}</QueryClientProvider>
}
