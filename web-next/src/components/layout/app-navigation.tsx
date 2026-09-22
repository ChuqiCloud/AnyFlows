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
  announcements: { key: 'announcements', icon: 'solar:notes-linear', href: '/console/system-settings/announcements' },
  authenticationSettings: { key: 'authenticationSettings', icon: 'solar:password-linear', href: '/console/system-settings/authentication' },
  emailSettings: { key: 'emailSettings', icon: 'solar:letter-linear', href: '/console/system-settings/email' },
  networkSettings: { key: 'networkSettings', icon: 'solar:global-linear', href: '/console/system-settings/network' },
  paymentSettings: { key: 'paymentSettings', icon: 'solar:card-2-linear', href: '/console/system-settings/payment' },
  billingSettings: { key: 'billingSettings', icon: 'solar:bill-linear', href: '/console/system-settings/billing' },
  groupSettings: { key: 'groupSettings', icon: 'solar:folder-linear', href: '/console/system-settings/groups' },
  modelSettings: { key: 'modelSettings', icon: 'solar:box-minimalistic-linear', href: '/console/system-settings/models' },
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

/** 构建控制台侧栏导航节点树，交给 acmeidc 的 ModuleSidebarLayoutContainer 渲染。 */
export function useAppNavigationItems(role: 'admin' | 'user'): SidebarNode[] {
  const { t } = useTranslation()
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
      : filterOrganizationNavigationItems(
        organizationSectionItems,
        organizationPermissions,
        selectedId !== undefined,
        organizationPermissionsLoading,
      )

    const sections: Array<{ key: NavigationSectionKey; items: readonly NavKey[] }> = [
      { key: 'workspace', items: role === 'admin' ? adminWorkspaceItems : userWorkspaceItems },
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
        section.items.map((itemKey) => navNode(itemKey, t)),
      ))
  }, [organizationPermissions, organizationPermissionsLoading, role, selectedId, t])
}
