import type { ReactNode } from "react";

export interface ShellSlotProps {
  children?: ReactNode;
}

export interface AppShellProps extends ShellSlotProps {}

export interface ModuleShellProps extends ShellSlotProps {
  title?: string;
  sidebar?: ReactNode;
  navbar?: ReactNode;
  contentClassName?: string;
  shellClassName?: string;
  sidebarClassName?: string;
}

export interface PageShellProps extends ShellSlotProps {
  title?: string;
  description?: string;
}
