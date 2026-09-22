import { createModuleRouteMetaResolver } from "@/shared/modules";
import { lazyRoute, type AppRouteObject } from "@/shared/router";

import dashboardModule from "./module";

const routeMeta = createModuleRouteMetaResolver(dashboardModule, {
  requireAuth: true,
});

// 控制台首页就是 /console 本身，路由表会把这条绝对路径还原成 index 子路由。
export const dashboardRoutes: AppRouteObject[] = [
  {
    path: "/console",
    element: lazyRoute(
      () => import("@/features/dashboard/dashboard-page"),
      "DashboardPage",
    ),
    meta: routeMeta("/console"),
  },
];
