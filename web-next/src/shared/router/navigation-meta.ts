import type { NavNode } from "@/shared/navigation";

import type { AppRouteMeta } from "./types";

const normalizeHref = (href: string) => {
  if (href === "/") {
    return href;
  }

  return href.replace(/\/+$/, "");
};

function findNavigationNodeByHref(
  nodes: NavNode[],
  href: string,
): NavNode | null {
  const normalizedHref = normalizeHref(href);

  for (const node of nodes) {
    if (node.href && normalizeHref(node.href) === normalizedHref) {
      return node;
    }

    if (!node.children) {
      continue;
    }

    const matchedChild = findNavigationNodeByHref(node.children, href);

    if (matchedChild) {
      return matchedChild;
    }
  }

  return null;
}

function compactMeta(meta: AppRouteMeta): AppRouteMeta {
  return Object.fromEntries(
    Object.entries(meta).filter(([, value]) => value !== undefined),
  ) as AppRouteMeta;
}

export function createRouteMetaResolver(
  nodes: NavNode[],
  defaults: AppRouteMeta = {},
) {
  return (href?: string, overrides: AppRouteMeta = {}) => {
    const matchedNode = href ? findNavigationNodeByHref(nodes, href) : null;

    return compactMeta({
      requireAuth: true,
      ...defaults,
      module: matchedNode?.module ?? defaults.module,
      capability: matchedNode?.capability ?? defaults.capability,
      surface: matchedNode?.surface ?? defaults.surface,
      ...overrides,
    });
  };
}
