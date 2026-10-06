import { createModuleRouteMetaResolver } from "@/shared/modules";
import { lazyRoute, type AppRouteObject } from "@/shared/router";

import debugTracesModule from "./module";

const routeMeta = createModuleRouteMetaResolver(debugTracesModule, {
  requireAuth: true,
});

export const debugTracesRoutes: AppRouteObject[] = [
  {
    path: "/console/debug-traces",
    element: lazyRoute(
      () => import("@/features/debug-traces/debug-trace-page"),
      "DebugTracePage",
    ),
    meta: routeMeta("/console/debug-traces"),
  },
];
