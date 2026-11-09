import { createModuleRouteMetaResolver } from '@/shared/modules'
import { lazyRoute, type AppRouteObject } from '@/shared/router'
import merchantPoolModule from './module'

const routeMeta = createModuleRouteMetaResolver(merchantPoolModule, { requireAuth: true })
export const merchantPoolRoutes: AppRouteObject[] = [
  { path: '/console/merchant-pool', element: lazyRoute(() => import('@/features/merchant-pool/merchant-pool-page'), 'MerchantPoolPage'), meta: routeMeta('/console/merchant-pool') },
  { path: '/console/merchant-pool/routes', element: lazyRoute(() => import('@/features/merchant-pool/merchant-pool-routes-page'), 'MerchantPoolRoutesPage'), meta: routeMeta('/console/merchant-pool/routes') },
  { path: '/console/merchant-pool/merchants/:id', element: lazyRoute(() => import('@/features/merchant-pool/merchant-detail-page'), 'MerchantDetailPage'), meta: routeMeta('/console/merchant-pool/merchants/:id') },
  { path: '/console/merchant', element: lazyRoute(() => import('@/features/merchant-pool/merchant-workspace-page'), 'MerchantWorkspacePage'), meta: routeMeta('/console/merchant') },
  { path: '/console/system-settings/merchant-pool', element: lazyRoute(() => import('@/features/merchant-pool/merchant-admin-page'), 'MerchantAdminPage'), meta: { ...routeMeta('/console/system-settings/merchant-pool'), surface: 'admin' } },
]
