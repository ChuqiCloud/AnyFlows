import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'
import { useConsoleExtensionNavigation } from '@/extensions/query'

import {
  adminWorkspaceItems,
  type NavKey,
  type NavigationSectionKey,
  systemSectionItems,
  userWorkspaceItems,
} from '@/components/layout/app-navigation-model'
import type { SidebarNode } from '@/shared/app-shell'
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
  accountVerification: { key: 'accountVerification', icon: 'solar:verified-check-linear', href: '/console/account-verification' },
  accountVerificationSettings: { key: 'accountVerificationSettings', icon: 'solar:settings-linear', href: '/console/system-settings/account-verification' },
  wallet: { key: 'wallet', icon: 'solar:wallet-linear', href: '/console/wallet' },
  subscriptions: { key: 'subscriptions', icon: 'solar:calendar-mark-linear', href: '/console/subscriptions' },
  invitations: { key: 'invitations', icon: 'solar:gift-linear', href: '/console/invitations' },
  profile: { key: 'profile', icon: 'solar:user-id-linear', href: '/console/profile' },
  users: { key: 'users', icon: 'solar:users-group-two-rounded-linear', href: '/console/users' },
  subscriptionManagement: { key: 'subscriptionManagement', icon: 'solar:layers-minimalistic-linear', href: '/console/subscription-management' },
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

function customNavigationTree(links: SiteSidebarLink[], english: boolean, role: 'admin' | 'user'): SidebarNode[] {
  const roots: SidebarNode[] = []
  const parents: Array<{ node: SidebarNode; visible: boolean }> = []
  for (const [index, link] of links.entries()) {
    const level = Math.min(3, Math.max(1, link.level ?? 1))
    const visible = link.audience === 'all' || role === 'admin'
    const node: SidebarNode = {
      key: `custom:${index}:${link.url}`,
      title: english && link.label_en ? link.label_en : link.label,
      href: link.kind === 'group' ? undefined : link.url,
      icon: customIcons[link.icon] ?? customIcons.link,
      children: [],
    }
    if (level === 1) {
      parents.length = 0
      parents.push({ node, visible })
      if (visible) roots.push(node)
      continue
    }
    const parent = parents[level - 2]
    if (!parent) continue
    parents.length = level
    parents[level - 1] = { node, visible: parent.visible && visible }
    if (!parent.visible || !visible) continue
    if (parent.node.href && parent.node.children?.length === 0) {
      parent.node.children.push({ ...parent.node, key: `${parent.node.key}:overview`, children: undefined })
      delete parent.node.href
    }
    parent.node.children?.push(node)
  }
  return roots.filter((node) => node.href || (node.children?.length ?? 0) > 0)
}

/** 构建控制台侧栏导航节点树，交给 acmeidc 的 ModuleSidebarLayoutContainer 渲染。 */
export function useAppNavigationItems(role: 'admin' | 'user'): SidebarNode[] {
  const extensionRoutes = useConsoleExtensionNavigation(role)
  const { i18n, t } = useTranslation()
  const siteQuery = usePublicSiteSettings()
  const customLinks = siteQuery.data?.navigation.sidebar_links

  return useMemo(() => {
    const sections: Array<{ key: NavigationSectionKey; items: readonly NavKey[] }> = [
      { key: 'workspace', items: role === 'admin' ? adminWorkspaceItems : userWorkspaceItems },
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
          ...(section.key === 'workspace' ? extensionRoutes.map((route): SidebarNode => ({
            key: `extension:${route.path}`, title: t(route.titleKey), href: route.path, icon: 'solar:buildings-2-linear',
          })) : []),
          ...(section.key === 'workspace' ? customNavigationTree(
            customLinks ?? [],
            i18n.resolvedLanguage?.startsWith('en') ?? false,
            role,
          ) : []),
        ],
      ))
  }, [customLinks, extensionRoutes, i18n.resolvedLanguage, role, t])
}
