import { createModuleRouteMetaResolver } from "@/shared/modules";
import { lazyRoute, type AppRouteObject } from "@/shared/router";

import channelsModule from "./module";

const routeMeta = createModuleRouteMetaResolver(channelsModule, {
  requireAuth: true,
});

export const channelsRoutes: AppRouteObject[] = [
  {
    path: "/console/channels",
    element: lazyRoute(
      () => import("@/features/channels/channel-page"),
      "ChannelPage",
    ),
    meta: routeMeta("/console/channels"),
  },
];
