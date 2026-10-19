import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { apiClient } from '@/lib/api'
import {
  listAdminProxyPools, createAdminProxyPool, updateAdminProxyPool,
  deleteAdminProxyPool, refreshAdminProxyPool, checkAdminProxyPool, bindAdminProxyPool,
} from '@/lib/api/generated/sdk.gen'
import type { AdminProxyPool, AdminProxyPoolWriteRequestWritable } from '@/lib/api/generated/types.gen'
import { adminCredentialProxiesQueryKey } from './credential-proxy-api'

export type { AdminProxyPool }
export type ProxyPoolWrite = AdminProxyPoolWriteRequestWritable
const poolsKey = ['admin-proxy-pools'] as const
export function useAdminProxyPools() {
  return useQuery({ queryKey: poolsKey, refetchInterval: 15_000, queryFn: async ({ signal }) => {
    const { data } = await listAdminProxyPools({ client: apiClient, signal })
    return data.pools
  } })
}
type PoolAction = { kind: 'save'; id?: number; body: ProxyPoolWrite }
  | { kind: 'refresh' | 'check' | 'delete'; id: number }
  | { kind: 'bind'; channelId: number; poolId: number | null }
export function useProxyPoolAction() {
  const cache = useQueryClient()
  return useMutation({ mutationFn: async (action: PoolAction) => {
    if (action.kind === 'save') {
      if (action.id !== undefined) return (await updateAdminProxyPool({ client: apiClient, path: { id: action.id }, body: action.body })).data
      return (await createAdminProxyPool({ client: apiClient, body: action.body })).data
    }
    if (action.kind === 'bind') { await bindAdminProxyPool({ client: apiClient, body: { channel_id: action.channelId, pool_id: action.poolId } }); return }
    if (action.kind === 'delete') { await deleteAdminProxyPool({ client: apiClient, path: { id: action.id } }); return }
    if (action.kind === 'refresh') { await refreshAdminProxyPool({ client: apiClient, path: { id: action.id } }); return }
    await checkAdminProxyPool({ client: apiClient, path: { id: action.id } })
  }, onSettled: async () => {
    await Promise.all([cache.invalidateQueries({ queryKey: poolsKey }), cache.invalidateQueries({ queryKey: adminCredentialProxiesQueryKey })])
  } })
}
