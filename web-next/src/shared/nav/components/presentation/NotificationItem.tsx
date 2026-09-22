import type { NotificationItem as NotificationItemType } from "../../types";

import React from "react";
import { Avatar, Badge, Button, cn } from "@heroui/react";
import { Icon } from "@iconify/react";
import { useTranslation } from "react-i18next";

export interface NotificationItemProps
  extends
    Omit<React.HTMLAttributes<HTMLDivElement>, "title">,
    Omit<NotificationItemType, "id"> {
  onAccept?: () => void;
  onDecline?: () => void;
}

/**
 * 通知项组件
 */
const NotificationItem = React.forwardRef<
  HTMLDivElement,
  NotificationItemProps
>(
  (
    {
      avatar,
      icon,
      title,
      description,
      type = "default",
      time,
      isRead,
      className,
      onAccept,
      onDecline,
      ...props
    },
    ref,
  ) => {
    const { t } = useTranslation();
    // 不同类型通知的操作区
    const renderActions = (): React.ReactNode => {
      switch (type) {
        case "request":
          return (
            <div className="flex gap-2 mt-2">
              <Button color="primary" size="sm" onPress={onAccept}>
                {t("notifications.accept")}
              </Button>
              <Button size="sm" variant="flat" onPress={onDecline}>
                {t("notifications.decline")}
              </Button>
            </div>
          );
        case "system":
          return (
            <Button
              className="mt-2 -ml-2"
              color="primary"
              size="sm"
              variant="light"
            >
              {t("notifications.details")}
            </Button>
          );
        default:
          return null;
      }
    };

    // 获取图标颜色
    const getIconColor = (): string => {
      switch (type) {
        case "request":
          return "text-primary";
        case "system":
          return "text-warning";
        case "file":
          return "text-secondary";
        default:
          return "text-default-500";
      }
    };

    return (
      <div
        ref={ref}
        className={cn(
          "flex gap-3 px-4 py-3 transition-colors cursor-pointer",
          "hover:bg-default-100",
          "border-b border-divider last:border-b-0",
          { "bg-primary-50/50 dark:bg-primary-50/10": !isRead },
          className,
        )}
        {...props}
      >
        {/* 头像/图标 */}
        <div className="relative flex-none">
          <Badge
            color="primary"
            content=""
            isInvisible={isRead}
            placement="bottom-right"
            shape="circle"
            size="sm"
          >
            {avatar ? (
              <Avatar size="sm" src={avatar} />
            ) : (
              <div
                className={cn(
                  "flex items-center justify-center w-9 h-9 rounded-full",
                  "bg-default-100 dark:bg-default-50",
                )}
              >
                <Icon
                  className={getIconColor()}
                  icon={icon || "solar:bell-bold"}
                  width={18}
                />
              </div>
            )}
          </Badge>
        </div>

        {/* 内容 */}
        <div className="flex-1 min-w-0">
          <p className="text-small font-medium text-foreground truncate">
            {title}
          </p>
          <p className="text-tiny text-default-500 line-clamp-2">
            {description}
          </p>
          <time className="text-tiny text-default-400">{time}</time>
          {renderActions()}
        </div>
      </div>
    );
  },
);

NotificationItem.displayName = "NotificationItem";

export default NotificationItem;
