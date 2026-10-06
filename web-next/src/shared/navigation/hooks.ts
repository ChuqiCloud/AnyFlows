import { useContext, useMemo } from "react";
import { useTranslation } from "react-i18next";

import { NavigationContext } from "./context";
import type { NavNode, NavigationQuery } from "./types";
import { filterNavigationNodes } from "./filters";
import { useAccessControl } from "@/shared/access-control";

export function useNavigation() {
  const context = useContext(NavigationContext);

  if (!context) {
    throw new Error("useNavigation 必须在 NavigationProvider 内使用");
  }

  return context;
}

export function useNavigationNodes(query?: NavigationQuery) {
  const { t } = useTranslation();
  const { registry } = useNavigation();
  const access = useAccessControl();

  /*
   * 注册表里存的是 i18n key（acmeidc 原件存字面量），统一在这里翻译一次：
   * Navbar / ProductDrawer 等展示组件保持原件不变，切换语言也能整体生效。
   * 未命中词条时回退原始值，字面量标题仍然可直接工作。
   */
  const nodes = useMemo(() => {
    const translate = (list: NavNode[]): NavNode[] =>
      list.map((node) => ({
        ...node,
        title: t(node.title, { defaultValue: node.title }),
        description:
          node.description === undefined
            ? undefined
            : t(node.description, { defaultValue: node.description }),
        sectionTitle:
          node.sectionTitle === undefined
            ? undefined
            : t(node.sectionTitle, { defaultValue: node.sectionTitle }),
        children: node.children ? translate(node.children) : undefined,
      }));

    return translate(registry);
  }, [registry, t]);

  return useMemo<NavNode[]>(
    () =>
      filterNavigationNodes(
        nodes,
        {
          hasCapability:
            access.status === "ready" ? access.hasCapability : () => true,
          canAccessSurface: access.canAccessSurface,
        },
        query,
      ),
    [access.canAccessSurface, access.hasCapability, access.status, nodes, query],
  );
}
