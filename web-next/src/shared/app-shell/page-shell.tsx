import type { PageShellProps } from "./types";

export function PageShell({ children, title, description }: PageShellProps) {
  return (
    <section className="px-6 py-4">
      {title ? (
        <header className="mb-4">
          <h2 className="text-base font-semibold">{title}</h2>
          {description ? (
            <p className="mt-1 text-sm text-default-500">{description}</p>
          ) : null}
        </header>
      ) : null}
      {children}
    </section>
  );
}
