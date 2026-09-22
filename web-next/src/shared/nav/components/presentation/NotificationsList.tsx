import type { NotificationItem as NotificationItemType } from "../../types";

import React from "react";
import { Button, Chip, cn, ScrollShadow, Tab, Tabs } from "@heroui/react";
import { Icon } from "@iconify/react";
import { useTranslation } from "react-i18next";

import { NotificationTab } from "../../types";

import NotificationItem from "./NotificationItem";

export interface NotificationsListProps {
  activeTab: NotificationTab;
  notifications: NotificationItemType[];
  tabCounts: Record<NotificationTab, number>;
  onTabChange: (tab: NotificationTab) => void;
  onMarkAllAsRead: () => void;
  onArchiveAll: () => void;
  className?: string;
  scrollHeight?: string;
}

/**
 * 通知列表组件（共享逻辑）
 */
export const NotificationsList: React.FC<NotificationsListProps> = ({
  activeTab,
  notifications,
  tabCounts,
  onTabChange,
  onMarkAllAsRead,
  onArchiveAll,
  className,
  scrollHeight = "h-[400px]",
}) => {
  const { t } = useTranslation();
  return (
    <div className={cn("flex flex-col", className)}>
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-3 border-b border-divider">
        <div className="flex items-center gap-2">
          <h3 className="text-base font-semibold">{t("notifications.title")}</h3>
          {tabCounts.unread > 0 && (
            <Chip color="danger" size="sm" variant="flat">
              {tabCounts.unread}
            </Chip>
          )}
        </div>
        <Button
          color="primary"
          isDisabled={tabCounts.unread === 0}
          size="sm"
          variant="light"
          onPress={onMarkAllAsRead}
        >
          {t("notifications.markLoadedRead")}
        </Button>
      </div>

      {/* Tabs */}
      <Tabs
        aria-label={t("notifications.categories")}
        classNames={{
          base: "w-full",
          tabList:
            "gap-4 px-4 py-0 w-full relative rounded-none border-b border-divider",
          cursor: "w-full",
          tab: "max-w-fit px-1 h-10",
        }}
        color="primary"
        selectedKey={activeTab}
        variant="underlined"
        onSelectionChange={(key) => onTabChange(key as NotificationTab)}
      >
        <Tab
          key={NotificationTab.All}
          title={
            <div className="flex items-center gap-1.5">
              <span>{t("notifications.all")}</span>
              <Chip size="sm" variant="flat">
                {tabCounts.all}
              </Chip>
            </div>
          }
        />
        <Tab
          key={NotificationTab.Unread}
          title={
            <div className="flex items-center gap-1.5">
              <span>{t("notifications.unread")}</span>
              {tabCounts.unread > 0 && (
                <Chip color="danger" size="sm" variant="flat">
                  {tabCounts.unread}
                </Chip>
              )}
            </div>
          }
        />
        <Tab key={NotificationTab.Archive} title={t("notifications.archive")} />
      </Tabs>

      {/* List */}
      <ScrollShadow className={cn("w-full", scrollHeight)}>
        {notifications.length > 0 ? (
          notifications.map((item) => (
            <NotificationItem key={item.id} {...item} />
          ))
        ) : (
          <div className="flex flex-col items-center justify-center h-full gap-2 py-12">
            <Icon
              className="text-default-300"
              icon="solar:bell-off-linear"
              width={48}
            />
            <p className="text-small text-default-400">
              {activeTab === NotificationTab.Archive
                ? t("notifications.emptyArchive")
                : activeTab === NotificationTab.Unread
                  ? t("notifications.emptyUnread")
                  : t("notifications.empty")}
            </p>
          </div>
        )}
      </ScrollShadow>

      {/* Footer */}
      <div className="flex items-center justify-end gap-2 px-4 py-3 border-t border-divider">
        <Button size="sm" variant="light">
          {t("notifications.settings")}
        </Button>
        {activeTab !== NotificationTab.Archive && notifications.length > 0 && (
          <Button size="sm" variant="flat" onPress={onArchiveAll}>
            {t("notifications.archiveAll")}
          </Button>
        )}
      </div>
    </div>
  );
};
