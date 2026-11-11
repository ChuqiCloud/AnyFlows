import { createModuleRouteMetaResolver } from "@/shared/modules";
import { lazyRoute, type AppRouteObject } from "@/shared/router";

import apiKeysModule from "./module";

const routeMeta = createModuleRouteMetaResolver(apiKeysModule, {
  requireAuth: true,
});

export const apiKeysRoutes: AppRouteObject[] = [
  {
    path: "/console/api-keys",
    element: lazyRoute(
      () => import("@/features/api-keys/access-credentials-page"),
      "AccessCredentialsPage",
    ),
    meta: routeMeta("/console/api-keys"),
  },
];
