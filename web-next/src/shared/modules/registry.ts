import type { NavNode } from "@/shared/navigation/types";
import type { FeatureReducersMap } from "@/shared/store/feature-registry";

import { mergeNavigationNodes } from "@/shared/navigation/registry";

import type { ModuleDefinition } from "./types";

export function defineModule<const TModule extends ModuleDefinition>(
  module: TModule,
) {
  return module;
}

export function compareModuleDefinitions(
  left: ModuleDefinition,
  right: ModuleDefinition,
) {
  if (left.order !== right.order) {
    return left.order - right.order;
  }

  return left.id.localeCompare(right.id);
}

export function collectModuleNavigation(modules: readonly ModuleDefinition[]) {
  return modules.reduce<NavNode[]>(
    (registry, module) =>
      module.navigation
        ? mergeNavigationNodes(registry, [...module.navigation])
        : registry,
    [],
  );
}

export function collectModuleReducers(modules: readonly ModuleDefinition[]) {
  const reducers = modules.reduce<Partial<FeatureReducersMap>>((registry, module) => {
    if (module.reducers) {
      Object.assign(registry, module.reducers);
    }

    return registry;
  }, {});

  return reducers as FeatureReducersMap;
}
