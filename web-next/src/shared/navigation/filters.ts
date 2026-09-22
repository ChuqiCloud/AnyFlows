import type { NavNode, NavigationQuery } from "./types";

export interface NavigationFilterOptions {
  hasCapability: (requirement?: string | string[]) => boolean;
  canAccessSurface: (surface?: NavNode["surface"]) => boolean;
}

const matchesQuery = (node: NavNode, query?: NavigationQuery) => {
  if (!query) {
    return true;
  }

  if (query.group && node.group !== query.group) {
    return false;
  }

  if (query.module && node.module !== query.module) {
    return false;
  }

  return true;
};

const sortNodes = (nodes: NavNode[]) =>
  [...nodes].sort((left, right) => {
    const leftOrder = left.order ?? Number.MAX_SAFE_INTEGER;
    const rightOrder = right.order ?? Number.MAX_SAFE_INTEGER;

    if (leftOrder !== rightOrder) {
      return leftOrder - rightOrder;
    }

    return left.title.localeCompare(right.title);
  });

export function filterNavigationNodes(
  nodes: NavNode[],
  options: NavigationFilterOptions,
  query?: NavigationQuery,
): NavNode[] {
  return sortNodes(nodes).flatMap((node) => {
    if (!matchesQuery(node, query)) {
      return [];
    }

    if (!options.hasCapability(node.capability)) {
      return [];
    }

    if (!options.canAccessSurface(node.surface)) {
      return [];
    }

    const children = node.children
      ? filterNavigationNodes(node.children, options)
      : undefined;

    if (node.children && (!children || children.length === 0) && !node.href) {
      return [];
    }

    return [
      {
        ...node,
        children,
      },
    ];
  });
}

export function groupNavigationNodesBySection(nodes: NavNode[]) {
  const sections = new Map<
    string,
    {
      key: string;
      title: string;
      items: NavNode[];
    }
  >();

  for (const node of nodes) {
    const key = node.section ?? "default";
    const title = node.sectionTitle ?? "其他";
    const current = sections.get(key);

    if (current) {
      current.items.push(node);
      continue;
    }

    sections.set(key, {
      key,
      title,
      items: [node],
    });
  }

  return Array.from(sections.values());
}
