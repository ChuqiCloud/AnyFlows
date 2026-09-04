import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'

const read = (path) => readFile(new URL(path, import.meta.url), 'utf8')

test('企业开通申请的 HTTP、OpenAPI、生成客户端和前端入口保持同步', async () => {
  const [openapi, sdk, types, route, page, en, zh] = await Promise.all([
    read('../openapi/openapi.json').then(JSON.parse),
    read('../src/lib/api/generated/sdk.gen.ts'),
    read('../src/lib/api/generated/types.gen.ts'),
    read('../src/app-route.ts'),
    read('../src/features/organization-provisioning/organization-provisioning-page.tsx'),
    read('../src/i18n/locales/en.json').then(JSON.parse),
    read('../src/i18n/locales/zh.json').then(JSON.parse),
  ])

  const paths = [
    ['/api/account/organization-provisioning', 'post', 'createOrganizationProvisioningRequest'],
    ['/api/account/organization-provisioning/active', 'get', 'getActiveOrganizationProvisioningRequest'],
    ['/api/account/organization-provisioning/{request_key}', 'get', 'getOrganizationProvisioningRequest'],
    ['/api/admin/organization-provisioning', 'get', 'listAdminOrganizationProvisioningRequests'],
    ['/api/admin/organization-provisioning/{request_key}', 'get', 'getAdminOrganizationProvisioningRequest'],
    ['/api/admin/organization-provisioning/{request_key}/decision', 'post', 'decideOrganizationProvisioningRequest'],
  ]
  for (const [path, method, operationId] of paths) {
    assert.equal(openapi.paths[path]?.[method]?.operationId, operationId)
    assert.match(sdk, new RegExp(`export const ${operationId}\\b`))
  }
  for (const schema of ['OrganizationProvisioningStatus', 'OrganizationProvisioningDecisionKind', 'OrganizationProvisioningCreateRequest', 'OrganizationProvisioningEntitlement', 'OrganizationProvisioningDecisionRequest', 'OrganizationProvisioningRequest', 'OrganizationProvisioningRequestListResponse']) {
    assert.ok(openapi.components.schemas[schema])
    assert.match(types, new RegExp(`export type ${schema}\\b`))
  }
  assert.match(route, /organization-provisioning/)
  assert.match(page, /useActiveOrganizationProvisioning/)
  assert.match(page, /useAdminOrganizationProvisioning/)
  assert.equal(typeof en.organizationProvisioning.title, 'string')
  assert.equal(typeof zh.organizationProvisioning.title, 'string')
  assert.equal(typeof en.organizationConsole.empty.applyProvisioning, 'string')
  assert.equal(typeof zh.organizationConsole.empty.applyProvisioning, 'string')
})
