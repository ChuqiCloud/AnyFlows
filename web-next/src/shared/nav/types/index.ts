export interface NavLinkItem {
  key: string;
  label: string;
  href: string;
  icon?: string;
  badge?: number;
}

export interface NavIconButton {
  key: string;
  icon: string;
  href?: string;
  label: string;
  onClick?: () => void;
}

export type NotificationType = "default" | "request" | "file" | "system";

export interface NotificationItem {
  id: string;
  isRead?: boolean;
  avatar?: string;
  icon?: string;
  title: string;
  description: string;
  time: string;
  type?: NotificationType;
}

// erasableSyntaxOnly 下不允许 enum，改用常量对象 + 同名类型，调用点（含 NotificationTab.All 等取值）保持不变。
export const NotificationTab = {
  All: "all",
  Unread: "unread",
  Archive: "archive",
} as const;

export type NotificationTab =
  (typeof NotificationTab)[keyof typeof NotificationTab];
