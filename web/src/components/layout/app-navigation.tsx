import {
  Activity,
  Boxes,
  Bot,
  BriefcaseBusiness,
  Building2,
  CalendarRange,
  ChevronRight,
  CircleDollarSign,
  CircleCheckBig,
  CircleUserRound,
  Fingerprint,
  Gauge,
  GitBranch,
  History,
  KeyRound,
  Landmark,
  Layers3,
  ListChecks,
  Mail,
  MessageSquareText,
  Network,
  PackageSearch,
  PanelsTopLeft,
  ScrollText,
  ServerCog,
  Settings2,
  ShieldCheck,
  Ticket,
  TicketCheck,
  UserRound,
  UsersRound,
  Video,
  WalletCards,
  Waypoints,
} from 'lucide-react'
import type { LucideIcon } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { Collapsible } from 'radix-ui'

import { routeFromHash } from '@/app-route'
import {
  activeKeyByView,
  adminNavigationItems,
  adminNavigationSections,
  adminSystemItems,
  adminWorkspaceItems,
  type AdminSectionKey,
  type NavKey,
  userNavigationGroups,
  userNavigationItems,
} from '@/components/layout/app-navigation-model'
import {
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarMenuSub,
  SidebarMenuSubButton,
  SidebarMenuSubItem,
  useSidebar,
} from '@/components/ui/sidebar'
import { cn } from '@/lib/utils'

type AppNavigationProps = {
  className?: string
  /** compact 模式用于移动端横向导航条。 */
  compact?: boolean
  role: 'admin' | 'user'
}

type NavItem = {
  key: NavKey
  icon: LucideIcon
  href: `#/console${string}`
}

const navItems = {
  overview: { key: 'overview', icon: Gauge, href: '#/console' },
  models: { key: 'models', icon: Boxes, href: '#/console/models' },
  playground: { key: 'playground', icon: MessageSquareText, href: '#/console/playground' },
  videoTasks: { key: 'videoTasks', icon: Video, href: '#/console/video-tasks' },
  apiKeys: { key: 'apiKeys', icon: KeyRound, href: '#/console/api-keys' },
  organizationApiKeys: { key: 'organizationApiKeys', icon: KeyRound, href: '#/console/organization-api-keys' },
  organizationActivity: { key: 'organizationActivity', icon: History, href: '#/console/organization-activity' },
  organizationBudgets: { key: 'organizationBudgets', icon: Building2, href: '#/console/organization-budgets' },
  organizationContractPrices: { key: 'organizationContractPrices', icon: CircleDollarSign, href: '#/console/organization-contract-prices' },
  organizationMembers: { key: 'organizationMembers', icon: UsersRound, href: '#/console/organization-members' },
  organizationDepartments: { key: 'organizationDepartments', icon: Building2, href: '#/console/organization-departments' },
  organizationOverview: { key: 'organizationOverview', icon: Gauge, href: '#/console/organization' },
  organizationSettings: { key: 'organizationSettings', icon: Settings2, href: '#/console/organization-settings' },
  organizationServiceAccounts: { key: 'organizationServiceAccounts', icon: Bot, href: '#/console/organization-service-accounts' },
  organizationWallet: { key: 'organizationWallet', icon: WalletCards, href: '#/console/organization-wallet' },
  wallet: { key: 'wallet', icon: WalletCards, href: '#/console/wallet' },
  subscriptions: { key: 'subscriptions', icon: CalendarRange, href: '#/console/subscriptions' },
  invitations: { key: 'invitations', icon: TicketCheck, href: '#/console/invitations' },
  profile: { key: 'profile', icon: UserRound, href: '#/console/profile' },
  users: { key: 'users', icon: UsersRound, href: '#/console/users' },
  subscriptionManagement: { key: 'subscriptionManagement', icon: ListChecks, href: '#/console/subscription-management' },
  redemptionCodes: { key: 'redemptionCodes', icon: Ticket, href: '#/console/redemption-codes' },
  refunds: { key: 'refunds', icon: CircleCheckBig, href: '#/console/refunds' },
  siteSettings: { key: 'siteSettings', icon: PanelsTopLeft, href: '#/console/system-settings/site' },
  authenticationSettings: { key: 'authenticationSettings', icon: Fingerprint, href: '#/console/system-settings/authentication' },
  emailSettings: { key: 'emailSettings', icon: Mail, href: '#/console/system-settings/email' },
  networkSettings: { key: 'networkSettings', icon: Network, href: '#/console/system-settings/network' },
  paymentSettings: { key: 'paymentSettings', icon: Landmark, href: '#/console/system-settings/payment' },
  billingSettings: { key: 'billingSettings', icon: CircleDollarSign, href: '#/console/system-settings/billing' },
  groupSettings: { key: 'groupSettings', icon: Layers3, href: '#/console/system-settings/groups' },
  modelSettings: { key: 'modelSettings', icon: PackageSearch, href: '#/console/system-settings/models' },
  channels: { key: 'channels', icon: Network, href: '#/console/channels' },
  credentials: { key: 'credentials', icon: KeyRound, href: '#/console/credentials' },
  credentialProxies: { key: 'credentialProxies', icon: ServerCog, href: '#/console/proxies' },
  routes: { key: 'routes', icon: GitBranch, href: '#/console/routes' },
  keys: { key: 'keys', icon: ShieldCheck, href: '#/console/tokens' },
  usage: { key: 'usage', icon: ScrollText, href: '#/console/usage-logs' },
  debugTraces: { key: 'debugTraces', icon: Activity, href: '#/console/debug-traces' },
} as const satisfies Record<NavKey, NavItem>

