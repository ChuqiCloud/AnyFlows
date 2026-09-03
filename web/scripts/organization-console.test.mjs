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
  assert.deepEqual(routeFromHash('#/console/organization-activity?tab=audit'), { view: 'organization-activity', activityTab: 'audit' })
  assert.deepEqual(routeFromHash('#/console/organization-activity?tab=unknown'), { view: 'organization-activity' })
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
    ['/api/organizations/{organization_id}/members/{membership_id}/custom-role', 'patch', 'setOrganizationMemberCustomRole'],
    ['/api/organizations/{organization_id}/teams', 'get', 'listOrganizationTeams'],
    ['/api/organizations/{organization_id}/invitations/preview', 'post', 'previewOrganizationInvitations'],
    ['/api/organizations/{organization_id}/wallet', 'get', 'getOrganizationWallet'],
    ['/api/organizations/{organization_id}/wallet/entries', 'get', 'listOrganizationWalletEntries'],
    ['/api/organizations/{organization_id}/topups', 'post', 'createOrganizationTopupOrder'],
    ['/api/organizations/{organization_id}/sso/providers', 'get', 'listOrganizationSsoProviders'],
    ['/api/organizations/{organization_id}/sso/domains', 'get', 'listOrganizationSsoDomains'],
    ['/api/organizations/{organization_id}/sso/domains', 'post', 'createOrganizationSsoDomain'],
    ['/api/organizations/{organization_id}/sso/domains/{domain_id}/verify', 'post', 'verifyOrganizationSsoDomain'],
    ['/api/organizations/{organization_id}/sso/policy', 'get', 'getOrganizationSsoPolicy'],
    ['/api/organizations/{organization_id}/sso/policy', 'put', 'updateOrganizationSsoPolicy'],
  ]
  for (const [path, method, operationId] of operations) {
    assert.equal(openapi.paths[path][method].operationId, operationId)
  }
})

test('企业 SSO 面板只在授权允许时读取 Provider，并显式保留未闭环状态', async () => {
  const [apiSource, panelSource, domainsSource, policySource, pageSource] = await Promise.all([
    readFile(new URL('../src/features/organization-sso/organization-sso-api.ts', import.meta.url), 'utf8'),
    readFile(new URL('../src/features/organization-sso/organization-sso-panel.tsx', import.meta.url), 'utf8'),
    readFile(new URL('../src/features/organization-sso/organization-sso-domains.tsx', import.meta.url), 'utf8'),
    readFile(new URL('../src/features/organization-sso/organization-sso-policy.tsx', import.meta.url), 'utf8'),
    readFile(new URL('../src/features/organization-console/organization-settings-page.tsx', import.meta.url), 'utf8'),
  ])
  assert.match(apiSource, /enabled,\n    queryFn/)
  assert.match(apiSource, /listOrganizationSsoProviders/)
  assert.match(panelSource, /organization\.sso\.read/)
  assert.match(panelSource, /organization\.sso\.manage/)
  assert.match(panelSource, /query\.isPending/)
  assert.match(panelSource, /query\.isError/)
  assert.match(panelSource, /providers\.length === 0/)
  assert.match(panelSource, /OrganizationSsoDomains/)
  assert.match(domainsSource, /useOrganizationSsoDomains/)
  assert.match(domainsSource, /useCreateOrganizationSsoDomain/)
  assert.match(domainsSource, /useVerifyOrganizationSsoDomain/)
  assert.match(domainsSource, /txt_record_value/)
  assert.match(domainsSource, /verifyMutation\.isPending/)
  assert.match(domainsSource, /query\.isError/)
  assert.match(domainsSource, /query\.data\.length === 0/)
  assert.match(domainsSource, /organizationConsole\.settings\.sso\.domains\.status/)
  assert.match(apiSource, /useOrganizationSsoPolicy/)
  assert.match(panelSource, /OrganizationSsoPolicyPanel/)
  assert.match(panelSource, /type ClosureStatus/)
  assert.match(panelSource, /queryClosureStatus/)
  assert.match(panelSource, /organization\.audit\.read/)
  assert.match(panelSource, /organization-activity\?tab=audit/)
  assert.match(panelSource, /'restricted'/)
  assert.match(policySource, /organization\.sso\.recovery/)
  assert.match(policySource, /query\.isPending/)
  assert.match(policySource, /recovery_ready/)
  assert.match(policySource, /suspendMutation\.mutateAsync/)
  assert.doesNotMatch(policySource, /localStorage|sessionStorage/)
  assert.match(pageSource, /organization\.sso\.recovery/)
})

test('成员自定义角色分配遵循服务端授权、CAS 与公开标识边界', async () => {
  const [apiSource, pageSource, editorSource] = await Promise.all([
    readFile(new URL('../src/features/organization-members/organization-member-api.ts', import.meta.url), 'utf8'),
    readFile(new URL('../src/features/organization-members/organization-member-page.tsx', import.meta.url), 'utf8'),
    readFile(new URL('../src/features/organization-members/organization-member-editor-sheet.tsx', import.meta.url), 'utf8'),
  ])
  assert.equal(openapi.components.schemas.OrganizationMembershipResponse.properties.custom_role_key.type[1], 'null')
  assert.equal(openapi.components.schemas.OrganizationMemberCustomRoleRequest.properties.role_key.type[1], 'null')
  assert.match(apiSource, /useSetOrganizationMemberCustomRole/)
  assert.match(apiSource, /setOrganizationMemberCustomRole/)
  assert.match(pageSource, /organization\.roles\.assign/)
  assert.match(pageSource, /custom_roles_enabled/)
  assert.match(editorSource, /expected_version: updated\.version/)
  assert.match(editorSource, /role_key: customRoleKey \|\| null/)
})

