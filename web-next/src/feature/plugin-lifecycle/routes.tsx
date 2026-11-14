import { createModuleRouteMetaResolver } from '@/shared/modules'
import { lazyRoute, type AppRouteObject } from '@/shared/router'
import pluginLifecycleModule from './module'

const routeMeta = createModuleRouteMetaResolver(pluginLifecycleModule, { requireAuth: true, surface: 'admin' })

export const pluginLifecycleRoutes: AppRouteObject[] = [
  {
    path: '/console/plugin-lifecycle',
    element: lazyRoute(() => import('@/features/plugin-lifecycle/plugin-lifecycle-page'), 'PluginLifecyclePage'),
    meta: routeMeta('/console/plugin-lifecycle'),
  },
]
