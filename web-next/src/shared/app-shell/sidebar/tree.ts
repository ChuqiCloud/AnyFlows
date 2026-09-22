import type { SidebarNode } from "./types";

const isNavigableHref = (href?: string): href is string =>
  Boolean(href && href !== "#");

export const getSidebarNodeChildren = (item: SidebarNode): SidebarNode[] =>
  item.children ?? [];

export const findSidebarKeyByPath = (
  items: SidebarNode[],
  path: string,
): string | null => {
  /*
   * 全树收集匹配后再取最优：若在每棵子树内就地取最优并立即返回，
   * 第一棵子树的前缀匹就会短路整棵树。这里 /console 是所有控制台路径的前缀，
   * 先出现的「运行概览」会抢在真正命中的节点之前被返回。
   */
  const matches: Array<{ key: string; score: number }> = [];

  const collect = (nodes: SidebarNode[]) => {
    for (const node of nodes) {
      if (isNavigableHref(node.href)) {
        if (path === node.href) {
          matches.push({ key: node.key, score: Number.MAX_SAFE_INTEGER });
        } else if (path.startsWith(`${node.href}/`)) {
          matches.push({ key: node.key, score: node.href.length });
        }
      }

      collect(getSidebarNodeChildren(node));
    }
  };

  collect(items);
  matches.sort((left, right) => right.score - left.score);

  return matches[0]?.key ?? null;
};

/** 取树中第一个可导航条目的 key：用于当前路由不在侧栏里时兜底，保证选中态始终落在真实存在的条目上。 */
export const findFirstSidebarKey = (items: SidebarNode[]): string | null => {
  for (const item of items) {
    if (isNavigableHref(item.href)) {
      return item.key;
    }

    const nestedKey = findFirstSidebarKey(getSidebarNodeChildren(item));

    if (nestedKey) {
      return nestedKey;
    }
  }

  return null;
};

const containsSidebarKey = (nodes: SidebarNode[], key: string): boolean =>
  nodes.some((node) => (
    node.key === key || containsSidebarKey(getSidebarNodeChildren(node), key)
  ));

/**
 * 当前路由落在哪个顶层板块，供侧栏板块切换器跟随路由。
 * 必须先在全树选出最优条目再回溯板块：/console 是所有控制台路径的前缀，
 * 逐板块试匹配会让渠道、系统设置这类页面被开头的「运行概览」抢走。
 */
export const findSidebarSectionKeyByPath = (
  items: SidebarNode[],
  path: string,
): string | null => {
  const key = findSidebarKeyByPath(items, path);

  if (key) {
    const section = items.find((item) => containsSidebarKey([item], key));

    if (section) {
      return section.key;
    }
  }

  return items[0]?.key ?? null;
};

export const findSidebarHrefByKey = (
  items: SidebarNode[],
  key: string,
): string | null => {
  for (const item of items) {
    if (item.key === key && isNavigableHref(item.href)) {
      return item.href;
    }

    const nestedHref = findSidebarHrefByKey(getSidebarNodeChildren(item), key);

    if (nestedHref) {
      return nestedHref;
    }
  }

  return null;
};
