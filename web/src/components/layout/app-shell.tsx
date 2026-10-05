import type { ReactNode } from 'react'
import { BellRing, CircleUserRound, LogOut, Search, ShieldCheck, WalletCards } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { AppNavigation } from '@/components/layout/app-navigation'
import { LanguageToggle } from '@/components/language-toggle'
import { ThemeToggle } from '@/components/theme-toggle'
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarHeader,
  SidebarInset,
  SidebarProvider,
  SidebarTrigger,
} from '@/components/ui/sidebar'
import { Kbd } from '@/components/ui/kbd'
import { Button } from '@/components/ui/button'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { AnnouncementStrip } from '@/features/announcements/announcement-strip'
import { SiteBrandMark } from '@/features/site-settings/site-brand-mark'
import { usePublicSiteSettings } from '@/features/site-settings/site-settings-api'
import { useBalanceDisplay } from '@/features/site-settings/use-balance-display'
import { useUserWalletSummary } from '@/features/wallet/wallet-api'
import { useUserNotifications } from '@/features/profile/profile-api'
import type { SessionUser } from '@/lib/api/generated/types.gen'
import { getManagementSessionToken } from '@/lib/api/session-token'

type AppShellProps = {
  title: string
  children: ReactNode
  currentUser: SessionUser
  onLogout: () => void
}

function getDefaultOpen() {
  try {
    // 读取 shadcn SidebarProvider 写入的 cookie
    const match = document.cookie.match(/(?:^|;\s*)sidebar_state=([^;]*)/)
    if (match) return match[1] !== 'false'
  } catch {
    // Cookie 不可读时保留侧边栏默认展开，不阻断控制台渲染。
  }
  return true
}

