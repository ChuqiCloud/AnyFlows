import type { AppShellProps } from "./types";

export function AppShell({ children }: AppShellProps) {
  return (
    <div className="min-h-screen bg-background text-foreground">{children}</div>
  );
}
