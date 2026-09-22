import {
  Button,
  Divider,
  Dropdown,
  DropdownItem,
  DropdownMenu,
  DropdownSection,
  DropdownTrigger,
  Link,
  Navbar as HeroUINavbar,
  NavbarBrand,
  NavbarContent,
  NavbarItem,
  NavbarMenu,
  NavbarMenuItem,
  NavbarMenuToggle,
} from '@heroui/react'
import { Icon } from '@iconify/react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Link as RouterLink, useNavigate } from 'react-router-dom'

import { ThemeSwitch } from '@/shared/components/theme-switch'
import { useNotifications } from '@/shared/nav/hooks/use-notifications'
import { NotificationsDrawer } from '@/shared/nav/components/presentation/NotificationsDrawer'
import { NavSearch } from '@/shared/nav/components/presentation/NavSearch'
import { ProductDrawer } from '@/shared/nav/components/presentation/ProductDrawer'
import type { SessionUser } from '@/lib/api/generated/types.gen'

import { SiteBrandMark } from '@/features/site-settings/site-brand-mark'
import { usePublicSiteSettings } from '@/features/site-settings/site-settings-api'

export type ConsoleNavbarProps = {
  currentUser: SessionUser
  onLogout: () => void
}

const brandConfig = {
  name: 'AnyFlows',
  href: '/',
}

function AccountMenu({ currentUser, onLogout }: ConsoleNavbarProps) {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const roleLabel = t(`auth.account.${currentUser.role}`)
  const initial = currentUser.role === 'admin' ? 'A' : 'U'

  return (
    <Dropdown placement="bottom-end">
      <DropdownTrigger>
        <Button
          aria-label={t('nav.accountMenu')}
          className="min-w-8 h-8 bg-primary/10 px-0 font-medium text-primary hover:bg-primary/15 data-[hover=true]:bg-primary/15 dark:bg-primary/20"
          radius="full"
          size="sm"
          variant="flat"
        >
          {initial}
        </Button>
      </DropdownTrigger>
      <DropdownMenu
        aria-label={t('nav.accountMenu')}
        variant="flat"
        onAction={(key) => {
          if (key === 'profile') navigate('/console/profile')
          if (key === 'logout') onLogout()
        }}
      >
        <DropdownSection showDivider aria-label={roleLabel}>
          <DropdownItem
            key="identity"
            isReadOnly
            className="cursor-default"
            description={`#${currentUser.id}`}
            startContent={
              <span className="flex h-8 w-8 items-center justify-center rounded-full bg-default-100 text-sm font-semibold text-default-700">
                {initial}
              </span>
            }
          >
            {roleLabel}
          </DropdownItem>
        </DropdownSection>
        <DropdownItem key="profile">{t('profile.title')}</DropdownItem>
        <DropdownItem key="logout" color="danger">{t('auth.account.logout')}</DropdownItem>
      </DropdownMenu>
    </Dropdown>
  )
}

/** 控制台顶栏保持与 acmeidc-console Navbar 相同的结构、组件顺序与 classNames。 */
export function ConsoleNavbar({ currentUser, onLogout }: ConsoleNavbarProps) {
  const { t } = useTranslation()
  const [isProductDrawerOpen, setIsProductDrawerOpen] = useState(false)
  const siteQuery = usePublicSiteSettings()
  const siteName = siteQuery.data?.site_name ?? brandConfig.name
  const logoUrl = siteQuery.data?.brand.logo_url
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
  } = useNotifications()

  return (
    <>
      <HeroUINavbar
        isBordered
        classNames={{
          base: 'bg-background/80 backdrop-blur-lg backdrop-saturate-150',
          wrapper: 'px-4 sm:px-6',
          item: 'data-[active=true]:text-primary',
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
                logoUrl={logoUrl}
                siteName={siteName}
              />
              <span className="hidden font-bold text-inherit sm:inline">
                {siteName}
              </span>
            </RouterLink>
            <Button
              as={RouterLink}
              className="ml-1 hidden font-medium text-default-600 sm:flex"
              radius="full"
              size="sm"
              startContent={<Icon icon="solar:widget-2-linear" width={16} />}
              to="/"
              variant="light"
            >
              {t('nav.console')}
            </Button>
            <Button
              className="hidden sm:flex"
              radius="full"
              size="sm"
              startContent={<Icon icon="solar:widget-5-linear" width={16} />}
              variant="flat"
              onPress={() => setIsProductDrawerOpen(true)}
            >
              {t('nav.products.section')}
            </Button>
          </NavbarBrand>
        </NavbarContent>

        <NavbarContent className="gap-1" justify="end">
          <NavbarItem className="hidden lg:flex">
            <NavSearch />
          </NavbarItem>

          <Divider className="hidden h-5 mx-1 lg:flex" orientation="vertical" />

          <NavbarItem className="hidden lg:flex">
            <ThemeSwitch />
          </NavbarItem>

          <NavbarItem>
            <Button
              isIconOnly
              aria-label={t('notifications.title')}
              className="overflow-visible"
              radius="full"
              size="sm"
              variant="light"
              onPress={openNotifications}
            >
              <Icon className="text-default-500" icon="solar:bell-linear" width={20} />
              {unreadCount > 0 ? (
                <span className="absolute -right-0.5 -top-0.5 min-w-4 rounded-full bg-danger px-1 text-tiny leading-4 text-danger-foreground">
                  {unreadCount > 99 ? '99+' : unreadCount}
                </span>
              ) : null}
            </Button>
          </NavbarItem>

          <NavbarItem className="lg:hidden">
            <NavSearch />
          </NavbarItem>

          <NavbarItem>
            <AccountMenu currentUser={currentUser} onLogout={onLogout} />
          </NavbarItem>
        </NavbarContent>

        <NavbarMenu className="gap-2 pt-4">
          <NavbarMenuItem>
            <Link as={RouterLink} className="w-full py-2" color="foreground" size="lg" to="/console">
              {t('nav.console')}
            </Link>
          </NavbarMenuItem>
          <NavbarMenuItem>
            <div className="flex items-center justify-between py-2">
              <span className="text-foreground">{t('nav.theme')}</span>
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
      <ProductDrawer isOpen={isProductDrawerOpen} onClose={() => setIsProductDrawerOpen(false)} />
    </>
  )
}
