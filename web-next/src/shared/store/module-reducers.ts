import type { ModuleDefinition } from "@/shared/modules";
import type { FeatureReducersMap } from "./feature-registry";

import { forgeModules } from "@/feature//modules";
import { store, type AppStore } from "./index";

export type ModuleReducersRegistry = Record<
  string,
  Partial<FeatureReducersMap> | undefined
>;

export function collectModuleReducersById(modules: readonly ModuleDefinition[]) {
  return modules.reduce<ModuleReducersRegistry>((registry, module) => {
    registry[module.id] = module.reducers;

    return registry;
  }, {});
}

export const featureReducersByModule = collectModuleReducersById(forgeModules);

export function getModuleReducers(moduleId?: string) {
  if (!moduleId) {
    return undefined;
  }

  return featureReducersByModule[moduleId];
}

export function ensureModuleReducers(moduleId?: string) {
  const reducers = getModuleReducers(moduleId);

  if (!reducers) {
    return false;
  }

  return (store as AppStore).injectReducers(reducers);
}