export function AppShell({ title, children, currentUser, onLogout }: AppShellProps) {
  const { t } = useTranslation()
  const sessionActive = Boolean(getManagementSessionToken())
  // 控制台与首页共用公开站点设置缓存，保存站点设置后可即时同步品牌展示。
  const siteQuery = usePublicSiteSettings()
  const walletQuery = useUserWalletSummary(sessionActive)
  const notificationsQuery = useUserNotifications(sessionActive)
  const { formatQuota } = useBalanceDisplay()
  const site = siteQuery.data
  const siteName = site?.site_name ?? t('brand.name')
  const siteScope = site?.brand.tagline ?? t('brand.scope')
  const balance = walletQuery.data?.balance
  const balanceText = balance === undefined ? '--' : formatQuota(balance)
  const balanceLabel = balance === undefined
    ? t('shell.balanceUnavailable')
    : t('shell.balanceValue', { value: formatQuota(balance) })
  const balanceHref = '#/console/wallet'
  const unreadCount = (notificationsQuery.data?.pages[0]?.unread_count ?? 0)
  const hasUnreadCount = notificationsQuery.data !== undefined
  const notificationsLabel = hasUnreadCount && unreadCount > 0
    ? t('shell.notificationsUnread', { count: unreadCount })
    : t('shell.notifications')

  return (
    <SidebarProvider defaultOpen={getDefaultOpen()}>
      {/* ── 侧边栏 ────────────────────────────────────────── */}
      <Sidebar
        collapsible="icon"
        mobileTitle={siteName}
        mobileDescription={siteScope}
      >
        <SidebarHeader className="flex-row items-center gap-2 px-2.5 py-2.5">
          <SiteBrandMark
            className="shrink-0 size-7"
            siteName={siteName}
            logoUrl={site?.brand.logo_url}
          />
          <div className="min-w-0 leading-tight transition-opacity group-data-[state=collapsed]:hidden">
            <div className="truncate text-[0.8125rem] font-semibold">{siteName}</div>
            <div className="truncate text-[0.6875rem] text-muted-foreground">{siteScope}</div>
          </div>
        </SidebarHeader>

        <SidebarContent className="px-1.5 py-1.5">
          <AppNavigation role={currentUser.role} />
        </SidebarContent>

        <SidebarFooter className="px-1.5 pb-2.5">
          <p className="truncate px-2 text-[0.625rem] text-muted-foreground/60 transition-opacity group-data-[state=collapsed]:opacity-0 group-data-[state=collapsed]:select-none">
            {t('shell.version')}
          </p>
        </SidebarFooter>
      </Sidebar>

      {/* ── 主内容区 ──────────────────────────────────────── */}
      <SidebarInset>
        <header className="sticky top-0 z-20 flex h-11 items-center justify-between gap-4 border-b border-[var(--hairline)] bg-background/80 px-3 backdrop-blur-xl md:px-4">
          <div className="flex min-w-0 items-center gap-2">
            <SidebarTrigger
              className="-ml-1 size-7 text-muted-foreground hover:text-foreground"
              collapseLabel={t('shell.collapse')}
              expandLabel={t('shell.expand')}
            />
            <span className="h-3.5 w-px bg-[var(--hairline)]" aria-hidden="true" />
            <h1 className="truncate text-sm font-semibold">{title}</h1>
          </div>

          <div className="flex items-center gap-1">
            <Tooltip>
              <TooltipTrigger asChild>
                <a
                  href={balanceHref}
                  className="flex h-7 min-w-7 items-center justify-center gap-1.5 rounded-lg px-1.5 text-xs text-muted-foreground transition-colors hover:bg-surface-2 hover:text-foreground sm:min-w-[4.75rem]"
                  aria-label={balanceLabel}
                >
                  <WalletCards className={`size-3.5 shrink-0 text-success`} aria-hidden="true" />
                  <span className="hidden max-w-24 truncate font-medium tabular-nums sm:inline">
                    {balanceText}
                  </span>
                </a>
              </TooltipTrigger>
              <TooltipContent side="bottom">{balanceLabel}</TooltipContent>
            </Tooltip>
            <Tooltip>
              <TooltipTrigger asChild>
                <a
                  href="#/console/profile?section=notifications"
                  className="relative flex size-7 items-center justify-center rounded-lg text-muted-foreground transition-colors hover:bg-surface-2 hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/60 focus-visible:outline-none"
                  aria-label={notificationsLabel}
                >
                  <BellRing className="size-3.5" aria-hidden="true" />
                  {hasUnreadCount && unreadCount > 0 ? <span aria-hidden="true" className="absolute -right-0.5 -top-0.5 min-w-3.5 rounded-full bg-destructive px-1 text-center text-[0.5625rem] font-semibold leading-3 text-destructive-foreground">{unreadCount > 99 ? '99+' : unreadCount}</span> : null}
                </a>
              </TooltipTrigger>
              <TooltipContent side="bottom">{notificationsLabel}</TooltipContent>
            </Tooltip>
            <button
              type="button"
              className="hidden items-center gap-1.5 rounded-md border border-[var(--hairline)] px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-surface-2 hover:text-foreground sm:flex"
              aria-label={t('shell.search')}
            >
              <Search className="size-3.5" aria-hidden="true" />
              <span className="pr-4">{t('shell.search')}</span>
              <Kbd keys={['⌘', 'K']} />
            </button>
            <LanguageToggle />
            <ThemeToggle />
            <div className="ml-1 hidden h-7 items-center gap-1.5 border-l border-[var(--hairline)] pl-2 text-xs text-muted-foreground sm:flex">
              {currentUser.role === 'admin'
                ? <ShieldCheck className="size-3.5 text-info" aria-hidden="true" />
                : <CircleUserRound className="size-3.5 text-info" aria-hidden="true" />}
              <span>{t(`auth.account.${currentUser.role}`)}</span>
              <span className="text-foreground">#{currentUser.id}</span>
            </div>
            <Tooltip>
              <TooltipTrigger asChild>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-sm"
                  className="ml-0.5 text-muted-foreground hover:text-foreground"
                  aria-label={t('auth.account.logout')}
                  onClick={onLogout}
                >
                  <LogOut className="size-3.5" aria-hidden="true" />
                </Button>
              </TooltipTrigger>
              <TooltipContent side="bottom">{t('auth.account.logout')}</TooltipContent>
            </Tooltip>
          </div>
        </header>

        <main className="flex-1 px-4 py-4 md:px-5">
          <AnnouncementStrip />
          <div className="mx-auto w-full max-w-[960px]">{children}</div>
        </main>
      </SidebarInset>
    </SidebarProvider>
  )
}
