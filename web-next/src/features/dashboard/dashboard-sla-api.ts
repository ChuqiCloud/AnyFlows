import { keepPreviousData, useQuery } from '@tanstack/react-query'

import { apiClient } from '@/lib/api'
import { getAdminServiceLevels } from '@/lib/api/generated/sdk.gen'

export function useServiceLevels(dimension: 'model' | 'channel', search: string, page: number, pageSize: number, sort: 'requests' | 'failures') {
  return useQuery({
    queryKey: ['admin-service-levels', dimension, search, page, pageSize, sort],
    placeholderData: keepPreviousData,
    queryFn: async ({ signal }) => {
      const { data } = await getAdminServiceLevels({ client: apiClient, signal, query: { dimension, search, page, page_size: pageSize, sort } })
      return data
    },
    staleTime: 30_000,
  })
}
