import { useCallback, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";

import {
  useMarkUserNotificationsRead,
  useUserNotifications,
} from "@/features/profile/profile-api";

import { NotificationTab, type NotificationItem } from "../types";

const formatTime = (value: number, language?: string) =>
  new Intl.DateTimeFormat(language?.startsWith("zh") ? "zh-CN" : "en-US", {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(new Date(value * 1000));

/** 读取 web 版已经接通的通知事实接口，并适配顶部通知抽屉。 */
export const useNotifications = () => {
  const { t, i18n } = useTranslation();
  const [activeTab, setActiveTab] = useState<NotificationTab>(
    NotificationTab.All,
  );
  const [isOpen, setIsOpen] = useState(false);
  const userQuery = useUserNotifications();
  const markUserRead = useMarkUserNotificationsRead();

  const notifications = useMemo<NotificationItem[]>(() => {
    const userEntries = userQuery.data?.pages.flatMap((page) => page.entries) ?? [];
    const language = i18n.resolvedLanguage;

    return [...userEntries].sort((left, right) => right.occurred_at - left.occurred_at).map((entry) => {
      const isRead = entry.read_at !== null && entry.read_at !== undefined;
      const title = entry.kind === "product_update"
        ? (language?.startsWith("zh")
          ? entry.announcement_title_zh ?? entry.announcement_title_en
          : entry.announcement_title_en ?? entry.announcement_title_zh) ?? t("profile.notificationHistory.kind.product_update")
        : t(`profile.notificationHistory.kind.${entry.kind}`);
      const description = entry.kind === "balance_alert"
        ? t("profile.notificationHistory.balanceDetail", {
          observed: entry.observed_quota ?? "--",
          threshold: entry.threshold_quota ?? "--",
        })
        : entry.kind === "subscription_balance_alert"
          ? t("profile.notificationHistory.subscriptionDetail", {
            used: entry.quota_used ?? "--",
            amount: entry.quota_amount ?? "--",
            threshold: entry.threshold_percent ?? "--",
          })
          : entry.kind === "subscription_purchase"
            ? t("profile.notificationHistory.purchaseDetail")
            : entry.announcement_body_zh ?? entry.announcement_body_en ?? t("profile.notificationHistory.productUpdateDetail");

      return {
        id: `user:${entry.id}`,
        icon: entry.kind === "product_update" ? "solar:megaphone-linear" : "solar:bell-linear",
        title,
        description,
        time: formatTime(entry.occurred_at, language),
        type: (entry.kind === "product_update" ? "system" : "default") as NotificationItem["type"],
        isRead,
      };
    });

  }, [i18n.resolvedLanguage, t, userQuery.data]);

  const unreadCount = userQuery.data?.pages[0]?.unread_count ?? 0;
  const unreadNotifications = notifications.filter((item) => !item.isRead);
  const tabCounts = useMemo(
    () => ({
      all: notifications.length,
      unread: unreadCount,
      archive: 0,
    }),
    [notifications.length, unreadCount],
  );

  const openNotifications = useCallback(() => setIsOpen(true), []);
  const closeNotifications = useCallback(() => setIsOpen(false), []);
  const changeTab = useCallback((tab: NotificationTab) => setActiveTab(tab), []);

  const markAllAsRead = useCallback(() => {
    const userIds = unreadNotifications
      .filter((item) => item.id.startsWith("user:"))
      .map((item) => Number(item.id.slice("user:".length)));
    if (userIds.length) void markUserRead.mutateAsync(userIds);
  }, [markUserRead, unreadNotifications]);

  return {
    isOpen,
    activeTab,
    notifications: activeTab === NotificationTab.Unread
      ? unreadNotifications
      : activeTab === NotificationTab.Archive
        ? []
        : notifications,
    unreadCount,
    tabCounts,
    openNotifications,
    closeNotifications,
    changeTab,
    markAllAsRead,
    markAsRead: (_id: string) => undefined,
    archiveAll: () => undefined,
  };
};

