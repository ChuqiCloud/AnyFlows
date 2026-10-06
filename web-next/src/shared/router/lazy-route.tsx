import {
  lazy,
  type ComponentType,
  type ReactNode,
} from "react";

import { ensureModuleReducers } from "@/shared/store/module-reducers";

import { withRouteSuspense } from "./route-suspense";

type RouteModule = Record<string, unknown>;

export const lazyRoute = <
  TModule extends RouteModule,
  TExportName extends keyof TModule,
>(
  loader: () => Promise<TModule>,
  exportName: TExportName,
  fallback?: ReactNode,
) => {
  const LazyComponent = lazy(async () => {
    const module = await loader();

    return {
      default: module[exportName] as ComponentType<any>,
    };
  });

  return withRouteSuspense(<LazyComponent />, fallback);
};

export const lazyModuleRoute = <
  TModule extends RouteModule,
  TExportName extends keyof TModule,
>(
  moduleId: string,
  loader: () => Promise<TModule>,
  exportName: TExportName,
  fallback?: ReactNode,
) => {
  const LazyComponent = lazy(async () => {
    ensureModuleReducers(moduleId);

    const module = await loader();

    return {
      default: module[exportName] as ComponentType<any>,
    };
  });

  return withRouteSuspense(<LazyComponent />, fallback);
};
