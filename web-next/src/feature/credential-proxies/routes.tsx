import { createModuleRouteMetaResolver } from "@/shared/modules";
import { lazyRoute, type AppRouteObject } from "@/shared/router";

import credentialProxiesModule from "./module";

const routeMeta = createModuleRouteMetaResolver(credentialProxiesModule, {
  requireAuth: true,
});

export const credentialProxiesRoutes: AppRouteObject[] = [
  {
    path: "/console/proxies",
    element: lazyRoute(
      () => import("@/features/credential-proxies/credential-proxy-page"),
      "CredentialProxyPage",
    ),
    meta: routeMeta("/console/proxies"),
  },
];
