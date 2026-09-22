import { createModuleRouteMetaResolver } from "@/shared/modules";
import { lazyRoute, type AppRouteObject } from "@/shared/router";

import invitationsModule from "./module";

const routeMeta = createModuleRouteMetaResolver(invitationsModule, {
  requireAuth: true,
});

export const invitationsRoutes: AppRouteObject[] = [
  {
    path: "/console/invitations",
    element: lazyRoute(
      () => import("@/features/invitations/invitation-page"),
      "InvitationPage",
    ),
    meta: routeMeta("/console/invitations"),
  },
];
