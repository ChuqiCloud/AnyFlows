import { useNavigate } from "react-router-dom";
import {
  Button,
  Chip,
  Dropdown,
  DropdownItem,
  DropdownMenu,
  DropdownSection,
  DropdownTrigger,
} from "@heroui/react";
import { Icon } from "@iconify/react";
import { useTranslation } from "react-i18next";

import { useUserProfile } from "@/features/profile/profile-api";
import type { SessionUser } from "@/lib/api/generated/types.gen";

import { useUserMenu } from "../../hooks/use-user-menu";

/** 账号菜单沿用 acmeidc 原件的结构与外观，只把跳转换成 AnyFlows 的真实入口。 */
const menuRoutes: Record<string, string> = {
  account: "/console/profile",
  realname: "/console/account-verification",
  security: "/console/profile",
  permission: "/console/api-keys",
};

type UserMenuProps = {
  currentUser?: SessionUser;
  onLogout?: () => void;
};

export const UserMenu = ({ currentUser, onLogout }: UserMenuProps) => {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const profileQuery = useUserProfile();
  const {
    userInfo,
    loading,
    isVerified,
    isPending,
    contextStatus,
    contextError,
    retryContextSync,
    handleLogout,
  } = useUserMenu();

  const principalLabel =
    profileQuery.data?.username ||
    userInfo?.displayName ||
    userInfo?.phone ||
    userInfo?.email ||
    (currentUser ? String(currentUser.id) : t("accountMenu.identityUnavailable"));

  const getInitial = () => principalLabel.charAt(0).toUpperCase();

  const handleMenuAction = (key: string) => {
    if (key === "logout") {
      if (onLogout) {
        onLogout();
      } else {
        void handleLogout();
      }

      return;
    }

    const href = menuRoutes[key];

    if (href) {
      navigate(href);
    }
  };

  const renderRealnameChip = () => {
    if (loading) {
      return null;
    }

    if (isVerified) {
      return (
        <Chip color="success" size="sm" variant="flat">
          {t("accountMenu.verified")}
        </Chip>
      );
    }

    if (isPending) {
      return (
        <Chip color="primary" size="sm" variant="flat">
          {t("accountMenu.pending")}
        </Chip>
      );
    }

    return (
      <Chip color="warning" size="sm" variant="flat">
        {t("accountMenu.unverified")}
      </Chip>
    );
  };

  return (
    <Dropdown placement="bottom-end">
      <DropdownTrigger>
        <Button
          aria-label={t("accountMenu.label")}
          className="min-w-8 h-8 bg-primary/10 px-0 font-medium text-primary hover:bg-primary/15 data-[hover=true]:bg-primary/15 dark:bg-primary/20"
          radius="full"
          size="sm"
          variant="flat"
        >
          {getInitial()}
        </Button>
      </DropdownTrigger>

      <DropdownMenu
        aria-label={t("accountMenu.label")}
        variant="flat"
        onAction={(key) => handleMenuAction(key as string)}
      >
        <DropdownSection showDivider aria-label={t("accountMenu.identity")}>
          <DropdownItem
            key="principal"
            isReadOnly
            className="cursor-default"
            description={userInfo?.principalId || t("accountMenu.identityUnavailable")}
            startContent={
              <span className="flex h-8 w-8 items-center justify-center rounded-full bg-default-100 text-sm font-semibold text-default-700">
                {getInitial()}
              </span>
            }
          >
            <span className="block max-w-56 truncate">{principalLabel}</span>
          </DropdownItem>
        </DropdownSection>

        {contextStatus === "error" ? (
          <DropdownSection showDivider aria-label={t("accountMenu.context")}>
            <DropdownItem
              key="context-error"
              isReadOnly
              className="cursor-default"
              description={contextError || t("accountMenu.contextErrorDescription")}
              startContent={
                <Icon
                  className="text-warning"
                  icon="solar:danger-triangle-linear"
                  width={18}
                />
              }
            >
              {t("accountMenu.contextError")}
            </DropdownItem>
            <DropdownItem
              key="context-retry"
              startContent={<Icon icon="solar:refresh-linear" width={18} />}
              onPress={() => {
                void retryContextSync();
              }}
            >
              {t("accountMenu.retryContext")}
            </DropdownItem>
          </DropdownSection>
        ) : null}
        <DropdownSection showDivider>
          <DropdownItem key="account">{t("accountMenu.account")}</DropdownItem>
          <DropdownItem key="realname" endContent={renderRealnameChip()}>
            {t("accountMenu.verification")}
          </DropdownItem>
          <DropdownItem key="security">{t("accountMenu.security")}</DropdownItem>
          <DropdownItem key="permission">{t("accountMenu.access")}</DropdownItem>
        </DropdownSection>

        <DropdownItem key="logout" color="danger">
          {t("auth.account.logout")}
        </DropdownItem>
      </DropdownMenu>
    </Dropdown>
  );
};
