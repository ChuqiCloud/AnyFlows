import { useQuery } from '@tanstack/react-query'

import { apiClient } from '@/lib/api'

export type PublicServiceStatus = {
  sampled_at: number
  period_start: number
  period_end: number
  status: 'unavailable' | 'unknown' | 'insufficient_data' | 'operational' | 'degraded' | 'monitoring'
  api_ready: boolean
  sample_count: number | null
  successful_request_count: number | null
  failed_request_count: number | null
  unknown_request_count: number | null
  availability_percent: number | null
  average_success_duration_ms: number | null
  hourly: Array<{ period_start: number; successful_request_count: number; failed_request_count: number; unknown_request_count: number }>
}

export function usePublicServiceStatus() {
  return useQuery({
    queryKey: ['public-service-status'],
    queryFn: async ({ signal }) => (await apiClient.get<{ 200: PublicServiceStatus }, unknown, true>({ url: '/api/public/service-status', signal, throwOnError: true })).data,
    staleTime: 20_000,
    refetchInterval: 60_000,
    refetchIntervalInBackground: false,
  })
}
