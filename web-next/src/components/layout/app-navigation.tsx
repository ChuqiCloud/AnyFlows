import { useAccountEligibility } from '@/features/account-verification/account-verification-api'
import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'

import {
  adminWorkspaceItems,
  enterpriseAdminItems,
  filterOrganizationNavigationItems,
  type NavKey,
  type NavigationSectionKey,
  organizationSectionItems,
  systemSectionItems,
  userWorkspaceItems,
} from '@/components/layout/app-navigation-model'
import type { SidebarNode } from '@/shared/app-shell'
import { useOrganizationWorkspace } from '@/features/organization-console/organization-workspace-context'
import { usePublicSiteSettings } from '@/features/site-settings/site-settings-api'
import type { SiteSidebarLink } from '@/lib/api/generated/types.gen'

const customIcons: Record<SiteSidebarLink['icon'], string> = {
  link: 'solar:link-linear', globe: 'solar:global-linear', book: 'solar:book-linear',
  sparkles: 'solar:stars-linear', building: 'solar:buildings-2-linear',
  message: 'solar:chat-round-dots-linear', headphones: 'solar:headphones-round-linear',
  shield: 'solar:shield-check-linear', chart: 'solar:chart-square-linear', home: 'solar:home-2-linear',
  dashboard: 'solar:widget-4-linear', settings: 'solar:settings-linear', users: 'solar:users-group-rounded-linear',
  user: 'solar:user-linear', briefcase: 'solar:case-linear', calendar: 'solar:calendar-mark-linear',
  card: 'solar:card-linear', key: 'solar:key-linear', lock: 'solar:lock-keyhole-linear',
  file: 'solar:document-text-linear', folder: 'solar:folder-linear', help: 'solar:question-circle-linear',
  info: 'solar:info-circle-linear', bell: 'solar:bell-linear', mail: 'solar:letter-linear',
  phone: 'solar:phone-linear', map: 'solar:map-point-linear', database: 'solar:database-linear',
  server: 'solar:server-minimalistic-linear', code: 'solar:code-linear', terminal: 'solar:terminal-linear',
  bot: 'solar:cpu-linear', cpu: 'solar:cpu-linear', workflow: 'solar:routing-2-linear', gauge: 'solar:gauge-linear',
  rocket: 'solar:rocket-2-linear', megaphone: 'solar:megaphone-linear', shopping: 'solar:bag-3-linear',
  monitor: 'solar:monitor-linear', cloud: 'solar:cloud-linear', bookmark: 'solar:bookmark-linear',
  graduation: 'solar:square-academic-cap-linear', newspaper: 'solar:document-text-linear',
  clipboard: 'solar:clipboard-check-linear', list: 'solar:list-check-linear', wrench: 'solar:wad-of-money-linear',
  search: 'solar:magnifer-linear', star: 'solar:star-linear', heart: 'solar:heart-linear',
  zap: 'solar:bolt-linear', command: 'solar:command-linear', panels: 'solar:sidebar-minimalistic-linear',
}

type NavItem = {
  key: NavKey
  icon: string
  href: `/console${string}`
}

