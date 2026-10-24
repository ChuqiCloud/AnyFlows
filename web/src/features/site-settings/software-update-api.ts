import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { apiClient } from '@/lib/api'

import type { SoftwareStatus } from '@/lib/api/generated/types.gen'
export type { SoftwareStatus }

const key = ['admin-software'] as const
const security = [{ key: 'bearerAuth', scheme: 'bearer', type: 'http' }] as const

export function useSoftwareStatus() {
  return useQuery<SoftwareStatus>({ queryKey: key, staleTime: 60_000, retry: false, refetchOnWindowFocus: false,
    refetchInterval: (query) => {
      const status = query.state.data
      const pending = status?.launch_mode === 'launcher' && ['preparing', 'restarting', 'starting', 'backoff', 'waiting_previous_process'].includes(status.update_phase)
      return pending && Date.now() - query.state.dataUpdatedAt < 12 * 60_000 ? 2_000 : false
    },
    queryFn: async ({ signal }) => (await apiClient.get<{ 200: SoftwareStatus }, unknown, true>({ url: '/api/admin/software', security, signal, throwOnError: true })).data,
  })
}

export function useSoftwareUpdate(action: 'check' | 'install') {
  const client = useQueryClient()
  return useMutation({
    onMutate: async () => {
      if (action !== 'install') return
      await client.cancelQueries({ queryKey: key })
      const current = client.getQueryData<SoftwareStatus>(key)
      if (current?.launch_mode === 'launcher') client.setQueryData(key, { ...current, update_phase: 'preparing' })
    },
    mutationFn: async (selection?: { tag: string; nodeId: string }) => (await apiClient.post<{ 200: SoftwareStatus }, unknown, true>({
      url: `/api/admin/software/${action}`, security, throwOnError: true,
      ...(action === 'install' ? { body: { tag: selection?.tag, expected_node_id: selection?.nodeId }, headers: { 'Content-Type': 'application/json' } } : {}),
    })).data,
    onSuccess: (status) => client.setQueryData(key, status),
  })
}
