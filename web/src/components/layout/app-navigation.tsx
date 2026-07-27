import { Gauge, Network } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { cn } from '@/lib/utils'
import {
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
} from '@/components/ui/sidebar'

type AppNavigationProps = {
  className?: string
  /** compact 模式用于移动端横向导航条 */
  compact?: boolean
  /** 已废弃，shadcn Sidebar 会通过 collapsible="icon" 自动处理折叠 */
  collapsed?: boolean
}

const navItems = [
  { key: 'overview', icon: Gauge, href: '#/console' },
  { key: 'channels', icon: Network, href: '#/console/channels' },
] as const

function useActiveKey() {
  if (typeof window === 'undefined') return 'overview'
  const hash = window.location.hash
  const segment = hash.replace('#/console/', '').replace('#/console', '').split('/')[0]
  return navItems.some((n) => n.key === segment) ? segment : 'overview'
}

export function AppNavigation({ className, compact = false }: AppNavigationProps) {
  const { t } = useTranslation()
  const activeKey = useActiveKey()

  /* ── 移动端横向导航条（compact 模式）────────────────────── */
  if (compact) {
    return (
      <nav
        className={cn(
          'flex gap-0.5 overflow-x-auto [scrollbar-width:none] [&::-webkit-scrollbar]:hidden',
          className,
        )}
        aria-label={t('nav.label')}
      >
        {navItems.map((item) => {
          const IconComponent = item.icon
          const active = item.key === activeKey
          return (
            <a
              key={item.key}
              href={item.href}
              aria-current={active ? 'page' : undefined}
              className={cn(
                'flex h-8 shrink-0 items-center gap-2 rounded-lg px-3',
                'text-sm font-medium whitespace-nowrap transition-colors duration-150',
                'focus-visible:ring-2 focus-visible:ring-ring/60 focus-visible:outline-none',
                active
                  ? 'bg-surface-2 text-foreground'
                  : 'text-muted-foreground hover:bg-surface-2/60 hover:text-foreground',
              )}
            >
              <IconComponent
                className={cn('size-4 shrink-0', active ? 'text-brand' : 'text-current')}
                aria-hidden="true"
              />
              {t(`nav.${item.key}`)}
            </a>
          )
        })}
      </nav>
    )
  }

  /* ── 桌面侧边栏导航（shadcn SidebarMenu）────────────────── */
  return (
    <SidebarMenu className={className}>
      {navItems.map((item) => {
        const IconComponent = item.icon
        const active = item.key === activeKey
        return (
          <SidebarMenuItem key={item.key}>
            <SidebarMenuButton
              asChild
              isActive={active}
              tooltip={t(`nav.${item.key}`)}
              className={active ? 'text-brand' : undefined}
            >
              <a href={item.href} aria-current={active ? 'page' : undefined}>
                <IconComponent
                  className={cn('shrink-0', active ? 'text-brand' : 'text-sidebar-foreground/70')}
                  aria-hidden="true"
                />
                <span>{t(`nav.${item.key}`)}</span>
              </a>
            </SidebarMenuButton>
          </SidebarMenuItem>
        )
      })}
    </SidebarMenu>
  )
}