/* 图标沿用 acmeidc 的 Solar（Iconify）字符串：图尺寸、配色和选中态由 SidebarPresentation 统一接管。 */
const navItems = {
  overview: { key: 'overview', icon: 'solar:home-2-linear', href: '/console' },
  models: { key: 'models', icon: 'solar:planet-2-linear', href: '/console/models' },
  playground: { key: 'playground', icon: 'solar:chat-round-dots-linear', href: '/console/playground' },
  videoTasks: { key: 'videoTasks', icon: 'solar:video-frame-linear', href: '/console/video-tasks' },
  apiKeys: { key: 'apiKeys', icon: 'solar:key-linear', href: '/console/api-keys' },
  organizationApiKeys: { key: 'organizationApiKeys', icon: 'solar:key-square-linear', href: '/console/organization-api-keys' },
  organizationActivity: { key: 'organizationActivity', icon: 'solar:history-linear', href: '/console/organization-activity' },
  organizationBudgets: { key: 'organizationBudgets', icon: 'solar:wallet-money-linear', href: '/console/organization-budgets' },
  organizationContractPrices: { key: 'organizationContractPrices', icon: 'solar:hand-money-linear', href: '/console/organization-contract-prices' },
  organizationCredit: { key: 'organizationCredit', icon: 'solar:banknote-linear', href: '/console/organization-credit' },
  organizationApprovals: { key: 'organizationApprovals', icon: 'solar:clipboard-check-linear', href: '/console/organization-approvals' },
  organizationMembers: { key: 'organizationMembers', icon: 'solar:users-group-rounded-linear', href: '/console/organization-members' },
  organizationDepartments: { key: 'organizationDepartments', icon: 'solar:buildings-2-linear', href: '/console/organization-departments' },
  organizationOverview: { key: 'organizationOverview', icon: 'solar:chart-square-linear', href: '/console/organization' },
  organizationOnboarding: { key: 'organizationOnboarding', icon: 'solar:clipboard-list-linear', href: '/console/organization-onboarding' },
  accountVerification: { key: 'accountVerification', icon: 'solar:verified-check-linear', href: '/console/account-verification' },
  accountVerificationSettings: { key: 'accountVerificationSettings', icon: 'solar:settings-linear', href: '/console/system-settings/account-verification' },
  organizationProvisioning: { key: 'organizationProvisioning', icon: 'solar:add-circle-linear', href: '/console/organization-provisioning' },
  organizationVerification: { key: 'organizationVerification', icon: 'solar:verified-check-linear', href: '/console/organization-verification' },
  organizationSettings: { key: 'organizationSettings', icon: 'solar:settings-linear', href: '/console/organization-settings' },
  organizationServiceAccounts: { key: 'organizationServiceAccounts', icon: 'solar:cpu-linear', href: '/console/organization-service-accounts' },
  organizationWallet: { key: 'organizationWallet', icon: 'solar:card-linear', href: '/console/organization-wallet' },
  wallet: { key: 'wallet', icon: 'solar:wallet-linear', href: '/console/wallet' },
  subscriptions: { key: 'subscriptions', icon: 'solar:calendar-mark-linear', href: '/console/subscriptions' },
  invitations: { key: 'invitations', icon: 'solar:gift-linear', href: '/console/invitations' },
  profile: { key: 'profile', icon: 'solar:user-id-linear', href: '/console/profile' },
  users: { key: 'users', icon: 'solar:users-group-two-rounded-linear', href: '/console/users' },
  subscriptionManagement: { key: 'subscriptionManagement', icon: 'solar:layers-minimalistic-linear', href: '/console/subscription-management' },
  organizationPlans: { key: 'organizationPlans', icon: 'solar:layers-linear', href: '/console/organization-plans' },
  platformOrganizations: { key: 'platformOrganizations', icon: 'solar:buildings-3-linear', href: '/console/platform-organizations' },
  redemptionCodes: { key: 'redemptionCodes', icon: 'solar:ticket-linear', href: '/console/redemption-codes' },
  refunds: { key: 'refunds', icon: 'solar:check-circle-linear', href: '/console/refunds' },
  siteSettings: { key: 'siteSettings', icon: 'solar:monitor-linear', href: '/console/system-settings/site' },
  siteNavigation: { key: 'siteNavigation', icon: 'solar:menu-dots-linear', href: '/console/system-settings/navigation' },
  announcements: { key: 'announcements', icon: 'solar:notes-linear', href: '/console/system-settings/announcements' },
  authenticationSettings: { key: 'authenticationSettings', icon: 'solar:password-linear', href: '/console/system-settings/authentication' },
  emailSettings: { key: 'emailSettings', icon: 'solar:letter-linear', href: '/console/system-settings/email' },
  networkSettings: { key: 'networkSettings', icon: 'solar:global-linear', href: '/console/system-settings/network' },
  paymentSettings: { key: 'paymentSettings', icon: 'solar:card-2-linear', href: '/console/system-settings/payment' },
  billingSettings: { key: 'billingSettings', icon: 'solar:bill-linear', href: '/console/system-settings/billing' },
  groupSettings: { key: 'groupSettings', icon: 'solar:folder-linear', href: '/console/system-settings/groups' },
  modelSettings: { key: 'modelSettings', icon: 'solar:box-minimalistic-linear', href: '/console/system-settings/models' },
  modelProviders: { key: 'modelProviders', icon: 'solar:buildings-2-linear', href: '/console/system-settings/model-providers' },
  channels: { key: 'channels', icon: 'solar:server-minimalistic-linear', href: '/console/channels' },
  credentials: { key: 'credentials', icon: 'solar:key-minimalistic-square-linear', href: '/console/credentials' },
  credentialProxies: { key: 'credentialProxies', icon: 'solar:server-square-linear', href: '/console/proxies' },
  routes: { key: 'routes', icon: 'solar:routing-2-linear', href: '/console/routes' },
  keys: { key: 'keys', icon: 'solar:shield-check-linear', href: '/console/tokens' },
  usage: { key: 'usage', icon: 'solar:document-text-linear', href: '/console/usage-logs' },
  debugTraces: { key: 'debugTraces', icon: 'solar:bug-linear', href: '/console/debug-traces' },
} as const satisfies Record<NavKey, NavItem>

