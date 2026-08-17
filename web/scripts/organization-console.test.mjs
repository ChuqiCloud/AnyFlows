import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'

import { routeFromHash } from '../src/app-route.ts'

const openapi = JSON.parse(await readFile(new URL('../openapi/openapi.json', import.meta.url), 'utf8'))

test('企业控制台路由覆盖首版任务域', () => {
  assert.deepEqual(routeFromHash('#/console/organization'), { view: 'organization-overview' })
  assert.deepEqual(routeFromHash('#/console/organization-members'), { view: 'organization-members' })
  assert.deepEqual(routeFromHash('#/console/organization-wallet'), { view: 'organization-wallet' })
  assert.deepEqual(routeFromHash('#/console/organization-settings'), { view: 'organization-settings' })
})

test('企业缓存键始终包含企业域', async () => {
  const apiSource = await readFile(
    new URL('../src/features/organization-budgets/organization-budget-api.ts', import.meta.url),
    'utf8',
  )
  assert.match(apiSource, /const organizationQueryRoot = \['organization'\] as const/)
  assert.match(apiSource, /return \[\.\.\.organizationQueryRoot, organizationId, \.\.\.resources\] as const/)
})

test('切回个人空间时保留最后企业偏好并只清理企业缓存', async () => {
  const workspaceSource = await readFile(
    new URL('../src/features/organization-console/organization-workspace-context.tsx', import.meta.url),
    'utf8',
  )
  assert.match(workspaceSource, /const selectPersonalWorkspace = useCallback/)
  assert.match(workspaceSource, /await clearOrganizationWorkspaceCache\(queryClient, preferredId\)/)
  assert.doesNotMatch(workspaceSource, /selectPersonalWorkspace: \(\) => changeSelection\(undefined\)/)
})

test('企业成员、团队、邀请和钱包使用现有服务端契约', () => {
  const operations = [
    ['/api/organizations/{organization_id}/members', 'get', 'listOrganizationMembers'],
    ['/api/organizations/{organization_id}/members/{membership_id}', 'patch', 'changeOrganizationMember'],
    ['/api/organizations/{organization_id}/teams', 'get', 'listOrganizationTeams'],
    ['/api/organizations/{organization_id}/invitations/preview', 'post', 'previewOrganizationInvitations'],
    ['/api/organizations/{organization_id}/wallet', 'get', 'getOrganizationWallet'],
    ['/api/organizations/{organization_id}/wallet/entries', 'get', 'listOrganizationWalletEntries'],
    ['/api/organizations/{organization_id}/topups', 'post', 'createOrganizationTopupOrder'],
  ]
  for (const [path, method, operationId] of operations) {
    assert.equal(openapi.paths[path][method].operationId, operationId)
  }
})

test('企业充值恢复状态按企业隔离', async () => {
  const runtime = await readFile(new URL('../src/features/wallet/wallet-topup-runtime.ts', import.meta.url), 'utf8')
  const panel = await readFile(new URL('../src/features/wallet/wallet-topup-panel.tsx', import.meta.url), 'utf8')
  assert.match(runtime, /topupStorageKey\(scope\)/)
  assert.match(panel, /organization:\$\{organizationId\}/)
  assert.match(panel, /#\/console\/organization-wallet/)
})

test('企业控制台具备中英文入口和关键状态文案', async () => {
  const locales = await Promise.all(['zh', 'en'].map(async (locale) => JSON.parse(
    await readFile(new URL(`../src/i18n/locales/${locale}.json`, import.meta.url), 'utf8'),
  )))
  for (const locale of locales) {
    assert.equal(typeof locale.nav.organizationOverview, 'string')
    assert.equal(typeof locale.nav.organizationMembers, 'string')
    assert.equal(typeof locale.nav.organizationWallet, 'string')
    assert.equal(typeof locale.nav.organizationSettings, 'string')
    assert.equal(typeof locale.organizationConsole.workspace.personal, 'string')
    assert.equal(typeof locale.organizationConsole.invitations.batch.preview, 'string')
    assert.equal(typeof locale.organizationConsole.wallet.adjustment.title, 'string')
    assert.equal(typeof locale.invitations.organization.accept, 'string')
  }
})
