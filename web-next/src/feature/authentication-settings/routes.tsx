import { createModuleRouteMetaResolver } from "@/shared/modules";
import { lazyRoute, type AppRouteObject } from "@/shared/router";

import authenticationSettingsModule from "./module";

const routeMeta = createModuleRouteMetaResolver(authenticationSettingsModule, {
  requireAuth: true,
});

/** /console/registration 是认证设置的历史别名入口，与主入口指向同一页面。 */
export const authenticationSettingsRoutes: AppRouteObject[] = [
  {
    path: "/console/system-settings/authentication",
    element: lazyRoute(
      () => import("@/features/authentication-settings/authentication-settings-page"),
      "AuthenticationSettingsPage",
    ),
    meta: routeMeta("/console/system-settings/authentication"),
  },
  {
    path: "/console/registration",
    element: lazyRoute(
      () => import("@/features/authentication-settings/authentication-settings-page"),
      "AuthenticationSettingsPage",
    ),
    meta: routeMeta("/console/registration"),
  },
];
