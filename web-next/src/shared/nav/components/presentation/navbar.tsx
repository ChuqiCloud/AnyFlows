import {
  Button,
  Divider,
  Link,
  Navbar as HeroUINavbar,
  NavbarBrand,
  NavbarContent,
  NavbarItem,
  NavbarMenu,
  NavbarMenuItem,
  NavbarMenuToggle,
} from "@heroui/react";
import { useState } from "react";
import { Link as RouterLink } from "react-router-dom";
import { Icon } from "@iconify/react";
import { useTranslation } from "react-i18next";

import { ThemeSwitch } from "@/shared/components/theme-switch";
import { useNavigationNodes } from "@/shared/navigation";
import { SiteBrandMark } from "@/features/site-settings/site-brand-mark";
import { usePublicSiteSettings } from "@/features/site-settings/site-settings-api";

import { useNotifications } from "../../hooks/use-notifications";
import type { NavIconButton, NavLinkItem } from "../../types";

import { NotificationsDrawer } from "./NotificationsDrawer";
import { NavIconButtons, NotificationButton } from "./NavIconButtons";
import { NavSearch } from "./NavSearch";
import { NavLinks } from "./NavLinks";
import { ProductDrawer } from "./ProductDrawer";
import { UserMenu } from "./UserMenu";

import type { SessionUser } from "@/lib/api/generated/types.gen";

const brandConfig = {
  name: "AnyFlows",
  href: "/",
};

type NavbarProps = {
  currentUser?: SessionUser;
  onLogout?: () => void;
};

export const Navbar = ({ currentUser, onLogout }: NavbarProps) => {
  const { t } = useTranslation();
  const [isProductDrawerOpen, setIsProductDrawerOpen] = useState(false);
  const siteQuery = usePublicSiteSettings();
  const siteName = siteQuery.data?.site_name ?? brandConfig.name;
  const siteLogo = siteQuery.data?.brand.logo_url;
  const topNodes = useNavigationNodes({ group: "top" });
  const navLinks: NavLinkItem[] = topNodes
    .filter((node) => node.href && node.variant !== "icon")
    .map((node) => ({
      key: node.key,
      label: node.title,
      href: node.href!,
      icon: node.icon,
      badge: node.badge,
    }));
  const navIconButtons: NavIconButton[] = topNodes
    .filter((node) => node.href && node.variant === "icon")
    .map((node) => ({
      key: node.key,
      icon: node.icon || "solar:widget-2-linear",
      href: node.href,
      label: node.title,
    }));

  const {
    isOpen,
    activeTab,
    notifications,
    unreadCount,
    tabCounts,
    openNotifications,
    closeNotifications,
    changeTab,
    markAllAsRead,
    archiveAll,
  } = useNotifications();

  return (
    <>
      <HeroUINavbar
        isBordered
        classNames={{
          base: "bg-background/80 backdrop-blur-lg backdrop-saturate-150",
          wrapper: "px-4 sm:px-6",
          item: "data-[active=true]:text-primary",
        }}
        height="60px"
        maxWidth="full"
      >
        <NavbarContent className="gap-2 grow-0" justify="start">
          <NavbarMenuToggle className="sm:hidden" />

          <NavbarBrand className="gap-2 grow-0">
            <RouterLink className="flex items-center gap-2" to={brandConfig.href}>
              <SiteBrandMark
                className="size-7"
                logoUrl={siteLogo}
                siteName={siteName}
              />
              <span className="hidden font-bold text-inherit sm:inline">
                {siteName}
              </span>
            </RouterLink>
            <Button
              as={RouterLink}
              className="ml-1 hidden shrink-0 font-medium text-default-600 sm:flex"
              radius="full"
              size="sm"
              startContent={<Icon icon="solar:widget-2-linear" width={16} />}
              to="/console"
              variant="light"
            >
              {t("nav.console")}
            </Button>
            <Button
              className="hidden shrink-0 sm:flex"
              radius="full"
              size="sm"
              startContent={<Icon icon="solar:widget-5-linear" width={16} />}
              variant="flat"
              onPress={() => setIsProductDrawerOpen(true)}
            >
              {t("nav.products.section")}
            </Button>
          </NavbarBrand>
        </NavbarContent>

        <NavbarContent className="gap-1" justify="end">
          <NavbarItem className="hidden lg:flex">
            <NavSearch />
          </NavbarItem>

          <NavLinks links={navLinks} />

          <Divider className="hidden h-5 mx-1 lg:flex" orientation="vertical" />

          <NavIconButtons buttons={navIconButtons} />

          <NavbarItem className="hidden lg:flex">
            <ThemeSwitch />
          </NavbarItem>

          <NotificationButton
            unreadCount={unreadCount}
            onPress={openNotifications}
          />

          <NavbarItem className="lg:hidden">
            <NavSearch />
          </NavbarItem>

          <NavbarItem>
            <UserMenu currentUser={currentUser} onLogout={onLogout} />
          </NavbarItem>
        </NavbarContent>

        <NavbarMenu className="gap-2 pt-4">
          {navLinks.map((link) => (
            <NavbarMenuItem key={link.key}>
              <Link
                as={RouterLink}
                className="w-full py-2"
                color="foreground"
                size="lg"
                to={link.href}
              >
                {link.icon ? (
                  <Icon
                    className="mr-2 text-default-500"
                    icon={link.icon}
                    width={20}
                  />
                ) : null}
                {link.label}
              </Link>
            </NavbarMenuItem>
          ))}

          {navIconButtons.length > 0 ? <Divider className="my-2" /> : null}

          {navIconButtons.map((button) => (
            <NavbarMenuItem key={button.key}>
              <Link
                as={button.href ? RouterLink : undefined}
                className="w-full py-2"
                color="foreground"
                size="lg"
                to={button.href}
              >
                <Icon
                  className="mr-2 text-default-500"
                  icon={button.icon}
                  width={20}
                />
                {button.label}
              </Link>
            </NavbarMenuItem>
          ))}

          <NavbarMenuItem>
            <div className="flex items-center justify-between py-2">
              <span className="text-foreground">{t("nav.theme")}</span>
              <ThemeSwitch />
            </div>
          </NavbarMenuItem>
        </NavbarMenu>
      </HeroUINavbar>

      <NotificationsDrawer
        activeTab={activeTab}
        isOpen={isOpen}
        notifications={notifications}
        tabCounts={tabCounts}
        onArchiveAll={archiveAll}
        onClose={closeNotifications}
        onMarkAllAsRead={markAllAsRead}
        onTabChange={changeTab}
      />
      <ProductDrawer
        isOpen={isProductDrawerOpen}
        onClose={() => setIsProductDrawerOpen(false)}
      />
    </>
  );
};
