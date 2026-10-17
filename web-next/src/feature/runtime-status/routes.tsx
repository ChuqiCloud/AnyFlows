import { createModuleRouteMetaResolver } from '@/shared/modules'
import { lazyRoute, type AppRouteObject } from '@/shared/router'
import runtimeStatusModule from './module'

const routeMeta = createModuleRouteMetaResolver(runtimeStatusModule, { requireAuth: true, surface: 'admin' })

export const runtimeStatusRoutes: AppRouteObject[] = [
  {
    path: '/console/runtime-status',
    element: lazyRoute(() => import('@/features/runtime-status/runtime-status-page'), 'RuntimeStatusPage'),
    meta: routeMeta('/console/runtime-status'),
  },
]
