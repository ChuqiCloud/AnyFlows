import { createModuleRouteMetaResolver } from "@/shared/modules";
import { lazyRoute, type AppRouteObject } from "@/shared/router";

import modelManagementModule from "./module";

const routeMeta = createModuleRouteMetaResolver(modelManagementModule, {
  requireAuth: true,
});

export const modelManagementRoutes: AppRouteObject[] = [
  {
    path: "/console/system-settings/models",
    element: lazyRoute(
      () => import("@/features/model-management/model-management-page"),
      "ModelManagementPage",
    ),
    meta: routeMeta("/console/system-settings/models"),
  },
  {
    path: "/console/system-settings/model-providers",
    element: lazyRoute(
      () => import("@/features/model-management/model-provider-catalog-page"),
      "ModelProviderCatalogPage",
    ),
    meta: routeMeta("/console/system-settings/model-providers"),
  },
];
