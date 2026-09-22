import type { NavIconButton } from "../../types";

import { Badge, Button, cn, NavbarItem } from "@heroui/react";
import { Link as RouterLink } from "react-router-dom";
import { Icon } from "@iconify/react";
import React from "react";
import { useTranslation } from "react-i18next";

import { SiteTooltip } from "@/shared/components/site-tooltip";

interface NavIconButtonsProps {
  buttons: NavIconButton[];
  className?: string;
}

/**
 * 导航图标按钮组件
 */
export const NavIconButtons: React.FC<NavIconButtonsProps> = ({
  buttons,
  className,
}) => {
  return (
    <div className={cn("hidden lg:flex items-center", className)}>
      {buttons.map((btn) => (
        <NavbarItem key={btn.key}>
          <SiteTooltip content={btn.label} placement="bottom">
            <Button
              isIconOnly
              aria-label={btn.label}
              as={btn.href ? RouterLink : undefined}
              radius="full"
              size="sm"
              to={btn.href}
              variant="light"
              onPress={btn.onClick}
            >
              <Icon className="text-default-500" icon={btn.icon} width={20} />
            </Button>
          </SiteTooltip>
        </NavbarItem>
      ))}
    </div>
  );
};

interface NotificationButtonProps {
  unreadCount: number;
  onPress: () => void;
}

/**
 * 通知按钮组件
 */
export const NotificationButton: React.FC<NotificationButtonProps> = ({
  unreadCount,
  onPress,
}) => {
  const { t } = useTranslation();
  return (
    <NavbarItem>
      <SiteTooltip content={t("notifications.title")} placement="bottom">
        <Button
          isIconOnly
          aria-label={t("notifications.title")}
          className="overflow-visible"
          radius="full"
          size="sm"
          variant="light"
          onPress={onPress}
        >
          <Badge
            color="danger"
            content={unreadCount > 99 ? "99+" : unreadCount}
            isInvisible={unreadCount === 0}
            showOutline={false}
            size="sm"
          >
            <Icon
              className="text-default-500"
              icon="solar:bell-linear"
              width={20}
            />
          </Badge>
        </Button>
      </SiteTooltip>
    </NavbarItem>
  );
};
