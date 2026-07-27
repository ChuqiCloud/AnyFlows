import type { ReactNode } from 'react'
import { LogOut, Search, ShieldCheck } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { AppNavigation } from '@/components/layout/app-navigation'
import { BrandMark } from '@/components/layout/brand-mark'
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
import type { SessionUser } from '@/lib/api/generated/types.gen'

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

  return (
    <SidebarProvider defaultOpen={getDefaultOpen()}>
      {/* ── 侧边栏 ────────────────────────────────────────── */}
      <Sidebar
        collapsible="icon"
        mobileTitle={t('nav.label')}
        mobileDescription={t('brand.scope')}
      >
        <SidebarHeader className="flex-row items-center gap-2 px-2.5 py-2.5">
          <BrandMark className="shrink-0 size-7" />
          <div className="min-w-0 leading-tight transition-opacity group-data-[state=collapsed]:hidden">
            <div className="truncate text-[0.8125rem] font-semibold">{t('brand.name')}</div>
            <div className="truncate text-[0.6875rem] text-muted-foreground">{t('brand.scope')}</div>
          </div>
        </SidebarHeader>

        <SidebarContent className="px-1.5 py-1.5">
          <AppNavigation />
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
              <ShieldCheck className="size-3.5 text-info" aria-hidden="true" />
              <span>{t('auth.account.admin')}</span>
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
          <div className="mx-auto w-full max-w-[960px]">{children}</div>
        </main>
      </SidebarInset>
    </SidebarProvider>
  )
}
