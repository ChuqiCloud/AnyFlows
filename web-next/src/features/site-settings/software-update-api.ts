import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { apiClient } from '@/lib/api'

export type SoftwareStatus = {
  edition: string
  version: string
  core_version: string
  commit: string | null
  build_time: string | null
  target: string
  deployment: string
  node_id: string
  check_enabled: boolean
  install_enabled: boolean
  install_blocked_reason: string | null
  restart_required: boolean
  checked_at: number | null
  update_available: boolean
  latest: null | { version: string; tag: string; url: string; notes: string; published_at: string | null; package_name: string | null; package_url: string | null }
}

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
    mutationFn: async (tag?: string) => (await apiClient.post<{ 200: SoftwareStatus }, unknown, true>({
      url: `/api/admin/software/${action}`, security, throwOnError: true,
      ...(action === 'install' ? { body: { tag }, headers: { 'Content-Type': 'application/json' } } : {}),
    })).data,
    onSuccess: (status) => client.setQueryData(key, status),
  })
}
