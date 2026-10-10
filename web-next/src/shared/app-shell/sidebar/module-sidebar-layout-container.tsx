import type { SidebarNode } from "./types";

import { ScrollShadow } from "@heroui/react";
import React, { useCallback, useMemo } from "react";
import { useLocation, useNavigate } from "react-router-dom";

import { SidebarContainer, type SidebarContainerProps } from "@/shared/app-shell";

import { findFirstSidebarKey, findSidebarHrefByKey, findSidebarKeyByPath } from "./tree";

export type ModuleSidebarLayoutContainerProps = {
  title: string;
  items: SidebarNode[];
  defaultSelectedKey: string;
  /** 标题右侧的动作槽，通常放工作空间切换器。 */
  headerAction?: React.ReactNode;
  containerClassName?: string;
  /** 分组/条目样式覆盖，透传给 SidebarContainer。 */
  sectionClasses?: SidebarContainerProps["sectionClasses"];
};

export const ModuleSidebarLayoutContainer = ({
  title,
  items,
  defaultSelectedKey,
  headerAction,
  containerClassName = "h-full border-r border-divider w-60 flex flex-col bg-content1",
  sectionClasses,
}: ModuleSidebarLayoutContainerProps) => {
  const location = useLocation();
  const navigate = useNavigate();
  const pathname = location.pathname;

  const selectedKey = useMemo(() => {
    const matchedKey = findSidebarKeyByPath(items, pathname);

    if (matchedKey) {
      return matchedKey;
    }

    /*
     * 当前路由不在侧栏里时兜底。
     * 这里必须返回一个树里真实存在的 key：HeroUI 对不存在的 key 不渲染任何选中态，
     * 整片侧栏就会变成「没有高亮」。侧栏条目会随会话角色和企业权限异步重建，
     * 用户角色的菜单里也本来就没有 overview，所以不能直接回退到调用方给的默认 key。
     */
    if (findSidebarHrefByKey(items, defaultSelectedKey)) {
      return defaultSelectedKey;
    }

    return findFirstSidebarKey(items) ?? defaultSelectedKey;
  }, [defaultSelectedKey, items, pathname]);

  const handleItemAction = useCallback(
    (key: React.Key) => {
      const href = findSidebarHrefByKey(items, String(key));

      if (href) {
        if (href.startsWith('https://')) {
          window.open(href, '_blank', 'noopener,noreferrer');
        } else {
          navigate(href);
        }
      }
    },
    [items, navigate],
  );

  return (
    <div className={containerClassName}>
      <div className="px-4 py-3 flex items-center justify-between gap-2 border-b border-divider">
        <h2 className="min-w-0 truncate pl-2 text-base font-semibold text-foreground">{title}</h2>
        {headerAction}
      </div>

      <ScrollShadow className="flex-1 py-4 px-3">
        <SidebarContainer
          defaultSelectedKey={selectedKey}
          items={items}
          sectionClasses={sectionClasses}
          onItemAction={handleItemAction}
        />
      </ScrollShadow>
    </div>
  );
};
