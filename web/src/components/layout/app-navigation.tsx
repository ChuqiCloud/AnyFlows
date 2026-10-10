import {
  Activity,
  Boxes,
  Bot,
  BriefcaseBusiness,
  BookOpen,
  Building2,
  CalendarRange,
  ChevronRight,
  CircleDollarSign,
  ClipboardCheck,
  CircleCheckBig,
  Fingerprint,
  Gauge,
  GitBranch,
  Globe2,
  Headphones,
  KeyRound,
  Landmark,
  Layers3,
  Link2,
  ListChecks,
  Mail,
  Megaphone,
  MessageSquareText,
  Network,
  PackageSearch,
  PanelsTopLeft,
  ScrollText,
  ServerCog,
  Settings2,
  ShieldCheck,
  Sparkles,
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
import { useConsoleExtensionNavigation } from '@/extensions/query'
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
import { usePublicSiteSettings } from '@/features/site-settings/site-settings-api'
import type { SiteSidebarLink } from '@/lib/api/generated/types.gen'

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
  accountVerification: { key: 'accountVerification', icon: ShieldCheck, href: '#/console/account-verification' },
  accountVerificationSettings: { key: 'accountVerificationSettings', icon: Settings2, href: '#/console/system-settings/account-verification' },
  wallet: { key: 'wallet', icon: WalletCards, href: '#/console/wallet' },
  subscriptions: { key: 'subscriptions', icon: CalendarRange, href: '#/console/subscriptions' },
  invitations: { key: 'invitations', icon: TicketCheck, href: '#/console/invitations' },
  profile: { key: 'profile', icon: UserRound, href: '#/console/profile' },
  users: { key: 'users', icon: UsersRound, href: '#/console/users' },
  subscriptionManagement: { key: 'subscriptionManagement', icon: ListChecks, href: '#/console/subscription-management' },
  redemptionCodes: { key: 'redemptionCodes', icon: Ticket, href: '#/console/redemption-codes' },
  refunds: { key: 'refunds', icon: CircleCheckBig, href: '#/console/refunds' },
  siteSettings: { key: 'siteSettings', icon: PanelsTopLeft, href: '#/console/system-settings/site' },
  siteNavigation: { key: 'siteNavigation', icon: PanelsTopLeft, href: '#/console/system-settings/navigation' },
  announcements: { key: 'announcements', icon: Megaphone, href: '#/console/system-settings/announcements' },
  authenticationSettings: { key: 'authenticationSettings', icon: Fingerprint, href: '#/console/system-settings/authentication' },
  emailSettings: { key: 'emailSettings', icon: Mail, href: '#/console/system-settings/email' },
  networkSettings: { key: 'networkSettings', icon: Network, href: '#/console/system-settings/network' },
  paymentSettings: { key: 'paymentSettings', icon: Landmark, href: '#/console/system-settings/payment' },
  billingSettings: { key: 'billingSettings', icon: CircleDollarSign, href: '#/console/system-settings/billing' },
  groupSettings: { key: 'groupSettings', icon: Layers3, href: '#/console/system-settings/groups' },
  modelSettings: { key: 'modelSettings', icon: PackageSearch, href: '#/console/system-settings/models' },
  modelProviders: { key: 'modelProviders', icon: Building2, href: '#/console/system-settings/model-providers' },
  channels: { key: 'channels', icon: Network, href: '#/console/channels' },
  credentials: { key: 'credentials', icon: KeyRound, href: '#/console/credentials' },
  credentialProxies: { key: 'credentialProxies', icon: ServerCog, href: '#/console/proxies' },
  routes: { key: 'routes', icon: GitBranch, href: '#/console/routes' },
  keys: { key: 'keys', icon: ShieldCheck, href: '#/console/tokens' },
  usage: { key: 'usage', icon: ScrollText, href: '#/console/usage-logs' },
  debugTraces: { key: 'debugTraces', icon: Activity, href: '#/console/debug-traces' },
} as const satisfies Record<NavKey, NavItem>

const sectionIcons = {
  operations: BriefcaseBusiness,
  gateway: Waypoints,
  platform: Settings2,
} as const satisfies Record<AdminSectionKey, LucideIcon>

const customIcons: Record<SiteSidebarLink['icon'], LucideIcon> = {
  link: Link2, globe: Globe2, book: BookOpen, sparkles: Sparkles,
  building: Building2, message: MessageSquareText, headphones: Headphones,
  shield: ShieldCheck, chart: Gauge, home: Boxes, dashboard: PanelsTopLeft, settings: Settings2,
  users: UsersRound, user: UserRound, briefcase: BriefcaseBusiness, calendar: CalendarRange,
  card: WalletCards, key: KeyRound, lock: ShieldCheck, file: ScrollText, folder: Layers3,
  help: CircleCheckBig, info: MessageSquareText, bell: Megaphone, mail: Mail, phone: Headphones,
  map: Globe2, database: ServerCog, server: Network, code: Waypoints, terminal: Bot,
  bot: Bot, cpu: Bot, workflow: Waypoints, gauge: Gauge, rocket: Sparkles, megaphone: Megaphone,
  shopping: WalletCards, monitor: PanelsTopLeft, cloud: Globe2, bookmark: BookOpen,
  graduation: BookOpen, newspaper: ScrollText, clipboard: ClipboardCheck, list: ListChecks,
  wrench: Settings2, search: Globe2, star: Sparkles, heart: CircleCheckBig, zap: Sparkles,
  command: PanelsTopLeft, panels: PanelsTopLeft,
}

