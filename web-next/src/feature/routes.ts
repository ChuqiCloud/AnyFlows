import type { AppRouteObject } from "@/shared/router";

type RoutesFile = {
  [key: string]: unknown;
};

const routeFiles = import.meta.glob<RoutesFile>("./*/routes.tsx", {
  eager: true,
});

/*
 * 约定：每个业务域的 routes.tsx 导出一个以 Routes 结尾的命名导出。
 * 这里不再用 module.id 拼路径查找——域文件夹名是连字符（api-keys），
 * 而 module.id 是驼峰（apiKeys），两者不必一致，拼路径会让路由被静默丢弃。
 * 各域路径互不重复，useRoutes 按具体度排名，声明顺序不影响匹配。
 */
const resolveRoutesExport = (routeFile: RoutesFile): AppRouteObject[] => {
  const exportName = Object.keys(routeFile).find((name) =>
    name.endsWith("Routes"),
  );

  if (!exportName) {
    return [];
  }

  const routes = routeFile[exportName];

  return Array.isArray(routes) ? (routes as AppRouteObject[]) : [];
};

export const forgeRoutes: AppRouteObject[] = Object.values(routeFiles).flatMap(
  (routeFile) => resolveRoutesExport(routeFile),
);
