import { useEffect, useState } from 'react'
import { useQuery } from '@tanstack/react-query'

import { apiClient } from '@/lib/api'
import type { RuntimeStatus } from './runtime-status-model'

export function useRuntimeStatus() {
  const [visible, setVisible] = useState(() => document.visibilityState === 'visible')
  useEffect(() => {
    const onVisibility = () => setVisible(document.visibilityState === 'visible')
    document.addEventListener('visibilitychange', onVisibility)
    return () => document.removeEventListener('visibilitychange', onVisibility)
  }, [])
  return useQuery({
    queryKey: ['admin-runtime-status'],
    enabled: visible,
    queryFn: async ({ signal }) => {
      const { data } = await apiClient.get<{ 200: RuntimeStatus }, unknown, true>({
        url: '/api/admin/runtime-status',
        security: [{ key: 'bearerAuth', scheme: 'bearer', type: 'http' }],
        signal,
        throwOnError: true,
      })
      return data
    },
    staleTime: 5_000,
    refetchInterval: visible ? 10_000 : false,
    refetchIntervalInBackground: false,
    retry: false,
    gcTime: 0,
  })
}