type CustomNavigationNode = { link: SiteSidebarLink; children: SiteSidebarLink[] }

function customNavigationTree(links: SiteSidebarLink[], role: 'admin' | 'user'): CustomNavigationNode[] {
  const roots: CustomNavigationNode[] = []
  for (const link of links) {
    if ((link.level ?? 1) > 1 && roots.length > 0) {
      const parent = roots.at(-1)!
      if (parent.link.audience === 'all' || role === 'admin') {
        if (parent.link.kind !== 'group' && parent.children.length === 0 && parent.link.url) parent.children.push(parent.link)
        if (link.audience === 'all' || role === 'admin') parent.children.push(link)
      }
    } else roots.push({ link, children: [] })
  }
  return roots.filter(({ link, children }) => (link.audience === 'all' || role === 'admin') && (link.kind !== 'group' || children.length > 0))
}

function CustomMenuGroup({ node, label }: { node: CustomNavigationNode; label: (link: SiteSidebarLink) => string }) {
  const [open, setOpen] = useState(() => node.children.some((child) => child.url.startsWith('/') && window.location.hash.slice(1) === child.url))
  const { setOpen: setSidebarOpen, state: sidebarState } = useSidebar()
  const Icon = customIcons[node.link.icon] ?? Link2
  const name = label(node.link)
  return <Collapsible.Root asChild open={open} onOpenChange={setOpen}>
    <SidebarMenuItem>
      <SidebarMenuButton type="button" tooltip={name} className="h-9 rounded-lg px-3" aria-expanded={open} onClick={() => { if (sidebarState === 'collapsed') setSidebarOpen(true); setOpen(!open) }}>
        <Icon aria-hidden="true" /><span className="truncate">{name}</span><ChevronRight className={cn('ml-auto transition-transform duration-200 group-data-[collapsible=icon]:hidden', open && 'rotate-90')} aria-hidden="true" />
      </SidebarMenuButton>
      <Collapsible.Content className={cn('grid transition-[grid-template-rows,opacity] duration-200', open ? 'grid-rows-[1fr] opacity-100' : 'pointer-events-none grid-rows-[0fr] opacity-0')} inert={open ? undefined : true}>
        <div className="min-h-0 overflow-hidden"><SidebarMenuSub className="gap-0.5 py-1.5 pl-3">
          {node.children.map((child, index) => {
            const ChildIcon = customIcons[child.icon] ?? Link2
            const external = !child.url.startsWith('/')
            return <SidebarMenuSubItem key={`${index}:${child.url}`}><SidebarMenuSubButton asChild size="sm" isActive={!external && window.location.hash.slice(1) === child.url} className="h-8 gap-2.5 rounded-lg px-2.5"><a href={external ? child.url : `#${child.url}`} target={external ? '_blank' : undefined} rel={external ? 'noopener noreferrer' : undefined}><ChildIcon aria-hidden="true" /><span className="truncate">{label(child)}</span></a></SidebarMenuSubButton></SidebarMenuSubItem>
          })}
        </SidebarMenuSub></div>
      </Collapsible.Content>
    </SidebarMenuItem>
  </Collapsible.Root>
}

