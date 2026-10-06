import { createModuleRouteMetaResolver } from "@/shared/modules";
import { lazyRoute, type AppRouteObject } from "@/shared/router";

import billingSettingsModule from "./module";

const routeMeta = createModuleRouteMetaResolver(billingSettingsModule, {
  requireAuth: true,
});

export const billingSettingsRoutes: AppRouteObject[] = [
  {
    path: "/console/system-settings/billing",
    element: lazyRoute(
      () => import("@/features/billing-settings/billing-settings-page"),
      "BillingSettingsPage",
    ),
    meta: routeMeta("/console/system-settings/billing"),
  },
];
