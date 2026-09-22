import type { NavNode } from "@/shared/navigation/types";
import type { FeatureReducersMap } from "@/shared/store/feature-registry";

export interface ModuleShellDefinition {
  title: string;
  defaultSelectedKey: string;
}

export interface ModuleDefinition<
  TReducers extends Partial<FeatureReducersMap> | undefined =
    | Partial<FeatureReducersMap>
    | undefined,
> {
  id: string;
  order: number;
  navigation?: readonly NavNode[];
  shell?: ModuleShellDefinition;
  reducers?: TReducers;
}