function CustomNavigation({ links, role }: { links: SiteSidebarLink[]; role: 'admin' | 'user' }) {
  const { i18n } = useTranslation()
  const nodes = customNavigationTree(links, role)
  if (nodes.length === 0) return null
  const label = (link: SiteSidebarLink) => i18n.resolvedLanguage?.startsWith('en') && link.label_en ? link.label_en : link.label
  return <SidebarGroup className="p-0 py-1.5">
    <SidebarGroupContent><SidebarMenu className="gap-0.5">
      {nodes.map((node, index) => {
        if (node.link.kind === 'group' || node.children.length > 0) return <CustomMenuGroup key={index} node={node} label={label} />
        const Icon = customIcons[node.link.icon] ?? Link2
        const external = !node.link.url.startsWith('/')
        return <SidebarMenuItem key={index}><SidebarMenuButton asChild tooltip={label(node.link)} isActive={!external && window.location.hash.slice(1) === node.link.url} className="h-9 rounded-lg px-3"><a href={external ? node.link.url : `#${node.link.url}`} target={external ? '_blank' : undefined} rel={external ? 'noopener noreferrer' : undefined}><Icon aria-hidden="true" /><span className="truncate">{label(node.link)}</span></a></SidebarMenuButton></SidebarMenuItem>
      })}
    </SidebarMenu></SidebarGroupContent>
  </SidebarGroup>
}

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
      <SidebarMenuButton
        asChild
        isActive={active}
        tooltip={t(`nav.${item.key}`)}
        className={cn('h-9 rounded-lg px-3', active && 'bg-sidebar-accent/80 text-brand')}
      >
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
    <SidebarGroup className="p-0 py-1.5">
      <SidebarGroupLabel className="h-6 px-3 text-[0.6875rem] font-semibold text-sidebar-foreground/60">{label}</SidebarGroupLabel>
      <SidebarGroupContent>
        <SidebarMenu className="gap-0.5">{items.map((item) => <NavigationItem key={item} itemKey={item} activeKey={activeKey} />)}</SidebarMenu>
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
          className={cn('h-9 rounded-lg px-3', active && 'bg-sidebar-accent/80 text-brand')}
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
            <SidebarMenuSub className="gap-0.5 py-1.5 pl-3">
              {items.map((itemKey) => {
                const item = navItems[itemKey]
                const ItemIcon = item.icon
                const itemActive = item.key === activeKey
                return (
                  <SidebarMenuSubItem key={item.key}>
                    <SidebarMenuSubButton asChild size="sm" isActive={itemActive} className={cn('h-8 gap-2.5 rounded-lg px-2.5', itemActive && 'text-brand')}>
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

  return (
    <>
      <NavigationGroup label={t('nav.groups.workspace')} items={adminWorkspaceItems} activeKey={activeKey} />
      <SidebarGroup className="p-0 py-1.5">
        <SidebarGroupLabel className="h-6 px-3 text-[0.6875rem] font-semibold text-sidebar-foreground/60">{t('nav.sections.system')}</SidebarGroupLabel>
        <SidebarGroupContent><SidebarMenu className="gap-0.5">
            {adminNavigationSections.map(renderSection)}
            {adminSystemItems.map((item) => <NavigationItem key={item} itemKey={item} activeKey={activeKey} />)}
        </SidebarMenu></SidebarGroupContent>
      </SidebarGroup>
    </>
  )
}

export function AppNavigation({ className, compact = false, role }: AppNavigationProps) {
  const extensionRoutes = useConsoleExtensionNavigation(role)
  const { t } = useTranslation()
  const siteQuery = usePublicSiteSettings()
  const customLinks = siteQuery.data?.navigation.sidebar_links ?? []
  const activeKey = useActiveKey()
  const visibleUserGroups = userNavigationGroups
  const visibleItems = role === 'admin'
    ? adminNavigationItems
    : visibleUserGroups.flatMap((group) => group.items)

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
        {extensionRoutes.map((route) => <a key={route.path} href={`#${route.path}`} className="flex h-8 shrink-0 items-center gap-2 rounded-lg px-3 text-sm font-medium text-muted-foreground"><Building2 className="size-4" aria-hidden="true" />{t(route.titleKey)}</a>)}
        {customNavigationTree(customLinks, role).flatMap((node) => node.link.kind === 'group' || node.children.length > 0 ? node.children : [node.link]).map((link, index) => {
          const Icon = customIcons[link.icon] ?? Link2
          const external = !link.url.startsWith('/')
          return <a key={`${index}:${link.url}`} href={external ? link.url : `#${link.url}`} target={external ? '_blank' : undefined} rel={external ? 'noopener noreferrer' : undefined} className="flex h-8 shrink-0 items-center gap-2 rounded-lg px-3 text-sm font-medium text-muted-foreground"><Icon className="size-4" aria-hidden="true" />{link.label}</a>
        })}
      </nav>
    )
  }

  return (
    <div className={cn('flex flex-col gap-1', className)}>
      {role === 'admin'
        ? <AdminNavigation activeKey={activeKey} />
        : visibleUserGroups.map((group) => (
          <NavigationGroup key={group.key} label={t(`nav.groups.${group.key}`)} items={group.items} activeKey={activeKey} />
        ))}
      {extensionRoutes.length > 0 ? <SidebarGroup className="p-0 py-1.5"><SidebarGroupContent><SidebarMenu className="gap-0.5">
        {extensionRoutes.map((route) => <SidebarMenuItem key={route.path}><SidebarMenuButton asChild tooltip={t(route.titleKey)} isActive={window.location.hash.split('?')[0] === `#${route.path}`} className="h-9 rounded-lg px-3"><a href={`#${route.path}`}><Building2 aria-hidden="true" /><span>{t(route.titleKey)}</span></a></SidebarMenuButton></SidebarMenuItem>)}
      </SidebarMenu></SidebarGroupContent></SidebarGroup> : null}
      <CustomNavigation links={customLinks} role={role} />
    </div>
  )
}
