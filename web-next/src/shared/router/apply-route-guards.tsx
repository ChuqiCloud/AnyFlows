import type { ReactNode } from "react";
import type { RouteObject } from "react-router-dom";

import { Outlet } from "react-router-dom";

import type { AppRouteMeta, AppRouteObject } from "./types";

import { ForbiddenPage } from "./pages";
import { RouteGuard } from "./guard";

interface RouteBoundaryProps {
  children: ReactNode;
  meta?: AppRouteMeta;
}

function RouteBoundary({ children, meta }: RouteBoundaryProps) {
  return (
    <RouteGuard fallback={<ForbiddenPage />} meta={meta}>
      {children}
    </RouteGuard>
  );
}

function wrapRouteElement(element: ReactNode, meta?: AppRouteMeta) {
  if (!meta) {
    return element;
  }

  return <RouteBoundary meta={meta}>{element}</RouteBoundary>;
}

function applyRouteGuard(route: AppRouteObject): RouteObject {
  const { children, element, meta, ...rest } = route;
  const nextChildren = children?.map(applyRouteGuard);

  const nextElement =
    element || nextChildren ? wrapRouteElement(element ?? <Outlet />, meta) : element;

  return {
    ...rest,
    children: nextChildren,
    element: nextElement,
  } as RouteObject;
}

export function applyRouteGuards(routes: AppRouteObject[]): RouteObject[] {
  return routes.map(applyRouteGuard);
}
