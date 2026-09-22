import { Outlet } from "react-router-dom";

import { ModuleLayout, type ModuleLayoutProps } from "./module-layout";

export type ModuleRouteLayoutProps = Omit<ModuleLayoutProps, "children">;

export function ModuleRouteLayout(props: ModuleRouteLayoutProps) {
  return (
    <ModuleLayout {...props}>
      <Outlet />
    </ModuleLayout>
  );
}
