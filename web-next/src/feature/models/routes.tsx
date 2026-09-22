import { ModelPage } from "@/features/models/model-page";
import { createModuleRouteMetaResolver } from "@/shared/modules";
import type { AppRouteObject } from "@/shared/router";
import { useConsoleSession } from "@/routes/console-layout";

import modelsModule from "./module";

const routeMeta = createModuleRouteMetaResolver(modelsModule, {
  requireAuth: true,
});

// 模型广场需要按当前会话切换管理员视图，lazyRoute 无法传 props，故保留本地适配组件。
function ModelsRoute() {
  const session = useConsoleSession();

  return <ModelPage authenticated admin={session.user.role === "admin"} />;
}

export const modelsRoutes: AppRouteObject[] = [
  {
    path: "/console/models",
    element: <ModelsRoute />,
    meta: routeMeta("/console/models"),
  },
];
