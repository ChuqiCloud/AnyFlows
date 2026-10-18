import { createModuleRouteMetaResolver } from '@/shared/modules'
import { lazyRoute, type AppRouteObject } from '@/shared/router'
import clientProfilesModule from './module'
const routeMeta = createModuleRouteMetaResolver(clientProfilesModule, { requireAuth: true })
export const clientProfilesRoutes: AppRouteObject[] = [{
  path: '/console/client-profiles',
  element: lazyRoute(() => import('@/features/client-profiles/client-profile-page'), 'ClientProfilePage'),
  meta: routeMeta('/console/client-profiles'),
}]