function navNode(itemKey: NavKey, t: (key: string) => string): SidebarNode {
  const item = navItems[itemKey]

  return {
    key: item.key,
    title: t(`nav.${item.key}`),
    href: item.href,
    icon: item.icon,
  }
}

function groupNode(
  key: string,
  title: string,
  children: SidebarNode[],
): SidebarNode {
  return { key: `group:${key}`, title, children }
}

function customNavigationTree(links: SiteSidebarLink[], english: boolean): SidebarNode[] {
  const roots: SidebarNode[] = []
  const stack: Array<{ level: number; node: SidebarNode }> = []
  for (const [index, link] of links.entries()) {
    const level = Math.max(1, Math.min(3, link.level ?? 1))
    while (stack.length >= level) stack.pop()
    const node: SidebarNode = {
      key: `custom:${index}:${link.url}`,
      title: english && link.label_en ? link.label_en : link.label,
      href: link.url,
      icon: customIcons[link.icon] ?? customIcons.link,
      children: [],
    }
    const parent = stack.at(-1)?.node
    if (parent) parent.children?.push(node)
    else roots.push(node)
    stack.push({ level, node })
  }
  return roots
}

/** 构建控制台侧栏导航节点树，交给 acmeidc 的 ModuleSidebarLayoutContainer 渲染。 */
export function useAppNavigationItems(role: 'admin' | 'user'): SidebarNode[] {
  const eligibility = useAccountEligibility()
  const { i18n, t } = useTranslation()
  const siteQuery = usePublicSiteSettings()
  const customLinks = siteQuery.data?.navigation.sidebar_links
  const {
    organizationPermissions,
    organizationPermissionsLoading,
    selectedId,
  } = useOrganizationWorkspace()

  return useMemo(() => {
    /*
     * 侧栏固定三个板块：工作台 / 企业管理 / 系统管理。
     * 前两档两类角色都有，系统管理只给管理员——它装的都是平台运维与网关配置。
     * 用户角色的企业条目要按成员权限裁剪，管理员则看完整集合。
     */
    const enterpriseItems: readonly NavKey[] = role === 'admin'
      ? [...organizationSectionItems, ...enterpriseAdminItems]
      : selectedId === undefined && eligibility.data?.enterprise_verified
      ? ['organizationProvisioning']
      : filterOrganizationNavigationItems(
        organizationSectionItems,
        organizationPermissions,
        selectedId !== undefined,
        organizationPermissionsLoading,
      )

    const sections: Array<{ key: NavigationSectionKey; items: readonly NavKey[] }> = [
      { key: 'workspace', items: role === 'admin' ? adminWorkspaceItems : userWorkspaceItems.filter((item) => item !== 'organizationProvisioning' || !enterpriseItems.includes('organizationProvisioning')) },
      { key: 'enterprise', items: enterpriseItems },
    ]
    if (role === 'admin') {
      sections.push({ key: 'system', items: systemSectionItems })
    }

    return sections
      .filter((section) => section.items.length > 0)
      .map((section) => groupNode(
        section.key,
        t(`nav.sections.${section.key}`),
        [
          ...section.items.map((itemKey) => navNode(itemKey, t)),
          ...(section.key === 'workspace' ? customNavigationTree(
            (customLinks ?? []).filter((link) => link.audience === 'all' || role === 'admin'),
            i18n.resolvedLanguage?.startsWith('en') ?? false,
          ) : []),
        ],
      ))
  }, [customLinks, eligibility.data?.enterprise_verified, i18n.resolvedLanguage, organizationPermissions, organizationPermissionsLoading, role, selectedId, t])
}
