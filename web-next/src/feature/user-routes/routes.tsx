import { createModuleRouteMetaResolver } from '@/shared/modules'
import { lazyRoute, type AppRouteObject } from '@/shared/router'
import userRoutesModule from './module'

const routeMeta = createModuleRouteMetaResolver(userRoutesModule, { requireAuth: true })
export const userRoutesRoutes: AppRouteObject[] = [{
  path: '/console/user-routes', element: lazyRoute(() => import('@/features/user-routes/user-route-page'), 'UserRoutePage'), meta: routeMeta('/console/user-routes'),
}]
