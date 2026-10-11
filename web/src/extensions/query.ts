import { useQuery } from '@tanstack/react-query'

import { apiClient } from '@/lib/api'
import { listExtensionCatalog } from '@/lib/api/generated/sdk.gen'
import { supportsExtensionRoute } from './model'
import { consoleExtensionRoutes } from './registry'

export function useConsoleExtensionCatalog() {
  return useQuery({
    enabled: consoleExtensionRoutes.length > 0,
    queryKey: ['console-extension-catalog'],
    queryFn: async ({ signal }) => {
      const { data } = await listExtensionCatalog({ client: apiClient, signal })
      return new Set(data.flatMap((extension) => extension.capabilities))
    },
    staleTime: 30_000,
  })
}

export function useConsoleExtensionNavigation(role: 'user' | 'admin') {
  const catalog = useConsoleExtensionCatalog()
  return consoleExtensionRoutes.filter((route) =>
    (route.access !== 'admin' || role === 'admin')
    && catalog.isSuccess && supportsExtensionRoute(route, catalog.data),
  )
}
