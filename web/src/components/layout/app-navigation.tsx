import { useAccountEligibility } from '@/features/account-verification/account-verification-api'
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
  History,
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
import { Collapsible } from 'radix-ui'

import { routeFromHash } from '@/app-route'
import {
  activeKeyByView,
  adminNavigationItems,
  adminNavigationSections,
  adminSystemItems,
  adminWorkspaceItems,
  filterOrganizationNavigationItems,
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
import { useOrganizationWorkspace } from '@/features/organization-console/organization-workspace-context'
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
  organizationApiKeys: { key: 'organizationApiKeys', icon: KeyRound, href: '#/console/organization-api-keys' },
  organizationActivity: { key: 'organizationActivity', icon: History, href: '#/console/organization-activity' },
  organizationBudgets: { key: 'organizationBudgets', icon: Building2, href: '#/console/organization-budgets' },
  organizationContractPrices: { key: 'organizationContractPrices', icon: CircleDollarSign, href: '#/console/organization-contract-prices' },
  organizationCredit: { key: 'organizationCredit', icon: Landmark, href: '#/console/organization-credit' },
  organizationApprovals: { key: 'organizationApprovals', icon: ClipboardCheck, href: '#/console/organization-approvals' },
  organizationMembers: { key: 'organizationMembers', icon: UsersRound, href: '#/console/organization-members' },
  organizationDepartments: { key: 'organizationDepartments', icon: Building2, href: '#/console/organization-departments' },
  organizationOverview: { key: 'organizationOverview', icon: Gauge, href: '#/console/organization' },
  organizationOnboarding: { key: 'organizationOnboarding', icon: ClipboardCheck, href: '#/console/organization-onboarding' },
  accountVerification: { key: 'accountVerification', icon: ShieldCheck, href: '#/console/account-verification' },
  accountVerificationSettings: { key: 'accountVerificationSettings', icon: Settings2, href: '#/console/system-settings/account-verification' },
  organizationProvisioning: { key: 'organizationProvisioning', icon: ClipboardCheck, href: '#/console/organization-provisioning' },
  organizationVerification: { key: 'organizationVerification', icon: ShieldCheck, href: '#/console/organization-verification' },
  organizationSettings: { key: 'organizationSettings', icon: Settings2, href: '#/console/organization-settings' },
  organizationServiceAccounts: { key: 'organizationServiceAccounts', icon: Bot, href: '#/console/organization-service-accounts' },
  organizationWallet: { key: 'organizationWallet', icon: WalletCards, href: '#/console/organization-wallet' },
  wallet: { key: 'wallet', icon: WalletCards, href: '#/console/wallet' },
  subscriptions: { key: 'subscriptions', icon: CalendarRange, href: '#/console/subscriptions' },
  invitations: { key: 'invitations', icon: TicketCheck, href: '#/console/invitations' },
  profile: { key: 'profile', icon: UserRound, href: '#/console/profile' },
  users: { key: 'users', icon: UsersRound, href: '#/console/users' },
  subscriptionManagement: { key: 'subscriptionManagement', icon: ListChecks, href: '#/console/subscription-management' },
  organizationPlans: { key: 'organizationPlans', icon: Layers3, href: '#/console/organization-plans' },
  platformOrganizations: { key: 'platformOrganizations', icon: Building2, href: '#/console/platform-organizations' },
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
  organization: Building2,
  enterprise: Building2,
  operations: BriefcaseBusiness,
  gateway: Waypoints,
  platform: Settings2,
} as const satisfies Record<AdminSectionKey, LucideIcon>

const customIcons: Record<SiteSidebarLink['icon'], LucideIcon> = {
  link: Link2, globe: Globe2, book: BookOpen, sparkles: Sparkles,
  building: Building2, message: MessageSquareText, headphones: Headphones,
  shield: ShieldCheck, chart: Gauge,
}

