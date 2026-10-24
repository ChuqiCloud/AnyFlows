import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { apiClient } from '@/lib/api'

import type { SoftwareStatus } from '@/lib/api/generated/types.gen'
export type { SoftwareStatus }

const key = ['admin-software'] as const
const security = [{ key: 'bearerAuth', scheme: 'bearer', type: 'http' }] as const

export function useSoftwareStatus() {
  return useQuery({ queryKey: key, staleTime: 60_000, retry: false, refetchOnWindowFocus: false,
    queryFn: async ({ signal }) => (await apiClient.get<{ 200: SoftwareStatus }, unknown, true>({ url: '/api/admin/software', security, signal, throwOnError: true })).data,
  })
}

export function useSoftwareUpdate(action: 'check' | 'install') {
  const client = useQueryClient()
  return useMutation({
    mutationFn: async (selection?: { tag: string; nodeId: string }) => (await apiClient.post<{ 200: SoftwareStatus }, unknown, true>({
      url: `/api/admin/software/${action}`, security, throwOnError: true,
      ...(action === 'install' ? { body: { tag: selection?.tag, expected_node_id: selection?.nodeId }, headers: { 'Content-Type': 'application/json' } } : {}),
    })).data,
    onSuccess: (status) => client.setQueryData(key, status),
  })
}
