import type { NotificationItem } from "../../types";

import React from "react";
import { Drawer, DrawerContent } from "@heroui/react";

import { NotificationTab } from "../../types";
import { NotificationsList } from "./NotificationsList";

export interface NotificationsDrawerProps {
  isOpen: boolean;
  onClose: () => void;
  activeTab: NotificationTab;
  notifications: NotificationItem[];
  tabCounts: Record<NotificationTab, number>;
  onTabChange: (tab: NotificationTab) => void;
  onMarkAllAsRead: () => void;
  onArchiveAll: () => void;
}

/**
 * 通知抽屉组件
 */
export const NotificationsDrawer: React.FC<NotificationsDrawerProps> = ({
  isOpen,
  onClose,
  ...listProps
}) => {
  return (
    <Drawer
      hideCloseButton
      isOpen={isOpen}
      placement="right"
      size="sm"
      onClose={onClose}
    >
      <DrawerContent>
        <NotificationsList
          {...listProps}
          scrollHeight="h-[calc(100vh-180px)]"
        />
      </DrawerContent>
    </Drawer>
  );
};
