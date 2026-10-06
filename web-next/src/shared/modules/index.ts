export type { ModuleDefinition, ModuleShellDefinition } from "./types";
export {
  compareModuleDefinitions,
  collectModuleNavigation,
  collectModuleReducers,
  defineModule,
} from "./registry";
export {
  createModuleRouteMetaResolver,
  createModuleShellRoutes,
  getModuleRouteLayoutProps,
} from "./route-utils";
export type { ModuleRouteMetaResolver } from "./route-utils";