test('企业自定义角色目录只在授权允许时读取，并限制为服务端可授予权限', async () => {
  const [apiSource, panelSource, editorSource, permissionsSource] = await Promise.all([
    readFile(new URL('../src/features/organization-roles/organization-custom-role-api.ts', import.meta.url), 'utf8'),
    readFile(new URL('../src/features/organization-roles/organization-custom-role-panel.tsx', import.meta.url), 'utf8'),
    readFile(new URL('../src/features/organization-roles/organization-custom-role-editor-sheet.tsx', import.meta.url), 'utf8'),
    readFile(new URL('../src/features/organization-roles/organization-custom-role-permissions.ts', import.meta.url), 'utf8'),
  ])
  const operations = [
    ['/api/organizations/{organization_id}/roles', 'get', 'listOrganizationCustomRoles'],
    ['/api/organizations/{organization_id}/roles', 'post', 'createOrganizationCustomRole'],
    ['/api/organizations/{organization_id}/roles/{role_key}', 'patch', 'updateOrganizationCustomRole'],
    ['/api/organizations/{organization_id}/roles/{role_key}/disable', 'post', 'disableOrganizationCustomRole'],
  ]
  for (const [path, method, operationId] of operations) {
    assert.equal(openapi.paths[path][method].operationId, operationId)
  }
  assert.match(apiSource, /queryKey: organizationQueryKey\(organizationId, 'custom-roles', 'pages'\)/)
  assert.match(apiSource, /expected_version: expectedVersion/)
  assert.match(panelSource, /customRolesEnabled && canRead/)
  assert.match(panelSource, /organization\.roles\.read/)
  assert.match(panelSource, /organization\.roles\.manage/)
  assert.match(editorSource, /expected_version: role\.version/)
  assert.match(editorSource, /CUSTOM_ROLE_PERMISSION_CODES/)
  assert.match(permissionsSource, /organization\.roles\.read/)
  assert.doesNotMatch(permissionsSource, /organization\.ownership\.transfer/)
  assert.doesNotMatch(permissionsSource, /organization\.close/)
  assert.doesNotMatch(permissionsSource, /organization\.roles\.manage/)
  assert.doesNotMatch(permissionsSource, /organization\.roles\.assign/)
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
    assert.equal(typeof locale.organizationConsole.empty.openInvitations, 'string')
    assert.equal(typeof locale.organizationConsole.empty.provisioningHint, 'string')
    assert.equal(typeof locale.organizationConsole.overview.entitlement.validUntil, 'string')
    assert.equal(typeof locale.organizationConsole.overview.funding.wallet, 'string')
    assert.equal(typeof locale.organizationConsole.members.fields.customRole, 'string')
    assert.equal(typeof locale.organizationConsole.members.customRoles.disabledByEntitlement, 'string')
    assert.equal(typeof locale.organizationConsole.customRoles.title, 'string')
    assert.equal(typeof locale.organizationConsole.customRoles.editor.permissionsHint, 'string')
    assert.equal(typeof locale.organizationConsole.customRoles.disable.description, 'string')
    assert.equal(typeof locale.organizationConsole.permissions.organization.roles.read, 'string')
    assert.equal(typeof locale.organizationConsole.settings.sso.title, 'string')
    assert.equal(typeof locale.organizationConsole.settings.sso.domains.title, 'string')
    assert.equal(typeof locale.organizationConsole.settings.sso.domains.verify, 'string')
    assert.equal(typeof locale.organizationConsole.settings.sso.closure.available, 'string')
    assert.equal(typeof locale.organizationConsole.settings.sso.closure.hint, 'string')
    assert.equal(typeof locale.organizationConsole.invitations.batch.preview, 'string')
    assert.equal(typeof locale.organizationConsole.wallet.adjustment.title, 'string')
    assert.equal(typeof locale.invitations.organization.accept, 'string')
  }
})

test('企业空状态引导已有邀请中心，概览只展示服务端授权与资金边界', async () => {
  const [shellSource, overviewSource] = await Promise.all([
    readFile(new URL('../src/features/organization-console/organization-page-shell.tsx', import.meta.url), 'utf8'),
    readFile(new URL('../src/features/organization-console/organization-overview-page.tsx', import.meta.url), 'utf8'),
  ])
  assert.match(shellSource, /#\/console\/invitations/)
  assert.match(shellSource, /organizationConsole\.empty\.provisioningHint/)
  assert.match(overviewSource, /overview\.entitlement\.valid_until/)
  assert.match(overviewSource, /overview\.entitlement\.grace_until/)
  assert.match(overviewSource, /overview\.funding\.wallet/)
  assert.match(overviewSource, /overview\.funding\.upgrade/)
})