function CustomNavigation({ links }: { links: SiteSidebarLink[] }) {
  const { i18n, t } = useTranslation()
  if (links.length === 0) return null
  return <SidebarGroup className="p-0 py-1.5">
    <SidebarGroupLabel className="h-6 px-3 text-[0.6875rem] font-semibold text-sidebar-foreground/60">{t('nav.customLinks')}</SidebarGroupLabel>
    <SidebarGroupContent><SidebarMenu className="gap-0.5">{links.map((link, index) => {
      const Icon = customIcons[link.icon] ?? Link2
      const external = !link.url.startsWith('/')
      return <SidebarMenuItem key={`${index}:${link.url}`}><SidebarMenuButton asChild tooltip={i18n.resolvedLanguage?.startsWith('en') && link.label_en ? link.label_en : link.label} className={cn('h-9 rounded-lg px-3', link.style === 'accent' && 'bg-primary/8 text-primary')}><a href={external ? link.url : `#${link.url}`} target={external ? '_blank' : undefined} rel={external ? 'noopener noreferrer' : undefined}><Icon aria-hidden="true" /><span className="truncate">{i18n.resolvedLanguage?.startsWith('en') && link.label_en ? link.label_en : link.label}</span></a></SidebarMenuButton></SidebarMenuItem>
    })}</SidebarMenu></SidebarGroupContent>
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

  const organizationSection = adminNavigationSections.find((section) => section.key === 'organization')
  const enterpriseSection = adminNavigationSections.find((section) => section.key === 'enterprise')
  const systemSections = adminNavigationSections.filter((section) => (
    section.key !== 'organization' && section.key !== 'enterprise'
  ))
  const renderSectionGroup = (label: string, section: typeof adminNavigationSections[number] | undefined) => section ? (
    <SidebarGroup className="p-0 py-1.5">
      <SidebarGroupLabel className="h-6 px-3 text-[0.6875rem] font-semibold text-sidebar-foreground/60">{label}</SidebarGroupLabel>
      <SidebarGroupContent><SidebarMenu className="gap-0.5">{renderSection(section)}</SidebarMenu></SidebarGroupContent>
    </SidebarGroup>
  ) : null
  return (
    <>
      <NavigationGroup label={t('nav.groups.workspace')} items={adminWorkspaceItems} activeKey={activeKey} />
      {renderSectionGroup(t('nav.sections.organization'), organizationSection)}
      {renderSectionGroup(t('nav.sections.enterprise'), enterpriseSection)}
      <SidebarGroup className="p-0 py-1.5">
        <SidebarGroupLabel className="h-6 px-3 text-[0.6875rem] font-semibold text-sidebar-foreground/60">{t('nav.sections.system')}</SidebarGroupLabel>
        <SidebarGroupContent><SidebarMenu className="gap-0.5">
            {systemSections.map(renderSection)}
            {adminSystemItems.map((item) => <NavigationItem key={item} itemKey={item} activeKey={activeKey} />)}
        </SidebarMenu></SidebarGroupContent>
      </SidebarGroup>
    </>
  )
}

export function AppNavigation({ className, compact = false, role }: AppNavigationProps) {
  const { t } = useTranslation()
  const siteQuery = usePublicSiteSettings()
  const customLinks = (siteQuery.data?.navigation.sidebar_links ?? []).filter((link) => link.audience === 'all' || role === 'admin')
  const activeKey = useActiveKey()
  const workspace = useOrganizationWorkspace()
  const eligibility = useAccountEligibility()
  const organizationItems: readonly NavKey[] = workspace.selectedId === undefined && eligibility.data?.enterprise_verified
    ? ['organizationProvisioning']
    : filterOrganizationNavigationItems(
    userNavigationGroups.find((group) => group.key === 'organization')?.items ?? [],
    workspace.organizationPermissions,
    workspace.selectedId !== undefined,
    workspace.organizationPermissionsLoading,
  )
  const visibleUserGroups = userNavigationGroups
    .map((group) => group.key === 'organization' ? { ...group, items: organizationItems } : { ...group, items: group.items.filter((item) => item !== 'organizationProvisioning' || !organizationItems.includes('organizationProvisioning')) })
    .filter((group) => group.key !== 'organization' || group.items.length > 0)
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
        {customLinks.map((link, index) => {
          const Icon = customIcons[link.icon] ?? Link2
          const external = !link.url.startsWith('/')
          return <a key={`${index}:${link.url}`} href={external ? link.url : `#${link.url}`} target={external ? '_blank' : undefined} rel={external ? 'noopener noreferrer' : undefined} className={cn('flex h-8 shrink-0 items-center gap-2 rounded-lg px-3 text-sm font-medium', link.style === 'accent' ? 'bg-primary/8 text-primary' : 'text-muted-foreground')}><Icon className="size-4" aria-hidden="true" />{link.label}</a>
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
      <CustomNavigation links={customLinks} />
    </div>
  )
}
