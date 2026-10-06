import { createModuleRouteMetaResolver } from "@/shared/modules";
import { lazyRoute, type AppRouteObject } from "@/shared/router";

import networkSettingsModule from "./module";

const routeMeta = createModuleRouteMetaResolver(networkSettingsModule, {
  requireAuth: true,
});

export const networkSettingsRoutes: AppRouteObject[] = [
  {
    path: "/console/system-settings/network",
    element: lazyRoute(
      () => import("@/features/network-settings/network-settings-page"),
      "NetworkSettingsPage",
    ),
    meta: routeMeta("/console/system-settings/network"),
  },
];