const sectionIcons = {
  personal: CircleUserRound,
  organization: Building2,
  operations: BriefcaseBusiness,
  gateway: Waypoints,
  platform: Settings2,
} as const satisfies Record<AdminSectionKey, LucideIcon>

function useActiveKey() {
  if (typeof window === 'undefined') return 'overview'
  return activeKeyByView[routeFromHash(window.location.hash).view] ?? 'overview'
}

function NavigationItem({ itemKey, activeKey }: { itemKey: NavKey; activeKey: NavKey }) {
  const { t } = useTranslation()
  const item = navItems[itemKey]
  const Icon = item.icon
  const active = item.key === activeKey
  return (
    <SidebarMenuItem>
      <SidebarMenuButton asChild isActive={active} tooltip={t(`nav.${item.key}`)} className={active ? 'text-brand' : undefined}>
        <a href={item.href} aria-current={active ? 'page' : undefined}>
          <Icon className={cn('shrink-0', active ? 'text-brand' : 'text-sidebar-foreground/70')} aria-hidden="true" />
          <span>{t(`nav.${item.key}`)}</span>
        </a>
      </SidebarMenuButton>
    </SidebarMenuItem>
  )
}

function NavigationGroup({ label, items, activeKey }: { label: string; items: readonly NavKey[]; activeKey: NavKey }) {
  return (
    <SidebarGroup className="p-0">
      <SidebarGroupLabel className="h-7 px-2 text-[0.6875rem]">{label}</SidebarGroupLabel>
      <SidebarGroupContent>
        <SidebarMenu>{items.map((item) => <NavigationItem key={item} itemKey={item} activeKey={activeKey} />)}</SidebarMenu>
      </SidebarGroupContent>
    </SidebarGroup>
  )
}

