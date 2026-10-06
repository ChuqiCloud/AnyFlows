import type { ReactNode } from "react";
import type { AccessSurface } from "@/shared/access-control";

export type NavigationGroup = "top" | "product" | "module";
export type NavigationVariant = "link" | "icon";

export interface NavigationScope {
  group?: NavigationGroup;
  module?: string;
  surface?: AccessSurface;
  keyPrefix?: string;
}

export interface NavNode {
  key: string;
  title: string;
  href?: string;
  icon?: string;
  description?: string;
  module?: string;
  surface?: AccessSurface;
  capability?: string | string[];
  group?: NavigationGroup;
  variant?: NavigationVariant;
  badge?: number;
  section?: string;
  sectionTitle?: string;
  order?: number;
  endContent?: ReactNode;
  children?: NavNode[];
}

export interface NavigationQuery {
  group?: NavigationGroup;
  module?: string;
}

export interface NavigationContextValue {
  registry: NavNode[];
  register: (nodes: NavNode[]) => void;
  replace: (nodes: NavNode[]) => void;
  reset: () => void;
}
