import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'

import {
  activeKeyByView,
  adminNavigationItems,
  adminNavigationSections,
  adminSystemItems,
  adminWorkspaceItems,
  userNavigationGroups,
  userNavigationItems,
} from '../src/components/layout/app-navigation-model.ts'

const expectedAdminItems = [
  'overview', 'models', 'playground', 'videoTasks',
  'apiKeys', 'wallet', 'subscriptions', 'invitations', 'profile',
  'organizationOverview', 'organizationMembers', 'organizationDepartments', 'organizationApiKeys', 'organizationServiceAccounts', 'organizationBudgets', 'organizationContractPrices', 'organizationApprovals',
  'organizationWallet', 'organizationActivity', 'organizationSettings', 'organizationCredit', 'organizationProvisioning',
  'platformOrganizations', 'users', 'subscriptionManagement', 'organizationPlans', 'redemptionCodes', 'refunds', 'paymentSettings', 'billingSettings',
  'modelSettings', 'groupSettings', 'channels', 'credentials', 'credentialProxies', 'routes',
  'siteSettings', 'authenticationSettings', 'announcements', 'emailSettings', 'networkSettings', 'keys',
  'usage', 'debugTraces',
]

test('管理员导航覆盖全部入口且没有重复项', () => {
  assert.deepEqual([...adminNavigationItems].sort(), [...expectedAdminItems].sort())
  assert.equal(new Set(adminNavigationItems).size, adminNavigationItems.length)
})

test('管理员工作台和任务域保持规划顺序', () => {
  assert.deepEqual(adminWorkspaceItems, ['overview', 'usage', 'models', 'playground', 'videoTasks'])
  assert.deepEqual(adminNavigationSections.map((section) => section.key), [
    'personal', 'organization', 'operations', 'gateway', 'platform',
  ])
  assert.deepEqual(adminSystemItems, ['debugTraces'])
})

test('调用日志是角色感知的共享入口而不是管理员任务域', () => {
  assert.ok(adminWorkspaceItems.includes('usage'))
  assert.ok(userNavigationGroups.find((group) => group.key === 'account')?.items.includes('usage'))
  assert.ok(adminNavigationSections.every((section) => !section.items.includes('usage')))
})

test('普通用户导航增加独立企业空间且保留原账户入口', () => {
  assert.deepEqual(userNavigationGroups.map((group) => group.key), ['use', 'organization', 'account'])
  assert.deepEqual(userNavigationItems, [
    'models', 'playground', 'videoTasks',
    'organizationOverview', 'organizationMembers', 'organizationDepartments', 'organizationApiKeys', 'organizationServiceAccounts', 'organizationBudgets', 'organizationContractPrices', 'organizationCredit', 'organizationApprovals',
    'organizationWallet', 'organizationActivity', 'organizationSettings',
  'apiKeys', 'wallet', 'usage', 'subscriptions', 'invitations', 'organizationProvisioning', 'profile',
  ])
})

test('每个管理员入口都有路由反向映射', () => {
  const mappedItems = new Set(Object.values(activeKeyByView))
  for (const item of expectedAdminItems) assert.ok(mappedItems.has(item), `缺少 ${item} 的活动路由映射`)
})

test('管理员导航分组具备完整中英文文案', async () => {
  const locales = await Promise.all(['zh', 'en'].map(async (locale) => JSON.parse(
    await readFile(new URL(`../src/i18n/locales/${locale}.json`, import.meta.url), 'utf8'),
  )))
  for (const locale of locales) {
    assert.equal(typeof locale.nav.groups.workspace, 'string')
    assert.equal(typeof locale.nav.groups.personal, 'string')
    assert.equal(typeof locale.nav.groups.organization, 'string')
    for (const section of adminNavigationSections) {
      assert.equal(typeof locale.nav.sections[section.key], 'string')
    }
  }
})

test('子菜单仅为当前活动项使用强调色图标', async () => {
  const sidebarSource = await readFile(new URL('../src/components/ui/sidebar.tsx', import.meta.url), 'utf8')
  assert.match(sidebarSource, /\[&>svg\]:text-sidebar-foreground\/70/)
  assert.match(sidebarSource, /data-active:\[&>svg\]:text-sidebar-accent-foreground/)
  assert.doesNotMatch(sidebarSource, /\[&>svg\]:shrink-0 \[&>svg\]:text-sidebar-accent-foreground/)
})
