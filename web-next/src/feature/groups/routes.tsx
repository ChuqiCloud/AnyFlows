import { createModuleRouteMetaResolver } from "@/shared/modules";
import { lazyRoute, type AppRouteObject } from "@/shared/router";

import groupsModule from "./module";

const routeMeta = createModuleRouteMetaResolver(groupsModule, {
  requireAuth: true,
});

export const groupsRoutes: AppRouteObject[] = [
  {
    path: "/console/system-settings/groups",
    element: lazyRoute(
      () => import("@/features/groups/group-page"),
      "GroupPage",
    ),
    meta: routeMeta("/console/system-settings/groups"),
  },
];
