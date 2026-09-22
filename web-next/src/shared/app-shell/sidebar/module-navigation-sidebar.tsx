import { useMemo } from "react";

import { useAccessControl } from "@/shared/access-control";
import { filterNavigationNodes, useNavigation } from "@/shared/navigation";
import {
  ModuleSidebarLayoutContainer,
  type ModuleSidebarLayoutContainerProps,
} from "@/shared/app-shell";

export interface ModuleNavigationSidebarProps extends Omit<
  ModuleSidebarLayoutContainerProps,
  "items"
> {
  module: string;
}

export function ModuleNavigationSidebar({
  module,
  ...props
}: ModuleNavigationSidebarProps) {
  const { registry } = useNavigation();
  const access = useAccessControl();

  const nodes = useMemo(
    () =>
      filterNavigationNodes(
        registry,
        {
          hasCapability:
            access.status === "ready" ? access.hasCapability : () => true,
          canAccessSurface: access.canAccessSurface,
        },
        {
          group: "module",
          module,
        },
      ),
    [
      access.canAccessSurface,
      access.hasCapability,
      module,
      registry,
    ],
  );

  return (
    <ModuleSidebarLayoutContainer
      {...props}
      items={nodes}
    />
  );
}
