import type { ReactNode } from "react";
import type { RouteObject } from "react-router-dom";
import type { AccessSurface } from "@/shared/access-control";

export interface AppRouteMeta {
  requireAuth?: boolean;
  capability?: string | string[];
  module?: string;
  surface?: AccessSurface;
  hideInNav?: boolean;
}

export interface RouteGuardProps {
  children: ReactNode;
  fallback?: ReactNode;
  meta?: AppRouteMeta;
}

export type AppRouteObject = Omit<RouteObject, "children"> & {
  children?: AppRouteObject[];
  meta?: AppRouteMeta;
};