function CollapsibleNavigationSection({
  sectionKey,
  items,
  activeKey,
  open,
  onOpenChange,
}: {
  sectionKey: AdminSectionKey
  items: readonly NavKey[]
  activeKey: NavKey
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const { t } = useTranslation()
  const { setOpen: setSidebarOpen, state: sidebarState } = useSidebar()
  const Icon = sectionIcons[sectionKey]
  const active = items.includes(activeKey)
  const contentId = `admin-nav-${sectionKey}`

  return (
    <Collapsible.Root asChild open={open} onOpenChange={onOpenChange}>
      <SidebarMenuItem>
        <SidebarMenuButton
          type="button"
          isActive={active}
          tooltip={t(`nav.sections.${sectionKey}`)}
          className={active ? 'text-brand' : undefined}
          aria-controls={contentId}
          aria-expanded={open}
          onClick={() => {
            if (sidebarState === 'collapsed') setSidebarOpen(true)
            onOpenChange(!open)
          }}
        >
          <Icon className={cn('shrink-0', active ? 'text-brand' : 'text-sidebar-foreground/70')} aria-hidden="true" />
          <span>{t(`nav.sections.${sectionKey}`)}</span>
          <ChevronRight className={cn('ml-auto transition-transform duration-200 group-data-[collapsible=icon]:hidden', open && 'rotate-90')} aria-hidden="true" />
        </SidebarMenuButton>
        <Collapsible.Content
          id={contentId}
          forceMount
          inert={open ? undefined : true}
          aria-hidden={!open}
          className={cn(
            'grid transition-[grid-template-rows,opacity] duration-200 ease-[var(--ease-ai-out)]',
            open ? 'grid-rows-[1fr] opacity-100' : 'pointer-events-none grid-rows-[0fr] opacity-0',
          )}
        >
          <div className="min-h-0 overflow-hidden">
            <SidebarMenuSub className="gap-0 py-1">
              {items.map((itemKey) => {
                const item = navItems[itemKey]
                const ItemIcon = item.icon
                const itemActive = item.key === activeKey
                return (
                  <SidebarMenuSubItem key={item.key}>
                    <SidebarMenuSubButton asChild size="sm" isActive={itemActive} className={itemActive ? 'text-brand' : undefined}>
                      <a href={item.href} aria-current={itemActive ? 'page' : undefined}>
                        <ItemIcon aria-hidden="true" />
                        <span>{t(`nav.${item.key}`)}</span>
                      </a>
                    </SidebarMenuSubButton>
                  </SidebarMenuSubItem>
                )
              })}
            </SidebarMenuSub>
          </div>
        </Collapsible.Content>
      </SidebarMenuItem>
    </Collapsible.Root>
  )
}

/** 管理员按任务域折叠低频入口，当前页面所属分组默认保持可见。 */
function AdminNavigation({ activeKey }: { activeKey: NavKey }) {
  const { t } = useTranslation()
  const [sectionState, setSectionState] = useState<Partial<Record<AdminSectionKey, boolean>>>({})
  const renderSection = (section: typeof adminNavigationSections[number]) => {
    const items: readonly NavKey[] = section.items
    const active = items.includes(activeKey)
    const open = sectionState[section.key] ?? active
    return (
      <CollapsibleNavigationSection
        key={section.key}
        sectionKey={section.key}
        items={items}
        activeKey={activeKey}
        open={open}
        onOpenChange={(value) => setSectionState((current) => ({ ...current, [section.key]: value }))}
      />
    )
  }

  const personalSection = adminNavigationSections.find((section) => section.key === 'personal')
  const systemSections = adminNavigationSections.filter((section) => section.key !== 'personal')
  return (
    <>
      <NavigationGroup label={t('nav.groups.workspace')} items={adminWorkspaceItems} activeKey={activeKey} />
      {personalSection ? (
        <SidebarGroup className="p-0">
          <SidebarGroupLabel className="h-7 px-2 text-[0.6875rem]">{t('nav.groups.personal')}</SidebarGroupLabel>
          <SidebarGroupContent><SidebarMenu>{renderSection(personalSection)}</SidebarMenu></SidebarGroupContent>
        </SidebarGroup>
      ) : null}
      <SidebarGroup className="p-0">
        <SidebarGroupLabel className="h-7 px-2 text-[0.6875rem]">{t('nav.groups.system')}</SidebarGroupLabel>
        <SidebarGroupContent>
          <SidebarMenu>
            {systemSections.map(renderSection)}
            {adminSystemItems.map((item) => <NavigationItem key={item} itemKey={item} activeKey={activeKey} />)}
          </SidebarMenu>
        </SidebarGroupContent>
      </SidebarGroup>
    </>
  )
}

export function AppNavigation({ className, compact = false, role }: AppNavigationProps) {
  const { t } = useTranslation()
  const activeKey = useActiveKey()
  const visibleItems = role === 'admin' ? adminNavigationItems : userNavigationItems

  if (compact) {
    return (
      <nav className={cn('flex gap-0.5 overflow-x-auto [scrollbar-width:none] [&::-webkit-scrollbar]:hidden', className)} aria-label={t('nav.label')}>
        {visibleItems.map((itemKey) => {
          const item = navItems[itemKey]
          const Icon = item.icon
          const active = item.key === activeKey
          return (
            <a
              key={item.key}
              href={item.href}
              aria-current={active ? 'page' : undefined}
              className={cn(
                'flex h-8 shrink-0 items-center gap-2 rounded-lg px-3 text-sm font-medium whitespace-nowrap transition-colors duration-150 focus-visible:ring-2 focus-visible:ring-ring/60 focus-visible:outline-none',
                active ? 'bg-surface-2 text-foreground' : 'text-muted-foreground hover:bg-surface-2/60 hover:text-foreground',
              )}
            >
              <Icon className={cn('size-4 shrink-0', active ? 'text-brand' : 'text-current')} aria-hidden="true" />
              {t(`nav.${item.key}`)}
            </a>
          )
        })}
      </nav>
    )
  }

  return (
    <div className={cn('flex flex-col gap-1', className)}>
      {role === 'admin'
        ? <AdminNavigation activeKey={activeKey} />
        : userNavigationGroups.map((group) => (
          <NavigationGroup key={group.key} label={t(`nav.groups.${group.key}`)} items={group.items} activeKey={activeKey} />
        ))}
    </div>
  )
}
