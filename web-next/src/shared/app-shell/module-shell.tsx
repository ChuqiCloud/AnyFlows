import type { ModuleShellProps } from "./types";

export function ModuleShell({
  children,
  title,
  sidebar,
  navbar,
  contentClassName = "min-w-0 flex-1",
  shellClassName = "flex h-screen w-full flex-col",
  sidebarClassName = "shrink-0",
}: ModuleShellProps) {
  return (
    <div className={shellClassName}>
      {navbar}
      <div className="flex flex-1 overflow-hidden">
        {sidebar ? <aside className={sidebarClassName}>{sidebar}</aside> : null}
        <main className={contentClassName}>
          {title ? (
            <div className="border-b border-divider px-6 py-4">
              <h1 className="text-lg font-semibold">{title}</h1>
            </div>
          ) : null}
          {children}
        </main>
      </div>
    </div>
  );
}
