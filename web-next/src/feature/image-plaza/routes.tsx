import { createModuleRouteMetaResolver } from "@/shared/modules";
import { lazyRoute, type AppRouteObject } from "@/shared/router";

import imagePlazaModule from "./module";

const routeMeta = createModuleRouteMetaResolver(imagePlazaModule, {
  requireAuth: true,
});

export const imagePlazaRoutes: AppRouteObject[] = [
  {
    path: "/console/image-plaza",
    element: lazyRoute(
      () => import("@/features/image-plaza/image-plaza-page"),
      "ImagePlazaPage",
    ),
    meta: routeMeta("/console/image-plaza"),
  },
];
