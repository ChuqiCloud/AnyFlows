import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'

import { activeKeyByView, adminNavigationItems, adminNavigationSections, adminSystemItems, adminWorkspaceItems, userNavigationGroups, userNavigationItems } from '../src/components/layout/app-navigation-model.ts'

const expectedAdminItems = [
  'overview', 'models', 'playground', 'videoTasks', 'usage', 'apiKeys', 'wallet', 'subscriptions', 'invitations', 'accountVerification', 'profile',
  'subscriptionManagement', 'redemptionCodes', 'refunds', 'paymentSettings', 'billingSettings', 'modelSettings', 'modelProviders', 'groupSettings', 'channels', 'credentials', 'credentialProxies', 'routes',
  'users', 'siteSettings', 'siteNavigation', 'authenticationSettings', 'announcements', 'emailSettings', 'networkSettings', 'accountVerificationSettings', 'keys', 'debugTraces',
]

test('管理员导航覆盖公共核心入口且没有重复项', () => {
  assert.deepEqual([...adminNavigationItems].sort(), [...expectedAdminItems].sort())
  assert.equal(new Set(adminNavigationItems).size, adminNavigationItems.length)
})

test('管理员工作台和任务域保持规划顺序', () => {
  assert.deepEqual(adminWorkspaceItems, ['overview', 'models', 'playground', 'videoTasks', 'usage', 'apiKeys', 'wallet', 'subscriptions', 'invitations', 'accountVerification', 'profile'])
  assert.deepEqual(adminNavigationSections.map((section) => section.key), ['operations', 'gateway', 'platform'])
  assert.deepEqual(adminSystemItems, ['debugTraces'])
})

test('调用日志是角色感知的共享入口', () => {
  assert.ok(adminWorkspaceItems.includes('usage'))
  assert.ok(userNavigationGroups[0].items.includes('usage'))
  assert.ok(adminNavigationSections.every((section) => !section.items.includes('usage')))
})

test('普通用户导航集中到公共工作台', () => {
  assert.deepEqual(userNavigationGroups.map((group) => group.key), ['workspace'])
  assert.deepEqual(userNavigationItems, ['models', 'playground', 'videoTasks', 'usage', 'apiKeys', 'wallet', 'subscriptions', 'invitations', 'accountVerification', 'profile'])
})

test('每个管理员入口都有路由反向映射', () => {
  const mappedItems = new Set(Object.values(activeKeyByView))
  for (const item of expectedAdminItems) assert.ok(mappedItems.has(item), `缺少 ${item} 的活动路由映射`)
})

test('管理员导航分组具备完整中英文文案', async () => {
  const locales = await Promise.all(['zh', 'en'].map(async (locale) => JSON.parse(await readFile(new URL(`../src/i18n/locales/${locale}.json`, import.meta.url), 'utf8'))))
  for (const locale of locales) {
    assert.equal(typeof locale.nav.groups.workspace, 'string')
    assert.equal(typeof locale.nav.sections.system, 'string')
    for (const section of adminNavigationSections) assert.equal(typeof locale.nav.sections[section.key], 'string')
  }
})

test('子菜单仅为当前活动项使用强调色图标', async () => {
  const sidebarSource = await readFile(new URL('../src/components/ui/sidebar.tsx', import.meta.url), 'utf8')
  assert.match(sidebarSource, /\[&>svg\]:text-sidebar-foreground\/70/)
  assert.match(sidebarSource, /data-active:\[&>svg\]:text-sidebar-accent-foreground/)
})
