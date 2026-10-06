import { createElement } from "react";

import { ModuleRouteLayout, type ModuleRouteLayoutProps } from "@/shared/app-shell";
import type { AppRouteMeta, AppRouteObject } from "@/shared/router";

import { createRouteMetaResolver } from "@/shared/router/navigation-meta";

import type { ModuleDefinition, ModuleShellDefinition } from "./types";

type ModuleWithShell = ModuleDefinition & {
  shell: ModuleShellDefinition;
};

export function createModuleRouteMetaResolver(
  module: ModuleDefinition,
  defaults: AppRouteMeta = {},
) {
  return createRouteMetaResolver(
    module.navigation ? [...module.navigation] : [],
    {
      module: module.id,
      ...defaults,
    },
  );
}

export type ModuleRouteMetaResolver = ReturnType<
  typeof createModuleRouteMetaResolver
>;

export function getModuleRouteLayoutProps(
  module: ModuleWithShell,
): Pick<ModuleRouteLayoutProps, "defaultSelectedKey" | "module" | "title"> {
  return {
    defaultSelectedKey: module.shell.defaultSelectedKey,
    module: module.id,
    title: module.shell.title,
  };
}

interface CreateModuleShellRoutesOptions {
  path: string;
  children: (routeMeta: ModuleRouteMetaResolver) => AppRouteObject[];
  element?: AppRouteObject["element"];
  metaDefaults?: AppRouteMeta;
}

export function createModuleShellRoutes(
  module: ModuleWithShell,
  options: CreateModuleShellRoutesOptions,
): AppRouteObject[] {
  const routeMeta = createModuleRouteMetaResolver(module, options.metaDefaults);

  return [
    {
      path: options.path,
      element:
        options.element ??
        createElement(ModuleRouteLayout, getModuleRouteLayoutProps(module)),
      meta: routeMeta(options.path),
      children: options.children(routeMeta),
    },
  ];
}
