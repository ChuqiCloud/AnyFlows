import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { apiClient } from '@/lib/api'
import {
  createAdminCredentialProxy,
  deleteAdminCredentialProxy,
  listAdminCredentialProxies,
  updateAdminCredentialProxy,
} from '@/lib/api/generated/sdk.gen'
import type {
  AdminCredentialProxy,
  AdminCredentialProxyScheme,
  AdminCredentialProxyWriteRequestWritable,
} from '@/lib/api/generated/types.gen'

export type { AdminCredentialProxy, AdminCredentialProxyScheme }
export type AdminCredentialProxyWriteRequest = AdminCredentialProxyWriteRequestWritable

export const adminCredentialProxiesQueryKey = ['admin-credential-proxies'] as const

/** 读取完整代理目录，凭据选择器与管理页共享同一脱敏缓存。 */
export function useAdminCredentialProxies() {
  return useQuery({
    queryKey: adminCredentialProxiesQueryKey,
    staleTime: 30_000,
    queryFn: async ({ signal }) => {
      const proxies: AdminCredentialProxy[] = []
      const seen = new Set<number>()
      let after: number | undefined
      while (true) {
        const { data } = await listAdminCredentialProxies({
          client: apiClient,
          query: { after, limit: 100 },
          signal,
        })
        proxies.push(...data.proxies)
        const next = data.next_cursor ?? undefined
        if (next === undefined) return proxies
        if (seen.has(next)) throw new Error('专属代理目录返回了重复游标')
        seen.add(next)
        after = next
      }
    },
  })
}

export function useCreateAdminCredentialProxy() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (body: AdminCredentialProxyWriteRequest) => {
      const { data } = await createAdminCredentialProxy({
        client: apiClient,
        body,
      })
      return data
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: adminCredentialProxiesQueryKey }),
  })
}

export function useUpdateAdminCredentialProxy() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async ({ id, body }: { id: number; body: AdminCredentialProxyWriteRequest }) => {
      const { data } = await updateAdminCredentialProxy({
        client: apiClient,
        path: { id },
        body,
      })
      return data
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: adminCredentialProxiesQueryKey }),
  })
}

export function useDeleteAdminCredentialProxy() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (id: number) => {
      await deleteAdminCredentialProxy({
        client: apiClient,
        path: { id },
      })
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: adminCredentialProxiesQueryKey }),
  })
}
