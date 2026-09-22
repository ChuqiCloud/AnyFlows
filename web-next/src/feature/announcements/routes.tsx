import { createModuleRouteMetaResolver } from "@/shared/modules";
import { lazyRoute, type AppRouteObject } from "@/shared/router";

import announcementsModule from "./module";

const routeMeta = createModuleRouteMetaResolver(announcementsModule, {
  requireAuth: true,
});

export const announcementsRoutes: AppRouteObject[] = [
  {
    path: "/console/system-settings/announcements",
    element: lazyRoute(
      () => import("@/features/announcements/announcement-page"),
      "AnnouncementPage",
    ),
    meta: routeMeta("/console/system-settings/announcements"),
  },
];
