import type { ReactNode } from "react";

import { Navbar } from "@/shared/nav";

import { ModuleNavigationSidebar } from "./sidebar";
import { ModuleShell } from "./module-shell";

export interface ModuleLayoutProps {
  children: ReactNode;
  module: string;
  title: string;
  defaultSelectedKey: string;
}

export function ModuleLayout({
  children,
  module,
  title,
  defaultSelectedKey,
}: ModuleLayoutProps) {
  return (
    <ModuleShell
      contentClassName="flex-1 overflow-auto"
      navbar={<Navbar />}
      sidebar={
        <ModuleNavigationSidebar
          defaultSelectedKey={defaultSelectedKey}
          module={module}
          title={title}
        />
      }
    >
      {children}
    </ModuleShell>
  );
}
