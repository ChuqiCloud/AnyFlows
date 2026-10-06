import {
  collectModuleNavigation,
  collectModuleReducers,
  compareModuleDefinitions,
} from "@/shared/modules";
import type { ModuleDefinition } from "@/shared/modules";

type ModuleFile = {
  default?: ModuleDefinition;
};

const moduleFiles = import.meta.glob<ModuleFile>("./*/module.ts", {
  eager: true,
});

export const forgeModules = Object.values(moduleFiles)
  .map((file) => file.default)
  .filter((module): module is ModuleDefinition => Boolean(module))
  .sort(compareModuleDefinitions);

export const forgeNavigationNodes = collectModuleNavigation(forgeModules);
export const forgeFeatureReducers = collectModuleReducers(forgeModules);
