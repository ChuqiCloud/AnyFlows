import { lazyRoute, type AppRouteObject } from '@/shared/router'

export const publicStatusRoutes: AppRouteObject[] = [{
  path: '/status',
  element: lazyRoute(() => import('@/features/public-status/public-status-page'), 'PublicStatusPage'),
  meta: { requireAuth: false, hideInNav: true },
}]
