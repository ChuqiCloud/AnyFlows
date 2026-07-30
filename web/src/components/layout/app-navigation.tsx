import {
  Boxes,
  Gauge,
  KeyRound,
  Mail,
  MessageSquareText,
  Network,
  ScrollText,
  ShieldCheck,
  UserRoundCog,
  UsersRound,
} from 'lucide-react'
import type { LucideIcon } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { cn } from '@/lib/utils'
import {
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
} from '@/components/ui/sidebar'

type AppNavigationProps = {
  className?: string
  /** compact 模式用于移动端横向导航条 */
  compact?: boolean
  role: 'admin' | 'user'
}

type NavItem = {
  key: 'overview' | 'models' | 'playground' | 'apiKeys' | 'users' | 'emailSettings' | 'registration' | 'channels' | 'keys' | 'usage'
  icon: LucideIcon
  href: `#/console${string}`
}

type NavGroup = {
  key: 'use' | 'account' | 'system'
  adminOnly: boolean
  items: readonly NavItem[]
}

const navGroups: readonly NavGroup[] = [
  {
    key: 'use',
    adminOnly: false,
    items: [
      { key: 'models', icon: Boxes, href: '#/console/models' },
      { key: 'playground', icon: MessageSquareText, href: '#/console/playground' },
    ],
  },
  {
    key: 'account',
    adminOnly: false,
    items: [
      { key: 'apiKeys', icon: KeyRound, href: '#/console/api-keys' },
    ],
  },
  {
    key: 'system',
    adminOnly: true,
    items: [
      { key: 'overview', icon: Gauge, href: '#/console' },
      { key: 'users', icon: UsersRound, href: '#/console/users' },
      { key: 'emailSettings', icon: Mail, href: '#/console/system-settings/email' },
      { key: 'registration', icon: UserRoundCog, href: '#/console/registration' },
      { key: 'channels', icon: Network, href: '#/console/channels' },
      { key: 'keys', icon: ShieldCheck, href: '#/console/tokens' },
      { key: 'usage', icon: ScrollText, href: '#/console/usage-logs' },
    ],
  },
]

const navItems = navGroups.flatMap((group) => group.items)

function useActiveKey() {
  if (typeof window === 'undefined') return 'overview'
  const hash = window.location.hash
  return navItems.find((item) => item.key !== 'overview' && hash.startsWith(item.href))?.key ?? 'overview'
}

export function AppNavigation({ className, compact = false, role }: AppNavigationProps) {
  const { t } = useTranslation()
  const activeKey = useActiveKey()
  const visibleGroups = navGroups.filter((group) => role === 'admin' || !group.adminOnly)
  const visibleItems = visibleGroups.flatMap((group) => group.items)

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
        {visibleItems.map((item) => {
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

  /* 桌面侧边栏保留角色分组，折叠时由 shadcn 隐藏分组标题。 */
  return (
    <div className={cn('flex flex-col gap-1', className)}>
      {visibleGroups.map((group) => (
        <SidebarGroup key={group.key} className="p-0">
          <SidebarGroupLabel className="h-7 px-2 text-[0.6875rem]">
            {t(`nav.groups.${group.key}`)}
          </SidebarGroupLabel>
          <SidebarGroupContent>
            <SidebarMenu>
              {group.items.map((item) => {
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
          </SidebarGroupContent>
        </SidebarGroup>
      ))}
    </div>
  )
}
