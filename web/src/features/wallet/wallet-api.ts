import { useInfiniteQuery, useQuery } from '@tanstack/react-query'

import { apiClient } from '@/lib/api'
import { getUserWallet, listUserWalletEntries } from '@/lib/api/generated/sdk.gen'

export const userWalletSummaryQueryKey = ['user-wallet', 'summary'] as const
export const userWalletEntriesQueryKey = ['user-wallet', 'entries'] as const

/** 读取当前登录用户自己的钱包摘要，并周期校正顶栏余额。 */
export function useUserWalletSummary() {
  return useQuery({
    queryKey: userWalletSummaryQueryKey,
    queryFn: async ({ signal }) => {
      const { data } = await getUserWallet({ client: apiClient, signal })
      return data
    },
    refetchInterval: 30_000,
  })
}

/** 按账本 ID 倒序读取当前用户自己的余额变更事实。 */
export function useUserWalletEntries() {
  return useInfiniteQuery({
    queryKey: userWalletEntriesQueryKey,
    initialPageParam: undefined as number | undefined,
    queryFn: async ({ pageParam, signal }) => {
      const { data } = await listUserWalletEntries({
        client: apiClient,
        query: { before: pageParam, limit: 25 },
        signal,
      })
      return data
    },
    getNextPageParam: (page) => page.next_cursor ?? undefined,
  })
}
