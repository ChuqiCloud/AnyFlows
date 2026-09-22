import type { NavNode, NavigationScope } from "./types";

function pruneNavigationNode(node: NavNode): NavNode | null {
  const children = node.children
    ?.map((child) => pruneNavigationNode(child))
    .filter((child): child is NavNode => child !== null);

  if (!node.href && (!children || children.length === 0)) {
    return null;
  }

  return {
    ...node,
    children,
  };
}

function applyScopeToNode(
  node: NavNode,
  scope?: NavigationScope,
): NavNode {
  const keyPrefix = scope?.keyPrefix ? `${scope.keyPrefix}:` : "";
  const nextKey =
    keyPrefix && !node.key.startsWith(keyPrefix)
      ? `${keyPrefix}${node.key}`
      : node.key;

  return {
    ...node,
    key: nextKey,
    group: node.group ?? scope?.group,
    module: node.module ?? scope?.module,
    surface: node.surface ?? scope?.surface,
    children: node.children?.map((child) => applyScopeToNode(child, scope)),
  };
}

export function defineNavigation(nodes: NavNode[], scope?: NavigationScope) {
  return nodes
    .map((node) => applyScopeToNode(node, scope))
    .map((node) => pruneNavigationNode(node))
    .filter((node): node is NavNode => node !== null);
}

export function mergeNavigationNodes(existing: NavNode[], incoming: NavNode[]) {
  const registry = new Map(existing.map((node) => [node.key, node]));

  for (const node of incoming) {
    registry.set(node.key, node);
  }

  return Array.from(registry.values());
}
