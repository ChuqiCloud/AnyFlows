import type { ListboxProps, Selection } from "@heroui/react";
import type { SidebarNode, SidebarProps } from "./types";

import React from "react";

import { SidebarPresentation } from "@/shared/app-shell";

export type SidebarContainerProps = Omit<
  ListboxProps<SidebarNode>,
  "children"
> &
  SidebarProps & {
    onItemAction?: (key: React.Key) => void;
  };

export const SidebarContainer = React.forwardRef<
  HTMLElement,
  SidebarContainerProps
>(({ defaultSelectedKey, onSelect, onItemAction, ...props }, ref) => {
  /*
   * 选中态直接跟随路由（defaultSelectedKey 已是当前路由推导出的 key），不在本地存 state。
   * 侧栏条目会随会话角色、企业权限异步重建，一旦选中项在新集合里不存在，
   * HeroUI 会以空集合回调 selectionChange；本地 state 被清成 undefined 后，
   * 只要路由不变就再也不会恢复，表现就是「选中态背景消失」。
   */
  const handleSelectionChange = React.useCallback(
    (keys: Selection) => {
      const key = Array.from(keys)[0];

      if (key === undefined) {
        return;
      }

      onSelect?.(key as string);
      onItemAction?.(key);
    },
    [onSelect, onItemAction],
  );

  return (
    <SidebarPresentation
      ref={ref}
      selected={defaultSelectedKey ?? ""}
      onSelectionChange={handleSelectionChange}
      {...props}
    />
  );
});

SidebarContainer.displayName = "SidebarContainer";
