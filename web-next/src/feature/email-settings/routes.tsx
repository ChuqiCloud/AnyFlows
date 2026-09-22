import { createModuleRouteMetaResolver } from "@/shared/modules";
import { lazyRoute, type AppRouteObject } from "@/shared/router";

import emailSettingsModule from "./module";

const routeMeta = createModuleRouteMetaResolver(emailSettingsModule, {
  requireAuth: true,
});

export const emailSettingsRoutes: AppRouteObject[] = [
  {
    path: "/console/system-settings/email",
    element: lazyRoute(
      () => import("@/features/email-settings/email-settings-page"),
      "EmailSettingsPage",
    ),
    meta: routeMeta("/console/system-settings/email"),
  },
];
